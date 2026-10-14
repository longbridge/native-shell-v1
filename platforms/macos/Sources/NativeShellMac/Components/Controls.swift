// Native controls. Each wraps an AppKit control placed in the node's bounds
// and reports user changes with the payloads of `EventType::Change`.

import AppKit
import NativeShellShared

/// A component that hosts one control.
class NSHControlView: NSHComponentView {
    private(set) var controlView: NSView!
    override var handlesMouse: Bool { false }

    func makeControl() -> NSView { NSView() }
    override var accessibilityTarget: NSView { controlView ?? self }

    override func commonInit() {
        controlView = makeControl()
        addSubview(controlView)
    }

    override func layout() {
        super.layout()
        layoutControl()
    }

    func layoutControl() { controlView.frame = bounds }

    /// Centers the control at its natural size (AppKit controls have fixed heights).
    func centerControl(width: CGFloat? = nil) {
        let s = controlView.fittingSize
        let w = width ?? s.width
        controlView.frame = CGRect(x: ((bounds.width - w) / 2).rounded(), y: ((bounds.height - s.height) / 2).rounded(), width: w, height: s.height)
    }

    // MARK: Tint

    /// `TintColor` from the core, overriding the theme's `primary`.
    private var explicitTint: NSColor?

    /// Applies the accent color to the control's parts (subclasses).
    func applyTint(_ color: NSColor) {}

    func refreshTint() { applyTint(explicitTint ?? themeColor("primary")) }

    override func themeDidChange() { refreshTint() }

    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        refreshTint()
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        guard prop == .tintColor else { return super.applyProp(prop, value) }
        explicitTint = value.nsColor
        refreshTint()
        return true
    }
}

final class NSHSwitchView: NSHControlView {
    var control: NSSwitch { controlView as! NSSwitch }
    override func makeControl() -> NSView { NSSwitch() }

    override func commonInit() {
        super.commonInit()
        control.target = self
        control.action = #selector(changed)
    }

    override func layoutControl() { centerControl() }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .checked: control.state = (value.bool ?? false) ? .on : .off
        case .enabled: control.isEnabled = value.bool ?? true
        // The on state keeps the system accent (the macOS convention): TintColor is ignored.
        case .tintColor: break
        default: return super.applyProp(prop, value)
        }
        return true
    }

    @objc func changed() {
        if listens(EventMask.change) { send(.change, ["checked": control.state == .on]) }
    }
}

/// Checkbox (CheckStyle 0) or radio (1): the system button styles.
final class NSHCheckboxView: NSHControlView {
    var control: NSButton { controlView as! NSButton }
    private var style = 0
    override func makeControl() -> NSView { NSButton(checkboxWithTitle: "", target: nil, action: nil) }

    var checked: Bool { control.state == .on }

    override func commonInit() {
        super.commonInit()
        configure()
    }

    private func configure() {
        control.target = self
        control.action = #selector(changed)
        control.imagePosition = .imageOnly
    }

    override func layoutControl() { centerControl() }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .checked: control.state = (value.bool ?? false) ? .on : .off
        case .enabled: control.isEnabled = value.bool ?? true
        case .checkStyle:
            style = value.int ?? 0
            control.setButtonType(style == 1 ? .radio : .switch)
            configure()
            needsLayout = true
        case .tintColor: control.contentTintColor = value.nsColor
        default: return super.applyProp(prop, value)
        }
        return true
    }

    /// What clicking does (also the test entry point).
    func toggle() {
        guard control.isEnabled else { return }
        // A radio never unchecks itself.
        if style == 1, checked { return }
        control.state = checked ? .off : .on
        changed()
    }

    @objc private func changed() {
        if style == 1, !checked {
            control.state = .on
            return
        }
        if listens(EventMask.change) { send(.change, ["checked": checked]) }
    }
}

final class NSHSliderView: NSHControlView {
    var control: NSSlider { controlView as! NSSlider }
    override func makeControl() -> NSView { NSSlider(value: 0, minValue: 0, maxValue: 1, target: nil, action: nil) }
    override func applyTint(_ color: NSColor) { control.trackFillColor = color }

