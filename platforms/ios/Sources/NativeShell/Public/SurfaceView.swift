// The view an instance renders into. It owns the mounting manager for node 0,
// applies batches once per frame (the first one immediately), and keeps the
// core's Environment in sync with size, safe area, keyboard, appearance and
// content size category.

import UIKit

@objcMembers
public class NSHSurfaceView: UIView, SurfacePresenter, RootCommandHandler {
    public private(set) weak var instance: NSHInstance?
    let root: NSHComponentView
    let manager: MountingManager
    private var displayLink: CADisplayLink?
    private var keyboardHeight: CGFloat = 0
    /// The keyboard's end frame in screen coordinates while it is shown.
    private var keyboardFrame: CGRect?
    /// The view on top that the keyboard covers: a presented sheet, else the page
    /// stack. Set by NSHViewController; defaults to this view.
    var keyboardReference: (() -> UIView?)?
    private var lastEnvironment: [String: Any]?
    /// Pages, sheets and dialogs when no view controller presents them.
    private var overlays: [ObjectIdentifier: UIView] = [:]
    /// Overrides where environment safe areas come from (the view controller's).
    weak var safeAreaSource: UIView?
    /// Test hook: receives events instead of the instance.
    var eventSink: ((UInt32, EventType, String, UInt32) -> Void)?

