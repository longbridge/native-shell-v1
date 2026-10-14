// The view an instance renders into. It owns the mounting manager for node 0,
// applies batches as they arrive (the first one immediately, later ones
// coalesced on the main queue), routes hardware keys, and keeps the core's
// Environment in sync with size, safe area, backing scale and appearance.

import AppKit
import NativeShellShared

@objcMembers
public class NSHSurfaceView: NSView, SurfacePresenter, RootCommandHandler {
    public private(set) weak var instance: NSHInstance?
    let root: NSHComponentView
    let manager: MountingManager
    private var drainScheduled = false
    private var lastEnvironment: [String: Any]?
    /// Pages, sheets and dialogs when no view controller presents them.
    private var overlays: [ObjectIdentifier: OverlayContainer] = [:]
    /// Overrides where environment safe areas come from (the view controller's view).
    weak var safeAreaSource: NSView?
    /// Test hook: receives events instead of the instance.
    var eventSink: ((UInt32, EventType, String, UInt32) -> Void)?
    private var keyMonitor: Any?

    public init(runtime: NSHRuntime = .shared) {
        root = NSHComponentView(frame: .zero)
        root.viewType = .view
        manager = MountingManager(runtime: runtime, rootView: root)
        super.init(frame: .zero)
        wantsLayer = true
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
    }

    public convenience override init(frame: CGRect) {
        self.init(runtime: .shared)
        self.frame = frame
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    deinit {
        if let m = keyMonitor { NSEvent.removeMonitor(m) }
    }

    override public var isFlipped: Bool { true }

    /// Renders `instance` here. An instance renders into one surface at a time.
    public func attach(_ instance: NSHInstance) {
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
        guard !drainScheduled else { return }
        drainScheduled = true
        // The main queue also runs during live resize and scroll tracking.
        DispatchQueue.main.async { [weak self] in self?.drain() }
    }

    func drain() {
        drainScheduled = false
        instance?.drainBatches { manager.apply($0) }
    }

    /// Applies a batch buffer directly (tests and tools).
    @discardableResult
    func apply(_ data: Data) -> Bool { manager.apply(data) }

    // MARK: Keys

    override public func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let m = keyMonitor {
            NSEvent.removeMonitor(m)
            keyMonitor = nil
        }
        guard window != nil else { return }
        // A local monitor sees every key before the responder chain does, text
        // fields included, so each key is reported once and still reaches AppKit.
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .keyUp]) { [weak self] e in
            self?.route(e)
            return e
        }
        sendEnvironment()
    }

    /// Sends a key event to its node; true when it belonged to this surface.
    @discardableResult
    func route(_ e: NSEvent) -> Bool {
        guard let w = e.window, let mine = window, w === mine || w.sheetParent === mine || mine.sheets.contains(w) else { return false }
        guard let target = KeyEvents.target(for: w.firstResponder, in: manager, window: w), let payload = KeyEvents.payload(e) else { return false }
        target.send(.key, payload)
        return true
    }

    // MARK: Environment

    override public func layout() {
        super.layout()
        root.applyFrame(CGRect(origin: .zero, size: bounds.size))
        for (_, v) in overlays where v.superview === self { v.frame = bounds }
        sendEnvironment()
    }

    override public func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        sendEnvironment()
    }

    override public func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        if let s = window?.backingScaleFactor { TextMeasurer.scale = s }
        sendEnvironment()
    }

    /// The Environment the surface currently reports.
    public var environment: [String: Any] {
        let source = safeAreaSource ?? self
        return Environment.current(size: bounds.size, safeArea: source.safeAreaInsets, appearance: effectiveAppearance,
                                   scale: window?.backingScaleFactor ?? TextMeasurer.scale)
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
        container.autoresizingMask = [.width, .height]
        overlays[ObjectIdentifier(view)] = container
        addSubview(container)
    }

    func detachSurface(_ view: NSHComponentView, in manager: MountingManager) {
        overlays.removeValue(forKey: ObjectIdentifier(view))?.removeFromSuperview()
        view.removeFromSuperview()
    }

    func surfacesDidChange(in manager: MountingManager, animatedPages: Bool) {
        for (_, c) in overlays {
            c.isHidden = (c.surface as? NSHPageView)?.dismissing ?? false
            c.refresh(in: self)
        }
    }

    // MARK: Root commands

    private(set) lazy var toasts = ToastCenter(host: self)

    func showToast(_ payload: [String: Any]) { toasts.show(payload) }
    func themeDidChange() { toasts.themeDidChange() }
    func dismissToast(_ payload: [String: Any]) { toasts.dismiss(id: payload["id"].map { "\($0)" }) }

    /// Bottom inset toasts keep clear of.
    var toastInset: CGFloat { (safeAreaSource ?? self).safeAreaInsets.bottom }
}

/// A page, sheet or dialog shown inside the surface view itself.
final class OverlayContainer: NSView {
    let surface: NSHComponentView
    private let backdrop = BackdropView()

    init(surface: NSHComponentView) {
        self.surface = surface
        super.init(frame: .zero)
        wantsLayer = true
        if !(surface is NSHPageView) {
            backdrop.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.35).cgColor
            backdrop.onClick = { [weak self] in self?.backdropClicked() }
            addSubview(backdrop)
        } else {
            layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
        }
        addSubview(surface)
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    override var isFlipped: Bool { true }

    private func backdropClicked() {
        guard let s = surface as? NSHSurfaceNodeView, s.dismissable else { return }
        s.userDismissed()
    }

    override func layout() {
        super.layout()
        backdrop.frame = bounds
        position()
    }

    func refresh(in host: NSView) {
        position()
        if let d = surface as? NSHDialogView, let spec = d.alertSpec {
            AlertPresenter.present(spec, for: d, in: host.window)
        }
    }

    private func position() {
        let size = surface.nodeFrame.size
        if surface is NSHSheetView {
            surface.applyFrame(CGRect(x: 0, y: bounds.height - size.height, width: bounds.width, height: size.height))
        } else if surface is NSHDialogView {
            surface.applyFrame(CGRect(x: ((bounds.width - size.width) / 2).rounded(), y: ((bounds.height - size.height) / 2).rounded(),
                                      width: size.width, height: size.height))
        }
    }
}

/// Dims what is behind an overlay; a click on it is a dismissal.
final class BackdropView: NSView {
    var onClick: (() -> Void)?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    override func mouseDown(with event: NSEvent) {}
    override func mouseUp(with event: NSEvent) { onClick?() }
}

enum Environment {
    static func current(size: CGSize, safeArea: NSEdgeInsets, appearance: NSAppearance, scale: CGFloat) -> [String: Any] {
        [
            "width": Double(size.width),
            "height": Double(size.height),
            "scale": Double(scale > 0 ? scale : TextMeasurer.scale),
            "font_scale": Double(FontScale.value),
            "safe_area": [safeArea.top, safeArea.right, safeArea.bottom, safeArea.left].map { Double($0) },
            "keyboard_height": 0.0,
            "appearance": appearance.isDark ? "dark" : "light",
            "platform": "macos",
            "locale": Locale.preferredLanguages.first ?? "en",
            "clipboard_feedback": false,
        ]
    }
}