    private var step: Double = 0
    private var lastSent: TimeInterval = 0
    /// The core's value, re-applied when the range changes: props arrive in any
    /// order, and NSSlider clamps a value outside its current range.
    private var value: Double = 0

    override func commonInit() {
        super.commonInit()
        control.target = self
        control.action = #selector(changed)
        control.isContinuous = true
    }

    override func layoutControl() {
        let h = control.fittingSize.height
        control.frame = CGRect(x: 0, y: ((bounds.height - h) / 2).rounded(), width: bounds.width, height: h)
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .value:
            self.value = value.number ?? 0
            control.doubleValue = self.value
        case .min:
            control.minValue = value.number ?? 0
            control.doubleValue = self.value
        case .max:
            control.maxValue = value.number ?? 1
            control.doubleValue = self.value
        case .step: step = value.number ?? 0
        case .enabled: control.isEnabled = value.bool ?? true
        default: return super.applyProp(prop, value)
        }
        return true
    }

    private var stepped: Double {
        guard step > 0 else { return control.doubleValue }
        let v = control.minValue + ((control.doubleValue - control.minValue) / step).rounded() * step
        return min(max(v, control.minValue), control.maxValue)
    }

    /// While dragging: at most every 50 ms. The final value (mouse up, or a key) is always sent.
    @objc func changed() {
        let dragging = NSApp?.currentEvent.map { $0.type == .leftMouseDragged || $0.type == .leftMouseDown } ?? false
        if !dragging { control.doubleValue = stepped }
        value = stepped
        guard listens(EventMask.change) else { return }
        let now = ProcessInfo.processInfo.systemUptime
        if !dragging {
            send(.change, ["value": stepped, "final": true])
        } else if now - lastSent >= 0.05 {
            lastSent = now
            send(.change, ["value": stepped, "final": false])
        }
    }

    override func resetForReuse() {
        super.resetForReuse()
        // The resets above ran in no particular order and clamp each other.
        control.minValue = 0
        control.maxValue = 1
        control.doubleValue = 0
        value = 0
        step = 0
    }
}

final class NSHProgressView: NSHControlView {
    var control: NSProgressIndicator { controlView as! NSProgressIndicator }
    override func makeControl() -> NSView {
        let p = NSProgressIndicator()
        p.style = .bar
        p.isIndeterminate = false
        p.minValue = 0
        p.maxValue = 1
        return p
    }

    override func layoutControl() {
        let h: CGFloat = min(bounds.height, 6)
        control.frame = CGRect(x: 0, y: ((bounds.height - h) / 2).rounded(), width: bounds.width, height: h)
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .value: control.doubleValue = value.number ?? 0
        case .indeterminate:
            control.isIndeterminate = value.bool ?? false
            if control.isIndeterminate { control.startAnimation(nil) } else { control.stopAnimation(nil) }
        default: return super.applyProp(prop, value)
        }
        return true
    }
}

final class NSHSpinnerView: NSHControlView {
    var control: NSProgressIndicator { controlView as! NSProgressIndicator }
    override func makeControl() -> NSView {
        let p = NSProgressIndicator()
        p.style = .spinning
        p.isIndeterminate = true
        p.isDisplayedWhenStopped = true
        return p
    }

    override func layoutControl() {
        let side: CGFloat
        if min(bounds.width, bounds.height) >= 28 {
            control.controlSize = .regular
            side = 32
        } else {
            control.controlSize = .small
            side = 16
        }
        control.frame = CGRect(x: ((bounds.width - side) / 2).rounded(), y: ((bounds.height - side) / 2).rounded(), width: side, height: side)
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .indeterminate: break
        default: return super.applyProp(prop, value)
        }
        return true
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window != nil { control.startAnimation(nil) } else { control.stopAnimation(nil) }
    }
}

final class NSHSegmentedView: NSHControlView {
    var control: NSSegmentedControl { controlView as! NSSegmentedControl }
    override func makeControl() -> NSView { NSSegmentedControl() }
    override func applyTint(_ color: NSColor) { control.selectedSegmentBezelColor = color }

    private var items: [String] = []

    override func commonInit() {
        super.commonInit()
        control.trackingMode = .selectOne
        control.segmentDistribution = .fillEqually
        control.target = self
        control.action = #selector(changed)
    }

