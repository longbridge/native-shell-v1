// Validating decoder of one batch buffer. The whole batch is decoded before
// any of it is applied, so a malformed buffer is rejected as a unit and the
// view tree is never left half-updated.

import Foundation

public enum DecodeError: Error, Equatable, CustomStringConvertible {
    case misaligned
    case version(UInt32)
    case truncated(Int)
    case opcode(UInt8, Int)
    case unknown(String, UInt32)
    case index(UInt32)
    case utf8

    public var description: String {
        switch self {
        case .misaligned: return "buffer is not a whole number of words"
        case let .version(v): return "bad magic or unsupported protocol version \(v)"
        case let .truncated(at): return "truncated at word \(at)"
        case let .opcode(op, at): return "unknown opcode \(op) at word \(at)"
        case let .unknown(what, v): return "unknown \(what) value \(v)"
        case let .index(i): return "table index \(i) out of range"
        case .utf8: return "string table entry is not UTF-8"
        }
    }
}

public enum BatchDecoder {
    public static func decode(_ data: Data) throws -> [Mutation] {
        try data.withUnsafeBytes { try decode(UnsafeRawBufferPointer($0)) }
    }

    public static func decode(_ bytes: UnsafeRawBufferPointer) throws -> [Mutation] {
        guard bytes.count % 4 == 0 else { throw DecodeError.misaligned }
        let wordCount = bytes.count / 4
        guard wordCount >= Wire.headerWords else { throw DecodeError.truncated(wordCount) }
        @inline(__always) func word(_ i: Int) -> UInt32 {
            UInt32(littleEndian: bytes.loadUnaligned(fromByteOffset: i * 4, as: UInt32.self))
        }
        guard word(0) == Wire.magic, word(1) == Wire.protocolVersion else { throw DecodeError.version(word(1)) }
        let n = Int(word(2))
        let stringCount = Int(word(3))
        let arrayCount = Int(word(4))
        let (bodyEnd, overflow) = Wire.headerWords.addingReportingOverflow(n)
        guard !overflow, bodyEnd <= wordCount else { throw DecodeError.truncated(wordCount) }

        var at = bodyEnd
        var strings: [String] = []
        strings.reserveCapacity(min(stringCount, wordCount))
        for _ in 0 ..< stringCount {
            guard at < wordCount else { throw DecodeError.truncated(at) }
            let len = Int(word(at))
            at += 1
            let wc = (len + 3) / 4
            guard wc <= wordCount - at else { throw DecodeError.truncated(at) }
            let raw = UnsafeRawBufferPointer(rebasing: bytes[(at * 4) ..< (at * 4 + len)])
            guard let s = String(validatingUTF8Bytes: raw) else { throw DecodeError.utf8 }
            strings.append(s)
            at += wc
        }
        var arrays: [[Float]] = []
        arrays.reserveCapacity(min(arrayCount, wordCount))
        for _ in 0 ..< arrayCount {
            guard at < wordCount else { throw DecodeError.truncated(at) }
            let len = Int(word(at))
            at += 1
            guard len <= wordCount - at else { throw DecodeError.truncated(at) }
            var a = [Float]()
            a.reserveCapacity(len)
            for k in 0 ..< len { a.append(Float(bitPattern: word(at + k))) }
            arrays.append(a)
            at += len
        }

        var out: [Mutation] = []
        out.reserveCapacity(n / 3)
        var i = Wire.headerWords
        func take() throws -> UInt32 {
            guard i < bodyEnd else { throw DecodeError.truncated(i) }
            defer { i += 1 }
            return word(i)
        }
        func string(_ idx: UInt32) throws -> String {
            guard Int(idx) < strings.count else { throw DecodeError.index(idx) }
            return strings[Int(idx)]
        }
        while i < bodyEnd {
            let start = i
            let head = try take()
            let rawOp = UInt8(head & 0xFF)
            let node = rawOp & Wire.extendedNode != 0 ? try take() : head >> 8
            guard let op = Op(rawValue: rawOp & ~Wire.extendedNode) else { throw DecodeError.opcode(rawOp, start) }
            switch op {
            case .beginBatch:
                let seq = try take()
                out.append(.beginBatch(seq: seq, flags: try take()))
            case .endBatch:
                out.append(.endBatch)
            case .create:
                let t = try take()
                guard let type = NSHViewType(rawValue: t) else { throw DecodeError.unknown("view type", t) }
                out.append(.create(node: node, type: type))
            case .insert:
                let parent = try take()
                out.append(.insert(node: node, parent: parent, index: try take()))
            case .remove:
                let parent = try take()
                out.append(.remove(node: node, parent: parent, index: try take()))
            case .delete:
                out.append(.delete(node: node))
            case .move:
                let parent = try take()
                let from = try take()
                out.append(.move(node: node, parent: parent, from: from, to: try take()))
            case .setProp:
                let w = try take()
                guard let prop = Prop(rawValue: UInt16(w & 0xFFFF)) else { throw DecodeError.unknown("prop", w & 0xFFFF) }
                let kindRaw = UInt8((w >> 16) & 0xFF)
                guard let kind = ValueKind(rawValue: kindRaw) else { throw DecodeError.unknown("value kind", UInt32(kindRaw)) }
                let value: PropValue
                switch kind {
                case .null: value = .null
                case .bool: value = .bool(try take() != 0)
                case .i32: value = .i32(Int32(bitPattern: try take()))
                case .f32: value = .f32(Float(bitPattern: try take()))
                case .color: value = .color(try take())
                case .str: value = .str(try string(try take()))
                case .json: value = .json(try string(try take()))
                case .f32Array:
                    let idx = try take()
                    guard Int(idx) < arrays.count else { throw DecodeError.index(idx) }
                    value = .f32Array(arrays[Int(idx)])
                }
                out.append(.setProp(node: node, prop: prop, value: value))
            case .setFrame:
                let x = Float(bitPattern: try take())
                let y = Float(bitPattern: try take())
                let w = Float(bitPattern: try take())
                let h = Float(bitPattern: try take())
                out.append(.setFrame(node: node, frame: Frame(x: x, y: y, width: w, height: h)))
            case .setEventMask:
                out.append(.setEventMask(node: node, mask: try take()))
            case .command:
                let c = try take()
                guard let command = Command(rawValue: c) else { throw DecodeError.unknown("command", c) }
                out.append(.command(node: node, command: command, payload: try string(try take())))
            }
        }
        return out
    }
}

