// `TextInput`: a controlled UITextField (or UITextView when `Multiline`).
// See `ControlledText` for the value protocol.

import UIKit

final class NSHTextInputView: NSHComponentView, UITextFieldDelegate, UITextViewDelegate {
    private(set) var field: UITextField?
    private(set) var textView: UITextView?
    private let placeholderLabel = UILabel()
    private(set) var state = ControlledText()
    private var filter = InputFilter()
    /// Set while the renderer writes text, so delegate callbacks are not reported as edits.
    private var writing = false
    private var wasComposing = false
    private var multiline = false
    private var style: [String: Any] = [:]
    private var placeholder = ""
    private var placeholderColor: UIColor?
    private var returnKey = 0
    private var insets: UIEdgeInsets = .zero
    private var props: [Prop: PropValue] = [:]
    override var handlesTapGestures: Bool { false }
    override var accessibilityTarget: UIView { field ?? textView ?? self }

    override func commonInit() {
        // Taps in the padding around an inset field focus it too.
        let tap = UITapGestureRecognizer(target: self, action: #selector(focusFromPadding))
        tap.cancelsTouchesInView = false
        addGestureRecognizer(tap)
        placeholderLabel.numberOfLines = 0
        placeholderLabel.isUserInteractionEnabled = false
        build()
    }

    /// The UIKit view that takes input.
    var input: UIView & UITextInput { field ?? textView! }

    var text: String { field?.text ?? textView?.text ?? "" }

    var isComposing: Bool { input.markedTextRange != nil }

    private func build() {
        if let f = field { NotificationCenter.default.removeObserver(self, name: UITextField.textDidChangeNotification, object: f) }
        field?.removeFromSuperview()
        textView?.removeFromSuperview()
        placeholderLabel.removeFromSuperview()
        let old = field?.text ?? textView?.text ?? ""
        if multiline {
            let tv = TextViewBackend()
            tv.delegate = self
            tv.backgroundColor = .clear
            tv.textContainerInset = .zero
            tv.textContainer.lineFragmentPadding = 0
            tv.text = old
            tv.owner = self
            addSubview(tv)
            tv.addSubview(placeholderLabel)
            textView = tv
            field = nil
        } else {
            let f = TextFieldBackend()
            f.delegate = self
            f.text = old
            f.owner = self
            // The notification, not `.editingChanged`: it also covers IME marked-text updates.
            NotificationCenter.default.addObserver(self, selector: #selector(fieldChanged(_:)), name: UITextField.textDidChangeNotification, object: f)
            addSubview(f)
            field = f
            textView = nil
        }
        for (p, v) in props { applyInputProp(p, v) }
        input.accessibilityIdentifier = accessibilityIdentifier
        input.accessibilityLabel = accessibilityLabel
        applyStyle()
        updatePlaceholder()
        setNeedsLayout()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        // ContentInsets: the field itself is inset (UIKit does not apply text/editing
        // rect overrides consistently while editing); a text view insets its container.
        field?.frame = bounds.inset(by: insets)
        textView?.frame = bounds
        let w = max(0, bounds.width - insets.left - insets.right)
        placeholderLabel.frame = CGRect(x: insets.left, y: insets.top, width: w, height: placeholderLabel.sizeThatFits(CGSize(width: w, height: bounds.height)).height)
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
        case .placeholder, .secure, .keyboardType, .editable, .returnKey, .contentType, .autoCapitalize, .placeholderColor,
             .textStyle, .maxLength, .allowedChars, .contentInsets:
            if case .null = value { props[prop] = nil } else { props[prop] = value }
            applyInputProp(prop, value)
            return true
        default:
            return super.applyProp(prop, value)
        }
    }

    private func applyInputProp(_ prop: Prop, _ value: PropValue) {
        let traits = input as UITextInputTraits
        switch prop {
        case .placeholder:
            placeholder = value.string ?? ""
            updatePlaceholder()
        case .placeholderColor:
            placeholderColor = value.uiColor
            updatePlaceholder()
        case .secure:
            field?.isSecureTextEntry = value.bool ?? false
            textView?.isSecureTextEntry = value.bool ?? false
        case .keyboardType:
            let kinds: [UIKeyboardType] = [.default, .numberPad, .decimalPad, .emailAddress, .phonePad, .URL]
            let i = value.int ?? 0
            let k = i >= 0 && i < kinds.count ? kinds[i] : .default
            field?.keyboardType = k
            textView?.keyboardType = k
        case .editable:
            let on = value.bool ?? true
            field?.isEnabled = on
            textView?.isEditable = on
        case .returnKey:
            returnKey = value.int ?? 0
            let keys: [UIReturnKeyType] = [.default, .done, .go, .next, .search, .send]
            let k = returnKey >= 0 && returnKey < keys.count ? keys[returnKey] : .default
            field?.returnKeyType = k
            textView?.returnKeyType = k
        case .contentType:
            let types: [UITextContentType?] = [nil, .oneTimeCode, .username, .password, .emailAddress]
            let i = value.int ?? 0
            let t = i >= 0 && i < types.count ? types[i] : nil
            field?.textContentType = t
            textView?.textContentType = t
        case .autoCapitalize:
            let caps: [UITextAutocapitalizationType] = [.none, .words, .sentences, .allCharacters]
            let i = value.int ?? 2
            let c = i >= 0 && i < caps.count ? caps[i] : .sentences
            field?.autocapitalizationType = c
            textView?.autocapitalizationType = c
        case .textStyle:
            style = value.jsonObject ?? [:]
            applyStyle()
        case .contentInsets:
            let a = value.floats ?? []
            insets = a.count == 4 ? UIEdgeInsets(top: CGFloat(a[0]), left: CGFloat(a[3]), bottom: CGFloat(a[2]), right: CGFloat(a[1])) : .zero
            textView?.textContainerInset = insets
            field?.setNeedsLayout()
            setNeedsLayout()
        case .maxLength:
            filter.maxLength = max(0, value.int ?? 0)
        case .allowedChars:
            let s = value.string ?? ""
            filter.allowed = s.isEmpty ? nil : Set(s)
        default:
            break
        }
        _ = traits
    }

    private var font: UIFont {
        Fonts.font(size: (style.cgFloat("size") ?? 16) * FontScale.value, weight: style.int("weight") ?? 400, family: style.string("family"), italic: style.bool("italic") ?? false)
    }

    private func applyStyle() {
        let f = font
        let color = style.color("color") ?? .label
        var align = NSTextAlignment.natural
        switch style.string("align") {
        case "center": align = .center
        case "right": align = .right
        default: break
        }
        field?.font = f
        field?.textColor = color
        field?.textAlignment = align
        textView?.font = f
        textView?.textColor = color
        textView?.textAlignment = align
        placeholderLabel.font = f
        placeholderLabel.textAlignment = align
        updatePlaceholder()
    }

    private func updatePlaceholder() {
        let color = placeholderColor ?? .placeholderText
        if let f = field {
            f.attributedPlaceholder = placeholder.isEmpty ? nil : NSAttributedString(string: placeholder, attributes: [.foregroundColor: color, .font: font])
        }
        placeholderLabel.text = placeholder
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
        let input = self.input
        var fromEnd = 0
        if let sel = input.selectedTextRange {
            fromEnd = input.offset(from: sel.end, to: input.endOfDocument)
        }
        if let f = field { f.text = t } else { textView?.text = t }
        let n = (t as NSString).length
        if let p = input.position(from: input.beginningOfDocument, offset: max(0, n - fromEnd)) {
            input.selectedTextRange = input.textRange(from: p, to: p)
        }
        updatePlaceholder()
    }

    /// A user edit landed (typing, paste, IME update or commit).
    private func edited() {
        guard !writing else { return }
        let composing = isComposing
        if !composing, filter.maxLength > 0 || filter.allowed != nil {
            // An IME commit bypasses shouldChangeCharactersIn: enforce the limits now.
            let clamped = filter.clamp(text)
            if clamped != text { write(clamped) }
        }
        let count = state.didEdit()
        updatePlaceholder()
        if listens(EventMask.change) {
            send(.change, ["text": text, "selection": selection, "composing": composing, "count": Int(count)])
        }
        if wasComposing, !composing, case let .apply(t) = state.compositionEnded(current: text) { write(t) }
        wasComposing = composing
    }

    /// Selection as UTF-16 offsets.
    var selection: [Int] {
        guard let r = input.selectedTextRange else { return [0, 0] }
        return [input.offset(from: input.beginningOfDocument, to: r.start), input.offset(from: input.beginningOfDocument, to: r.end)]
    }

    private func admit(_ range: NSRange, _ replacement: String) -> Bool {
        guard !writing else { return true }
        // Marked (composing) text is the IME's business; limits apply on commit.
        if isComposing { return true }
        guard filter.maxLength > 0 || filter.allowed != nil else { return true }
        guard let admitted = filter.admit(current: text, range: range, replacement: replacement) else { return false }
        if admitted == replacement { return true }
        // Insert the shortened/filtered text ourselves, then report one edit.
        let new = (text as NSString).replacingCharacters(in: range, with: admitted)
        writing = true
        if let f = field { f.text = new } else { textView?.text = new }
        if let p = input.position(from: input.beginningOfDocument, offset: range.location + (admitted as NSString).length) {
            input.selectedTextRange = input.textRange(from: p, to: p)
        }
        writing = false
        edited()
        return false
    }

    // MARK: UITextField

    @objc private func fieldChanged(_ n: Notification) { edited() }

    func textField(_ textField: UITextField, shouldChangeCharactersIn range: NSRange, replacementString string: String) -> Bool {
        admit(range, string)
    }

    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        if listens(EventMask.submit) { send(.submit, ["text": text]) }
        if returnKey == 1 { textField.resignFirstResponder() }
        return false
    }

