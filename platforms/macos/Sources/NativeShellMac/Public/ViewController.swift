// Hosts an instance with navigation: the root surface is the bottom of a page
// stack, each `Page` the core attaches slides in above it, sheets and custom
// dialogs are window sheets, alerts are NSAlert sheets.
//
// The core owns the stack. User actions that change it (Back from the Go menu
// or Command-[, the mouse's back button, Escape or a click outside on a
// dismissable dialog) are reported (`Dismiss`, `BackPressed`); a page whose
// `CanGoBack` is false cannot be left natively.

import AppKit
import NativeShellShared

@objcMembers
open class NSHViewController: NSViewController, SurfacePresenter, SurfaceObserver, RootCommandHandler, NSMenuItemValidation {
    public let surfaceView: NSHSurfaceView
    public private(set) var instance: NSHInstance?
    private var pages: [NSHPageView] = []
    private var containers: [ObjectIdentifier: PageContainer] = [:]
    /// Containers on screen, bottom to top (pages leaving keep theirs until their transition ends).
    private var shown: [PageContainer] = []
    /// Pages the user left natively; kept off the stack until the core removes them.
    private var userPopped = Set<ObjectIdentifier>()
    private var modals: [ObjectIdentifier: ModalSheet] = [:]

    public init(instance: NSHInstance?, runtime: NSHRuntime = .shared) {
        surfaceView = NSHSurfaceView(runtime: runtime)
        super.init(nibName: nil, bundle: nil)
        surfaceView.manager.presenter = self
        if let i = instance { attach(i) }
    }

    public convenience init() { self.init(instance: nil) }

    public required init?(coder: NSCoder) { fatalError("not supported") }

    public func attach(_ instance: NSHInstance) {
        self.instance = instance
        surfaceView.attach(instance)
    }

    override open func loadView() {
        let v = StackView(frame: NSRect(x: 0, y: 0, width: 480, height: 640))
        v.onBack = { [weak self] in self?.performBack() }
        surfaceView.frame = v.bounds
        surfaceView.autoresizingMask = [.width, .height]
        v.addSubview(surfaceView)
        surfaceView.safeAreaSource = v
        view = v
    }

    override open func viewDidLayout() {
        super.viewDidLayout()
        // A resized window resizes its sheets (detents are fractions of it).
        modals.values.forEach { $0.resize() }
    }

    // MARK: SurfacePresenter

    func attachSurface(_ view: NSHComponentView, at index: Int, in manager: MountingManager) {
        (view as? NSHSurfaceNodeView)?.observer = self
        if let page = view as? NSHPageView {
            pages.removeAll { $0 === page }
            // Order pages by their position among the root's children.
            let order = manager.rootView.children.compactMap { $0 as? NSHPageView }
            pages.append(page)
            pages.sort { a, b in (order.firstIndex { $0 === a } ?? 0) < (order.firstIndex { $0 === b } ?? 0) }
        }
        // Sheets and dialogs are presented when the batch ends, once content and props are known.
    }

    func detachSurface(_ view: NSHComponentView, in manager: MountingManager) {
        let id = ObjectIdentifier(view)
        if let page = view as? NSHPageView {
            pages.removeAll { $0 === page }
            userPopped.remove(id)
            return
        }
        if let dialog = view as? NSHDialogView { dialog.dismissAlert() }
        modals.removeValue(forKey: id)?.close()
    }

    func surfacesDidChange(in manager: MountingManager, animatedPages: Bool) {
        syncPages(animated: animatedPages)
        for surface in manager.rootView.children {
            if let dialog = surface as? NSHDialogView, let spec = dialog.alertSpec {
                AlertPresenter.present(spec, for: dialog, in: view.window)
            } else if surface is NSHDialogView || surface is NSHSheetView {
                if let modal = modals[ObjectIdentifier(surface)] { modal.resize() } else { presentModal(for: surface) }
            }
        }
    }

    func surfacePropsChanged(_ surface: NSHComponentView) {
        if let page = surface as? NSHPageView, page === pages.last { title = page.title.isEmpty ? nil : page.title }
        modals[ObjectIdentifier(surface)]?.resize()
    }

    // MARK: Pages

    private func container(for page: NSHPageView) -> PageContainer {
        let id = ObjectIdentifier(page)
        if let c = containers[id] { return c }
        let c = PageContainer(page: page, runtime: surfaceView.manager.runtime, theme: surfaceView.manager.theme)
        containers[id] = c
        return c
    }

    /// The pages the stack shows: attached, not being dismissed, not left by the user.
    private var visiblePages: [NSHPageView] {
        pages.filter { !userPopped.contains(ObjectIdentifier($0)) && !$0.dismissing }
    }

