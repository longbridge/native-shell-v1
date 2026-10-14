// `TextInput`: a controlled NSTextField (NSSecureTextField when `Secure`, a
// scrolling NSTextView when `Multiline`). See `ControlledText` for the value
// protocol. The program draws the field's chrome; the renderer adds the
// system focus ring around it while it is being edited.

import AppKit
import NativeShellShared

final class NSHTextInputView: NSHComponentView, NSTextFieldDelegate, NSTextViewDelegate {
    private(set) var field: NSTextField?
    private(set) var scroll: NSScrollView?
    private(set) var textView: NSTextView?
    private let placeholderLabel = NSTextField(labelWithString: "")
    private(set) var state = ControlledText()
    private var filter = InputFilter()
    /// Set while the renderer writes text, so delegate callbacks are not reported as edits.
    private var writing = false
    private var wasComposing = false
    private var multiline = false
    private var secure = false
    private var editable = true
    private var style: [String: Any] = [:]
    private var placeholder = ""
    private var placeholderColor: NSColor?
    private var returnKey = 0
    private var insets = NSEdgeInsets()
    private var props: [Prop: PropValue] = [:]
    /// What the field showed when last reported or written: edits are reported once,
    /// whichever AppKit notification announces them first.
    private var lastSeen: (text: String, composing: Bool) = ("", false)
    override var handlesMouse: Bool { false }
    override var accessibilityTarget: NSView { field ?? textView ?? self }

    override func commonInit() {
        placeholderLabel.isEditable = false
        placeholderLabel.isSelectable = false
        placeholderLabel.drawsBackground = false
        placeholderLabel.isBordered = false
        placeholderLabel.maximumNumberOfLines = 0
        // Marked (IME) text changes the editor's storage without a text-did-change
        // notification; storage edits catch them, after the editor has settled.
        NotificationCenter.default.addObserver(self, selector: #selector(storageEdited(_:)), name: NSTextStorage.didProcessEditingNotification, object: nil)
        build()
    }

    @objc private func storageEdited(_ n: Notification) {
        // Measurement edits its own storages on the JS thread: only the main thread's matter.
        guard Thread.isMainThread, let storage = n.object as? NSTextStorage, storage === editor?.textStorage, isEditing || textView != nil else { return }
        DispatchQueue.main.async { [weak self] in self?.edited() }
    }

    /// The editor holding the text: the field editor while a field is edited, else the text view.
    private var editor: NSTextView? {
        if let f = field { return f.currentEditor() as? NSTextView }
        return textView
    }

    var text: String { editor?.string ?? field?.stringValue ?? "" }

    var isComposing: Bool { editor?.hasMarkedText() ?? false }

    var isEditing: Bool {
        guard let w = window else { return false }
        if let f = field { return f.currentEditor() != nil && w.firstResponder === f.currentEditor() }
        return w.firstResponder === textView
    }

    private func build() {
        let old = text
        let wasEditing = isEditing
        field?.removeFromSuperview()
        scroll?.removeFromSuperview()
        placeholderLabel.removeFromSuperview()
        field = nil
        scroll = nil
        textView = nil
        if multiline {
            let sv = NSScrollView()
            sv.drawsBackground = false
            sv.hasVerticalScroller = true
            sv.autohidesScrollers = true
            sv.borderType = .noBorder
            let tv = InputTextView(usingTextLayoutManager: false)
            tv.owner = self
            tv.delegate = self
            tv.drawsBackground = false
            tv.isRichText = false
            tv.allowsUndo = true
            tv.textContainer?.lineFragmentPadding = 0
            tv.isVerticallyResizable = true
            tv.autoresizingMask = [.width]
            tv.textContainer?.widthTracksTextView = true
            tv.string = old
            sv.documentView = tv
            addSubview(sv)
            tv.addSubview(placeholderLabel)
            scroll = sv
            textView = tv
        } else {
            let f: NSTextField = secure ? SecureInputField() : InputField()
            (f as? InputFieldOwned)?.owner = self
            f.delegate = self
            f.isBordered = false
            f.isBezeled = false
            f.drawsBackground = false
            f.focusRingType = .default
            f.usesSingleLineMode = true
            f.cell?.isScrollable = true
            f.cell?.wraps = false
            f.lineBreakMode = .byClipping
            f.stringValue = old
            addSubview(f)
            field = f
        }
        for (p, v) in props { applyInputProp(p, v) }
        accessibilityTarget.setAccessibilityIdentifier(accessibilityIdentifier())
        applyStyle()
        updatePlaceholder()
        needsLayout = true
        if wasEditing { focus() }
    }

    override func layout() {
        super.layout()
        let inner = CGRect(x: insets.left, y: insets.top, width: max(0, bounds.width - insets.left - insets.right),
                           height: max(0, bounds.height - insets.top - insets.bottom))
        if let f = field {
            // AppKit draws a field's text at its top: center the line in the inset area, as UIKit
            // does. The line keeps its full height even when the insets leave less room.
            let h = max(f.intrinsicContentSize.height, ceil(Fonts.lineHeight(font)))
            f.frame = CGRect(x: inner.minX, y: (inner.midY - h / 2).rounded(), width: inner.width, height: h)
        }
        if let sv = scroll, let tv = textView {
            sv.frame = bounds
            tv.textContainerInset = NSSize(width: insets.left, height: insets.top)
            tv.textContainer?.containerSize = NSSize(width: max(0, bounds.width - insets.left - insets.right), height: .greatestFiniteMagnitude)
            tv.minSize = NSSize(width: 0, height: bounds.height)
            tv.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: .greatestFiniteMagnitude)
            tv.frame.size.width = sv.contentSize.width
            placeholderLabel.frame = CGRect(x: insets.left, y: insets.top, width: inner.width,
                                            height: placeholderLabel.sizeThatFits(CGSize(width: inner.width, height: bounds.height)).height)
        }
    }

