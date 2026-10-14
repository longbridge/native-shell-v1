// `Text`: draws attributed text with the same TextKit 1 configuration the
// measurer used, so the drawn size matches the measured one. Selectable text
// switches to a TextKit 1 `UITextView` configured identically.

import NativeShellShared
import UIKit

final class NSHTextView: NSHComponentView {
    private let canvas = TextCanvas()
    private var selectableView: UITextView?
    private(set) var json: String?
    private var linksEnabled = true

    override func commonInit() {
        canvas.isOpaque = false
        canvas.backgroundColor = .clear
        canvas.contentMode = .redraw
        canvas.isUserInteractionEnabled = false
        addSubview(canvas)
        isAccessibilityElement = true
        accessibilityTraits = .staticText
    }

    var parsed: ParsedText? { canvas.text }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .attributedText:
            json = value.string
            let text = json.map(AttributedText.parse)
            canvas.text = text
            selectableView?.attributedText = text?.string
            if let t = text { configure(selectableView, t) }
            if accessibilityLabel == nil || accessibilityLabel == lastAutoLabel {
                lastAutoLabel = text?.string.string
                accessibilityLabel = lastAutoLabel
            }
            updateLinkTap()
        case .selectable:
            setSelectable(value.bool ?? false)
        case .linksEnabled:
            linksEnabled = value.bool ?? true
            updateLinkTap()
        default:
            return super.applyProp(prop, value)
        }
        return true
    }

    private var lastAutoLabel: String?

    override func layoutSubviews() {
        super.layoutSubviews()
        canvas.frame = bounds
        selectableView?.frame = bounds
    }

    private func setSelectable(_ on: Bool) {
        if on, selectableView == nil {
            let tv: UITextView
            if #available(iOS 16.0, *) { tv = UITextView(usingTextLayoutManager: false) } else { tv = UITextView() }
            tv.isEditable = false
            tv.isSelectable = true
            tv.isScrollEnabled = false
            tv.backgroundColor = .clear
            tv.textContainerInset = .zero
            tv.textContainer.lineFragmentPadding = 0
            tv.layoutManager.usesFontLeading = true
            tv.frame = bounds
            if let t = canvas.text {
                tv.attributedText = t.string
                configure(tv, t)
            }
            addSubview(tv)
            selectableView = tv
            canvas.isHidden = true
        } else if !on, let tv = selectableView {
            tv.removeFromSuperview()
            selectableView = nil
            canvas.isHidden = false
        }
    }

    private func configure(_ tv: UITextView?, _ t: ParsedText) {
        tv?.textContainer.maximumNumberOfLines = t.maxLines
        tv?.textContainer.lineBreakMode = t.lineBreakMode
    }

    // MARK: Links

    private var linkTap: UITapGestureRecognizer?

    private func updateLinkTap() {
        let want = linksEnabled && (canvas.text?.hasLinks ?? false)
        if want, linkTap == nil {
            let g = UITapGestureRecognizer(target: self, action: #selector(onLinkTap(_:)))
            g.cancelsTouchesInView = false
            addGestureRecognizer(g)
            linkTap = g
        } else if !want, let g = linkTap {
            removeGestureRecognizer(g)
            linkTap = nil
        }
    }

    @objc private func onLinkTap(_ g: UITapGestureRecognizer) {
        guard let text = canvas.text, let i = canvas.characterIndex(at: g.location(in: canvas)),
              i < text.string.length, let url = text.string.attribute(.nshLink, at: i, effectiveRange: nil) as? String
        else { return }
        if listens(EventMask.link) {
            send(.link, ["url": url])
        } else if let u = URL(string: url) {
            context?.runtime.openURL(u)
        }
    }

    /// The size the text draws at for the current bounds width (for tests).
    var drawnSize: CGSize { canvas.drawnSize }
}

/// Draws a `ParsedText` with its own TextKit stack. The layout is redone only
/// when the text object or the width changes.
final class TextCanvas: UIView {
    private let layout = TextLayout()

    /// Text is laid out at exactly the frame width the core measured it at.
    private var layoutWidth: CGFloat { bounds.width }

    var text: ParsedText? {
        didSet { if text !== oldValue { setNeedsDisplay() } }
    }

    override var frame: CGRect {
        didSet { if frame.size != oldValue.size { setNeedsDisplay() } }
    }

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
}