    func textFieldDidBeginEditing(_ textField: UITextField) { if listens(EventMask.focus) { send(.focus) } }
    func textFieldDidEndEditing(_ textField: UITextField) { if listens(EventMask.blur) { send(.blur) } }

    // MARK: UITextView

    func textView(_ textView: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
        admit(range, text)
    }

    func textViewDidChange(_ textView: UITextView) { edited() }
    func textViewDidBeginEditing(_ textView: UITextView) { if listens(EventMask.focus) { send(.focus) } }
    func textViewDidEndEditing(_ textView: UITextView) { if listens(EventMask.blur) { send(.blur) } }

    @objc private func focusFromPadding() {
        if (field?.isEnabled ?? textView?.isEditable) == true { _ = input.becomeFirstResponder() }
    }

    // MARK: Commands

    override func runCommand(_ command: Command, _ payload: [String: Any]) {
        switch command {
        case .focus: _ = input.becomeFirstResponder()
        case .blur: _ = input.resignFirstResponder()
        default: break
        }
    }

    override func prepareForReuse() {
        _ = input.resignFirstResponder()
        super.prepareForReuse()
        state.reset()
        writing = true
        if let f = field { f.text = "" } else { textView?.text = "" }
        writing = false
        wasComposing = false
    }

    /// IME composition changes that UIKit reports without an editing event.
    fileprivate func markedTextChanged() { edited() }
}

private final class TextFieldBackend: UITextField {
    weak var owner: NSHTextInputView?

    // Text views may keep typing keys to themselves: route before UIKit does.
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        KeyEvents.route(presses, phase: "down", to: owner)
        super.pressesBegan(presses, with: event)
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        KeyEvents.route(presses, phase: "up", to: owner)
        super.pressesEnded(presses, with: event)
    }

    override func unmarkText() {
        let had = markedTextRange != nil
        super.unmarkText()
        if had { owner?.markedTextChanged() }
    }
}

private final class TextViewBackend: UITextView {
    weak var owner: NSHTextInputView?

    // Text views may keep typing keys to themselves: route before UIKit does.
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        KeyEvents.route(presses, phase: "down", to: owner)
        super.pressesBegan(presses, with: event)
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        KeyEvents.route(presses, phase: "up", to: owner)
        super.pressesEnded(presses, with: event)
    }
}