    // MARK: Props

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .textValue:
            let o = value.jsonObject ?? [:]
            let count = UInt32(clamping: max(0, o.int("count") ?? 0))
            receive(text: value == .null ? "" : (o.string("text") ?? ""), count: value == .null ? UInt32.max : count)
            return true
        case .multiline:
            let m = value.bool ?? false
            if m != multiline {
                multiline = m
                build()
            }
            return true
        case .secure:
            let s = value.bool ?? false
            if s != secure {
                secure = s
                if !multiline { build() }
            }
            return true
        case .placeholder, .keyboardType, .editable, .returnKey, .contentType, .autoCapitalize, .placeholderColor,
             .textStyle, .maxLength, .allowedChars, .contentInsets:
            if case .null = value { props[prop] = nil } else { props[prop] = value }
            applyInputProp(prop, value)
            return true
        default:
            return super.applyProp(prop, value)
        }
    }

    private func applyInputProp(_ prop: Prop, _ value: PropValue) {
        switch prop {
        case .placeholder:
            placeholder = value.string ?? ""
            updatePlaceholder()
        case .placeholderColor:
            placeholderColor = value.nsColor
            updatePlaceholder()
        case .editable:
            editable = value.bool ?? true
            field?.isEditable = editable
            field?.isSelectable = true
            textView?.isEditable = editable
        case .returnKey:
            returnKey = value.int ?? 0
        case .contentType:
            // [none, one-time code, username, password, email]: what AutoFill offers.
            let types: [NSTextContentType?] = [nil, .oneTimeCode, .username, .password, nil]
            let i = value.int ?? 0
            field?.contentType = i >= 0 && i < types.count ? types[i] : nil
        case .textStyle:
            style = value.jsonObject ?? [:]
            applyStyle()
        case .contentInsets:
            let a = value.floats ?? []
            insets = a.count == 4 ? NSEdgeInsets(top: CGFloat(a[0]), left: CGFloat(a[3]), bottom: CGFloat(a[2]), right: CGFloat(a[1])) : NSEdgeInsets()
            needsLayout = true
        case .maxLength:
            filter.maxLength = max(0, value.int ?? 0)
        case .allowedChars:
            let s = value.string ?? ""
            filter.allowed = s.isEmpty ? nil : Set(s)
        default:
            // Keyboard type and auto-capitalization have no equivalent with a hardware keyboard.
            break
        }
    }

    private var font: NSFont {
        Fonts.font(size: (style.cgFloat("size") ?? 16) * FontScale.value, weight: style.int("weight") ?? 400, family: style.string("family"), italic: style.bool("italic") ?? false)
    }

    private func applyStyle() {
        let f = font
        let color = style.color("color") ?? .labelColor
        var align = NSTextAlignment.natural
        switch style.string("align") {
        case "center": align = .center
        case "right": align = .right
        default: break
        }
        field?.font = f
        field?.textColor = color
        field?.alignment = align
        textView?.font = f
        textView?.textColor = color
        textView?.alignment = align
        textView?.typingAttributes = [.font: f, .foregroundColor: color]
        textView?.insertionPointColor = color
        placeholderLabel.font = f
        placeholderLabel.alignment = align
        updatePlaceholder()
        needsLayout = true
    }

    private func updatePlaceholder() {
        let color = placeholderColor ?? .placeholderTextColor
        if let f = field {
            f.placeholderAttributedString = placeholder.isEmpty ? nil : NSAttributedString(string: placeholder, attributes: [.foregroundColor: color, .font: font])
            f.setAccessibilityPlaceholderValue(placeholder)
        }
        placeholderLabel.stringValue = placeholder
        placeholderLabel.textColor = color
        placeholderLabel.isHidden = !(textView != nil && text.isEmpty && !placeholder.isEmpty)
    }

    // MARK: Value protocol

    private func receive(text: String, count: UInt32) {
        let decision: ControlledText.Decision
        if count == UInt32.max {
            // Reset to default: no value.
            state.reset()
            decision = self.text.isEmpty ? .skip : .apply("")
        } else {
            decision = state.incoming(text: text, count: count, current: self.text, composing: isComposing)
        }
        if case let .apply(t) = decision { write(t) }
    }

    /// Writes text without reporting it, keeping the caret at the same distance from the end.
    private func write(_ t: String) {
        writing = true
        defer { writing = false }
        if let ed = editor {
            let fromEnd = (ed.string as NSString).length - NSMaxRange(ed.selectedRange())
            ed.string = t
            let n = (t as NSString).length
            let p = max(0, min(n, n - fromEnd))
            ed.setSelectedRange(NSRange(location: p, length: 0))
        }
        // The field keeps its own value too (the editor's text is copied back when editing ends).
        field?.stringValue = t
        lastSeen = (t, isComposing)
        updatePlaceholder()
    }

    /// A user edit landed (typing, paste, IME update or commit).
    func edited() {
        guard !writing else { return }
        let composing = isComposing
        guard text != lastSeen.text || composing != lastSeen.composing else { return }
        if !composing, filter.maxLength > 0 || filter.allowed != nil {
            // An IME commit bypasses shouldChangeText: enforce the limits now.
            let clamped = filter.clamp(text)
            if clamped != text { write(clamped) }
        }
        let count = state.didEdit()
        lastSeen = (text, composing)
        updatePlaceholder()
        if listens(EventMask.change) {
            send(.change, ["text": text, "selection": selection, "composing": composing, "count": Int(count)])
        }
        if wasComposing, !composing, case let .apply(t) = state.compositionEnded(current: text) { write(t) }
        wasComposing = composing
    }

    /// Selection as UTF-16 offsets.
    var selection: [Int] {
        guard let r = editor?.selectedRange() else {
            let n = (text as NSString).length
            return [n, n]
        }
        return [r.location, NSMaxRange(r)]
    }

    /// Whether the editor may replace `range` with `replacement`; inserts a shortened
    /// or filtered version itself when the limits require.
    func admit(_ range: NSRange, _ replacement: String?) -> Bool {
        guard !writing, let replacement = replacement else { return true }
        // Marked (composing) text is the IME's business; limits apply on commit.
        if isComposing { return true }
        guard filter.maxLength > 0 || filter.allowed != nil else { return true }
        guard let admitted = filter.admit(current: text, range: range, replacement: replacement) else { return false }
        if admitted == replacement { return true }
        let new = (text as NSString).replacingCharacters(in: range, with: admitted)
        writing = true
        editor?.string = new
        editor?.setSelectedRange(NSRange(location: range.location + (admitted as NSString).length, length: 0))
        writing = false
        edited()
        return false
    }

    func submit() {
        if listens(EventMask.submit) { send(.submit, ["text": text]) }
        if returnKey == 1 { window?.makeFirstResponder(nil) }
    }

    func focused() { if listens(EventMask.focus) { send(.focus) } }
    func blurred() { if listens(EventMask.blur) { send(.blur) } }

    // MARK: NSTextField

    func controlTextDidChange(_ n: Notification) { edited() }
    func controlTextDidEndEditing(_ n: Notification) { blurred() }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        guard selector == #selector(NSResponder.insertNewline(_:)) else { return false }
        submit()
        return true
    }

    // MARK: NSTextView (multiline)

    func textView(_ textView: NSTextView, shouldChangeTextIn range: NSRange, replacementString text: String?) -> Bool {
        admit(range, text)
    }

    func textDidChange(_ notification: Notification) { edited() }

    // MARK: Commands

    func focus() {
        guard let w = window else { return }
        if let f = field { w.makeFirstResponder(f) } else if let tv = textView { w.makeFirstResponder(tv) }
    }

    override func runCommand(_ command: Command, _ payload: [String: Any]) {
        switch command {
        case .focus: focus()
        case .blur: if isEditing { window?.makeFirstResponder(nil) }
        default: break
        }
    }

    override func resetForReuse() {
        if isEditing { window?.makeFirstResponder(nil) }
        super.resetForReuse()
        state.reset()
        writing = true
        field?.stringValue = ""
        textView?.string = ""
        writing = false
        wasComposing = false
        lastSeen = ("", false)
    }

    /// The focus ring follows the program-drawn field: this view's bounds and corners.
    func focusRing(in view: NSView) -> (CGRect, CGFloat) {
        (view.convert(bounds, from: self), layer?.cornerRadius ?? 0)
    }
}

