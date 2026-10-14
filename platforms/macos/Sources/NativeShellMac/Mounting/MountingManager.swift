// The AppKit side of `MountingCore`: makes views, gives them their context,
// presents surfaces, runs animations and root services, and holds the theme.
// The node table, pools and batch application are the shared core's. Main
// thread only.

import AppKit
import NativeShellShared

/// Presents the surfaces the core attaches to the root node: pages, sheets and
/// dialogs. `NSHViewController` implements it with a page stack and window sheets; a
/// bare `NSHSurfaceView` stacks them as subviews.
protocol SurfacePresenter: AnyObject {
    func attachSurface(_ view: NSHComponentView, at index: Int, in manager: MountingManager)
    func detachSurface(_ view: NSHComponentView, in manager: MountingManager)
    /// Batch applied: present or dismiss what changed. `animatedPages` is true when
    /// the batch carried `PresentPage` / `DismissPage`.
    func surfacesDidChange(in manager: MountingManager, animatedPages: Bool)
}

/// Root-level services the renderer offers on node 0.
protocol RootCommandHandler: AnyObject {
    func themeDidChange()
    func showToast(_ payload: [String: Any])
    /// `{"id"}` retracts one toast (visible or queued), `{}` every toast.
    func dismissToast(_ payload: [String: Any])
}

final class MountingManager: ComponentContext, MountingHost {
    let runtime: NSHRuntime
    /// The view node 0 maps to.
    let rootView: NSHComponentView
    weak var presenter: SurfacePresenter?
    let theme = ThemeState()
    weak var rootCommands: RootCommandHandler?
    /// Delivers events to the instance: node, type, JSON payload, seq of the last applied batch.
    var eventSink: ((UInt32, EventType, String, UInt32) -> Void)?
    var logger: ((NSHLogLevel, String) -> Void)?
    private(set) var core: MountingCore!

    init(runtime: NSHRuntime, rootView: NSHComponentView) {
        self.runtime = runtime
        self.rootView = rootView
        core = MountingCore(host: self, rootView: rootView)
        rootView.context = self
    }

    // MARK: The core's state (diagnostics, tests)

    var nodes: [UInt32: NSHComponentView] { core.nodes.compactMapValues { $0 as? NSHComponentView } }
    var nodeCount: Int { core.nodeCount }
    var lastSeq: UInt32 { core.lastSeq }
    var appliedBatches: Int { core.appliedBatches }
    var lastApplyMs: Double { core.lastApplyMs }
    func pooledCount(_ type: NSHViewType) -> Int { core.pooledCount(type) }
    func trimPools() { core.trimPools() }

    @discardableResult
    func apply(_ bytes: UnsafeRawBufferPointer) -> Bool { core.apply(bytes) }

    @discardableResult
    func apply(_ data: Data) -> Bool { core.apply(data) }

    // MARK: ComponentContext

    func component(_ node: UInt32) -> NSHComponentView? { core.component(node) as? NSHComponentView }

    func needsBatchEnd(_ view: NSHComponentView) { core.needsBatchEnd(view) }

    func send(_ node: UInt32, _ type: EventType, _ payload: [String: Any]) {
        eventSink?(node, type, JSON.string(payload), core.lastSeq)
    }

    func log(_ level: NSHLogLevel, _ message: String) {
        if let logger = logger { logger(level, message) } else { runtime.log(level, message) }
    }

    // MARK: MountingHost

    func makeView(_ type: NSHViewType) -> MountedView { ComponentRegistry.make(type) }
    func adopt(_ view: MountedView) { (view as? NSHComponentView)?.context = self }
    func release(_ view: MountedView) { (view as? NSHComponentView)?.context = nil }

    func attachSurface(_ view: MountedView, at index: Int) -> Bool {
        guard let presenter = presenter, let v = view as? NSHComponentView else { return false }
        presenter.attachSurface(v, at: index, in: self)
        return true
    }

    func detachSurface(_ view: MountedView) -> Bool {
        guard let presenter = presenter, let v = view as? NSHComponentView else { return false }
        presenter.detachSurface(v, in: self)
        return true
    }

    func surfacesDidChange(animatedPages: Bool) { presenter?.surfacesDidChange(in: self, animatedPages: animatedPages) }

    func updateTheme(_ tokens: [String: Any]?) {
        if let o = tokens { theme.update(o) } else { theme.reset() }
    }

    func themeDidChange() {
        rootCommands?.themeDidChange()
        // A bare surface view is both: tell it once.
        if let p = presenter as? RootCommandHandler, p !== rootCommands { p.themeDidChange() }
    }

    func animate(_ animation: [String: Any], moves: Bool, _ changes: @escaping () -> Void) {
        Animator.run(animation, moves: moves, changes)
    }

    func attachHostComponent(_ container: MountedView, name: String) -> Bool {
        guard let host = container as? NSHHostComponentView else { return false }
        host.attachComponent(named: name, runtime: runtime)
        return host.componentName != nil
    }

    func detachHostComponent(_ container: MountedView) { (container as? NSHHostComponentView)?.detachComponent() }

    func runRootCommand(_ command: Command, _ payload: [String: Any]) {
        switch command {
        case .showToast: rootCommands?.showToast(payload)
        case .dismissToast: rootCommands?.dismissToast(payload)
        case .openUrl: if let s = payload.string("url"), let url = URL(string: s) { runtime.openURL(url) }
        case .setClipboard:
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(payload.string("text") ?? "", forType: .string)
        case .haptic: Haptics.play(payload.string("kind") ?? "light")
        default: break
        }
    }

    func log(_ level: MountLogLevel, _ message: String) { log(NSHLogLevel(rawValue: level.rawValue) ?? .info, message) }
}

extension NSHComponentView: MountedView {
    var mountedParent: MountedView? { parentComponent }
    var mountedChildren: [MountedView] { children }
    var isInWindow: Bool { window != nil }

    /// Pages, sheets and dialogs: presented by the surface presenter when attached to the root.
    var isSurface: Bool {
        viewType == .page || viewType == .sheet || viewType == .dialog
    }

    // The core only hands back views it got from this renderer.
    func mountedInsert(_ child: MountedView, at index: Int) {
        if let c = child as? NSHComponentView { insertChild(c, at: index) }
    }

    func mountedRemove(_ child: MountedView) {
        if let c = child as? NSHComponentView { removeChild(c) }
    }

    func mountedMove(_ child: MountedView, to index: Int) {
        if let c = child as? NSHComponentView { moveChild(c, to: index) }
    }

    /// Child bookkeeping without placing the view (surfaces are placed by the presenter).
    func mountedInsertRecordOnly(_ child: MountedView, at index: Int) {
        guard let c = child as? NSHComponentView else { return }
        insertChild(c, at: index)
        c.removeFromSuperview()
    }

    func mountedPlace(_ child: MountedView, at index: Int) {
        if let c = child as? NSHComponentView { mountChildView(c, at: index) }
    }

    func removeFromNativeParent() { removeFromSuperview() }
}