    override func layoutControl() {
        let h = control.fittingSize.height
        control.frame = CGRect(x: 0, y: ((bounds.height - h) / 2).rounded(), width: bounds.width, height: h)
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .items:
            items = (value.jsonArray ?? []).map { ($0 as? String) ?? (($0 as? [String: Any])?.string("label") ?? "\($0)") }
            let selected = control.selectedSegment
            control.segmentCount = items.count
            for (i, t) in items.enumerated() { control.setLabel(t, forSegment: i) }
            control.selectedSegment = selected < items.count ? selected : -1
            needsLayout = true
        case .selectedIndex:
            let i = value.int ?? -1
            control.selectedSegment = i >= 0 && i < items.count ? i : -1
        case .enabled: control.isEnabled = value.bool ?? true
        default: return super.applyProp(prop, value)
        }
        return true
    }

    @objc func changed() {
        let i = control.selectedSegment
        guard listens(EventMask.change), i >= 0, i < items.count else { return }
        send(.change, ["index": i, "value": items[i]])
    }
}

final class NSHDatePickerView: NSHControlView {
    var control: NSDatePicker { controlView as! NSDatePicker }
    override func makeControl() -> NSView { NSDatePicker() }

    private var mode = 0
    private var raw: String?

    private func applyDate() {
        if let s = raw, let d = DateFormats.parse(s, mode: mode) { control.dateValue = d } else { control.dateValue = Date() }
    }

    override func commonInit() {
        super.commonInit()
        control.datePickerStyle = .textFieldAndStepper
        control.datePickerElements = .yearMonthDay
        control.isBezeled = true
        control.target = self
        control.action = #selector(changed)
    }

    // Trailing-aligned at its natural size, as a value in a form row.
    override func layoutControl() {
        let s = control.fittingSize
        let w = min(bounds.width, s.width)
        control.frame = CGRect(x: bounds.width - w, y: ((bounds.height - s.height) / 2).rounded(), width: w, height: s.height)
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .date:
            raw = value.string
            applyDate()
        case .dateMode:
            mode = value.int ?? 0
            control.datePickerElements = DatePickerChrome.elements(mode)
            applyDate()
            needsLayout = true
        case .enabled: control.isEnabled = value.bool ?? true
        default: return super.applyProp(prop, value)
        }
        return true
    }

    @objc func changed() {
        if listens(EventMask.change) { send(.change, ["date": DateFormats.format(control.dateValue, mode: mode)]) }
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

/// A pop-up button showing the current choice; its menu lists the items.
final class NSHSelectView: NSHControlView {
    var control: NSPopUpButton { controlView as! NSPopUpButton }
    override func makeControl() -> NSView { NSPopUpButton(frame: .zero, pullsDown: false) }

    private var items: [(label: String, value: Any, destructive: Bool, disabled: Bool)] = []
    private var menuTitle = ""
    private var placeholder = ""
    private var selected = -1

    override func commonInit() {
        super.commonInit()
        control.target = self
        control.action = #selector(picked)
        control.autoenablesItems = false
        refresh()
    }

    // Trailing-aligned at its natural width, as a value in a form row.
    override func layoutControl() {
        let s = control.fittingSize
        let w = min(bounds.width, s.width)
        control.frame = CGRect(x: bounds.width - w, y: ((bounds.height - s.height) / 2).rounded(), width: w, height: s.height)
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
            // The pop-up keeps the system control font and colors.
            break
        default: return super.applyProp(prop, value)
        }
        refresh()
        return true
    }

    private func refresh() {
        let menu = NSMenu(title: menuTitle)
        menu.autoenablesItems = false
        // With no choice yet, a hidden item shows the placeholder as the button title.
        if selected < 0 || selected >= items.count {
            let p = NSMenuItem(title: placeholder, action: nil, keyEquivalent: "")
            p.isHidden = true
            menu.addItem(p)
        }
        for (i, item) in items.enumerated() {
            let mi = NSMenuItem(title: item.label, action: nil, keyEquivalent: "")
            mi.tag = i
            mi.isEnabled = !item.disabled
            if item.destructive {
                mi.attributedTitle = NSAttributedString(string: item.label, attributes: [.foregroundColor: NSColor.systemRed])
            }
            menu.addItem(mi)
        }
        control.menu = menu
        if selected >= 0, selected < items.count { control.selectItem(withTag: selected) } else { control.selectItem(at: 0) }
        needsLayout = true
    }

    @objc private func picked() {
        guard let i = control.selectedItem?.tag else { return }
        choose(i)
    }

    /// Also the test entry point: what choosing menu item `i` does.
    func choose(_ i: Int) {
        guard i >= 0, i < items.count else { return }
        selected = i
        refresh()
        if listens(EventMask.change) { send(.change, ["index": i, "value": items[i].value]) }
    }

    /// The title the button shows (tests).
    var title: String { control.titleOfSelectedItem ?? "" }
}

