// `AttributedText` JSON to NSAttributedString. Measurement (JS thread) and
// drawing (main thread) both start from the string built here, with the same
// TextKit 1 configuration, so the size the layout engine was given is the size
// that gets drawn.

import NativeShellShared
import UIKit

/// The parsed form of one `AttributedText` value. Immutable, so one instance is
/// shared by the measurer on the JS thread and the views on the main thread.
final class ParsedText {
    let string: NSAttributedString
    /// 0: unlimited.
    let maxLines: Int
    let truncate: Bool
    /// The line height every line is forced to, or nil for natural line heights.
    let lineHeight: CGFloat?
    /// Line height of an empty string.
    let emptyLineHeight: CGFloat
    let hasLinks: Bool

    init(string: NSAttributedString, maxLines: Int, truncate: Bool, lineHeight: CGFloat?, emptyLineHeight: CGFloat, hasLinks: Bool) {
        self.string = string
        self.maxLines = maxLines
        self.truncate = truncate
        self.lineHeight = lineHeight
        self.emptyLineHeight = emptyLineHeight
        self.hasLinks = hasLinks
    }

    var lineBreakMode: NSLineBreakMode { truncate ? .byTruncatingTail : .byWordWrapping }
}

extension NSAttributedString.Key {
    /// URL of a link run. Not `.link`: that one recolors text and is reserved for UITextView.
    static let nshLink = NSAttributedString.Key("NSHLink")
}

enum AttributedText {
    static let defaultSize: CGFloat = 16
    private static let cache: NSCache<NSString, ParsedText> = {
        let c = NSCache<NSString, ParsedText>()
        // Large enough that text measured on the JS thread is still parsed when it is drawn.
        c.countLimit = 20_000
        return c
    }()

    static func clearCache() { cache.removeAllObjects() }

    static func parse(_ json: String) -> ParsedText {
        if let hit = cache.object(forKey: json as NSString) { return hit }
        let parsed = build(JSON.object(json) ?? [:])
        cache.setObject(parsed, forKey: json as NSString)
        return parsed
    }

    static func build(_ obj: [String: Any]) -> ParsedText {
        let runs = obj["runs"] as? [[String: Any]] ?? []
        let out = NSMutableAttributedString()
        var forcedLineHeight: CGFloat?
        var firstFont: UIFont?
        var hasLinks = false
        let scale = FontScale.value
        for run in runs {
            let text = run.string("text") ?? ""
            let size = (run.cgFloat("size").flatMap { $0 > 0 ? $0 : nil } ?? defaultSize) * scale
            let font = Fonts.font(size: size, weight: run.int("weight") ?? 400, family: run.string("family"),
                                  italic: run.bool("italic") ?? false)
            if firstFont == nil { firstFont = font }
            var attrs: [NSAttributedString.Key: Any] = [.font: font]
            attrs[.foregroundColor] = run.color("color") ?? UIColor.label
            if run.bool("underline") == true { attrs[.underlineStyle] = NSUnderlineStyle.single.rawValue }
            if run.bool("strike") == true { attrs[.strikethroughStyle] = NSUnderlineStyle.single.rawValue }
            if let ls = run.cgFloat("letter_spacing"), ls != 0 { attrs[.kern] = ls * scale }
            if let url = run.string("url") {
                attrs[.nshLink] = url
                hasLinks = true
            }
            if let lh = lineHeight(run, size: size, scale: scale) {
                forcedLineHeight = max(forcedLineHeight ?? 0, lh)
            }
            out.append(NSAttributedString(string: text, attributes: attrs))
        }
        let paragraph = NSMutableParagraphStyle()
        switch obj.string("align") {
        case "center": paragraph.alignment = .center
        case "right": paragraph.alignment = .right
        case "justify": paragraph.alignment = .justified
        default: paragraph.alignment = .natural
        }
        let truncate = obj.bool("truncate") ?? false
        let maxLines = max(0, obj.int("lines") ?? 0)
        paragraph.lineBreakMode = .byWordWrapping
        if let lh = forcedLineHeight {
            paragraph.minimumLineHeight = lh
            paragraph.maximumLineHeight = lh
        }
        let full = NSRange(location: 0, length: out.length)
        out.addAttribute(.paragraphStyle, value: paragraph, range: full)
        if let lh = forcedLineHeight {
            // Center glyphs in the forced line box (TextKit pins them to the bottom).
            out.enumerateAttribute(.font, in: full) { value, range, _ in
                guard let f = value as? UIFont else { return }
                out.addAttribute(.baselineOffset, value: (lh - f.lineHeight) / 2, range: range)
            }
        }
        let base = firstFont ?? Fonts.font(size: defaultSize, weight: 400, family: nil, italic: false)
        return ParsedText(string: out, maxLines: maxLines, truncate: truncate, lineHeight: forcedLineHeight,
                          emptyLineHeight: forcedLineHeight ?? base.lineHeight, hasLinks: hasLinks)
    }

    /// `line_height` in points (the core resolves multiples), scaled like the font size.
    private static func lineHeight(_ run: [String: Any], size: CGFloat, scale: CGFloat) -> CGFloat? {
        guard let v = run.cgFloat("line_height"), v > 0 else { return nil }
        return v * scale
    }
}

enum Fonts {
    static func weight(_ w: Int) -> UIFont.Weight {
        switch w {
        case ..<150: return .ultraLight
        case ..<250: return .thin
        case ..<350: return .light
        case ..<450: return .regular
        case ..<550: return .medium
        case ..<650: return .semibold
        case ..<750: return .bold
        case ..<850: return .heavy
        default: return .black
        }
    }

    static func font(size: CGFloat, weight w: Int, family: String?, italic: Bool) -> UIFont {
        var font: UIFont
        if let family = family, !family.isEmpty, family != "system", let custom = UIFont(name: family, size: size) {
            font = custom
            let traits: [UIFontDescriptor.TraitKey: Any] = [.weight: weight(w)]
            let d = custom.fontDescriptor.addingAttributes([.family: custom.familyName, .traits: traits])
            font = UIFont(descriptor: d, size: size)
        } else if family == "monospace" {
            font = UIFont.monospacedSystemFont(ofSize: size, weight: weight(w))
        } else {
            font = UIFont.systemFont(ofSize: size, weight: weight(w))
        }
        if italic, let d = font.fontDescriptor.withSymbolicTraits(font.fontDescriptor.symbolicTraits.union(.traitItalic)) {
            font = UIFont(descriptor: d, size: size)
        }
        return font
    }
}

/// The user's text size (Dynamic Type) as a factor on every font size. The core
/// never sees scaled sizes: measurement and drawing both apply it here.
enum FontScale {
    private static let lock = NSLock()
    private static var current: CGFloat = 1

    static var value: CGFloat {
        lock.lock(); defer { lock.unlock() }
        return current
    }

    static func factor(for traits: UITraitCollection) -> CGFloat {
        UIFontMetrics(forTextStyle: .body).scaledValue(for: 17, compatibleWith: traits) / 17
    }

    /// Main thread. Clears the text caches when the factor changes.
    @discardableResult
    static func update(_ traits: UITraitCollection) -> Bool {
        set(factor(for: traits))
    }

    @discardableResult
    static func set(_ f: CGFloat) -> Bool {
        lock.lock()
        let changed = abs(current - f) > 0.0001
        current = f
        lock.unlock()
        if changed {
            AttributedText.clearCache()
            TextMeasurer.shared.clear()
        }
        return changed
    }
}