    public init(runtime: NSHRuntime = .shared) {
        root = NSHComponentView(frame: .zero)
        root.viewType = .view
        manager = MountingManager(runtime: runtime, rootView: root)
        super.init(frame: .zero)
        addSubview(root)
        manager.presenter = self
        manager.rootCommands = self
        manager.eventSink = { [weak self] node, type, payload, seq in
            guard let self = self else { return }
            if let sink = self.eventSink { return sink(node, type, payload, seq) }
            self.instance?.dispatchEvent(node: node, type: type, payload: payload, seq: seq)
        }
        manager.logger = { [weak self] level, message in
            if let i = self?.instance { i.reportLog(level, message) } else { runtime.log(level, message) }
        }
        let nc = NotificationCenter.default
        nc.addObserver(self, selector: #selector(keyboardChanged(_:)), name: UIResponder.keyboardWillChangeFrameNotification, object: nil)
        nc.addObserver(self, selector: #selector(keyboardHidden(_:)), name: UIResponder.keyboardWillHideNotification, object: nil)
        nc.addObserver(self, selector: #selector(contentSizeChanged), name: UIContentSizeCategory.didChangeNotification, object: nil)
    }

    public convenience override init(frame: CGRect) {
        self.init(runtime: .shared)
        self.frame = frame
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    deinit {
        displayLink?.invalidate()
    }

    /// Renders `instance` here. An instance renders into one surface at a time.
    public func attach(_ instance: NSHInstance) {
        FontScale.update(traitCollection)
        self.instance?.surface = nil
        self.instance = instance
        instance.surface = self
        lastEnvironment = nil
        sendEnvironment()
        if instance.batchPending { scheduleBatches() }
    }

    public func detach() {
        instance?.surface = nil
        instance = nil
        displayLink?.invalidate()
        displayLink = nil
    }

    /// Number of native views currently mounted (diagnostics, tests).
    public var mountedViewCount: Int { manager.nodeCount }

    // MARK: Batches

    func scheduleBatches() {
        // The first batch is applied right away so the first frame is complete.
        if manager.appliedBatches == 0 {
            drain()
            return
        }
        if displayLink == nil {
            let link = CADisplayLink(target: DisplayLinkProxy(self), selector: #selector(DisplayLinkProxy.tick))
            link.add(to: .main, forMode: .common)
            displayLink = link
        }
        displayLink?.isPaused = false
    }

    func drain() {
        displayLink?.isPaused = true
        instance?.drainBatches { manager.apply($0) }
    }

    /// Applies a batch buffer directly (tests and tools).
    @discardableResult
    func apply(_ data: Data) -> Bool { manager.apply(data) }

    // MARK: Keys

    /// The surface takes key presses when nothing inside it has focus (app-wide bindings).
    public override var canBecomeFirstResponder: Bool { true }

    public override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        KeyEvents.route(presses, phase: "down", to: root, isRoot: true)
        super.pressesBegan(presses, with: event)
    }

    public override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        KeyEvents.route(presses, phase: "up", to: root, isRoot: true)
        super.pressesEnded(presses, with: event)
    }

    // MARK: Environment

    public override func layoutSubviews() {
        super.layoutSubviews()
        root.bounds = CGRect(origin: .zero, size: bounds.size)
        root.center = CGPoint(x: bounds.midX, y: bounds.midY)
        for (_, v) in overlays where v.superview === self { v.frame = bounds }
        sendEnvironment()
    }

    public override func safeAreaInsetsDidChange() {
        super.safeAreaInsetsDidChange()
        sendEnvironment()
    }

    public override func traitCollectionDidChange(_ previous: UITraitCollection?) {
        super.traitCollectionDidChange(previous)
        if previous?.userInterfaceStyle != traitCollection.userInterfaceStyle
            || previous?.preferredContentSizeCategory != traitCollection.preferredContentSizeCategory {
            FontScale.update(traitCollection)
            sendEnvironment()
        }
    }

    @objc private func contentSizeChanged() {
        FontScale.update(traitCollection)
        sendEnvironment()
    }

    @objc private func keyboardChanged(_ n: Notification) {
        guard let end = (n.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? NSValue)?.cgRectValue else { return }
        keyboardFrame = end
        updateKeyboard()
    }

    @objc private func keyboardHidden(_ n: Notification) {
        keyboardFrame = nil
        updateKeyboard()
    }

    /// Recomputes how much of the visible top surface the keyboard covers. This view
    /// itself may be off screen (under a pushed page), so the overlap is measured on
    /// the reference view; also called when pages or sheets change.
    func updateKeyboard() {
        var height: CGFloat = 0
        if let kb = keyboardFrame {
            let ref = keyboardReference?() ?? (safeAreaSource ?? self)
            if let window = ref.window {
                let local = ref.convert(kb, from: window.screen.coordinateSpace)
                height = min(ref.bounds.height, max(0, ref.bounds.maxY - local.minY))
            } else {
                height = max(0, UIScreen.main.bounds.maxY - kb.minY)
            }
        }
        guard height != keyboardHeight else { return }
        keyboardHeight = height
        sendEnvironment()
    }

    /// The Environment the surface currently reports.
    public var environment: [String: Any] {
        let source = safeAreaSource ?? self
        return Environment.current(size: bounds.size, safeArea: source.safeAreaInsets, traits: traitCollection, keyboard: keyboardHeight)
    }

    func sendEnvironment() {
        guard let instance = instance, bounds.width > 0, bounds.height > 0 else { return }
        let env = environment
        if let last = lastEnvironment, NSDictionary(dictionary: last).isEqual(to: env) { return }
        lastEnvironment = env
        instance.setEnvironment(env)
    }

    // MARK: Surfaces (no view controller)

    func attachSurface(_ view: NSHComponentView, at index: Int, in manager: MountingManager) {
        let container = OverlayContainer(surface: view)
        container.frame = bounds
        container.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        overlays[ObjectIdentifier(view)] = container
        addSubview(container)
    }

    func detachSurface(_ view: NSHComponentView, in manager: MountingManager) {
        if let c = overlays.removeValue(forKey: ObjectIdentifier(view)) {
            c.removeFromSuperview()
        }
        view.removeFromSuperview()
    }

    func surfacesDidChange(in manager: MountingManager, animatedPages: Bool) {
        for (_, c) in overlays {
            guard let c = c as? OverlayContainer else { continue }
            c.isHidden = (c.surface as? NSHPageView)?.dismissing ?? false
            c.refresh(in: self)
        }
    }

    // MARK: Root commands

    private(set) lazy var toasts = ToastCenter(host: self)

    func showToast(_ payload: [String: Any]) { toasts.show(payload) }
    func dismissToast(_ payload: [String: Any]) { toasts.dismiss(id: payload["id"].map { "\($0)" }) }

    /// Bottom inset toasts keep clear of (safe area and keyboard).
    var toastInset: CGFloat { (safeAreaSource ?? self).safeAreaInsets.bottom + keyboardHeight }
}

/// Breaks the retain cycle CADisplayLink creates with its target.
private final class DisplayLinkProxy {
    weak var surface: NSHSurfaceView?
    init(_ s: NSHSurfaceView) { surface = s }
    @objc func tick() { surface?.drain() }
}

/// A page, sheet or dialog shown inside the surface view itself.
final class OverlayContainer: UIView {
    let surface: NSHComponentView
    private let backdrop = UIControl()

    init(surface: NSHComponentView) {
        self.surface = surface
        super.init(frame: .zero)
        if !(surface is NSHPageView) {
            backdrop.backgroundColor = UIColor.black.withAlphaComponent(0.35)
            backdrop.addTarget(self, action: #selector(backdropTapped), for: .touchUpInside)
            addSubview(backdrop)
        } else {
            backgroundColor = .systemBackground
        }
        addSubview(surface)
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    @objc private func backdropTapped() {
        guard let s = surface as? NSHSurfaceNodeView, s.dismissable else { return }
        s.userDismissed()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        backdrop.frame = bounds
        position()
    }

    func refresh(in host: UIView) {
        position()
        if let d = surface as? NSHDialogView, let spec = d.alertSpec, d.window != nil || superview != nil {
            AlertPresenter.present(spec, for: d, from: host)
        }
    }

    private func position() {
        let size = surface.nodeFrame.size
        if surface is NSHSheetView {
            surface.applyFrame(CGRect(x: 0, y: bounds.height - size.height, width: bounds.width, height: size.height))
        } else if surface is NSHDialogView {
            surface.applyFrame(CGRect(x: (bounds.width - size.width) / 2, y: (bounds.height - size.height) / 2, width: size.width, height: size.height))
        }
    }
}

enum Environment {
    static func current(size: CGSize, safeArea: UIEdgeInsets, traits: UITraitCollection, keyboard: CGFloat) -> [String: Any] {
        let fontScale = FontScale.factor(for: traits)
        return [
            "width": Double(size.width),
            "height": Double(size.height),
            "scale": Double(traits.displayScale > 0 ? traits.displayScale : TextMeasurer.scale),
            "font_scale": Double(fontScale),
            "safe_area": [safeArea.top, safeArea.right, safeArea.bottom, safeArea.left].map { Double($0) },
            "keyboard_height": Double(keyboard),
            "appearance": traits.userInterfaceStyle == .dark ? "dark" : "light",
            "platform": "ios",
            "locale": Locale.preferredLanguages.first ?? "en",
        ]
    }
}
