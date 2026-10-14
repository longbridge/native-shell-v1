@testable import NativeShellMac
import XCTest

final class DecoderTests: XCTestCase {
    private func sample() -> [Mutation] {
        [
            .beginBatch(seq: 7, flags: BatchFlags.initial),
            .create(node: 1, type: .view),
            .setProp(node: 1, prop: .backgroundColor, value: .color(0xFF11_2233)),
            .setProp(node: 1, prop: .cornerRadii, value: .f32Array([4, 4, 0, 0])),
            .setProp(node: 1, prop: .a11yLabel, value: .str("héllo 😀")),
            .setProp(node: 1, prop: .testId, value: .str("héllo 😀")),
            .setProp(node: 1, prop: .opacity, value: .null),
            .setProp(node: 1, prop: .maxLength, value: .i32(-3)),
            .setProp(node: 1, prop: .hidden, value: .bool(true)),
            .setProp(node: 1, prop: .value, value: .f32(0.5)),
            .setProp(node: 1, prop: .attributedText, value: .json("{\"runs\":[]}")),
            .setProp(node: 1, prop: .placeholder, value: .str("")),
            .insert(node: 1, parent: 0, index: 0),
            .create(node: 0x0100_0000, type: .text),
            .insert(node: 0x0100_0000, parent: 1, index: 0),
            .move(node: 2, parent: 1, from: 3, to: 0),
            .remove(node: 2, parent: 1, index: 0),
            .setFrame(node: 1, frame: Frame(x: 0, y: 1.5, width: 320, height: 44.25)),
            .setEventMask(node: 1, mask: EventMask.click | EventMask.longPress),
            .command(node: 0, command: .showToast, payload: "{\"title\":\"hi\"}"),
            .delete(node: 2),
            .endBatch,
        ]
    }

    func testRoundTripsEveryInstruction() throws {
        let m = sample()
        let bytes = BatchEncoder.encode(m)
        XCTAssertEqual(bytes.count % 4, 0)
        XCTAssertEqual(try BatchDecoder.decode(bytes), m)
    }

    func testEveryViewTypePropAndCommandRoundTrips() throws {
        var m: [Mutation] = [.beginBatch(seq: 1, flags: 0)]
        var raw: UInt32 = 1
        while let t = NSHViewType(rawValue: raw) { m.append(.create(node: raw, type: t)); raw += 1 }
        for p in 0 ... UInt16(200) { if let prop = Prop(rawValue: p) { m.append(.setProp(node: 1, prop: prop, value: .i32(Int32(p)))) } }
        raw = 1
        while let c = Command(rawValue: raw) { m.append(.command(node: 0, command: c, payload: "{}")); raw += 1 }
        m.append(.endBatch)
        XCTAssertEqual(try BatchDecoder.decode(BatchEncoder.encode(m)), m)
    }

    func testStringsAreInternedOnce() {
        let bytes = BatchEncoder.encode(sample())
        let words = bytes.withUnsafeBytes { Array($0.bindMemory(to: UInt32.self)) }
        // "héllo 😀" twice, the JSON text, the empty string and the toast payload.
        XCTAssertEqual(words[3], 4)
    }

    func testExtendedNodeIdsCostOneWord() {
        let small = BatchEncoder.encode([.delete(node: 5)])
        let big = BatchEncoder.encode([.delete(node: 0x0100_0000)])
        XCTAssertEqual(big.count, small.count + 4)
    }

    /// The Rust encoder's bytes for a tiny batch, written out by hand: this pins the wire format.
    func testMatchesRustWireLayout() throws {
        var w: [UInt32] = [0x3142_534E, 1, 0, 1, 1]
        let body: [UInt32] = [
            1, 9, 0, // BeginBatch seq 9 flags 0
            3 | (5 << 8), 2, // Create node 5 Text
            8 | (5 << 8), 20 | (6 << 16), 0, // SetProp AttributedText Json #0
            8 | (5 << 8), 2 | (7 << 16), 0, // SetProp CornerRadii F32Array #0
            9 | (5 << 8), Float(1).bitPattern, Float(2).bitPattern, Float(3).bitPattern, Float(4).bitPattern,
            2,
        ]
        w[2] = UInt32(body.count)
        w += body
        w += [2, 0x0000_6968] // "hi", little-endian
        w += [1, Float(8).bitPattern]
        let data = w.withUnsafeBufferPointer { Data(buffer: $0) }
        let m = try BatchDecoder.decode(data)
        XCTAssertEqual(m, [
            .beginBatch(seq: 9, flags: 0),
            .create(node: 5, type: .text),
            .setProp(node: 5, prop: .attributedText, value: .json("hi")),
            .setProp(node: 5, prop: .cornerRadii, value: .f32Array([8])),
            .setFrame(node: 5, frame: Frame(x: 1, y: 2, width: 3, height: 4)),
            .endBatch,
        ])
    }

    func testHostileBuffersAreRejectedWithoutCrashing() {
        let good = BatchEncoder.encode(sample())
        for cut in 0 ..< good.count {
            _ = try? BatchDecoder.decode(good.prefix(cut))
        }
        var bad = good
        bad[20] = 0xEE
        XCTAssertThrowsError(try BatchDecoder.decode(bad))
        XCTAssertThrowsError(try BatchDecoder.decode(Data([1, 2, 3]))) { XCTAssertEqual($0 as? DecodeError, .misaligned) }
        var wrongVersion = good
        wrongVersion[4] = 9
        XCTAssertThrowsError(try BatchDecoder.decode(wrongVersion)) { XCTAssertEqual($0 as? DecodeError, .version(9)) }

        // Pseudo-random garbage behind a valid header.
        var seed: UInt32 = 0x1234_5678
        func next() -> UInt32 {
            seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5
            return seed
        }
        var rejected = 0
        for _ in 0 ..< 5000 {
            var buf = good
            for _ in 0 ..< 4 {
                let r = next()
                let at = 20 + Int(r % UInt32(buf.count - 20))
                buf[at] = UInt8((r >> 8) & 0xFF)
            }
            if (try? BatchDecoder.decode(buf)) == nil { rejected += 1 }
        }
        XCTAssertGreaterThan(rejected, 0)

        // Header claiming absurd counts.
        var huge = good
        huge.replaceSubrange(8 ..< 12, with: [0xFF, 0xFF, 0xFF, 0x7F])
        XCTAssertThrowsError(try BatchDecoder.decode(huge))
        huge = good
        huge.replaceSubrange(12 ..< 20, with: [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF])
        XCTAssertThrowsError(try BatchDecoder.decode(huge))
        // Fully random buffers.
        for len in stride(from: 0, to: 400, by: 4) {
            let data = Data((0 ..< len).map { _ in UInt8(next() & 0xFF) })
            _ = try? BatchDecoder.decode(data)
        }
    }

    func testRejectedBatchLeavesTreeUntouched() {
        let h = Harness()
        defer { h.tearDown() }
        h.apply { $0.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 10, height: 10)) }
        var b = Batch(seq: 2)
        b.create(2, .view, parent: 1)
        var data = b.encoded()
        data.removeLast(4) // drop EndBatch's word and break the header count
        XCTAssertFalse(h.surface.apply(data))
        XCTAssertNil(h.view(2))
        XCTAssertEqual(h.view(1)?.children.count, 0)
    }
}