extension String {
    /// UTF-8 decoding that fails on invalid sequences instead of repairing them.
    public init?(validatingUTF8Bytes raw: UnsafeRawBufferPointer) {
        guard let s = String(bytes: raw, encoding: .utf8) else {
            if raw.isEmpty { self = ""; return }
            return nil
        }
        self = s
    }
}

/// Builds batch buffers, mirroring `Encoder` in protocol.rs. Used by the tests
/// to drive the renderers without the core.
public final class BatchEncoder {
    private var words: [UInt32] = []
    private var strings: [String] = []
    private var stringIndex: [String: UInt32] = [:]
    private var arrays: [[Float]] = []

    public init() {}

    private func header(_ op: Op, _ node: UInt32) {
        if node > Wire.nodeMask {
            words.append(UInt32(op.rawValue | Wire.extendedNode))
            words.append(node)
        } else {
            words.append(UInt32(op.rawValue) | (node << 8))
        }
    }

    private func intern(_ s: String) -> UInt32 {
        if let i = stringIndex[s] { return i }
        let i = UInt32(strings.count)
        strings.append(s)
        stringIndex[s] = i
        return i
    }

    public func push(_ m: Mutation) {
        switch m {
        case let .beginBatch(seq, flags):
            header(.beginBatch, 0); words += [seq, flags]
        case .endBatch:
            header(.endBatch, 0)
        case let .create(node, type):
            header(.create, node); words.append(type.rawValue)
        case let .insert(node, parent, index):
            header(.insert, node); words += [parent, index]
        case let .remove(node, parent, index):
            header(.remove, node); words += [parent, index]
        case let .delete(node):
            header(.delete, node)
        case let .move(node, parent, from, to):
            header(.move, node); words += [parent, from, to]
        case let .setProp(node, prop, value):
            header(.setProp, node)
            words.append(UInt32(prop.rawValue) | (UInt32(value.kind.rawValue) << 16))
            switch value {
            case .null: break
            case let .bool(b): words.append(b ? 1 : 0)
            case let .i32(v): words.append(UInt32(bitPattern: v))
            case let .f32(v): words.append(v.bitPattern)
            case let .color(c): words.append(c)
            case let .str(s), let .json(s): words.append(intern(s))
            case let .f32Array(a): words.append(UInt32(arrays.count)); arrays.append(a)
            }
        case let .setFrame(node, f):
            header(.setFrame, node); words += [f.x.bitPattern, f.y.bitPattern, f.width.bitPattern, f.height.bitPattern]
        case let .setEventMask(node, mask):
            header(.setEventMask, node); words.append(mask)
        case let .command(node, command, payload):
            header(.command, node); words += [command.rawValue, intern(payload)]
        }
    }

    public func finish() -> Data {
        var out: [UInt32] = [Wire.magic, Wire.protocolVersion, UInt32(words.count), UInt32(strings.count), UInt32(arrays.count)]
        out += words
        for s in strings {
            let bytes = Array(s.utf8)
            out.append(UInt32(bytes.count))
            var k = 0
            while k < bytes.count {
                var w: UInt32 = 0
                for b in 0 ..< 4 where k + b < bytes.count { w |= UInt32(bytes[k + b]) << (8 * UInt32(b)) }
                out.append(w)
                k += 4
            }
        }
        for a in arrays {
            out.append(UInt32(a.count))
            out += a.map(\.bitPattern)
        }
        return out.withUnsafeBufferPointer { Data(buffer: $0) }
    }

    public static func encode(_ ms: [Mutation]) -> Data {
        let e = BatchEncoder()
        ms.forEach(e.push)
        return e.finish()
    }
}
