// Native controls. Each wraps a UIKit control filling the node's bounds and
// reports user changes with the payloads of `EventType::Change`.

import NativeShellShared
import UIKit

/// A component that hosts one control filling its bounds.
class NSHControlView: NSHComponentView {
    private(set) var controlView: UIView!
    override var handlesTapGestures: Bool { false }

    func makeControl() -> UIView { UIView() }
    override var accessibilityTarget: UIView { controlView ?? self }

    override func commonInit() {
        controlView = makeControl()
        addSubview(controlView)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        layoutControl()
    }

    func layoutControl() { controlView.frame = bounds }

    // MARK: Tint

    /// `TintColor` from the core, overriding the theme's `primary`.
    private var explicitTint: UIColor?

    /// Applies the accent color to the control's parts (subclasses).
    func applyTint(_ color: UIColor) {}

    func refreshTint() { applyTint(explicitTint ?? themeColor("primary")) }

    override func themeDidChange() { refreshTint() }

    override func didMoveToSuperview() {
        super.didMoveToSuperview()
        refreshTint()
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        guard prop == .tintColor else { return super.applyProp(prop, value) }
        explicitTint = value.uiColor
        refreshTint()
        return true
    }
}

final class NSHSwitchView: NSHControlView {
    var control: UISwitch { controlView as! UISwitch }
    override func makeControl() -> UIView { UISwitch() }

    override func commonInit() {
        super.commonInit()
        control.addTarget(self, action: #selector(changed), for: .valueChanged)
    }

    // UISwitch has a fixed size: center it in the frame the core gave.
    override func layoutControl() {
        control.sizeToFit()
        control.center = CGPoint(x: bounds.midX, y: bounds.midY)
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .checked: control.setOn(value.bool ?? false, animated: window != nil)
        case .enabled: control.isEnabled = value.bool ?? true
        // The on state keeps the system green (the iOS convention): TintColor is ignored.
        case .tintColor: break
        default: return super.applyProp(prop, value)
        }
        return true
    }

    @objc private func changed() {
        if listens(EventMask.change) { send(.change, ["checked": control.isOn]) }
    }
}

/// Checkbox (CheckStyle 0) or radio (1), drawn natively.
final class NSHCheckboxView: NSHComponentView {
    private(set) var checked = false
    private var enabled = true
    private var style = 0
    private var color: UIColor?
    private let box = CAShapeLayer()
    private let mark = CAShapeLayer()
    override var handlesTapGestures: Bool { false }

    override func commonInit() {
        layer.addSublayer(box)
        layer.addSublayer(mark)
        addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(toggle)))
        isAccessibilityElement = true
        redraw()
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .checked: checked = value.bool ?? false
        case .enabled: enabled = value.bool ?? true
        case .checkStyle: style = value.int ?? 0
        case .tintColor: color = value.uiColor
        default: return super.applyProp(prop, value)
        }
        redraw()
        return true
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        redraw()
    }

    override func traitCollectionDidChange(_ previous: UITraitCollection?) {
        super.traitCollectionDidChange(previous)
        // Layer colors are resolved once; follow light/dark changes.
        redraw()
    }

    override func themeDidChange() { redraw() }

    private func redraw() {
        // The glyph fills the frame the core laid out (which grows with the font scale).
        let side = max(0, min(bounds.width, bounds.height))
        let unit = side / 22
        let r = CGRect(x: (bounds.width - side) / 2, y: (bounds.height - side) / 2, width: side, height: side)
        let tint = (color ?? themeColor("primary")).resolvedColor(with: traitCollection)
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        box.frame = bounds
        mark.frame = bounds
        box.lineWidth = 1.5 * unit
        if style == 1 {
            box.path = UIBezierPath(ovalIn: r.insetBy(dx: 0.75 * unit, dy: 0.75 * unit)).cgPath
            box.fillColor = UIColor.clear.cgColor
            box.strokeColor = (checked ? tint : UIColor.secondaryLabel).cgColor
            mark.path = UIBezierPath(ovalIn: r.insetBy(dx: side * 0.27, dy: side * 0.27)).cgPath
            mark.fillColor = tint.cgColor
            mark.strokeColor = nil
        } else {
            box.path = UIBezierPath(roundedRect: r.insetBy(dx: 0.75 * unit, dy: 0.75 * unit), cornerRadius: side * 0.22).cgPath
            box.fillColor = (checked ? tint : UIColor.clear).cgColor
            box.strokeColor = (checked ? tint : UIColor.secondaryLabel).cgColor
            let p = UIBezierPath()
            p.move(to: CGPoint(x: r.minX + side * 0.25, y: r.midY))
            p.addLine(to: CGPoint(x: r.minX + side * 0.43, y: r.minY + side * 0.68))
            p.addLine(to: CGPoint(x: r.minX + side * 0.76, y: r.minY + side * 0.32))
            mark.path = p.cgPath
            mark.fillColor = nil
            mark.strokeColor = UIColor.white.cgColor
            mark.lineWidth = 2 * unit
            mark.lineCap = .round
            mark.lineJoin = .round
        }
        mark.isHidden = !checked
        // Dim the drawing, not the view: the view's own opacity is the Opacity prop.
        box.opacity = enabled ? 1 : 0.4
        mark.opacity = enabled ? 1 : 0.4
        accessibilityTraits = checked ? [.button, .selected] : .button
        accessibilityValue = checked ? "1" : "0"
        CATransaction.commit()
    }

    @objc func toggle() {
        guard enabled else { return }
        // A radio never unchecks itself.
        if style == 1, checked { return }
        checked.toggle()
        redraw()
        if listens(EventMask.change) { send(.change, ["checked": checked]) }
    }
}

