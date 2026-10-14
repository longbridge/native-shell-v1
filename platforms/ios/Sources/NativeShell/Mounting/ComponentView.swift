// The base of every native view the core creates. It implements the `View`
// view type itself (background, corners, borders, shadow, gradient, transform,
// press feedback, gestures, accessibility) and the child bookkeeping all types
// share; subclasses add their own props on top.
//
// Reset contract: every prop handled here restores its default on `.null`.
// `prepareForReuse` replays `.null` for every prop the view has seen, so a
// pooled view comes back exactly as a fresh one.

import NativeShellShared
import UIKit

/// Where a component sends events and asks for renderer services.
protocol ComponentContext: AnyObject {
    func send(_ node: UInt32, _ type: EventType, _ payload: [String: Any])
    var runtime: NSHRuntime { get }
    /// Node views by id, for lists that host their rows.
    func component(_ node: UInt32) -> NSHComponentView?
    /// Asks for `batchDidEnd` on `view` at the end of the current batch.
    func needsBatchEnd(_ view: NSHComponentView)
    /// The instance's resolved theme tokens.
    var theme: ThemeState { get }
}

@objc(NSHComponentView)
open class NSHComponentView: UIView, UIGestureRecognizerDelegate {
    var nodeId: UInt32 = 0
    var viewType: NSHViewType = .view
    weak var context: ComponentContext?
    private(set) var eventMask: UInt32 = 0
    /// Native children in order, as the core sees them.
    private(set) var children: [NSHComponentView] = []
    weak var parentComponent: NSHComponentView?
    /// Props set since creation or the last reset, replayed as `.null` on reuse.
    private var seenProps = Set<UInt16>()
    /// Declared transitions (`Prop.animation`).
    var animation: [String: Any]?
    /// The frame the core gave, kept independent of transforms.
    private(set) var nodeFrame: CGRect = .zero

    required override public init(frame: CGRect) {
        super.init(frame: frame)
        commonInit()
    }

    required public init?(coder: NSCoder) { fatalError("not supported") }

    /// Subclasses set up their subviews here.
    func commonInit() {}

    /// The view children are added to.
    var childContainer: UIView { self }

    // MARK: Children

    func insertChild(_ child: NSHComponentView, at index: Int) {
        let i = max(0, min(index, children.count))
        child.parentComponent = self
        children.insert(child, at: i)
        mountChildView(child, at: i)
    }

    func removeChild(_ child: NSHComponentView) {
        guard let i = children.firstIndex(where: { $0 === child }) else { return }
        children.remove(at: i)
        child.parentComponent = nil
        unmountChildView(child)
    }

    func moveChild(_ child: NSHComponentView, to index: Int) {
        guard let from = children.firstIndex(where: { $0 === child }) else { return }
        children.remove(at: from)
        let to = max(0, min(index, children.count))
        children.insert(child, at: to)
        unmountChildView(child)
        mountChildView(child, at: to)
    }

    /// Places a child's view by its neighbours, never by absolute subview index,
    /// so internal subviews and layers never shift the order.
    func mountChildView(_ child: NSHComponentView, at index: Int) {
        let container = childContainer
        if index + 1 < children.count, children[index + 1].superview === container {
            container.insertSubview(child, belowSubview: children[index + 1])
        } else if index > 0, children[index - 1].superview === container {
            container.insertSubview(child, aboveSubview: children[index - 1])
        } else {
            container.addSubview(child)
        }
    }

    func unmountChildView(_ child: NSHComponentView) {
        child.removeFromSuperview()
    }

    // MARK: Frame

    func applyFrame(_ rect: CGRect) {
        nodeFrame = rect
        // bounds + center keep the frame meaningful while a transform is set.
        let size = CGSize(width: max(0, rect.width), height: max(0, rect.height))
        if bounds.size != size { bounds = CGRect(origin: bounds.origin, size: size) }
        center = CGPoint(x: rect.midX, y: rect.midY)
    }

