// Small helpers for the JSON-valued props. Every accessor tolerates missing or
// mistyped fields: values come from scripts and must never crash the host.

import AppKit

enum JSON {
    static func object(_ s: String) -> [String: Any]? {
        guard let data = s.data(using: .utf8) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed])) as? [String: Any]
    }

    static func array(_ s: String) -> [Any]? {
        guard let data = s.data(using: .utf8) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed])) as? [Any]
    }

    static func any(_ s: String) -> Any? {
        guard let data = s.data(using: .utf8) else { return nil }
        return try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed])
    }

    /// Serializes any JSON-able value (fragments included). Non-JSON values become `null`.
    static func string(_ value: Any?) -> String {
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

extension Dictionary where Key == String, Value == Any {
    func double(_ k: String) -> Double? { (self[k] as? NSNumber)?.doubleValue }
    func cgFloat(_ k: String) -> CGFloat? { double(k).map { CGFloat($0) } }
    func int(_ k: String) -> Int? { (self[k] as? NSNumber)?.intValue }
    func bool(_ k: String) -> Bool? { (self[k] as? NSNumber)?.boolValue }
    func string(_ k: String) -> String? { self[k] as? String }
    func object(_ k: String) -> [String: Any]? { self[k] as? [String: Any] }
    func color(_ k: String) -> NSColor? { self[k].flatMap(Colors.parse) }
}

enum Colors {
    /// `0xAARRGGBB`, the protocol's color encoding.
    static func argb(_ c: UInt32) -> NSColor {
        NSColor(srgbRed: CGFloat((c >> 16) & 0xFF) / 255, green: CGFloat((c >> 8) & 0xFF) / 255,
                blue: CGFloat(c & 0xFF) / 255, alpha: CGFloat(c >> 24) / 255)
    }

    /// A color in JSON: an ARGB integer, or a `#rgb` / `#rrggbb` / `#rrggbbaa` string.
    static func parse(_ v: Any) -> NSColor? {
        if let n = v as? NSNumber {
            let d = n.doubleValue
            guard d >= 0, d <= Double(UInt32.max) else { return nil }
            return argb(UInt32(d))
        }
        if let s = v as? String { return hex(s) }
        return nil
    }

    static func hex(_ s: String) -> NSColor? {
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
        return argb(a << 24 | r << 16 | g << 8 | b)
    }

    static func toHex(_ color: NSColor) -> String {
        var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        // Catalog and dynamic colors have no components until converted (getRed would raise).
        (color.usingColorSpace(.sRGB) ?? .black).getRed(&r, green: &g, blue: &b, alpha: &a)
        let c = { (v: CGFloat) in Int((max(0, min(1, v)) * 255).rounded()) }
        if c(a) == 255 { return String(format: "#%02x%02x%02x", c(r), c(g), c(b)) }
        return String(format: "#%02x%02x%02x%02x", c(r), c(g), c(b), c(a))
    }
}

extension PropValue {
    var bool: Bool? { if case let .bool(b) = self { return b }; return nil }
    var string: String? {
        switch self {
        case let .str(s), let .json(s): return s
        default: return nil
        }
    }

    var floats: [Float]? { if case let .f32Array(a) = self { return a }; return nil }
    var nsColor: NSColor? { if case let .color(c) = self { return Colors.argb(c) }; return nil }

    var number: Double? {
        switch self {
        case let .f32(f): return Double(f)
        case let .i32(i): return Double(i)
        case let .bool(b): return b ? 1 : 0
        default: return nil
        }
    }

    var int: Int? { number.map { Int($0) } }
    var jsonObject: [String: Any]? { if case let .json(s) = self { return JSON.object(s) }; return nil }
    var jsonArray: [Any]? { if case let .json(s) = self { return JSON.array(s) }; return nil }
}