final class NSHSliderView: NSHControlView {
    var control: UISlider { controlView as! UISlider }
    override func makeControl() -> UIView { UISlider() }
    override func applyTint(_ color: UIColor) { control.minimumTrackTintColor = color }

    private var step: Float = 0
    private var lastSent: TimeInterval = 0

    override func commonInit() {
        super.commonInit()
        control.minimumValue = 0
        control.maximumValue = 1
        control.addTarget(self, action: #selector(changed), for: .valueChanged)
        control.addTarget(self, action: #selector(ended), for: [.touchUpInside, .touchUpOutside, .touchCancel])
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .value: control.value = Float(value.number ?? 0)
        case .min: control.minimumValue = Float(value.number ?? 0)
        case .max: control.maximumValue = Float(value.number ?? 1)
        case .step: step = Float(value.number ?? 0)
        case .enabled: control.isEnabled = value.bool ?? true
        default: return super.applyProp(prop, value)
        }
        return true
    }

    private var stepped: Float {
        guard step > 0 else { return control.value }
        let v = control.minimumValue + ((control.value - control.minimumValue) / step).rounded() * step
        return min(max(v, control.minimumValue), control.maximumValue)
    }

    // While dragging: at most every 50 ms. The final value is always sent.
    @objc private func changed() {
        guard listens(EventMask.change) else { return }
        let now = ProcessInfo.processInfo.systemUptime
        if !control.isTracking {
            send(.change, ["value": Double(stepped), "final": true])
        } else if now - lastSent >= 0.05 {
            lastSent = now
            send(.change, ["value": Double(stepped), "final": false])
        }
    }

    override func resetForReuse() {
        super.resetForReuse()
        // The resets above ran in no particular order and clamp each other.
        control.minimumValue = 0
        control.maximumValue = 1
        control.value = 0
        step = 0
    }

    @objc private func ended() {
        control.value = stepped
        if listens(EventMask.change) { send(.change, ["value": Double(stepped), "final": true]) }
    }
}

final class NSHProgressView: NSHControlView {
    var control: UIProgressView { controlView as! UIProgressView }
    override func makeControl() -> UIView { UIProgressView(progressViewStyle: .default) }
    override func applyTint(_ color: UIColor) { control.progressTintColor = color }

    override func layoutControl() {
        let h = control.sizeThatFits(bounds.size).height
        control.frame = CGRect(x: 0, y: (bounds.height - h) / 2, width: bounds.width, height: h)
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .value: control.progress = Float(value.number ?? 0)
        case .indeterminate: break
        default: return super.applyProp(prop, value)
        }
        return true
    }
}

final class NSHSpinnerView: NSHControlView {
    var control: UIActivityIndicatorView { controlView as! UIActivityIndicatorView }
    override func makeControl() -> UIView { UIActivityIndicatorView() }
    override func applyTint(_ color: UIColor) { control.color = color }

    override func commonInit() {
        super.commonInit()
        control.style = .medium
        control.hidesWhenStopped = false
        control.startAnimating()
    }

    override func layoutControl() {
        control.sizeToFit()
        control.center = CGPoint(x: bounds.midX, y: bounds.midY)
        if bounds.width >= 36 { control.style = .large } else { control.style = .medium }
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .indeterminate: break
        default: return super.applyProp(prop, value)
        }
        return true
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window != nil { control.startAnimating() }
    }
}

final class NSHSegmentedView: NSHControlView {
    var control: UISegmentedControl { controlView as! UISegmentedControl }
    override func makeControl() -> UIView { UISegmentedControl() }
    override func applyTint(_ color: UIColor) {
        control.selectedSegmentTintColor = color
        control.setTitleTextAttributes([.foregroundColor: themeColor("primary_foreground")], for: .selected)
    }

    private var items: [String] = []