    /// A theme token as a dynamic color (defaults, overridden by the host's theme provider).
    func themeColor(_ token: String) -> UIColor {
        ThemeTokens.color(token, runtime: context?.runtime ?? .shared, theme: context?.theme)
    }

    /// The theme tokens changed: views that draw with them refresh.
    func themeDidChange() {}

    /// Called after a batch touched this node, once per batch.
    func batchDidEnd() {}

    /// Surfaces are not pooled: they may still be animating out when deleted.
    var isPoolable: Bool { !isSurface }

    /// The node is being deleted (before it is reset and pooled).
    func willDestroy() {}

    func runCommand(_ command: Command, _ payload: [String: Any]) {}

    override open func layoutSubviews() {
        super.layoutSubviews()
        layoutDecorations()
    }

    // MARK: Events

    func setEventMask(_ mask: UInt32) {
        eventMask = mask
        updateGestures()
    }

    func listens(_ bit: UInt32) -> Bool { eventMask & bit != 0 }

    func send(_ type: EventType, _ payload: [String: Any] = [:]) {
        context?.send(nodeId, type, payload)
    }

    // MARK: Props

    /// Applies one prop. Returns false when this view type does not know it.
    @discardableResult
    func setProp(_ prop: Prop, _ value: PropValue) -> Bool {
        if case .null = value { seenProps.remove(prop.rawValue) } else { seenProps.insert(prop.rawValue) }
        return applyProp(prop, value)
    }

