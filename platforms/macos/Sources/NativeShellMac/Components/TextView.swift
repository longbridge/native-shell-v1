// `Text`: draws attributed text with the same TextKit 1 configuration the
// measurer used, so the drawn size matches the measured one. Selectable text
// switches to a TextKit 1 `NSTextView` configured identically.

import AppKit
import NativeShellShared

final class NSHTextView: NSHComponentView {
    private let canvas = TextCanvas()
    private var selectableView: NSTextView?
    private(set) var json: String?
    private var linksEnabled = true
    /// The link under the pointer when the button went down.
    private var pressedLink: String?

    override func commonInit() {
        canvas.frame = bounds
        addSubview(canvas)
        setAccessibilityElement(true)
        setAccessibilityRole(.staticText)
    }

    var parsed: ParsedText? { canvas.text }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .attributedText:
            json = value.string
            let text = json.map(AttributedText.parse)
            canvas.text = text
            if let tv = selectableView, let t = text { configure(tv, t) }
            if accessibilityLabel() == nil || accessibilityLabel() == lastAutoLabel {
                lastAutoLabel = text?.string.string
                setAccessibilityLabel(lastAutoLabel)
            }
            window?.invalidateCursorRects(for: self)
        case .selectable:
            setSelectable(value.bool ?? false)
        case .linksEnabled:
            linksEnabled = value.bool ?? true
            window?.invalidateCursorRects(for: self)
        default:
            return super.applyProp(prop, value)
        }
        return true
    }

    private var lastAutoLabel: String?

    override func layout() {
        super.layout()
        canvas.frame = bounds
        selectableView?.frame = bounds
    }

    override func updateAccessibilityElement() {
        // Text is always readable, whatever its role.
        setAccessibilityElement(true)
    }

    private func setSelectable(_ on: Bool) {
        if on, selectableView == nil {
            let tv = NSTextView(usingTextLayoutManager: false)
            tv.isEditable = false
            tv.isSelectable = true
            tv.drawsBackground = false
            tv.textContainerInset = .zero
            tv.textContainer?.lineFragmentPadding = 0
            tv.layoutManager?.usesFontLeading = true
            tv.isVerticallyResizable = false
            tv.isHorizontallyResizable = false
            tv.frame = bounds
            if let t = canvas.text { configure(tv, t) }
            addSubview(tv)
            selectableView = tv
            canvas.isHidden = true
        } else if !on, let tv = selectableView {
            tv.removeFromSuperview()
            selectableView = nil
            canvas.isHidden = false
        }
    }

    override func themeDidChange() {
        selectableView?.selectedTextAttributes = [.backgroundColor: themeColor("selection")]
    }

    private func configure(_ tv: NSTextView, _ t: ParsedText) {
        tv.selectedTextAttributes = [.backgroundColor: themeColor("selection")]
        tv.textStorage?.setAttributedString(t.string)
        tv.textContainer?.maximumNumberOfLines = t.maxLines
        tv.textContainer?.lineBreakMode = t.lineBreakMode
    }

    // MARK: Links

    private func link(at p: CGPoint) -> String? {
        guard linksEnabled, let text = canvas.text, text.hasLinks, let i = canvas.characterIndex(at: p), i < text.string.length else { return nil }
        return text.string.attribute(.nshLink, at: i, effectiveRange: nil) as? String
    }

    override func mouseDown(with e: NSEvent) {
        pressedLink = link(at: convert(e.locationInWindow, from: nil))
        if pressedLink == nil { super.mouseDown(with: e) }
    }

    override func mouseUp(with e: NSEvent) {
        guard let url = pressedLink else { return super.mouseUp(with: e) }
        pressedLink = nil
        guard link(at: convert(e.locationInWindow, from: nil)) == url else { return }
        openLink(url)
    }

    func openLink(_ url: String) {
        if listens(EventMask.link) {
            send(.link, ["url": url])
        } else if let u = URL(string: url) {
            context?.runtime.openURL(u)
        }
    }

    override func resetCursorRects() {
        super.resetCursorRects()
        guard linksEnabled, let text = canvas.text, text.hasLinks else { return }
        for rect in canvas.linkRects() { addCursorRect(rect, cursor: .pointingHand) }
    }

    /// The size the text draws at for the current bounds width (for tests).
    var drawnSize: CGSize { canvas.drawnSize }
}

/// Draws a `ParsedText` with its own TextKit stack. The layout is redone only
/// when the text object or the width changes.
final class TextCanvas: NSView {
    private let layout = TextLayout()

    override var isFlipped: Bool { true }
    override var isOpaque: Bool { false }

    /// Text is laid out at exactly the frame width the core measured it at.
    private var layoutWidth: CGFloat { bounds.width }

    var text: ParsedText? {
        didSet { if text !== oldValue { needsDisplay = true } }
    }

    override func setFrameSize(_ newSize: NSSize) {
        let changed = newSize != frame.size
        super.setFrameSize(newSize)
        if changed { needsDisplay = true }
    }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func draw(_ rect: CGRect) {
        guard let text = text, text.string.length > 0 else { return }
        layout.layout(text, width: layoutWidth)
        layout.draw(at: .zero)
    }

    var drawnSize: CGSize {
        guard let text = text else { return .zero }
        layout.layout(text, width: layoutWidth)
        return layout.usedSize(text)
    }

    func characterIndex(at p: CGPoint) -> Int? {
        guard let text = text else { return nil }
        layout.layout(text, width: layoutWidth)
        return layout.characterIndex(at: p)
    }

    /// Bounding rects of the link runs.
    func linkRects() -> [CGRect] {
        guard let text = text else { return [] }
        layout.layout(text, width: layoutWidth)
        var out: [CGRect] = []
        text.string.enumerateAttribute(.nshLink, in: NSRange(location: 0, length: text.string.length)) { value, range, _ in
            guard value != nil else { return }
            let glyphs = layout.layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            layout.layoutManager.enumerateEnclosingRects(forGlyphRange: glyphs, withinSelectedGlyphRange: NSRange(location: NSNotFound, length: 0),
                                                         in: layout.container) { rect, _ in out.append(rect) }
        }
        return out
    }
}