    override func commonInit() {
        super.commonInit()
        control.addTarget(self, action: #selector(changed), for: .valueChanged)
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .items:
            items = (value.jsonArray ?? []).map { ($0 as? String) ?? (($0 as? [String: Any])?.string("label") ?? "\($0)") }
            let selected = control.selectedSegmentIndex
            control.removeAllSegments()
            for (i, t) in items.enumerated() { control.insertSegment(withTitle: t, at: i, animated: false) }
            control.selectedSegmentIndex = selected < items.count ? selected : UISegmentedControl.noSegment
        case .selectedIndex:
            let i = value.int ?? -1
            control.selectedSegmentIndex = i >= 0 && i < items.count ? i : UISegmentedControl.noSegment
        case .enabled: control.isEnabled = value.bool ?? true
        default: return super.applyProp(prop, value)
        }
        return true
    }

    @objc private func changed() {
        let i = control.selectedSegmentIndex
        guard listens(EventMask.change), i >= 0, i < items.count else { return }
        send(.change, ["index": i, "value": items[i]])
    }
}

final class NSHDatePickerView: NSHControlView {
    var control: UIDatePicker { controlView as! UIDatePicker }
    override func makeControl() -> UIView { UIDatePicker() }

    private var mode = 0
    private var raw: String?

    private func applyDate() {
        if let s = raw, let d = DateFormats.parse(s, mode: mode) { control.setDate(d, animated: false) } else { control.date = Date() }
    }

    override func commonInit() {
        super.commonInit()
        control.preferredDatePickerStyle = .compact
        control.datePickerMode = .date
        control.addTarget(self, action: #selector(changed), for: .valueChanged)
    }

    override func layoutControl() {
        let s = control.sizeThatFits(bounds.size)
        control.frame = CGRect(x: 0, y: (bounds.height - s.height) / 2, width: min(bounds.width, max(s.width, 0)), height: s.height)
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .date:
            raw = value.string
            applyDate()
        case .dateMode:
            mode = value.int ?? 0
            control.datePickerMode = mode == 1 ? .time : (mode == 2 ? .dateAndTime : .date)
            applyDate()
        case .enabled: control.isEnabled = value.bool ?? true
        default: return super.applyProp(prop, value)
        }
        return true
    }

    @objc private func changed() {
        if listens(EventMask.change) { send(.change, ["date": DateFormats.format(control.date, mode: mode)]) }
    }
}

enum DateFormats {
    private static func formatter(_ f: String) -> DateFormatter {
        let d = DateFormatter()
        d.locale = Locale(identifier: "en_US_POSIX")
        d.dateFormat = f
        return d
    }

    private static let day = formatter("yyyy-MM-dd")
    private static let minute = formatter("yyyy-MM-dd'T'HH:mm")
    private static let time = formatter("HH:mm")

    /// Wire formats by `DateMode`: 0 `YYYY-MM-DD`, 1 `HH:mm`, 2 `YYYY-MM-DDTHH:mm`.
    /// Parsing also accepts the other forms, so a mode change never drops the value.
    static func parse(_ s: String, mode: Int) -> Date? {
        let order: [DateFormatter] = mode == 1 ? [time, minute, day] : (mode == 2 ? [minute, day, time] : [day, minute, time])
        for f in order { if let d = f.date(from: s) { return d } }
        return nil
    }

    static func format(_ d: Date, mode: Int) -> String { (mode == 1 ? time : (mode == 2 ? minute : day)).string(from: d) }
}

/// A button showing the current choice and an up/down chevron; opens a native menu.
final class NSHSelectView: NSHControlView {
    var control: UIButton { controlView as! UIButton }
    override func makeControl() -> UIView { UIButton(type: .system) }
    override func applyTint(_ color: UIColor) { chevron.tintColor = color }

    private var items: [(label: String, value: Any, destructive: Bool, disabled: Bool)] = []
    private var menuTitle = ""
    private var placeholder = ""
    private var selected = -1

    private let chevron = UIImageView()