    /// Subclasses override, handle their own props and fall back to `super`.
    func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .backgroundColor:
            baseBackground = value.uiColor
            updateBackground()
        case .cornerRadii:
            let a = value.floats ?? []
            radii = a.count == 4 ? a.map { CGFloat($0) } : [0, 0, 0, 0]
            setNeedsDecorations()
        case .borderWidths:
            let a = value.floats ?? []
            borderWidths = a.count == 4 ? a.map { CGFloat($0) } : (a.count == 1 ? Array(repeating: CGFloat(a[0]), count: 4) : [0, 0, 0, 0])
            setNeedsDecorations()
        case .borderColor:
            borderColor = value.uiColor
            setNeedsDecorations()
        case .opacity:
            baseAlpha = value.number.map { CGFloat($0) } ?? 1
            alpha = baseAlpha
        case .shadow:
            shadow = value.jsonObject
            setNeedsDecorations()
        case .gradient:
            gradient = value.jsonObject
            setNeedsDecorations()
        case .clipsToBounds:
            clipsToBounds = value.bool ?? false
            setNeedsDecorations()
        case .hidden:
            isHidden = value.bool ?? false
        case .transform:
            applyTransform(value.jsonObject)
        case .animation:
            animation = value.jsonObject
        case .a11yLabel:
            accessibilityLabel = value.string
            if accessibilityTarget !== self { accessibilityTarget.accessibilityLabel = value.string }
            updateAccessibilityElement()
        case .a11yRole:
            role = value.int.flatMap { A11yRole(rawValue: Int32($0)) } ?? .none
            updateAccessibilityTraits()
        case .a11yHint:
            accessibilityHint = value.string
            if accessibilityTarget !== self { accessibilityTarget.accessibilityHint = value.string }
        case .a11yValue:
            accessibilityValue = value.string
        case .a11yState:
            a11yState = value.jsonObject ?? [:]
            updateAccessibilityTraits()
        case .testId:
            accessibilityIdentifier = value.string
            if accessibilityTarget !== self { accessibilityTarget.accessibilityIdentifier = value.string }
        case .a11yHidden:
            accessibilityElementsHidden = value.bool ?? false
        case .activeOpacity:
            activeOpacity = value.number.map { CGFloat($0) }
            updateGestures()
        case .activeBackground:
            activeBackground = value.uiColor
            updateGestures()
        case .hitSlop:
            let a = value.floats ?? []
            hitSlop = a.count == 4 ? UIEdgeInsets(top: CGFloat(a[0]), left: CGFloat(a[3]), bottom: CGFloat(a[2]), right: CGFloat(a[1])) : .zero
        case .focusable:
            break
        default:
            return false
        }
        return true
    }

    func prepareForReuse() {
        for raw in seenProps {
            if let p = Prop(rawValue: raw) { setProp(p, .null) }
        }
        seenProps.removeAll()
        setEventMask(0)
        animation = nil
        pressed = false
        transform = .identity
        alpha = 1
        applyFrame(.zero)
    }

    // MARK: Visuals

    private var baseBackground: UIColor?
    private var baseAlpha: CGFloat = 1
    private var radii: [CGFloat] = [0, 0, 0, 0]
    private var borderWidths: [CGFloat] = [0, 0, 0, 0]
    private var borderColor: UIColor?
    private var shadow: [String: Any]?
    private var gradient: [String: Any]?
    private var gradientLayer: CAGradientLayer?
    private var backgroundShape: CAShapeLayer?
    private var borderShape: CAShapeLayer?
    private var activeOpacity: CGFloat?
    private var activeBackground: UIColor?
    private var hitSlop: UIEdgeInsets = .zero
    private var role: A11yRole = .none
    private var a11yState: [String: Any] = [:]

    private var uniformRadius: CGFloat? {
        radii.allSatisfy({ $0 == radii[0] }) ? radii[0] : nil
    }

    private var currentBackground: UIColor? { pressed && activeBackground != nil ? activeBackground : baseBackground }

    private func updateBackground() {
        if uniformRadius != nil {
            layer.backgroundColor = currentBackground?.cgColor
            backgroundShape?.fillColor = nil
        } else {
            layer.backgroundColor = nil
            setNeedsDecorations()
        }
    }

    private func setNeedsDecorations() {
        setNeedsLayout()
        layoutDecorations()
    }

    /// Corners, borders, shadow and gradient depend on the bounds; redone on every layout.
    func layoutDecorations() {
        let b = bounds
        let caps = CATransaction.disableActions()
        CATransaction.setDisableActions(true)
        defer { CATransaction.setDisableActions(caps) }

        // The same clamped radii shape the fill, border, clip, gradient and shadow.
        let r = Paths.clampedRadii(radii, in: b.size)
        let uniform = uniformRadius != nil
        let path = uniform ? nil : Paths.roundedRect(b, radii: r)
        // iOS look: continuous corners, except circles and capsules, which stay exact.
        if #available(iOS 13.0, *) { layer.cornerCurve = Paths.isCapsule(r, in: b.size) ? .circular : .continuous }

        if uniform {
            layer.cornerRadius = r[0]
            backgroundShape?.removeFromSuperlayer()
            backgroundShape = nil
            layer.backgroundColor = currentBackground?.cgColor
        } else {
            layer.cornerRadius = 0
            let shape = backgroundShape ?? {
                let s = CAShapeLayer()
                layer.insertSublayer(s, at: 0)
                backgroundShape = s
                return s
            }()
            shape.frame = b
            shape.path = path
            shape.fillColor = currentBackground?.cgColor
            layer.backgroundColor = nil
        }

        // Borders: uniform ones on the layer; otherwise a ring between the border box and
        // the padding box (inner radii shrink by the adjacent widths, as in CSS).
        let widths = borderWidths
        let uniformWidth = widths.allSatisfy { $0 == widths[0] }
        borderShape?.removeFromSuperlayer()
        borderShape = nil
        if uniform && uniformWidth {
            layer.borderWidth = widths[0]
            layer.borderColor = (borderColor ?? .clear).cgColor
        } else {
            layer.borderWidth = 0
            if widths.contains(where: { $0 > 0 }) {
                let s = CAShapeLayer()
                s.frame = b
                s.path = Paths.ring(b, radii: r, widths: widths)
                s.fillRule = .evenOdd
                s.fillColor = (borderColor ?? .clear).cgColor
                s.strokeColor = nil
                layer.addSublayer(s)
                borderShape = s
            }
        }

        // Clipping with uneven corners needs a mask (cornerRadius only clips uniformly).
        if clipsToBounds, !uniform {
            let m = (layer.mask as? CAShapeLayer) ?? CAShapeLayer()
            m.frame = b
            m.path = path
            layer.mask = m
        } else if layer.mask is CAShapeLayer {
            layer.mask = nil
        }

        // Gradient.
        if let g = gradient, let stops = g["stops"] as? [[Any]], !stops.isEmpty {
            let gl = gradientLayer ?? {
                let l = CAGradientLayer()
                layer.insertSublayer(l, at: backgroundShape == nil ? 0 : 1)
                gradientLayer = l
                return l
            }()
            gl.frame = b
            gl.colors = stops.compactMap { $0.first.flatMap(Colors.parse)?.cgColor }
            gl.locations = stops.map { NSNumber(value: ($0.count > 1 ? ($0[1] as? NSNumber)?.doubleValue : nil) ?? 0) }
            // CSS convention: 0deg points up, 90deg to the right.
            let angle = (g.double("angle") ?? 180) * .pi / 180
            let dx = sin(angle) / 2, dy = -cos(angle) / 2
            gl.startPoint = CGPoint(x: 0.5 - dx, y: 0.5 - dy)
            gl.endPoint = CGPoint(x: 0.5 + dx, y: 0.5 + dy)
            gl.cornerRadius = uniform ? r[0] : 0
            if !uniform {
                let mask = CAShapeLayer()
                mask.path = path
                gl.mask = mask
            } else {
                gl.mask = nil
            }
        } else {
            gradientLayer?.removeFromSuperlayer()
            gradientLayer = nil
        }

        // Shadow.
        if let s = shadow, let color = s.color("color") {
            var a: CGFloat = 0
            color.getRed(nil, green: nil, blue: nil, alpha: &a)
            layer.shadowColor = color.withAlphaComponent(1).cgColor
            layer.shadowOpacity = Float(a)
            layer.shadowOffset = CGSize(width: s.cgFloat("x") ?? 0, height: s.cgFloat("y") ?? 0)
            layer.shadowRadius = (s.cgFloat("blur") ?? 0) / 2
            let spread = s.cgFloat("spread") ?? 0
            layer.shadowPath = Paths.roundedRect(b.insetBy(dx: -spread, dy: -spread), radii: r.map { $0 > 0 ? $0 + spread : 0 })
        } else if layer.shadowOpacity != 0 || layer.shadowPath != nil {
            layer.shadowOpacity = 0
            layer.shadowPath = nil
            layer.shadowRadius = 3
            layer.shadowOffset = CGSize(width: 0, height: -3)
            layer.shadowColor = UIColor.black.cgColor
        }
    }

    private func applyTransform(_ t: [String: Any]?) {
        guard let t = t else { transform = .identity; return }
        var m = CGAffineTransform.identity
        m = m.translatedBy(x: t.cgFloat("translate_x") ?? 0, y: t.cgFloat("translate_y") ?? 0)
        m = m.rotated(by: (t.cgFloat("rotate") ?? 0) * .pi / 180)
        let s = t.cgFloat("scale") ?? 1
        m = m.scaledBy(x: t.cgFloat("scale_x") ?? s, y: t.cgFloat("scale_y") ?? s)
        transform = m
    }

    // MARK: Accessibility

    /// The view assistive technologies should see. Views hosting a UIKit control
    /// (text fields, switches, sliders…) return the control: making the wrapper an
    /// accessibility element would hide the control and make it unusable.
    var accessibilityTarget: UIView { self }

    private func updateAccessibilityElement() {
        isAccessibilityElement = accessibilityTarget === self && (accessibilityLabel != nil || role != .none)
    }

    private func updateAccessibilityTraits() {
        var t: UIAccessibilityTraits = []
        switch role {
        case .button, .checkbox, .radio, .tab, .switchRole, .menu: t.insert(.button)
        case .link: t.insert(.link)
        case .header: t.insert(.header)
        case .image: t.insert(.image)
        case .text: t.insert(.staticText)
        case .search: t.insert(.searchField)
        case .slider: t.insert(.adjustable)
        case .progressBar: t.insert(.updatesFrequently)
        case .tabList: t.insert(.tabBar)
        default: break
        }
        if a11yState.bool("disabled") == true { t.insert(.notEnabled) }
        if a11yState.bool("selected") == true || a11yState.bool("checked") == true { t.insert(.selected) }
        accessibilityTraits = t
        updateAccessibilityElement()
    }

    // MARK: Keys

    override open func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        KeyEvents.route(presses, phase: "down", to: self)
        super.pressesBegan(presses, with: event)
    }

    override open func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        KeyEvents.route(presses, phase: "up", to: self)
        super.pressesEnded(presses, with: event)
    }

    // MARK: Touch

    /// VoiceOver activation (and UI tests) performs the click the script listens for.
    override open func accessibilityActivate() -> Bool {
        guard listens(EventMask.click) else { return super.accessibilityActivate() }
        send(.click, ["x": Double(bounds.midX), "y": Double(bounds.midY), "count": 1])
        return true
    }

    override open func point(inside point: CGPoint, with event: UIEvent?) -> Bool {
        if hitSlop == .zero { return super.point(inside: point, with: event) }
        let r = bounds.inset(by: UIEdgeInsets(top: -hitSlop.top, left: -hitSlop.left, bottom: -hitSlop.bottom, right: -hitSlop.right))
        return r.contains(point)
    }

    private var tapRecognizer: UITapGestureRecognizer?
    private var doubleTapRecognizer: UITapGestureRecognizer?
    private var longPressRecognizer: UILongPressGestureRecognizer?
    private var panRecognizer: UIPanGestureRecognizer?
    private var pinchRecognizer: UIPinchGestureRecognizer?
    private var pressRecognizer: PressGestureRecognizer?
    private var lastTap: (time: TimeInterval, point: CGPoint, count: Int)?

    var pressed = false {
        didSet {
            guard pressed != oldValue else { return }
            if let o = activeOpacity { alpha = pressed ? baseAlpha * o : baseAlpha }
            if activeBackground != nil { updateBackground(); layoutDecorations() }
        }
    }

    /// Whether gesture recognizers for taps belong on this view (controls handle their own).
    var handlesTapGestures: Bool { true }

    func updateGestures() {
        func toggle<G: UIGestureRecognizer>(_ g: inout G?, _ on: Bool, _ make: () -> G) {
            if on, g == nil {
                let r = make()
                r.delegate = self
                addGestureRecognizer(r)
                g = r
            } else if !on, let r = g {
                removeGestureRecognizer(r)
                g = nil
            }
        }
        let taps = handlesTapGestures
        toggle(&tapRecognizer, taps && listens(EventMask.click)) { UITapGestureRecognizer(target: self, action: #selector(onTap(_:))) }
        toggle(&doubleTapRecognizer, taps && listens(EventMask.doubleClick)) {
            let g = UITapGestureRecognizer(target: self, action: #selector(onDoubleTap(_:)))
            g.numberOfTapsRequired = 2
            return g
        }
        if let single = tapRecognizer, let double = doubleTapRecognizer { single.require(toFail: double) }
        toggle(&longPressRecognizer, taps && listens(EventMask.longPress)) {
            let g = UILongPressGestureRecognizer(target: self, action: #selector(onLongPress(_:)))
            g.minimumPressDuration = 0.5
            return g
        }
        toggle(&panRecognizer, listens(EventMask.pan)) { UIPanGestureRecognizer(target: self, action: #selector(onPan(_:))) }
        toggle(&pinchRecognizer, listens(EventMask.pinch)) { UIPinchGestureRecognizer(target: self, action: #selector(onPinch(_:))) }
        let press = activeOpacity != nil || activeBackground != nil || listens(EventMask.pressIn) || listens(EventMask.pressOut)
        toggle(&pressRecognizer, press) { PressGestureRecognizer(target: self, action: #selector(onPress(_:))) }
    }

    @objc func onTap(_ g: UITapGestureRecognizer) {
        let p = g.location(in: self)
        let now = ProcessInfo.processInfo.systemUptime
        var count = 1
        if let last = lastTap, now - last.time < 0.35, hypot(last.point.x - p.x, last.point.y - p.y) < 24 {
            count = last.count + 1
        }
        lastTap = (now, p, count)
        send(.click, ["x": Double(p.x), "y": Double(p.y), "count": count])
    }

    @objc func onDoubleTap(_ g: UITapGestureRecognizer) {
        let p = g.location(in: self)
        send(.doubleClick, ["x": Double(p.x), "y": Double(p.y), "count": 2])
    }

    @objc func onLongPress(_ g: UILongPressGestureRecognizer) {
        guard g.state == .began else { return }
        let p = g.location(in: self)
        send(.longPress, ["x": Double(p.x), "y": Double(p.y)])
    }

    @objc func onPan(_ g: UIPanGestureRecognizer) {
        guard let phase = Self.phase(g.state) else { return }
        let t = g.translation(in: self), v = g.velocity(in: self)
        send(.pan, ["phase": phase, "dx": Double(t.x), "dy": Double(t.y), "vx": Double(v.x), "vy": Double(v.y)])
    }

    @objc func onPinch(_ g: UIPinchGestureRecognizer) {
        guard let phase = Self.phase(g.state) else { return }
        send(.pinch, ["phase": phase, "scale": Double(g.scale)])
    }

    @objc func onPress(_ g: PressGestureRecognizer) {
        pressChanged(g.state)
    }

    func pressChanged(_ state: UIGestureRecognizer.State) {
        switch state {
        case .began:
            pressed = true
            if listens(EventMask.pressIn) { send(.pressIn) }
        case .ended, .cancelled, .failed:
            if pressed, listens(EventMask.pressOut) { send(.pressOut) }
            pressed = false
        default: break
        }
    }

    static func phase(_ s: UIGestureRecognizer.State) -> String? {
        switch s {
        case .began: return "began"
        case .changed: return "changed"
        case .ended: return "ended"
        case .cancelled, .failed: return "cancelled"
        default: return nil
        }
    }

    public func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        g is PressGestureRecognizer || other is PressGestureRecognizer || g is UIPinchGestureRecognizer
    }
}

/// Tracks a finger down on the view without ever blocking other recognizers:
/// press feedback (`ActiveOpacity`) and press in/out events.
final class PressGestureRecognizer: UIGestureRecognizer {
    private var start: CGPoint = .zero

    override init(target: Any?, action: Selector?) {
        super.init(target: target, action: action)
        cancelsTouchesInView = false
        delaysTouchesBegan = false
        delaysTouchesEnded = false
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        guard touches.count == 1, let t = touches.first, state == .possible else { return }
        start = t.location(in: view)
        state = .began
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        guard let t = touches.first, let v = view else { return }
        let p = t.location(in: v)
        if !v.bounds.insetBy(dx: -16, dy: -16).contains(p) || hypot(p.x - start.x, p.y - start.y) > 40 {
            state = .cancelled
        } else {
            state = .changed
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) { state = .ended }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) { state = .cancelled }
    override func canPrevent(_ preventedGestureRecognizer: UIGestureRecognizer) -> Bool { false }
    override func canBePrevented(by preventingGestureRecognizer: UIGestureRecognizer) -> Bool { false }
}

enum Paths {
    /// CSS corner clamping: when adjacent radii overflow a side, all radii scale
    /// down by the same factor (uniform radii end up at min(r, min(w, h) / 2)).
    static func clampedRadii(_ radii: [CGFloat], in size: CGSize) -> [CGFloat] {
        guard radii.count == 4 else { return [0, 0, 0, 0] }
        let r = radii.map { max(0, $0) }
        var f: CGFloat = 1
        let sides: [(CGFloat, CGFloat)] = [(size.width, r[0] + r[1]), (size.height, r[1] + r[2]), (size.width, r[2] + r[3]), (size.height, r[3] + r[0])]
        for (side, sum) in sides where sum > 0 { f = min(f, max(0, side) / sum) }
        return r.map { $0 * f }
    }

    /// The border area: border box minus padding box, for an even-odd fill.
    /// `widths` are `[top, right, bottom, left]`.
    static func ring(_ b: CGRect, radii r: [CGFloat], widths w: [CGFloat]) -> CGPath {
        let p = CGMutablePath()
        p.addPath(roundedRect(b, radii: r))
        let inner = CGRect(x: b.minX + w[3], y: b.minY + w[0], width: max(0, b.width - w[1] - w[3]), height: max(0, b.height - w[0] - w[2]))
        let ir = [max(0, r[0] - max(w[3], w[0])), max(0, r[1] - max(w[0], w[1])), max(0, r[2] - max(w[1], w[2])), max(0, r[3] - max(w[2], w[3]))]
        if inner.width > 0, inner.height > 0 { p.addPath(roundedRect(inner, radii: clampedRadii(ir, in: inner.size))) }
        return p
    }

    /// Whether a corner reaches half the shorter side: a circle or a capsule.
    static func isCapsule(_ radii: [CGFloat], in size: CGSize) -> Bool {
        let half = min(size.width, size.height) / 2
        return half > 0 && radii.contains { $0 >= half - 0.01 }
    }

    /// A rect with per-corner radii `[tl, tr, br, bl]`. Corners that share one radius
    /// get UIKit's continuous curve (the iOS look); capsules and mixed radii use
    /// circular arcs.
    static func roundedRect(_ r: CGRect, radii: [CGFloat]) -> CGPath {
        let p = CGMutablePath()
        guard radii.count == 4, radii.contains(where: { $0 > 0 }) else {
            p.addRect(r)
            return p
        }
        let nonZero = radii.filter { $0 > 0 }
        if !isCapsule(radii, in: r.size), nonZero.allSatisfy({ abs($0 - nonZero[0]) < 0.01 }) {
            var corners: UIRectCorner = []
            if radii[0] > 0 { corners.insert(.topLeft) }
            if radii[1] > 0 { corners.insert(.topRight) }
            if radii[2] > 0 { corners.insert(.bottomRight) }
            if radii[3] > 0 { corners.insert(.bottomLeft) }
            let rr = nonZero[0]
            return UIBezierPath(roundedRect: r, byRoundingCorners: corners, cornerRadii: CGSize(width: rr, height: rr)).cgPath
        }
        let (tl, tr, br, bl) = (radii[0], radii[1], radii[2], radii[3])
        p.move(to: CGPoint(x: r.minX + tl, y: r.minY))
        p.addLine(to: CGPoint(x: r.maxX - tr, y: r.minY))
        p.addArc(tangent1End: CGPoint(x: r.maxX, y: r.minY), tangent2End: CGPoint(x: r.maxX, y: r.minY + tr), radius: tr)
        p.addLine(to: CGPoint(x: r.maxX, y: r.maxY - br))
        p.addArc(tangent1End: CGPoint(x: r.maxX, y: r.maxY), tangent2End: CGPoint(x: r.maxX - br, y: r.maxY), radius: br)
        p.addLine(to: CGPoint(x: r.minX + bl, y: r.maxY))
        p.addArc(tangent1End: CGPoint(x: r.minX, y: r.maxY), tangent2End: CGPoint(x: r.minX, y: r.maxY - bl), radius: bl)
        p.addLine(to: CGPoint(x: r.minX, y: r.minY + tl))
        p.addArc(tangent1End: CGPoint(x: r.minX, y: r.minY), tangent2End: CGPoint(x: r.minX + tl, y: r.minY), radius: tl)
        p.closeSubpath()
        return p
    }
}
