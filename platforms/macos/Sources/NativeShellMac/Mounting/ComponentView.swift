// The base of every native view the core creates. It implements the `View`
// view type itself (background, corners, borders, shadow, gradient, transform,
// press feedback, pointer gestures, accessibility) and the child bookkeeping
// all types share; subclasses add their own props on top.
//
// Views are flipped (frames from the core are top-left points) and
// layer-backed; decorations live on the view's layer and its sublayers.
//
// Reset contract: every prop handled here restores its default on `.null`.
// `resetForReuse` replays `.null` for every prop the view has seen, so a
// pooled view comes back exactly as a fresh one.

import AppKit

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
open class NSHComponentView: NSView {
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
        wantsLayer = true
        layerContentsRedrawPolicy = .onSetNeedsDisplay
        commonInit()
    }

    required public init?(coder: NSCoder) { fatalError("not supported") }

    override open var isFlipped: Bool { true }

    /// Subclasses set up their subviews here.
    func commonInit() {}

    /// The view children are added to.
    var childContainer: NSView { self }

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
    /// so internal subviews never shift the order.
    func mountChildView(_ child: NSHComponentView, at index: Int) {
        let container = childContainer
        if index + 1 < children.count, children[index + 1].superview === container {
            container.addSubview(child, positioned: .below, relativeTo: children[index + 1])
        } else if index > 0, children[index - 1].superview === container {
            container.addSubview(child, positioned: .above, relativeTo: children[index - 1])
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
        let r = CGRect(x: rect.minX, y: rect.minY, width: max(0, rect.width), height: max(0, rect.height))
        if frame != r { frame = r }
        applyLayerTransform()
    }

    /// A theme token as a dynamic color (defaults, overridden by the host's theme provider).
    func themeColor(_ token: String) -> NSColor {
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

    override open func layout() {
        super.layout()
        layoutDecorations()
    }

    override open func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        // CGColors on layers do not follow the appearance by themselves.
        layoutDecorations()
    }

    // MARK: Events

    func setEventMask(_ mask: UInt32) {
        eventMask = mask
        updateAccessibilityElement()
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
            baseBackground = value.nsColor
            layoutDecorations()
        case .cornerRadii:
            let a = value.floats ?? []
            radii = a.count == 4 ? a.map { CGFloat($0) } : [0, 0, 0, 0]
            setNeedsDecorations()
        case .borderWidths:
            let a = value.floats ?? []
            borderWidths = a.count == 4 ? a.map { CGFloat($0) } : (a.count == 1 ? Array(repeating: CGFloat(a[0]), count: 4) : [0, 0, 0, 0])
            setNeedsDecorations()
        case .borderColor:
            borderColor = value.nsColor
            setNeedsDecorations()
        case .opacity:
            baseAlpha = value.number.map { CGFloat($0) } ?? 1
            alphaValue = baseAlpha
        case .shadow:
            shadowSpec = value.jsonObject
            setNeedsDecorations()
        case .gradient:
            gradient = value.jsonObject
            setNeedsDecorations()
        case .clipsToBounds:
            clips = value.bool ?? false
            setNeedsDecorations()
        case .hidden:
            isHidden = value.bool ?? false
        case .transform:
            transformSpec = value.jsonObject
            applyLayerTransform()
        case .animation:
            animation = value.jsonObject
        case .a11yLabel:
            accessibilityTarget.setAccessibilityLabel(value.string)
            updateAccessibilityElement()
        case .a11yRole:
            role = value.int.flatMap { A11yRole(rawValue: Int32($0)) } ?? .none
            updateAccessibilityRole()
        case .a11yHint:
            accessibilityTarget.setAccessibilityHelp(value.string)
        case .a11yValue:
            setAccessibilityValue(value.string)
        case .a11yState:
            a11yState = value.jsonObject ?? [:]
            updateAccessibilityRole()
        case .testId:
            setAccessibilityIdentifier(value.string)
            if accessibilityTarget !== self { accessibilityTarget.setAccessibilityIdentifier(value.string) }
        case .a11yHidden:
            a11yHidden = value.bool ?? false
            updateAccessibilityElement()
        case .activeOpacity:
            activeOpacity = value.number.map { CGFloat($0) }
        case .activeBackground:
            activeBackground = value.nsColor
        case .hitSlop, .focusable:
            // A pointer is precise: no slop. Keyboard focus follows the native controls.
            break
        default:
            return false
        }
        return true
    }

    func resetForReuse() {
        for raw in seenProps {
            if let p = Prop(rawValue: raw) { setProp(p, .null) }
        }
        seenProps.removeAll()
        setEventMask(0)
        animation = nil
        endTracking()
        pressed = false
        alphaValue = 1
        applyFrame(.zero)
    }

    // MARK: Visuals

    private var baseBackground: NSColor?
    private var baseAlpha: CGFloat = 1
    private var radii: [CGFloat] = [0, 0, 0, 0]
    private var borderWidths: [CGFloat] = [0, 0, 0, 0]
    private var borderColor: NSColor?
    private var shadowSpec: [String: Any]?
    private var gradient: [String: Any]?
    private var clips = false
    private var transformSpec: [String: Any]?
    private var gradientLayer: CAGradientLayer?
    private var backgroundShape: CAShapeLayer?
    private var borderShape: CAShapeLayer?
    private var activeOpacity: CGFloat?
    private var activeBackground: NSColor?
    private var role: A11yRole = .none
    private var a11yState: [String: Any] = [:]
    private var a11yHidden = false

    private var uniformRadius: CGFloat? {
        radii.allSatisfy({ $0 == radii[0] }) ? radii[0] : nil
    }

    private var currentBackground: NSColor? { pressed && activeBackground != nil ? activeBackground : baseBackground }

    private func cg(_ c: NSColor?) -> CGColor? { c?.cgColor(in: effectiveAppearance) }

    private func setNeedsDecorations() {
        needsLayout = true
        layoutDecorations()
    }

    /// Corners, borders, shadow and gradient depend on the bounds; redone on every layout.
    func layoutDecorations() {
        guard let layer = layer else { return }
        let b = bounds
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }

        // The same clamped radii shape the fill, border, clip, gradient and shadow.
        let r = Paths.clampedRadii(radii, in: b.size)
        let uniform = uniformRadius != nil
        let path = uniform ? nil : Paths.roundedRect(b, radii: r)
        // Continuous corners, except circles and capsules, which stay exact.
        layer.cornerCurve = Paths.isCapsule(r, in: b.size) ? .circular : .continuous

        if uniform {
            layer.cornerRadius = r[0]
            backgroundShape?.removeFromSuperlayer()
            backgroundShape = nil
            layer.backgroundColor = cg(currentBackground)
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
            shape.fillColor = cg(currentBackground)
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
            layer.borderColor = cg(borderColor ?? .clear)
        } else {
            layer.borderWidth = 0
            if widths.contains(where: { $0 > 0 }) {
                let s = CAShapeLayer()
                s.frame = b
                s.path = Paths.ring(b, radii: r, widths: widths)
                s.fillRule = .evenOdd
                s.fillColor = cg(borderColor ?? .clear)
                s.strokeColor = nil
                s.zPosition = 1
                layer.addSublayer(s)
                borderShape = s
            }
        }

        // Clipping: uniform corners clip through the layer; uneven ones need a mask.
        layer.masksToBounds = clips && uniform
        if clips, !uniform {
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
            // CSS convention: 0deg points up, 90deg to the right (the layer is flipped: y grows down).
            let angle = (g.double("angle") ?? 180) * .pi / 180
            let dx = sin(angle) / 2, dy = -cos(angle) / 2
            gl.startPoint = CGPoint(x: 0.5 - dx, y: 0.5 - dy)
            gl.endPoint = CGPoint(x: 0.5 + dx, y: 0.5 + dy)
            gl.cornerRadius = uniform ? r[0] : 0
            gl.cornerCurve = layer.cornerCurve
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
        if let s = shadowSpec, let color = s.color("color") {
            let rgb = color.usingColorSpace(.sRGB) ?? color
            layer.shadowColor = rgb.withAlphaComponent(1).cgColor
            layer.shadowOpacity = Float(rgb.alphaComponent)
            layer.shadowOffset = CGSize(width: s.cgFloat("x") ?? 0, height: s.cgFloat("y") ?? 0)
            layer.shadowRadius = (s.cgFloat("blur") ?? 0) / 2
            let spread = s.cgFloat("spread") ?? 0
            layer.shadowPath = Paths.roundedRect(b.insetBy(dx: -spread, dy: -spread), radii: r.map { $0 > 0 ? $0 + spread : 0 })
            // A layer that clips would clip its own shadow.
            if layer.masksToBounds { layer.masksToBounds = false }
        } else if layer.shadowOpacity != 0 || layer.shadowPath != nil {
            layer.shadowOpacity = 0
            layer.shadowPath = nil
        }
    }

    /// `Transform`, applied around the view's center. AppKit anchors layer-backed
    /// views at their origin, so the center is folded into the matrix.
    private func applyLayerTransform() {
        guard let layer = layer else { return }
        guard let t = transformSpec else {
            if !CATransform3DIsIdentity(layer.transform) { layer.transform = CATransform3DIdentity }
            return
        }
        var m = CGAffineTransform.identity
        m = m.translatedBy(x: t.cgFloat("translate_x") ?? 0, y: t.cgFloat("translate_y") ?? 0)
        m = m.rotated(by: (t.cgFloat("rotate") ?? 0) * .pi / 180)
        let s = t.cgFloat("scale") ?? 1
        m = m.scaledBy(x: t.cgFloat("scale_x") ?? s, y: t.cgFloat("scale_y") ?? s)
        let cx = bounds.width / 2, cy = bounds.height / 2
        let full = CGAffineTransform(translationX: -cx, y: -cy).concatenating(m).concatenating(CGAffineTransform(translationX: cx, y: cy))
        layer.setAffineTransform(full)
    }

    /// The transform the layer currently carries (tests).
    var layerTransform: CGAffineTransform { layer?.affineTransform() ?? .identity }

    // MARK: Accessibility

    /// The view assistive technologies should see. Views hosting an AppKit control
    /// return the control: making the wrapper an element would hide the control.
    var accessibilityTarget: NSView { self }

    func updateAccessibilityElement() {
        guard accessibilityTarget === self else { return }
        let named = accessibilityLabel() != nil || role != .none || listens(EventMask.click)
        setAccessibilityElement(named && !a11yHidden)
    }

    private func updateAccessibilityRole() {
        let r: NSAccessibility.Role?
        var sub: NSAccessibility.Subrole?
        switch role {
        case .button, .menu: r = .button
        case .checkbox: r = .checkBox
        case .switchRole: r = .checkBox; sub = .switch
        case .radio, .tab: r = .radioButton
        case .link: r = .link
        case .header: r = .staticText; sub = NSAccessibility.Subrole(rawValue: "AXHeading")
        case .image: r = .image
        case .text: r = .staticText
        case .textField: r = .textField
        case .search: r = .textField; sub = .searchField
        case .slider: r = .slider
        case .progressBar: r = .progressIndicator
        case .tabList: r = .tabGroup
        case .list: r = .list
        case .listItem, .alert: r = .group
        case .none: r = listens(EventMask.click) ? .button : nil
        }
        if accessibilityTarget === self {
            setAccessibilityRole(r)
            setAccessibilitySubrole(sub)
        }
        accessibilityTarget.setAccessibilityEnabled(a11yState.bool("disabled") != true)
        setAccessibilitySelected(a11yState.bool("selected") == true || a11yState.bool("checked") == true)
        updateAccessibilityElement()
    }

    /// VoiceOver's press (and tests) performs the click the script listens for.
    override open func accessibilityPerformPress() -> Bool {
        guard listens(EventMask.click) else { return super.accessibilityPerformPress() }
        send(.click, ["x": Double(bounds.midX), "y": Double(bounds.midY), "count": 1, "modifiers": KeyEvents.modifiers([])])
        return true
    }

    /// Clickable views without a declared role are buttons.
    override open func accessibilityRole() -> NSAccessibility.Role? {
        let r = super.accessibilityRole()
        if r == nil || r == .unknown, accessibilityTarget === self, listens(EventMask.click) { return .button }
        return r
    }

    // MARK: Pointer

    /// Whether this view tracks clicks itself (controls handle their own mouse).
    var handlesMouse: Bool { true }

    private var wantsMouse: Bool {
        handlesMouse && (eventMask & (EventMask.click | EventMask.doubleClick | EventMask.longPress | EventMask.pan
                | EventMask.pressIn | EventMask.pressOut) != 0 || activeOpacity != nil || activeBackground != nil)
    }

    private struct Tracking {
        var start: CGPoint
        var last: CGPoint
        var lastTime: TimeInterval
        var velocity: CGPoint = .zero
        var panning = false
        var longPressed = false
    }

    private var tracking: Tracking?
    private var longPressTimer: Timer?
    private var magnification: CGFloat = 0

    var pressed = false {
        didSet {
            guard pressed != oldValue else { return }
            if let o = activeOpacity { alphaValue = pressed ? baseAlpha * o : baseAlpha }
            if activeBackground != nil { layoutDecorations() }
        }
    }

    override open func mouseDown(with e: NSEvent) {
        guard wantsMouse else { return super.mouseDown(with: e) }
        // Control-click is the secondary click: the context action.
        if e.modifierFlags.contains(.control), listens(EventMask.longPress) { return secondaryClick(e) }
        let p = convert(e.locationInWindow, from: nil)
        tracking = Tracking(start: p, last: p, lastTime: e.timestamp)
        setPressed(true)
        if listens(EventMask.longPress) {
            longPressTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: false) { [weak self] _ in
                guard let self = self, var t = self.tracking, !t.panning else { return }
                t.longPressed = true
                self.tracking = t
                self.send(.longPress, ["x": Double(t.last.x), "y": Double(t.last.y)])
            }
        }
    }

    override open func mouseDragged(with e: NSEvent) {
        guard var t = tracking else { return super.mouseDragged(with: e) }
        let p = convert(e.locationInWindow, from: nil)
        let dt = max(0.001, e.timestamp - t.lastTime)
        t.velocity = CGPoint(x: (p.x - t.last.x) / dt, y: (p.y - t.last.y) / dt)
        t.last = p
        t.lastTime = e.timestamp
        if hypot(p.x - t.start.x, p.y - t.start.y) > 4 {
            longPressTimer?.invalidate()
            longPressTimer = nil
        }
        if listens(EventMask.pan), !t.longPressed {
            if !t.panning, hypot(p.x - t.start.x, p.y - t.start.y) > 4 {
                t.panning = true
                setPressed(false)
                tracking = t
                sendPan("began", t)
            } else if t.panning {
                tracking = t
                sendPan("changed", t)
            }
        }
        tracking = t
        // As AppKit buttons do: the pressed look follows the pointer in and out.
        if !t.panning { pressed = bounds.contains(p) }
    }

    override open func mouseUp(with e: NSEvent) {
        guard let t = tracking else { return super.mouseUp(with: e) }
        let p = convert(e.locationInWindow, from: nil)
        endTracking()
        if t.panning { return sendPan("ended", t) }
        setPressed(false)
        guard !t.longPressed, bounds.contains(p) else { return }
        let mods = KeyEvents.modifiers(e.modifierFlags)
        if e.clickCount == 2, listens(EventMask.doubleClick) {
            send(.doubleClick, ["x": Double(p.x), "y": Double(p.y), "count": 2])
        } else if listens(EventMask.click) {
            send(.click, ["x": Double(p.x), "y": Double(p.y), "count": max(1, e.clickCount), "modifiers": mods])
        }
    }

    override open func rightMouseDown(with e: NSEvent) {
        guard handlesMouse, listens(EventMask.longPress) else { return super.rightMouseDown(with: e) }
        secondaryClick(e)
    }

    /// A secondary click is the pointer's long press: both open a context action.
    private func secondaryClick(_ e: NSEvent) {
        let p = convert(e.locationInWindow, from: nil)
        send(.longPress, ["x": Double(p.x), "y": Double(p.y)])
    }

    override open func magnify(with e: NSEvent) {
        guard listens(EventMask.pinch) else { return super.magnify(with: e) }
        let phase: String
        switch e.phase {
        case .began: phase = "began"; magnification = 0
        case .ended: phase = "ended"
        case .cancelled: phase = "cancelled"
        default: phase = "changed"
        }
        magnification += e.magnification
        send(.pinch, ["phase": phase, "scale": Double(max(0.01, 1 + magnification))])
    }

    private func sendPan(_ phase: String, _ t: Tracking) {
        send(.pan, ["phase": phase, "dx": Double(t.last.x - t.start.x), "dy": Double(t.last.y - t.start.y),
                    "vx": Double(t.velocity.x), "vy": Double(t.velocity.y)])
    }

    private func setPressed(_ on: Bool) {
        if on {
            pressed = true
            if listens(EventMask.pressIn) { send(.pressIn) }
        } else {
            if pressed || tracking == nil, listens(EventMask.pressOut) { send(.pressOut) }
            pressed = false
        }
    }

    private func endTracking() {
        longPressTimer?.invalidate()
        longPressTimer = nil
        tracking = nil
    }
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

    /// A rect with per-corner radii `[tl, tr, br, bl]` (circular arcs).
    static func roundedRect(_ r: CGRect, radii: [CGFloat]) -> CGPath {
        let p = CGMutablePath()
        guard radii.count == 4, radii.contains(where: { $0 > 0 }) else {
            p.addRect(r)
            return p
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
