// Small helpers for the JSON-valued props and payloads, shared by the
// renderers. Every accessor tolerates missing or mistyped fields: values come
// from scripts and must never crash the host. Colors are parsed to the
// protocol's `0xAARRGGBB`; each renderer turns that into its own color type.

import CoreGraphics
import Foundation

public enum JSON {
    public static func object(_ s: String) -> [String: Any]? {
        guard let data = s.data(using: .utf8) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed])) as? [String: Any]
    }

    public static func array(_ s: String) -> [Any]? {
        guard let data = s.data(using: .utf8) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed])) as? [Any]
    }

    public static func any(_ s: String) -> Any? {
        guard let data = s.data(using: .utf8) else { return nil }
        return try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed])
    }

    /// Serializes any JSON-able value (fragments included). Non-JSON values become `null`.
    public static func string(_ value: Any?) -> String {
        guard let value = value, !(value is NSNull) else { return "null" }
        if JSONSerialization.isValidJSONObject(value) || value is String || value is NSNumber {
            if let data = try? JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed]),
               let s = String(data: data, encoding: .utf8) {
                return s
            }
        }
        return "null"
    }
}

// Read once per prop on the main thread: inlinable, so no cross-module call.
extension Dictionary where Key == String, Value == Any {
    @inlinable public func double(_ k: String) -> Double? { (self[k] as? NSNumber)?.doubleValue }
    @inlinable public func cgFloat(_ k: String) -> CGFloat? { double(k).map { CGFloat($0) } }
    @inlinable public func int(_ k: String) -> Int? { (self[k] as? NSNumber)?.intValue }
    @inlinable public func bool(_ k: String) -> Bool? { (self[k] as? NSNumber)?.boolValue }
    @inlinable public func string(_ k: String) -> String? { self[k] as? String }
    @inlinable public func object(_ k: String) -> [String: Any]? { self[k] as? [String: Any] }
    /// A color field as `0xAARRGGBB`.
    @inlinable public func argb(_ k: String) -> UInt32? { self[k].flatMap(ColorValue.argb) }
}

public enum ColorValue {
    /// A color in JSON: an ARGB integer, or a `#rgb` / `#rgba` / `#rrggbb` / `#rrggbbaa` string.
    public static func argb(_ v: Any) -> UInt32? {
        if let n = v as? NSNumber {
            let d = n.doubleValue
            guard d >= 0, d <= Double(UInt32.max) else { return nil }
            return UInt32(d)
        }
        if let s = v as? String { return hex(s) }
        return nil
    }

    public static func hex(_ s: String) -> UInt32? {
        guard s.hasPrefix("#") else { return nil }
        let h = String(s.dropFirst())
        guard h.allSatisfy(\.isHexDigit), let n = UInt32(h, radix: 16) else { return nil }
        let r, g, b, a: UInt32
        switch h.count {
        case 3: (r, g, b, a) = ((n >> 8 & 0xF) * 17, (n >> 4 & 0xF) * 17, (n & 0xF) * 17, 255)
        case 4: (r, g, b, a) = ((n >> 12 & 0xF) * 17, (n >> 8 & 0xF) * 17, (n >> 4 & 0xF) * 17, (n & 0xF) * 17)
        case 6: (r, g, b, a) = (n >> 16 & 0xFF, n >> 8 & 0xFF, n & 0xFF, 255)
        case 8: (r, g, b, a) = (n >> 24 & 0xFF, n >> 16 & 0xFF, n >> 8 & 0xFF, n & 0xFF)
        default: return nil
        }
        return a << 24 | r << 16 | g << 8 | b
    }
}

extension PropValue {
    @inlinable public var bool: Bool? { if case let .bool(b) = self { return b }; return nil }

    @inlinable public var string: String? {
        switch self {
        case let .str(s), let .json(s): return s
        default: return nil
        }
    }

    @inlinable public var floats: [Float]? { if case let .f32Array(a) = self { return a }; return nil }
    @inlinable public var argb: UInt32? { if case let .color(c) = self { return c }; return nil }

    @inlinable public var number: Double? {
        switch self {
        case let .f32(f): return Double(f)
        case let .i32(i): return Double(i)
        case let .bool(b): return b ? 1 : 0
        default: return nil
        }
    }

    @inlinable public var int: Int? { number.map { Int($0) } }
    @inlinable public var jsonObject: [String: Any]? { if case let .json(s) = self { return JSON.object(s) }; return nil }
    @inlinable public var jsonArray: [Any]? { if case let .json(s) = self { return JSON.array(s) }; return nil }
}
