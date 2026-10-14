// Hosts an instance full screen with real navigation: the root surface is the
// navigation root, each `Page` the core attaches is a pushed view controller,
// sheets and dialogs are presented modally, alerts are UIAlertControllers.
//
// The core owns the stack. User gestures that change it (back swipe, back
// button, sheet swipe-down, backdrop tap) are reported (`Dismiss`,
// `BackPressed`); a page whose `CanGoBack` is false cannot be left natively.

import UIKit

@objcMembers
open class NSHViewController: UIViewController, SurfacePresenter, SurfaceObserver, RootCommandHandler, UINavigationControllerDelegate,
    UIGestureRecognizerDelegate, UIAdaptivePresentationControllerDelegate {
    public let surfaceView: NSHSurfaceView
    public let navigation: UINavigationController
    public private(set) var instance: NSHInstance?
    private let rootController = UIViewController()
    private var pages: [NSHPageView] = []
    private var pageControllers: [ObjectIdentifier: PageController] = [:]
    /// Pages the user popped natively; kept off the stack until the core removes them.
    private var userPopped = Set<ObjectIdentifier>()
    private var modals: [ObjectIdentifier: UIViewController] = [:]
    private var settingStack = false

    public init(instance: NSHInstance?, runtime: NSHRuntime = .shared) {
        surfaceView = NSHSurfaceView(runtime: runtime)
        navigation = UINavigationController(rootViewController: rootController)
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

    override open func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        rootController.view.backgroundColor = .systemBackground
        surfaceView.frame = rootController.view.bounds
        surfaceView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        rootController.view.addSubview(surfaceView)
        navigation.delegate = self
        navigation.interactivePopGestureRecognizer?.delegate = self
        navigation.setNavigationBarHidden(true, animated: false)
        addChild(navigation)
        navigation.view.frame = view.bounds
        navigation.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(navigation.view)
        navigation.didMove(toParent: self)
        surfaceView.safeAreaSource = view
        surfaceView.keyboardReference = { [weak self] in self?.topSurfaceView }
    }

    /// What the keyboard can cover: a presented sheet, else the page stack.
    var topSurfaceView: UIView? {
        if let modal = presentedViewController as? ModalController, !modal.isBeingDismissed, !modal.closingByCore { return modal.view }
        return view
    }

    override open func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        // Hardware keys reach the root when no input has focus.
        surfaceView.becomeFirstResponder()
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
        if let vc = modals.removeValue(forKey: id) {
            (vc as? ModalController)?.closingByCore = true
            vc.presentingViewController?.dismiss(animated: true)
        }
        view.removeFromSuperview()
    }

    func surfacesDidChange(in manager: MountingManager, animatedPages: Bool) {
        syncPages(animated: animatedPages)
        for surface in manager.rootView.children {
            if let dialog = surface as? NSHDialogView, let spec = dialog.alertSpec {
                AlertPresenter.present(spec, for: dialog, from: view)
            } else if surface is NSHDialogView || surface is NSHSheetView {
                if let modal = modals[ObjectIdentifier(surface)] as? ModalController {
                    if let sheet = surface as? NSHSheetView { modal.applyDetents(sheet.detents) }
                    modal.view.setNeedsLayout()
                } else {
                    presentModal(for: surface)
                }
            }
        }
        surfaceView.updateKeyboard()
    }

    func surfacePropsChanged(_ surface: NSHComponentView) {
        if let page = surface as? NSHPageView, let vc = pageControllers[ObjectIdentifier(page)] { vc.apply() }
        if let modal = modals[ObjectIdentifier(surface)] {
            modal.isModalInPresentation = !((surface as? NSHSurfaceNodeView)?.dismissable ?? true)
            if let sheet = surface as? NSHSheetView { (modal as? ModalController)?.applyDetents(sheet.detents) }
        }
    }

    // MARK: Pages

    private func controller(for page: NSHPageView) -> PageController {
        let id = ObjectIdentifier(page)
        if let vc = pageControllers[id] { return vc }
        let vc = PageController(page: page)
        pageControllers[id] = vc
        return vc
    }

    /// Makes the navigation stack match the core's pages: attached, not being
    /// dismissed (`DismissPage`), not already popped by the user.
    private func syncPages(animated: Bool) {
        let visible = pages.filter { !userPopped.contains(ObjectIdentifier($0)) && !$0.dismissing }
        let wanted: [UIViewController] = [rootController] + visible.map { controller(for: $0) }
        let live = Set(pages.map { ObjectIdentifier($0) })
        // Controllers of deleted pages stay alive while the stack still shows them (exit transition).
        let shown = Set(navigation.viewControllers.map { ObjectIdentifier($0) })
        pageControllers = pageControllers.filter { live.contains($0.key) || shown.contains(ObjectIdentifier($0.value)) }
        guard wanted != navigation.viewControllers else { return }
        settingStack = true
        navigation.setViewControllers(wanted, animated: animated && view.window != nil)
        settingStack = false
    }

    public func navigationController(_ nav: UINavigationController, didShow vc: UIViewController, animated: Bool) {
        surfaceView.updateKeyboard()
        // Drop controllers that finished their exit transition.
        let onStack = Set(nav.viewControllers.map { ObjectIdentifier($0) })
        let live = Set(pages.map { ObjectIdentifier($0) })
        pageControllers = pageControllers.filter { live.contains($0.key) || onStack.contains(ObjectIdentifier($0.value)) }
        guard !settingStack else { return }
        // A page controller missing from the stack was popped by the user.
        for page in pages where !page.dismissing {
            let id = ObjectIdentifier(page)
            guard let pc = pageControllers[id], !onStack.contains(ObjectIdentifier(pc)), !userPopped.contains(id) else { continue }
            userPopped.insert(id)
            page.userDismissed()
        }
    }

    /// Back swipe: refused (and reported) when the top page cannot go back.
    public func gestureRecognizerShouldBegin(_ g: UIGestureRecognizer) -> Bool {
        guard g === navigation.interactivePopGestureRecognizer else { return true }
        guard navigation.viewControllers.count > 1 else { return false }
        if let top = navigation.topViewController as? PageController, !top.page.canGoBack {
            top.page.backPressed()
            return false
        }
        return true
    }

    /// What the system back action does (edge swipe end, accessibility escape).
    func performBack() {
        guard let top = navigation.topViewController as? PageController else { return }
        if top.page.canGoBack {
            navigation.popViewController(animated: view.window != nil)
        } else {
            top.page.backPressed()
        }
    }

    // MARK: Modals

    private func presentModal(for view: NSHComponentView) {
        let id = ObjectIdentifier(view)
        guard modals[id] == nil else { return }
        let vc = ModalController(surface: view, runtime: surfaceView.manager.runtime, theme: surfaceView.manager.theme)
        vc.onUserDismiss = { [weak self, weak view] in
            guard let view = view else { return }
            self?.modals[ObjectIdentifier(view)] = nil
            (view as? NSHSurfaceNodeView)?.userDismissed()
        }
        modals[id] = vc
        vc.presentationController?.delegate = vc
        let presenter = AlertPresenter.topViewController(from: self.view) ?? self
        presenter.present(vc, animated: self.view.window != nil) { [weak self] in self?.surfaceView.updateKeyboard() }
    }

    // MARK: Theme (RootCommandHandler: only the theme hook is used here)

    func themeDidChange() {
        for (_, m) in modals { (m as? ModalController)?.applyTheme() }
    }

    func showToast(_ payload: [String: Any]) { surfaceView.showToast(payload) }
    func dismissToast(_ payload: [String: Any]) { surfaceView.dismissToast(payload) }

    /// The modal controller showing a sheet or dialog node (tests).
    func modalController(for view: NSHComponentView) -> UIViewController? { modals[ObjectIdentifier(view)] }
}

