// Colors in the renderer's own type. JSON parsing, the tolerant accessors and
// ARGB parsing live in NativeShellShared; this adds NSColor on top.

import AppKit
import NativeShellShared

extension Dictionary where Key == String, Value == Any {
    func color(_ k: String) -> NSColor? { argb(k).map(Colors.argb) }
}

enum Colors {
    /// `0xAARRGGBB`, the protocol's color encoding.
    static func argb(_ c: UInt32) -> NSColor {
        NSColor(srgbRed: CGFloat((c >> 16) & 0xFF) / 255, green: CGFloat((c >> 8) & 0xFF) / 255,
                blue: CGFloat(c & 0xFF) / 255, alpha: CGFloat(c >> 24) / 255)
    }

    /// A color in JSON: an ARGB integer, or a `#rgb` / `#rrggbb` / `#rrggbbaa` string.
    static func parse(_ v: Any) -> NSColor? { ColorValue.argb(v).map(argb) }

    static func hex(_ s: String) -> NSColor? { ColorValue.hex(s).map(argb) }

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
    var nsColor: NSColor? { argb.map(Colors.argb) }
}
