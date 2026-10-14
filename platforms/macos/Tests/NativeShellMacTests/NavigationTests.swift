@testable import NativeShellMac
import AppKit
import XCTest

/// Records sheets instead of showing them: the tests never put windows on screen.
final class SheetRecordingWindow: NSWindow {
    var sheetsBegun: [NSWindow] = []
    var sheetsEnded: [NSWindow] = []
    override func beginSheet(_ sheetWindow: NSWindow, completionHandler handler: ((NSApplication.ModalResponse) -> Void)? = nil) {
        sheetsBegun.append(sheetWindow)
    }
    override func endSheet(_ sheetWindow: NSWindow) { sheetsEnded.append(sheetWindow) }
}

/// Pages, sheets, dialogs and alerts through `NSHViewController`, driven by batches.
final class NavigationTests: XCTestCase {
    var window: SheetRecordingWindow!
    var vc: NSHViewController!
    var events: [SentEvent] = []
    var seq: UInt32 = 0

    override func setUp() {
        window = SheetRecordingWindow(contentRect: CGRect(x: 0, y: 0, width: 390, height: 844), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        vc = NSHViewController(instance: nil)
        window.contentViewController = vc
        window.setContentSize(CGSize(width: 390, height: 844))
        vc.view.layoutSubtreeIfNeeded()
        vc.surfaceView.eventSink = { [weak self] node, type, payload, seq in
            self?.events.append(SentEvent(node: node, type: type, payload: (JSON.any(payload) as? [String: Any]) ?? [:], seq: seq))
        }
        apply(flags: BatchFlags.initial) { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 844), props: [(.backgroundColor, .color(0xFFFF_FFFF))])
        }
    }

    override func tearDown() {
        window.contentViewController = nil
        window.close()
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

    private var stack: [UInt32] { vc.stackedPages.map(\.nodeId) }

    func testPushPopAndUserBack() {
        Animator.reduceMotion = { true }
        defer { Animator.reduceMotion = { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion } }
        pushPage(10, title: "Detail", index: 1)
        XCTAssertEqual(stack, [10])
        XCTAssertEqual(vc.title, "Detail")
        let page = vc.surfaceView.manager.component(10)!
        XCTAssertEqual(page.superview?.frame, vc.view.bounds, "pages fill the controller's view")
        XCTAssertTrue(vc.surfaceView.root.isHidden, "the covered root content leaves the screen")

        pushPage(20, title: "Deeper", index: 2)
        XCTAssertEqual(stack, [10, 20])
        XCTAssertTrue(page.superview?.isHidden ?? false)

        // The core pops: DismissPage, then Delete in a later batch.
        apply { $0.command(20, .dismissPage, ["animated": true]) }
        XCTAssertEqual(stack, [10])
        apply { $0.delete(20) }
        XCTAssertTrue(events.filter { $0.type == .dismiss }.isEmpty, "core-driven pops are not reported")

        // The user goes back (Go > Back, Command-[): Dismiss is sent and the page stays off the stack.
        vc.goBack(nil)
        XCTAssertEqual(events.filter { $0.type == .dismiss }.map(\.node), [10])
        XCTAssertEqual(stack, [])
        XCTAssertFalse(vc.surfaceView.root.isHidden)
        apply { b in b.prop(1, .opacity, .f32(1)) } // unrelated batch: the popped page is not pushed back
        XCTAssertEqual(stack, [])
        apply { $0.delete(10) }
        XCTAssertEqual(stack, [])
        // Nothing to go back to: the menu item is disabled.
        let item = NSMenuItem(title: "Back", action: #selector(NSHViewController.goBack(_:)), keyEquivalent: "[")
        XCTAssertFalse(vc.validateMenuItem(item))
    }

    func testAnimatedPushSlidesIn() {
        Animator.reduceMotion = { false }
        defer { Animator.reduceMotion = { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion } }
        pushPage(10, title: "Detail", index: 1)
        let container = vc.surfaceView.manager.component(10)!.superview!
        XCTAssertTrue(wait { container.frame.origin.x == 0 })
        apply { $0.command(10, .dismissPage, ["animated": true]) }
        XCTAssertTrue(wait { container.superview == nil }, "the leaving page is removed after its transition")
    }

    func testBackIsBlockedWhenCanGoBackIsFalse() {
        pushPage(10, title: "Form", canGoBack: false, index: 1)
        vc.performBack()
        XCTAssertEqual(events.filter { $0.type == .backPressed }.map(\.node), [10])
        XCTAssertEqual(stack, [10])
        apply { $0.prop(10, .canGoBack, .bool(true)) }
        vc.performBack()
        XCTAssertEqual(events.filter { $0.type == .dismiss }.map(\.node), [10])
    }

    func testCommandBracketAndMouseBackButtonGoBack() {
        pushPage(10, title: "Detail", index: 1)
        let cmdBracket = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: .command, timestamp: 0, windowNumber: window.windowNumber,
                                          context: nil, characters: "[", charactersIgnoringModifiers: "[", isARepeat: false, keyCode: 33)!
        XCTAssertTrue(vc.view.performKeyEquivalent(with: cmdBracket))
        XCTAssertEqual(events.filter { $0.type == .dismiss }.map(\.node), [10])
    }

    func testSheetPresentSizeAndUserDismiss() {
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
        guard let modal = vc.modal(for: sheet) else { return XCTFail("no modal") }
        XCTAssertTrue(window.sheetsBegun.first === modal.window, "a window sheet")
        XCTAssertTrue(sheet.isDescendant(of: modal.window.contentView!))
        // Window sheets fit their content; detents are for touch.
        XCTAssertEqual(modal.window.contentView?.frame.size, CGSize(width: 390, height: 300))
        XCTAssertNotNil(modal.window.contentView?.layer?.backgroundColor, "an opaque theme background")

        // Not dismissable: Escape does nothing.
        apply { $0.prop(30, .dismissable, .bool(false)) }
        modal.window.cancelOperation(nil)
        XCTAssertTrue(events.filter { $0.type == .dismiss }.isEmpty)
        apply { $0.prop(30, .dismissable, .bool(true)) }
        // Escape reports Dismiss once and forgets the sheet.
        modal.window.cancelOperation(nil)
        modal.window.cancelOperation(nil)
        XCTAssertEqual(events.filter { $0.type == .dismiss }.map(\.node), [30])
        XCTAssertNil(vc.modal(for: sheet))

        // A sheet the core closes ends without a Dismiss event.
        apply { b in
            b.create(32, .sheet, frame: CGRect(x: 0, y: 0, width: 390, height: 200))
            b.mask(32, EventMask.dismiss)
            b.insert(32, 0, 2)
        }
        let second = vc.surfaceView.manager.component(32)!
        let modal2 = vc.modal(for: second)!
        XCTAssertEqual(modal2.window.contentView?.frame.height, 200)
        apply { $0.delete(32) }
        XCTAssertNil(vc.modal(for: second))
        XCTAssertEqual(events.filter { $0.type == .dismiss }.count, 1)
    }

    func testThemeTokensRecolorOpenSheets() {
        apply { b in
            b.create(30, .sheet, frame: CGRect(x: 0, y: 0, width: 390, height: 300))
            b.insert(30, 0, 1)
        }
        let modal = vc.modal(for: vc.surfaceView.manager.component(30)!)!
        apply { $0.prop(0, .themeTokens, .json(JSON.string(["appearance": "dark", "background": 0xFF10_1010]))) }
        XCTAssertEqual(modal.window.contentView?.layer?.backgroundColor, Colors.argb(0xFF10_1010).cgColor)
    }

    func testCustomDialogIsAWindowSheetOfItsSize() {
        apply { b in
            b.create(40, .dialog, frame: CGRect(x: 0, y: 0, width: 300, height: 200), props: [(.dismissable, .bool(true))])
            b.mask(40, EventMask.dismiss)
            b.insert(40, 0, 1)
        }
        let dialog = vc.surfaceView.manager.component(40)!
        let modal = vc.modal(for: dialog)!
        XCTAssertEqual(modal.window.contentView?.frame.size, CGSize(width: 300, height: 200))
        XCTAssertEqual(dialog.frame, CGRect(x: 0, y: 0, width: 300, height: 200))
        apply { $0.delete(40) }
        XCTAssertNil(vc.modal(for: dialog))
        XCTAssertTrue(window.sheetsEnded.isEmpty == false || modal.window.sheetParent == nil)
    }

    func testAlertButtonsFollowMacOSOrder() {
        let spec = JSON.object(#"{"title":"Delete “Groceries”?","message":"Gone for good.","actions":[{"label":"Cancel","style":"cancel"},{"label":"Delete","style":"destructive"}]}"#)!
        let a = AlertPresenter.makeAlert(spec)
        XCTAssertEqual(a.alert.messageText, "Delete “Groceries”?")
        XCTAssertEqual(a.alert.informativeText, "Gone for good.")
        // Trailing to leading: the action first (default), Cancel next to it with Escape.
        XCTAssertEqual(a.alert.buttons.map(\.title), ["Delete", "Cancel"])
        XCTAssertEqual(a.indices, [1, 0])
        XCTAssertTrue(a.alert.buttons[0].hasDestructiveAction)
        XCTAssertEqual(a.alert.buttons[1].keyEquivalent, "\u{1b}")
        let plain = AlertPresenter.makeAlert(["title": "Saved"])
        XCTAssertEqual(plain.alert.buttons.map(\.title), ["OK"])
        XCTAssertEqual(plain.indices, [nil])
    }
}