    /// Makes the stack on screen match the core's pages, sliding the top page in
    /// (push) or out (pop) when the change is animated.
    private func syncPages(animated: Bool) {
        let wanted = visiblePages.map(container(for:))
        title = visiblePages.last.flatMap { $0.title.isEmpty ? nil : $0.title }
        guard wanted.map(ObjectIdentifier.init) != shown.map(ObjectIdentifier.init) else { return }
        let oldTop = shown.last
        let leaving = shown.filter { c in !wanted.contains { $0 === c } }
        var below: NSView = surfaceView
        for c in wanted {
            c.frame = view.bounds
            c.autoresizingMask = [.width, .height]
            if c.superview !== view { view.addSubview(c, positioned: .above, relativeTo: below) }
            c.isHidden = false
            below = c
        }
        shown = wanted
        let slide = animated && view.window != nil && !Animator.reduceMotion()
        let w = view.bounds.width
        if slide, let top = wanted.last, top !== oldTop, !leaving.contains(where: { $0 === oldTop }) {
            // Push: the new page slides in from the trailing edge.
            top.frame.origin.x = w
            NSAnimationContext.runAnimationGroup({ ctx in
                ctx.duration = 0.3
                ctx.timingFunction = CAMediaTimingFunction(controlPoints: 0.2, 0.9, 0.3, 1)
                top.animator().frame.origin.x = 0
            }, completionHandler: { [weak self] in self?.hideCovered() })
        } else if slide, let old = oldTop, leaving.contains(where: { $0 === old }) {
            // Pop: the leaving page slides out; it stays above the stack until it is gone.
            view.addSubview(old, positioned: .above, relativeTo: nil)
            NSAnimationContext.runAnimationGroup({ ctx in
                ctx.duration = 0.25
                ctx.timingFunction = CAMediaTimingFunction(name: .easeIn)
                old.animator().frame.origin.x = w
            }, completionHandler: { [weak self] in
                leaving.forEach { $0.removeFromSuperview() }
                self?.dropDeadContainers()
            })
        } else {
            leaving.forEach { $0.removeFromSuperview() }
            hideCovered()
        }
        dropDeadContainers()
        if !slide { hideCovered() }
        if let top = wanted.last { view.window?.makeFirstResponder(top) }
    }

    /// Covered pages (and the root content) leave the accessibility tree and stop drawing.
    private func hideCovered() {
        for c in shown.dropLast() { c.isHidden = true }
        surfaceView.root.isHidden = !shown.isEmpty
    }

    /// Containers whose page is gone and which are off screen.
    private func dropDeadContainers() {
        let live = Set(pages.map(ObjectIdentifier.init))
        containers = containers.filter { live.contains($0.key) || $0.value.superview != nil }
    }

    /// The system back action: leaves the top page, or reports `BackPressed` when it can't go back.
    public func performBack() {
        guard let top = visiblePages.last else { return }
        if top.canGoBack {
            userPopped.insert(ObjectIdentifier(top))
            top.userDismissed()
            syncPages(animated: true)
        } else {
            top.backPressed()
        }
    }

    /// The Go menu's Back item (`goBack:` up the responder chain).
    @IBAction public func goBack(_ sender: Any?) { performBack() }

    public func validateMenuItem(_ item: NSMenuItem) -> Bool {
        if item.action == #selector(goBack(_:)) { return !visiblePages.isEmpty }
        return true
    }

    // MARK: Modals

    private func presentModal(for surface: NSHComponentView) {
        let id = ObjectIdentifier(surface)
        guard modals[id] == nil else { return }
        let sheet = ModalSheet(surface: surface, runtime: surfaceView.manager.runtime, theme: surfaceView.manager.theme)
        sheet.onUserDismiss = { [weak self, weak surface] in
            guard let surface = surface else { return }
            self?.modals[ObjectIdentifier(surface)] = nil
            (surface as? NSHSurfaceNodeView)?.userDismissed()
        }
        modals[id] = sheet
        sheet.present(from: view.window, parentSize: view.bounds.size)
    }

    /// The window sheet showing a sheet or dialog node (tests).
    func modal(for view: NSHComponentView) -> ModalSheet? { modals[ObjectIdentifier(view)] }

    /// Pages on the stack, bottom to top (tests).
    var stackedPages: [NSHPageView] { shown.map(\.page) }

    // MARK: Theme (RootCommandHandler: only the theme hook is used here)

    func themeDidChange() {
        modals.values.forEach { $0.applyTheme() }
        containers.values.forEach { $0.applyTheme() }
    }

    func showToast(_ payload: [String: Any]) { surfaceView.showToast(payload) }
    func dismissToast(_ payload: [String: Any]) { surfaceView.dismissToast(payload) }
}

/// The controller's view: flipped, and it turns the pointer's back gestures into Back.
private final class StackView: NSView {
    var onBack: (() -> Void)?