/// What the field subclasses share with their input view.
protocol InputFieldOwned: AnyObject {
    var owner: NSHTextInputView? { get set }
}

private extension NSTextField {
    /// Called from both field classes: focus, the field editor's filter hook, the ring.
    func ownerBecameFirstResponder(_ owner: NSHTextInputView?, _ ok: Bool) -> Bool {
        if ok { owner?.focused() }
        return ok
    }
}

private final class InputField: NSTextField, InputFieldOwned {
    weak var owner: NSHTextInputView?

    override func becomeFirstResponder() -> Bool { ownerBecameFirstResponder(owner, super.becomeFirstResponder()) }

    func textView(_ textView: NSTextView, shouldChangeTextIn range: NSRange, replacementString text: String?) -> Bool {
        owner?.admit(range, text) ?? true
    }

    override var focusRingMaskBounds: NSRect { owner?.focusRing(in: self).0 ?? bounds }

    override func drawFocusRingMask() {
        guard let (r, radius) = owner?.focusRing(in: self) else { return super.drawFocusRingMask() }
        NSBezierPath(roundedRect: r, xRadius: radius, yRadius: radius).fill()
    }
}

private final class SecureInputField: NSSecureTextField, InputFieldOwned {
    weak var owner: NSHTextInputView?

    override func becomeFirstResponder() -> Bool { ownerBecameFirstResponder(owner, super.becomeFirstResponder()) }

    func textView(_ textView: NSTextView, shouldChangeTextIn range: NSRange, replacementString text: String?) -> Bool {
        owner?.admit(range, text) ?? true
    }

    override var focusRingMaskBounds: NSRect { owner?.focusRing(in: self).0 ?? bounds }

    override func drawFocusRingMask() {
        guard let (r, radius) = owner?.focusRing(in: self) else { return super.drawFocusRingMask() }
        NSBezierPath(roundedRect: r, xRadius: radius, yRadius: radius).fill()
    }
}

private final class InputTextView: NSTextView {
    weak var owner: NSHTextInputView?

    override func becomeFirstResponder() -> Bool {
        let ok = super.becomeFirstResponder()
        if ok { owner?.focused() }
        return ok
    }

    override func resignFirstResponder() -> Bool {
        let ok = super.resignFirstResponder()
        if ok { owner?.blurred() }
        return ok
    }
}