    override func commonInit() {
        super.commonInit()
        control.showsMenuAsPrimaryAction = true
        control.contentHorizontalAlignment = .leading
        control.titleLabel?.font = SelectChrome.font()
        chevron.image = SelectChrome.chevron()

        chevron.contentMode = .center
        chevron.isUserInteractionEnabled = false
        addSubview(chevron)
        control.setTitleColor(.label, for: .normal)
        control.titleLabel?.lineBreakMode = .byTruncatingTail
        refresh()
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .menu:
            let m = value.jsonObject ?? [:]
            menuTitle = m.string("title") ?? ""
            if let p = m.string("placeholder") { placeholder = p }
            items = (m["items"] as? [[String: Any]] ?? []).map {
                ($0.string("label") ?? "", $0["value"] ?? NSNull(), $0.bool("destructive") ?? false, $0.bool("disabled") ?? false)
            }
        case .items:
            items = (value.jsonArray ?? []).map { item in
                if let o = item as? [String: Any] { return (o.string("label") ?? "", o["value"] ?? NSNull(), false, false) }
                return ("\(item)", item, false, false)
            }
        case .selectedIndex: selected = value.int ?? -1
        case .placeholder: placeholder = value.string ?? ""
        case .enabled: control.isEnabled = value.bool ?? true
        case .textStyle:
            let s = value.jsonObject ?? [:]
            control.titleLabel?.font = Fonts.font(size: (s.cgFloat("size") ?? 16) * FontScale.value, weight: s.int("weight") ?? 400, family: s.string("family"), italic: false)
            control.setTitleColor(s.color("color") ?? .label, for: .normal)
        default: return super.applyProp(prop, value)
        }
        refresh()
        return true
    }

    override func layoutControl() {
        super.layoutControl()
        let c = SelectChrome.chevronSize()
        chevron.frame = CGRect(x: bounds.width - SelectChrome.padding - c.width, y: (bounds.height - c.height) / 2, width: c.width, height: c.height)
        control.contentEdgeInsets = UIEdgeInsets(top: 0, left: SelectChrome.padding, bottom: 0, right: SelectChrome.padding + c.width + SelectChrome.gap)
    }

    private func refresh() {
        let title = selected >= 0 && selected < items.count ? items[selected].label : placeholder
        control.setTitle(title, for: .normal)
        let actions = items.enumerated().map { i, item -> UIAction in
            var attrs: UIMenuElement.Attributes = []
            if item.destructive { attrs.insert(.destructive) }
            if item.disabled { attrs.insert(.disabled) }
            return UIAction(title: item.label, attributes: attrs, state: i == selected ? .on : .off) { [weak self] _ in
                self?.choose(i)
            }
        }
        control.menu = UIMenu(title: menuTitle, children: actions)
    }

    /// Also the test entry point: what tapping menu item `i` does.
    func choose(_ i: Int) {
        guard i >= 0, i < items.count else { return }
        selected = i
        refresh()
        if listens(EventMask.change) { send(.change, ["index": i, "value": items[i].value]) }
    }
}

/// Geometry of the select button, shared by the view and `nsh/select` measurement
/// (which runs on the JS thread, so it uses font metrics only).
enum SelectChrome {
    static let padding: CGFloat = 12
    static let gap: CGFloat = 6
    static let minHeight: CGFloat = 44

    static func font() -> UIFont { .systemFont(ofSize: 16 * FontScale.value) }

    static func chevron() -> UIImage? {
        UIImage(systemName: "chevron.up.chevron.down", withConfiguration: UIImage.SymbolConfiguration(pointSize: 12 * FontScale.value, weight: .medium))
    }

    static func chevronSize() -> CGSize { chevron()?.size ?? CGSize(width: 10, height: 14) }

    static func size(label: String) -> CGSize {
        let f = font()
        let text = (label as NSString).size(withAttributes: [.font: f])
        let c = chevronSize()
        return CGSize(width: ceil(padding + text.width + gap + c.width + padding), height: max(minHeight, ceil(f.lineHeight + 16)))
    }
}

/// Size of the compact UIDatePicker for `nsh/date-picker`: the localized value in
/// a pill (two pills for date and time), from font metrics so it is safe off the main thread.
enum DatePickerChrome {
    static let padding: CGFloat = 12
    static let gap: CGFloat = 8

    static func size(value: String?, mode: Int) -> CGSize {
        let f = UIFont.systemFont(ofSize: 17 * FontScale.value)
        let d = value.flatMap { DateFormats.parse($0, mode: mode) } ?? Date()
        func pill(_ s: String) -> CGFloat { ceil((s as NSString).size(withAttributes: [.font: f]).width) + 2 * padding }
        let dateText = DateFormatter.localizedString(from: d, dateStyle: .medium, timeStyle: .none)
        let timeText = DateFormatter.localizedString(from: d, dateStyle: .none, timeStyle: .short)
        let width: CGFloat
        switch mode {
        case 1: width = pill(timeText)
        case 2: width = pill(dateText) + gap + pill(timeText)
        default: width = pill(dateText)
        }
        return CGSize(width: width, height: max(SelectChrome.minHeight, ceil(f.lineHeight + 14)))
    }
}

/// `measure_component` for the reserved built-in names.
enum BuiltinMeasure {
    static func measure(_ name: String, _ props: [String: Any]) -> CGSize? {
        switch name {
        case "nsh/select":
            let label = props.string("label").flatMap { $0.isEmpty ? nil : $0 } ?? props.string("placeholder") ?? ""
            return SelectChrome.size(label: label)
        case "nsh/date-picker":
            let mode = ["time": 1, "datetime": 2][props.string("mode") ?? "date"] ?? 0
            return DatePickerChrome.size(value: props.string("value"), mode: mode)
        default:
            return nil
        }
    }
}
