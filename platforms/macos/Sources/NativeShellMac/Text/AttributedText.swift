// `AttributedText` JSON to NSAttributedString. Measurement (JS thread) and
// drawing (main thread) both start from the string built here, with the same
// TextKit 1 configuration, so the size the layout engine was given is the size
// that gets drawn.

import AppKit

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
    /// URL of a link run. Not `.link`: that one recolors text and is reserved for NSTextView.
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
        var firstFont: NSFont?
        var hasLinks = false
        let scale = FontScale.value
        for run in runs {
            let text = run.string("text") ?? ""
            let size = (run.cgFloat("size").flatMap { $0 > 0 ? $0 : nil } ?? defaultSize) * scale
            let font = Fonts.font(size: size, weight: run.int("weight") ?? 400, family: run.string("family"),
                                  italic: run.bool("italic") ?? false)
            if firstFont == nil { firstFont = font }
            var attrs: [NSAttributedString.Key: Any] = [.font: font]
            attrs[.foregroundColor] = run.color("color") ?? NSColor.labelColor
            if run.bool("underline") == true { attrs[.underlineStyle] = NSUnderlineStyle.single.rawValue }
            if run.bool("strike") == true { attrs[.strikethroughStyle] = NSUnderlineStyle.single.rawValue }
            if let ls = run.cgFloat("letter_spacing"), ls != 0 { attrs[.kern] = ls * scale }
            if let url = run.string("url") {
                attrs[.nshLink] = url
                hasLinks = true
            }
            if let lh = run.cgFloat("line_height"), lh > 0 {
                forcedLineHeight = max(forcedLineHeight ?? 0, lh * scale)
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
                guard let f = value as? NSFont else { return }
                out.addAttribute(.baselineOffset, value: (lh - Fonts.lineHeight(f)) / 2, range: range)
            }
        }
        let base = firstFont ?? Fonts.font(size: defaultSize, weight: 400, family: nil, italic: false)
        return ParsedText(string: out, maxLines: maxLines, truncate: truncate, lineHeight: forcedLineHeight,
                          emptyLineHeight: forcedLineHeight ?? Fonts.lineHeight(base), hasLinks: hasLinks)
    }
}

enum Fonts {
    static func weight(_ w: Int) -> NSFont.Weight {
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

    /// The natural line height TextKit gives a line of this font (with font leading).
    static func lineHeight(_ f: NSFont) -> CGFloat { f.ascender - f.descender + f.leading }

    static func font(size: CGFloat, weight w: Int, family: String?, italic: Bool) -> NSFont {
        var font: NSFont
        if let family = family, !family.isEmpty, family != "system", family != "monospace", let custom = NSFont(name: family, size: size) {
            let traits: [NSFontDescriptor.TraitKey: Any] = [.weight: weight(w)]
            let d = custom.fontDescriptor.addingAttributes([.family: custom.familyName ?? family, .traits: traits])
            font = NSFont(descriptor: d, size: size) ?? custom
        } else if family == "monospace" {
            font = NSFont.monospacedSystemFont(ofSize: size, weight: weight(w))
        } else {
            font = NSFont.systemFont(ofSize: size, weight: weight(w))
        }
        if italic {
            let d = font.fontDescriptor.withSymbolicTraits(font.fontDescriptor.symbolicTraits.union(.italic))
            font = NSFont(descriptor: d, size: size) ?? font
        }
        return font
    }
}

/// A factor on every font size. macOS has no system-wide text size setting apps
/// can read, so it is 1 unless the host sets it; the core never sees scaled
/// sizes: measurement and drawing both apply it here.
enum FontScale {
    private static let lock = NSLock()
    private static var current: CGFloat = 1

    static var value: CGFloat {
        lock.lock(); defer { lock.unlock() }
        return current
    }

    /// Clears the text caches when the factor changes.
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
