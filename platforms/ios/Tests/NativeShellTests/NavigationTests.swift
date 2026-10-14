import NativeShellShared
@testable import NativeShell
import XCTest

/// Pages, sheets, dialogs and alerts through `NSHViewController`, driven by batches.
final class NavigationTests: XCTestCase {
    var window: UIWindow!
    var vc: NSHViewController!
    var events: [SentEvent] = []
    var seq: UInt32 = 0

    override func setUp() {
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
        vc = NSHViewController(instance: nil)
        window.rootViewController = vc
        window.makeKeyAndVisible()
        vc.view.layoutIfNeeded()
        vc.surfaceView.eventSink = { [weak self] node, type, payload, seq in
            self?.events.append(SentEvent(node: node, type: type, payload: (JSON.any(payload) as? [String: Any]) ?? [:], seq: seq))
        }
        apply(flags: BatchFlags.initial) { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 844), props: [(.backgroundColor, .color(0xFFFF_FFFF))])
        }
    }

    override func tearDown() {
        vc.presentedViewController?.dismiss(animated: false)
        window.isHidden = true
        window = nil
    }

    private func apply(flags: UInt32 = 0, _ build: (inout Batch) -> Void) {
        seq += 1
        var b = Batch(seq: seq, flags: flags)
        build(&b)
        XCTAssertTrue(vc.surfaceView.apply(b.encoded()))
    }

    private func pushPage(_ node: UInt32, title: String, canGoBack: Bool = true, index: UInt32) {
        apply { b in
            b.create(node, .page, frame: CGRect(x: 0, y: 0, width: 390, height: 844), props: [
                (.title, .str(title)), (.canGoBack, .bool(canGoBack)), (.backgroundColor, .color(0xFFEE_EEEE)),
            ])
            b.mask(node, EventMask.dismiss)
            b.create(node + 1, .text, parent: node, frame: CGRect(x: 20, y: 100, width: 200, height: 20),
                     props: [(.attributedText, .json(textJSON(title)))])
            b.insert(node, 0, index)
            b.command(node, .presentPage, ["animated": true])
        }
    }

    private var stack: [UIViewController] { vc.navigation.viewControllers }

    func testPushPopAndUserBack() {
        XCTAssertEqual(stack.count, 1)
        pushPage(10, title: "Detail", index: 1)
        XCTAssertTrue(wait { self.stack.count == 2 && self.vc.navigation.transitionCoordinator == nil })
        let page = stack[1] as! PageController
        XCTAssertEqual(page.title, "Detail")
        XCTAssertTrue(page.page.superview === page.view)
        XCTAssertTrue(vc.navigation.isNavigationBarHidden, "pages are full screen")

        pushPage(20, title: "Deeper", index: 2)
        XCTAssertTrue(wait { self.stack.count == 3 && self.vc.navigation.transitionCoordinator == nil })

        // The core pops: DismissPage, then Delete in a later batch.
        apply { $0.command(20, .dismissPage, ["animated": true]) }
        XCTAssertTrue(wait { self.stack.count == 2 && self.vc.navigation.transitionCoordinator == nil })
        apply { $0.delete(20) }
        XCTAssertEqual(stack.count, 2)
        XCTAssertTrue(events.filter { $0.type == .dismiss }.isEmpty, "core-driven pops are not reported")

        // The user pops natively: Dismiss is sent and the page stays off the stack.
        vc.navigation.popViewController(animated: false)
        XCTAssertTrue(wait { self.events.contains { $0.type == .dismiss && $0.node == 10 } })
        XCTAssertEqual(stack.count, 1)
        apply { b in b.prop(1, .opacity, .f32(1)) } // unrelated batch: the popped page is not pushed back
        XCTAssertEqual(stack.count, 1)
        apply { $0.delete(10) }
        XCTAssertEqual(stack.count, 1)
    }

    private func keyboard(_ height: CGFloat?) {
        let screen = window.screen.bounds
        if let h = height {
            // The window sits at the screen origin; its bottom is 844.
            let end = CGRect(x: 0, y: window.frame.maxY - h, width: screen.width, height: h + (screen.height - window.frame.maxY))
            NotificationCenter.default.post(name: UIResponder.keyboardWillChangeFrameNotification, object: nil,
                                            userInfo: [UIResponder.keyboardFrameEndUserInfoKey: NSValue(cgRect: end)])
        } else {
            NotificationCenter.default.post(name: UIResponder.keyboardWillHideNotification, object: nil, userInfo: [:])
        }
    }

    private var keyboardHeight: Double { (vc.surfaceView.environment["keyboard_height"] as? Double) ?? -1 }

    /// The keyboard reaches layout while a pushed page is on top (the root surface view is then off screen).
    func testKeyboardHeightOnAPushedPage() {
        keyboard(300)
        XCTAssertEqual(keyboardHeight, 300, accuracy: 0.5, "root")
        keyboard(nil)
        XCTAssertEqual(keyboardHeight, 0)
        pushPage(10, title: "Address", index: 1)
        XCTAssertTrue(wait { self.stack.count == 2 && self.vc.navigation.transitionCoordinator == nil })
        XCTAssertNil(vc.surfaceView.window, "the root surface view is under the page")
        keyboard(291)
        XCTAssertEqual(keyboardHeight, 291, accuracy: 0.5, "pushed page")
        keyboard(nil)
        XCTAssertEqual(keyboardHeight, 0)
    }

    func testSheetHasAnOpaqueThemeBackgroundAndTracksTheKeyboard() {
        apply { b in
            b.create(30, .sheet, frame: CGRect(x: 0, y: 0, width: 390, height: 300), props: [(.detents, .json(#"["medium"]"#))])
            b.insert(30, 0, 1)
        }
        let sheet = vc.surfaceView.manager.component(30)!
        guard let modal = vc.modalController(for: sheet) else { return XCTFail("no modal") }
        modal.loadViewIfNeeded()
        for style in [UIUserInterfaceStyle.light, .dark] {
            let bg = modal.view.backgroundColor?.resolvedColor(with: UITraitCollection(userInterfaceStyle: style))
            XCTAssertEqual(bg, Colors.hex(style == .dark ? "#000000" : "#ffffff"), "opaque theme background (\(style.rawValue))")
            var a: CGFloat = 0
            bg?.getRed(nil, green: nil, blue: nil, alpha: &a)
            XCTAssertEqual(a, 1)
        }
        // ThemeTokens from the core re-color an open sheet.
        apply { $0.prop(0, .themeTokens, .json(JSON.string(["appearance": "dark", "background": 0xFF10_1010]))) }
        XCTAssertEqual(modal.view.backgroundColor, Colors.argb(0xFF10_1010))
        // With the sheet on top, the keyboard is measured against it.
        XCTAssertTrue(vc.topSurfaceView === modal.view)
        keyboard(300)
        XCTAssertGreaterThan(keyboardHeight, 0)
        keyboard(nil)
        XCTAssertEqual(keyboardHeight, 0)
    }

    /// DismissPage and Delete in one batch: the page keeps its content for the exit transition.
    func testDismissAndDeleteInOneBatchKeepsContentWhileLeaving() {
        pushPage(10, title: "Leaving", index: 1)
        XCTAssertTrue(wait { self.stack.count == 2 && self.vc.navigation.transitionCoordinator == nil })
        let page = (stack[1] as! PageController).page
        XCTAssertEqual(page.children.count, 1)
        apply { b in
            b.command(10, .dismissPage, ["animated": true])
            b.delete(10)
        }
        XCTAssertNil(vc.surfaceView.manager.component(10))
        XCTAssertNil(vc.surfaceView.manager.component(11))
        XCTAssertEqual(page.subviews.compactMap { $0 as? NSHTextView }.count, 1, "content stays during the exit animation")
        XCTAssertTrue(wait { self.stack.count == 1 })
        XCTAssertEqual(vc.surfaceView.manager.pooledCount(.text), 0, "views of a leaving page are not recycled under it")
    }

    func testBackIsBlockedWhenCanGoBackIsFalse() {
        pushPage(10, title: "Form", canGoBack: false, index: 1)
        XCTAssertTrue(wait { self.stack.count == 2 && self.vc.navigation.transitionCoordinator == nil })
        let pop = vc.navigation.interactivePopGestureRecognizer!
        XCTAssertFalse(vc.gestureRecognizerShouldBegin(pop))
        XCTAssertEqual(events.filter { $0.type == .backPressed }.map(\.node), [10])
        vc.performBack()
        XCTAssertEqual(events.filter { $0.type == .backPressed }.count, 2)
        XCTAssertEqual(stack.count, 2)
        XCTAssertTrue(stack[1].navigationItem.hidesBackButton)

        apply { $0.prop(10, .canGoBack, .bool(true)) }
        XCTAssertTrue(vc.gestureRecognizerShouldBegin(pop))
        vc.performBack()
        XCTAssertTrue(wait { self.events.contains { $0.type == .dismiss } })
    }

    func testInitialBatchPagesDoNotAnimate() {
        let w = UIWindow(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
        let c = NSHViewController(instance: nil)
        w.rootViewController = c
        w.makeKeyAndVisible()
        var b = Batch(seq: 1, flags: BatchFlags.initial)
        b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 844))
        b.create(10, .page, parent: 0, index: 1, frame: CGRect(x: 0, y: 0, width: 390, height: 844))
        b.command(10, .presentPage, ["animated": true])
        c.surfaceView.apply(b.encoded())
        XCTAssertEqual(c.navigation.viewControllers.count, 2)
        XCTAssertNil(c.navigation.transitionCoordinator, "applied without a transition")
        w.isHidden = true
    }

    // The package's test process has no UIApplication, so UIKit never finishes modal
    // transitions here: these tests check what the presenter asks UIKit to do. The
    // Example app's UI test exercises the same flows in a real app.

    func testSheetPresentAndUserDismiss() {
        apply { b in
            b.create(30, .sheet, frame: CGRect(x: 0, y: 0, width: 390, height: 300), props: [
                (.detents, .json(#"["medium", {"height": 320}]"#)), (.dismissable, .bool(true)),
            ])
            b.mask(30, EventMask.dismiss)
            b.create(31, .text, parent: 30, frame: CGRect(x: 16, y: 16, width: 200, height: 20), props: [(.attributedText, .json(textJSON("Sheet")))])
            b.insert(30, 0, 1)
        }
        let sheet = vc.surfaceView.manager.component(30) as! NSHSheetView
        XCTAssertEqual(sheet.detents, [.medium, .height(320)])
        guard let modal = vc.modalController(for: sheet) as? ModalController else { return XCTFail("no modal") }
        XCTAssertTrue(vc.presentedViewController === modal)
        modal.loadViewIfNeeded()
        XCTAssertTrue(sheet.isDescendant(of: modal.view))
        XCTAssertFalse(modal.isModalInPresentation)
        if #available(iOS 15.0, *) {
            XCTAssertEqual(modal.modalPresentationStyle, .pageSheet)
            XCTAssertEqual(modal.sheetPresentationController?.detents.count, 2)
        }
        // Not dismissable: modal in presentation.
        apply { $0.prop(30, .dismissable, .bool(false)) }
        XCTAssertTrue(modal.isModalInPresentation)
        apply { $0.prop(30, .dismissable, .bool(true)) }

        // A user swipe-down reports Dismiss once and forgets the modal.
        modal.presentationControllerDidDismiss(modal.presentationController!)
        XCTAssertEqual(events.filter { $0.type == .dismiss }.map(\.node), [30])
        XCTAssertNil(vc.modalController(for: sheet))

        // A sheet the core closes is dismissed without a Dismiss event.
        apply { b in
            b.create(32, .sheet, frame: CGRect(x: 0, y: 0, width: 390, height: 200))
            b.mask(32, EventMask.dismiss)
            b.insert(32, 0, 2)
        }
        let second = vc.surfaceView.manager.component(32)!
        guard let modal2 = vc.modalController(for: second) as? ModalController else { return XCTFail("no second modal") }
        apply { $0.delete(32) }
        XCTAssertTrue(modal2.closingByCore)
        XCTAssertNil(vc.modalController(for: second))
        modal2.presentationControllerDidDismiss(modal2.presentationController!)
        XCTAssertEqual(events.filter { $0.type == .dismiss }.count, 1)
    }

    func testCustomDialogIsCentered() {
        apply { b in
            b.create(40, .dialog, frame: CGRect(x: 0, y: 0, width: 300, height: 200), props: [(.dismissable, .bool(true)), (.backgroundColor, .color(0xFFFF_FFFF))])
            b.mask(40, EventMask.dismiss)
            b.insert(40, 0, 1)
        }
        let dialog = vc.surfaceView.manager.component(40)!
        guard let modal = vc.modalController(for: dialog) as? ModalController else { return XCTFail("no modal") }
        XCTAssertEqual(modal.modalPresentationStyle, .overFullScreen)
        modal.loadViewIfNeeded()
        modal.view.frame = CGRect(x: 0, y: 0, width: 390, height: 844)
        modal.view.layoutIfNeeded()
        XCTAssertEqual(dialog.frame, CGRect(x: 45, y: 322, width: 300, height: 200))
        apply { $0.delete(40) }
        XCTAssertTrue(modal.closingByCore)
        XCTAssertNil(vc.modalController(for: dialog))
    }

    func testAlertActionsAreReported() {
        apply { b in
            b.create(50, .dialog, props: [(.alertSpec, .json(#"{"title":"Delete?","message":"Gone for good.","actions":[{"label":"Cancel","style":"cancel"},{"label":"Delete","style":"destructive"}]}"#))])
            b.mask(50, EventMask.action)
            b.insert(50, 0, 1)
        }
        let dialog = vc.surfaceView.manager.component(50) as! NSHDialogView
        guard let alert = dialog.alert else { return XCTFail("no alert") }
        XCTAssertNil(vc.modalController(for: dialog), "alerts are UIAlertControllers, not custom modals")
        XCTAssertEqual(alert.title, "Delete?")
        XCTAssertEqual(alert.message, "Gone for good.")
        XCTAssertEqual(alert.actions.map(\.title), ["Cancel", "Delete"])
        XCTAssertEqual(alert.actions.map(\.style), [.cancel, .destructive])
        // Trigger the destructive action's handler.
        typealias Handler = @convention(block) (UIAlertAction) -> Void
        if let block = alert.actions[1].value(forKey: "handler") {
            let handler = unsafeBitCast(block as AnyObject, to: Handler.self)
            handler(alert.actions[1])
        }
        XCTAssertEqual(events.filter { $0.type == .action }.map { $0.payload.int("index") }, [1])
        XCTAssertNil(dialog.alert)
        apply { $0.delete(50) }
    }
}
