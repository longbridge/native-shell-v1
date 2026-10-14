// Drives every example program through its key screens in light and dark
// appearance and saves the screenshots to Example/screenshots/ as
// `<example>-<screen>-<light|dark>.png` (also attached to the test result).
// `-example <name> -reset` opens a program directly with empty storage.

import XCTest

final class ExampleUITests: XCTestCase {
    static let screenshots = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("screenshots")

    private var appearance = "light"

    override func setUp() {
        continueAfterFailure = false
        try? FileManager.default.createDirectory(at: Self.screenshots, withIntermediateDirectories: true)
    }

    override func tearDown() {
        XCUIDevice.shared.appearance = .light
    }

    /// Runs `body` once per appearance, relaunching the app each time.
    private func inBothAppearances(_ body: () -> Void) {
        for mode in [XCUIDevice.Appearance.light, .dark] {
            XCUIDevice.shared.appearance = mode
            appearance = mode == .dark ? "dark" : "light"
            body()
        }
    }

    private func launch(_ example: String?, extra: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = (example.map { ["-example", $0, "-reset"] } ?? []) + extra
        app.launch()
        return app
    }

    private func snap(_ name: String) {
        // Let transitions and the keyboard settle before capturing.
        Thread.sleep(forTimeInterval: 0.8)
        let shot = XCUIScreen.main.screenshot()
        let file = "\(name)-\(appearance)"
        let attachment = XCTAttachment(screenshot: shot)
        attachment.name = file
        attachment.lifetime = .keepAlways
        add(attachment)
        try? shot.pngRepresentation.write(to: Self.screenshots.appendingPathComponent("\(file).png"))
    }