/// Control chrome measured on the main thread once, so `measure_component`
/// (on the JS thread) can size native controls from font metrics alone.
enum ControlMetrics {
    private static let lock = NSLock()
    private static var popUp = (chrome: CGFloat(30), height: CGFloat(22))
    private static var datePicker = (chrome: CGFloat(30), height: CGFloat(22))

    /// Main thread.
    static func calibrate() {
        let font = NSFont.systemFont(ofSize: NSFont.systemFontSize)
        let b = NSPopUpButton(frame: .zero, pullsDown: false)
        b.addItem(withTitle: "Weekly")
        let text = ("Weekly" as NSString).size(withAttributes: [.font: font]).width
        let p = NSDatePicker()
        p.datePickerStyle = .textFieldAndStepper
        p.datePickerElements = .yearMonthDay
        p.dateValue = Date(timeIntervalSince1970: 0)
        let dateText = (DatePickerChrome.text(p.dateValue, mode: 0) as NSString).size(withAttributes: [.font: font]).width
        lock.lock()
        popUp = (max(0, b.fittingSize.width - text), b.fittingSize.height)
        datePicker = (max(0, p.fittingSize.width - dateText), p.fittingSize.height)
        lock.unlock()
    }

    static func popUpChrome() -> (chrome: CGFloat, height: CGFloat) {
        lock.lock(); defer { lock.unlock() }
        return popUp
    }

    static func datePickerChrome() -> (chrome: CGFloat, height: CGFloat) {
        lock.lock(); defer { lock.unlock() }
        return datePicker
    }
}

enum SelectChrome {
    /// Form rows keep at least this height around the control.
    static let minHeight: CGFloat = 28

    static func size(label: String) -> CGSize {
        let f = NSFont.systemFont(ofSize: NSFont.systemFontSize)
        let text = (label as NSString).size(withAttributes: [.font: f])
        let c = ControlMetrics.popUpChrome()
        return CGSize(width: ceil(text.width + c.chrome), height: max(minHeight, c.height))
    }
}

/// The text field and stepper NSDatePicker for `nsh/date-picker`.
enum DatePickerChrome {
    static func elements(_ mode: Int) -> NSDatePicker.ElementFlags {
        mode == 1 ? .hourMinute : (mode == 2 ? [.yearMonthDay, .hourMinute] : .yearMonthDay)
    }

    /// The text the picker shows: the system short formats.
    static func text(_ d: Date, mode: Int) -> String {
        switch mode {
        case 1: return DateFormatter.localizedString(from: d, dateStyle: .none, timeStyle: .short)
        case 2: return DateFormatter.localizedString(from: d, dateStyle: .short, timeStyle: .short)
        default: return DateFormatter.localizedString(from: d, dateStyle: .short, timeStyle: .none)
        }
    }

    static func size(value: String?, mode: Int) -> CGSize {
        let f = NSFont.systemFont(ofSize: NSFont.systemFontSize)
        let d = value.flatMap { DateFormats.parse($0, mode: mode) } ?? Date()
        // Digits differ in width per value: measure a wide sample of the same format too.
        let sample = DateFormats.parse("2028-12-28T20:48", mode: 2) ?? d
        let w = max((text(d, mode: mode) as NSString).size(withAttributes: [.font: f]).width,
                    (text(sample, mode: mode) as NSString).size(withAttributes: [.font: f]).width)
        let c = ControlMetrics.datePickerChrome()
        return CGSize(width: ceil(w + c.chrome + 4), height: max(SelectChrome.minHeight, c.height))
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