/// A pushed, full-screen page (no navigation bar: programs draw their own headers).
final class PageController: UIViewController {
    let page: NSHPageView

    init(page: NSHPageView) {
        self.page = page
        super.init(nibName: nil, bundle: nil)
        apply()
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        view.addSubview(page)
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        if page.superview !== view { view.addSubview(page) }
    }

    func apply() {
        // The title names the page for accessibility and the back-history menu.
        title = page.title
        navigationItem.title = page.title
        navigationItem.hidesBackButton = !page.canGoBack
    }

    override func accessibilityPerformEscape() -> Bool {
        if page.canGoBack { return false }
        page.backPressed()
        return true
    }
}

/// A sheet (bottom, with detents) or a custom dialog (centered, dimmed backdrop).
final class ModalController: UIViewController, UIAdaptivePresentationControllerDelegate {
    let surface: NSHComponentView
    var onUserDismiss: (() -> Void)?
    var closingByCore = false
    private let backdrop = UIControl()
    private let isSheet: Bool
    private let nativeSheet: Bool

    private let runtime: NSHRuntime
    /// Behind a custom (iOS 14) sheet's content.
    private let sheetBackground = UIView()

    private weak var theme: ThemeState?

    init(surface: NSHComponentView, runtime: NSHRuntime = .shared, theme: ThemeState? = nil) {
        self.surface = surface
        self.runtime = runtime
        self.theme = theme
        isSheet = surface is NSHSheetView
        if #available(iOS 15.0, *) { nativeSheet = surface is NSHSheetView } else { nativeSheet = false }
        super.init(nibName: nil, bundle: nil)
        if nativeSheet {
            modalPresentationStyle = .pageSheet
        } else {
            modalPresentationStyle = .overFullScreen
            modalTransitionStyle = .crossDissolve
        }
        isModalInPresentation = !((surface as? NSHSurfaceNodeView)?.dismissable ?? true)
        if let sheet = surface as? NSHSheetView { applyDetents(sheet.detents) }
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    func applyDetents(_ wanted: [SheetDetent]) {
        guard #available(iOS 15.0, *), nativeSheet, let sheet = sheetPresentationController else { return }
        var detents: [UISheetPresentationController.Detent] = []
        // No detents: fit the content height.
        let entries = wanted.isEmpty ? [SheetDetent.height(surface.nodeFrame.height)] : wanted
        for d in entries {
            switch d {
            case .medium: detents.append(.medium())
            case .large: detents.append(.large())
            case let .height(h):
                if #available(iOS 16.0, *) {
                    detents.append(.custom { ctx in min(h, ctx.maximumDetentValue) })
                } else {
                    detents.append(h > UIScreen.main.bounds.height / 2 ? .large() : .medium())
                }
            }
        }
        sheet.detents = detents
        sheet.prefersGrabberVisible = true
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        // Sheets get the opaque theme background (the system sheet material is
        // translucent); content may still paint over it.
        applyTheme()
        if !nativeSheet {
            if isSheet {
                sheetBackground.layer.cornerRadius = 12
                sheetBackground.layer.maskedCorners = [.layerMinXMinYCorner, .layerMaxXMinYCorner]
            }
            backdrop.backgroundColor = UIColor.black.withAlphaComponent(0.35)
            backdrop.addTarget(self, action: #selector(backdropTapped), for: .touchUpInside)
            view.addSubview(backdrop)
            if isSheet { view.addSubview(sheetBackground) }
        }
        view.addSubview(surface)
    }

    /// The opaque theme background behind sheet content.
    func applyTheme() {
        let background = ThemeTokens.color("background", runtime: runtime, theme: theme)
        if nativeSheet { view.backgroundColor = background } else { sheetBackground.backgroundColor = background }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        backdrop.frame = view.bounds
        let size = surface.nodeFrame.size
        if nativeSheet {
            surface.applyFrame(CGRect(origin: .zero, size: CGSize(width: view.bounds.width, height: size.height)))
        } else if isSheet {
            surface.applyFrame(CGRect(x: 0, y: view.bounds.height - size.height, width: view.bounds.width, height: size.height))
            sheetBackground.frame = CGRect(x: 0, y: view.bounds.height - size.height, width: view.bounds.width, height: size.height + 40)
            surface.layer.cornerRadius = surface.layer.cornerRadius == 0 ? 12 : surface.layer.cornerRadius
        } else {
            surface.applyFrame(CGRect(x: (view.bounds.width - size.width) / 2, y: (view.bounds.height - size.height) / 2,
                                      width: size.width, height: size.height))
        }
    }

    @objc private func backdropTapped() {
        guard !isModalInPresentation else { return }
        dismiss(animated: true) { [weak self] in self?.onUserDismiss?() }
    }

    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        guard !closingByCore else { return }
        onUserDismiss?()
    }
}