    private func element(_ app: XCUIApplication, _ id: String) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: id).firstMatch
    }

    @discardableResult
    private func wait(_ app: XCUIApplication, _ id: String, timeout: TimeInterval = 10) -> XCUIElement {
        let e = element(app, id)
        if !e.waitForExistence(timeout: timeout) {
            snap("failed-\(id)")
            XCTFail("\(id) appears")
        }
        return e
    }

    private func tap(_ app: XCUIApplication, _ id: String) {
        wait(app, id).tap()
    }

    /// Dismisses a sheet the platform way: a swipe down on it.
    private func dismissSheet(_ app: XCUIApplication, _ id: String) {
        let sheet = element(app, id)
        let start = sheet.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.05))
        start.press(forDuration: 0.05, thenDragTo: start.withOffset(CGVector(dx: 0, dy: 600)))
        XCTAssertTrue(sheet.waitForNonExistence(timeout: 5), "\(id) dismissed")
    }

    private func back(_ app: XCUIApplication) {
        tap(app, "back")
    }

    func testLauncher() {
        inBothAppearances {
            let app = launch(nil)
            XCTAssertTrue(app.navigationBars["Examples"].waitForExistence(timeout: 10))
            snap("launcher")
            // A program opens on the stack and the edge swipe returns to the list.
            app.cells["example-tasks"].tap()
            wait(app, "tasks")
            let edge = app.coordinate(withNormalizedOffset: CGVector(dx: 0.0, dy: 0.5))
            edge.press(forDuration: 0.05, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)))
            XCTAssertTrue(app.cells["example-tasks"].waitForExistence(timeout: 5), "back on the launcher")
        }
    }

    func testTasks() {
        inBothAppearances {
            let app = launch("tasks")
            wait(app, "task-1")
            tap(app, "toggle-completed")
            snap("tasks")
            tap(app, "task-3")
            wait(app, "editor")
            snap("tasks-edit")
            tap(app, "editor-delete")
            wait(app, "undo-bar")
            snap("tasks-undo")
            tap(app, "undo")
            wait(app, "task-3")
            let field = wait(app, "draft")
            field.tap()
            field.typeText("Water the plants")
            snap("tasks-compose")
        }
    }

    func testSettings() {
        inBothAppearances {
            let app = launch("settings")
            wait(app, "account")
            snap("settings")
            tap(app, "account")
            let email = wait(app, "email")
            snap("settings-account")
            email.tap()
            email.typeText("@\n")
            wait(app, "email-error")
            snap("settings-account-error")
            back(app)
            XCTAssertTrue(app.alerts.firstMatch.waitForExistence(timeout: 5), "leaving with edits asks first")
            app.alerts.firstMatch.buttons["Discard changes"].tap()
            let scroll = wait(app, "settings")
            let clear = element(app, "clear-cache")
            scroll.swipeUp()
            for _ in 0 ..< 3 where !clear.isHittable { scroll.swipeUp() }
            snap("settings-bottom")
            clear.tap()
            XCTAssertTrue(app.alerts.firstMatch.waitForExistence(timeout: 5))
            snap("settings-clear-cache")
            app.alerts.firstMatch.buttons["Clear cache"].tap()
        }
    }

    func testChat() {
        inBothAppearances {
            let app = launch("chat")
            wait(app, "bubble-m10")
            snap("chat")
            tap(app, "bubble-m10")
            wait(app, "actions")
            snap("chat-actions")
            tap(app, "action-copy")
            let field = wait(app, "draft")
            field.tap()
            field.typeText("Great, see you at the station")
            snap("chat-compose")
            app.keyboards.buttons["send"].firstMatch.tap()
            wait(app, "delivery")
        }
    }

    func testWatchlist() {
        inBothAppearances {
            let app = launch("watchlist")
            wait(app, "cell-HALO.US")
            Thread.sleep(forTimeInterval: 2)
            snap("watchlist")
            tap(app, "cell-KITE.US")
            wait(app, "detail")
            snap("watchlist-detail")
            dismissSheet(app, "detail")
            tap(app, "add")
            wait(app, "add-sheet")
            snap("watchlist-add")
        }
    }

    func testComponents() {
        inBothAppearances {
            let app = launch("components")
            wait(app, "open-buttons")
            snap("components")

            tap(app, "open-buttons")
            let post = wait(app, "post")
            post.tap()
            post.typeText("The tomatoes finally came in.")
            tap(app, "publish")
            wait(app, "publish")
            snap("components-buttons")
            back(app)

            tap(app, "open-fields")
            for (id, text) in [("name", "Ada Lovelace"), ("street", "12 Harbour Street"), ("city", "Springfield"), ("postal", "123")] {
                let f = wait(app, id)
                f.tap()
                f.typeText(text)
            }
            tap(app, "save-address")
            wait(app, "postal-error")
            snap("components-fields")
            back(app)

            tap(app, "open-selection")
            tap(app, "email-digest")
            tap(app, "speed-express")
            snap("components-selection")
            back(app)

            tap(app, "open-progress")
            tap(app, "upload-start")
            Thread.sleep(forTimeInterval: 2)
            snap("components-progress")
            back(app)

            tap(app, "open-pickers")
            wait(app, "summary")
            snap("components-pickers")
            back(app)

            tap(app, "open-overlays")
            wait(app, "doc-name")
            snap("components-overlays")
            tap(app, "share")
            wait(app, "share-sheet")
            snap("components-overlays-sheet")
            dismissSheet(app, "share-sheet")
            tap(app, "rename")
            wait(app, "rename-dialog")
            snap("components-overlays-dialog")
            tap(app, "rename-cancel")
        }
    }

    func testGpuiTodolist() {
        inBothAppearances {
            let app = launch("gpui-todolist")
            let field = app.textFields.firstMatch
            XCTAssertTrue(field.waitForExistence(timeout: 10))
            for item in ["Buy milk", "Walk the dog"] {
                field.tap()
                field.typeText(item + "\n")
            }
            snap("gpui-todolist")
        }
    }

    /// Text at the largest accessibility size: layouts grow instead of clipping.
    func testLargeText() {
        let large = ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXL"]
        for example in ["tasks", "settings", "chat", "watchlist", "components"] {
            let app = launch(example, extra: large)
            Thread.sleep(forTimeInterval: 2)
            snap("\(example)-large-text")
            app.terminate()
        }
    }
}
