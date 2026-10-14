// One TextKit 1 stack configured exactly like every other: no line fragment
// padding, no insets, the container's line limit and truncation taken from the
// parsed text. Measurement and drawing both go through it.

import UIKit

final class TextLayout {
    let storage = NSTextStorage()
    let layoutManager = NSLayoutManager()
    let container = NSTextContainer(size: .zero)

    /// Width used for unconstrained layout. Finite, so centered lines keep float precision.
    static let unconstrained: CGFloat = 100_000

    init() {
        container.lineFragmentPadding = 0
        layoutManager.usesFontLeading = true
        layoutManager.addTextContainer(container)
        storage.addLayoutManager(layoutManager)
    }

    private weak var current: ParsedText?
    private var currentWidth: CGFloat = -1

    /// Lays `text` out in a container `width` wide.
    func layout(_ text: ParsedText, width: CGFloat) {
        if current === text, currentWidth == width { return }
        current = text
        currentWidth = width
        container.size = CGSize(width: width, height: .greatestFiniteMagnitude)
        container.maximumNumberOfLines = text.maxLines
        container.lineBreakMode = text.lineBreakMode
        storage.setAttributedString(text.string)
        layoutManager.ensureLayout(for: container)
    }

    /// Size of the laid out text, unrounded.
    func usedSize(_ text: ParsedText) -> CGSize {
        if text.string.length == 0 { return CGSize(width: 0, height: text.emptyLineHeight) }
        let used = layoutManager.usedRect(for: container)
        return CGSize(width: used.width, height: used.maxY)
    }

    /// Distance from the top to the first line's baseline.
    func firstBaseline(_ text: ParsedText) -> CGFloat {
        guard layoutManager.numberOfGlyphs > 0 else {
            let font = UIFont.systemFont(ofSize: AttributedText.defaultSize)
            return (text.emptyLineHeight + font.ascender + font.descender) / 2 - font.descender
        }
        let rect = layoutManager.lineFragmentRect(forGlyphAt: 0, effectiveRange: nil)
        let loc = layoutManager.location(forGlyphAt: 0)
        let offset = (storage.attribute(.baselineOffset, at: 0, effectiveRange: nil) as? CGFloat) ?? 0
        return rect.minY + loc.y - offset
    }

    func draw(at origin: CGPoint) {
        let range = layoutManager.glyphRange(for: container)
        layoutManager.drawBackground(forGlyphRange: range, at: origin)
        layoutManager.drawGlyphs(forGlyphRange: range, at: origin)
    }

    /// Character index under `point`, if the point is on a glyph.
    func characterIndex(at point: CGPoint) -> Int? {
        guard layoutManager.numberOfGlyphs > 0 else { return nil }
        var fraction: CGFloat = 0
        let glyph = layoutManager.glyphIndex(for: point, in: container, fractionOfDistanceThroughGlyph: &fraction)
        let rect = layoutManager.boundingRect(forGlyphRange: NSRange(location: glyph, length: 1), in: container)
        guard rect.insetBy(dx: -4, dy: -4).contains(point) else { return nil }
        return layoutManager.characterIndexForGlyph(at: glyph)
    }
}

/// Answers `measure_text` on any thread. Each thread gets its own TextKit
/// stack (TextKit 1 is safe off the main thread when objects are not shared).
final class TextMeasurer {
    static let shared = TextMeasurer()

    /// Device pixel scale, read on the main thread once; the JS thread must not touch UIScreen.
    static var scale: CGFloat = 3
    private let cache = NSCache<NSString, NSValue>()
    private let threadKey = "dev.nativeshell.textlayout"

    init() { cache.countLimit = 5000 }

    struct Metrics: Equatable {
        var width: CGFloat
        var height: CGFloat
        var baseline: CGFloat
    }

    /// width_mode: 0 max-content (unwrapped), 1 at most `width`, 2 exactly `width`.
    func measure(_ json: String, width: CGFloat, mode: Int) -> Metrics {
        let key = "\(mode)|\(width)|\(FontScale.value)|\(json)" as NSString
        if let hit = cache.object(forKey: key) {
            let v = hit.cgRectValue
            return Metrics(width: v.origin.x, height: v.origin.y, baseline: v.size.width)
        }
        let text = AttributedText.parse(json)
        let layout = threadLayout()
        let constrained = mode != 0 && width.isFinite && width >= 0
        layout.layout(text, width: constrained ? width : TextLayout.unconstrained)
        let used = layout.usedSize(text)
        // Widths are exact (the core snaps them to pixels and re-measures at the
        // snapped width); heights are rounded up to device pixels.
        let s = TextMeasurer.scale
        let ceilPx = { (v: CGFloat) in (v * s - 0.001).rounded(.up) / s }
        var w = used.width
        if mode == 2 { w = width } else if constrained { w = min(w, width) }
        let m = Metrics(width: w, height: ceilPx(used.height), baseline: layout.firstBaseline(text))
        cache.setObject(NSValue(cgRect: CGRect(x: m.width, y: m.height, width: m.baseline, height: 0)), forKey: key)
        return m
    }

    func clear() { cache.removeAllObjects() }

    private func threadLayout() -> TextLayout {
        let dict = Thread.current.threadDictionary
        if let l = dict[threadKey] as? TextLayout { return l }
        let l = TextLayout()
        dict[threadKey] = l
        return l
    }
}