    override var isFlipped: Bool { true }

    override func performKeyEquivalent(with e: NSEvent) -> Bool {
        // Command-[ is Back across macOS, with or without a Go menu.
        if e.modifierFlags.intersection(.deviceIndependentFlagsMask) == .command, e.charactersIgnoringModifiers == "[" {
            onBack?()
            return true
        }
        return super.performKeyEquivalent(with: e)
    }

    override func otherMouseUp(with e: NSEvent) {
        // Button 4: the mouse's back button.
        if e.buttonNumber == 3 { onBack?() } else { super.otherMouseUp(with: e) }
    }
}

/// One page of the stack: opaque, in the theme's background.
final class PageContainer: NSView {
    let page: NSHPageView
    private let runtime: NSHRuntime
    private weak var theme: ThemeState?

    init(page: NSHPageView, runtime: NSHRuntime, theme: ThemeState?) {
        self.page = page
        self.runtime = runtime
        self.theme = theme
        super.init(frame: .zero)
        wantsLayer = true
        addSubview(page)
        applyTheme()
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    func applyTheme() {
        layer?.backgroundColor = ThemeTokens.color("background", runtime: runtime, theme: theme).cgColor(in: effectiveAppearance)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyTheme()
    }

    // Clicks that reach the page's background never fall through to the pages below.
    override func mouseDown(with event: NSEvent) {}
}

/// A sheet node or a custom dialog, as a window sheet.
final class ModalSheet: NSObject {
    let surface: NSHComponentView
    let window: SheetWindow
    var onUserDismiss: (() -> Void)?
    private let content = SheetContentView()
    private let runtime: NSHRuntime
    private weak var theme: ThemeState?
    private weak var parent: NSWindow?
    private var parentSize: CGSize = .zero
    private var closed = false

    init(surface: NSHComponentView, runtime: NSHRuntime, theme: ThemeState?) {
        self.surface = surface
        self.runtime = runtime
        self.theme = theme
        window = SheetWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled, .fullSizeContentView],
                             backing: .buffered, defer: true)
        super.init()
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.isReleasedWhenClosed = false
        window.contentView = content
        content.addSubview(surface)
        window.onCancel = { [weak self] in self?.userCancel() }
        applyTheme()
    }

    var isDismissable: Bool { (surface as? NSHSurfaceNodeView)?.dismissable ?? true }

    func present(from parent: NSWindow?, parentSize: CGSize) {
        self.parent = parent
        self.parentSize = parentSize
        resize()
        parent?.beginSheet(window)
    }

    /// The size last reported with `Resize`.
    private(set) var reportedSize: CGSize?

    /// The sheet's size, within the parent window. A sheet with detents is as tall
    /// as its largest one and reports that size (`Resize`), so the core lays its
    /// content out to fill it; without detents the window fits the content.
    func resize() {
        if let p = parent?.contentView?.bounds.size { parentSize = p }
        let f = surface.nodeFrame
        let width = parentSize.width > 0 ? min(f.width, parentSize.width) : f.width
        let detents = (surface as? NSHSheetView)?.detents ?? []
        var height = f.height
        if !detents.isEmpty, parentSize.height > 0 {
            height = detents.map { d -> CGFloat in
                switch d {
                case .large: return parentSize.height - 40
                case .medium: return parentSize.height / 2
                case let .height(h): return h
                }
            }.max() ?? f.height
        }
        if parentSize.height > 0 { height = min(height, parentSize.height - 40) }
        let size = CGSize(width: max(1, width), height: max(1, height))
        window.setContentSize(size)
        surface.applyFrame(CGRect(x: 0, y: 0, width: width, height: detents.isEmpty ? f.height : size.height))
        if !detents.isEmpty, size != reportedSize {
            reportedSize = size
            (surface as? NSHSheetView)?.send(.resize, ["width": Double(size.width), "height": Double(size.height)])
        }
    }

    func applyTheme() {
        content.wantsLayer = true
        content.layer?.backgroundColor = ThemeTokens.color("background", runtime: runtime, theme: theme).cgColor(in: window.effectiveAppearance)
    }

    /// Escape, or the backdrop: dismissable sheets close and report it.
    private func userCancel() {
        guard isDismissable, !closed else { return }
        close()
        onUserDismiss?()
    }

    /// Takes the sheet off screen (the core removed it, or the user dismissed it).
    func close() {
        guard !closed else { return }
        closed = true
        if let p = window.sheetParent { p.endSheet(window) } else { window.orderOut(nil) }
    }
}

final class SheetWindow: NSWindow {
    var onCancel: (() -> Void)?
    override var canBecomeKey: Bool { true }
    override func cancelOperation(_ sender: Any?) { onCancel?() }
}

private final class SheetContentView: NSView {
    override var isFlipped: Bool { true }
}
