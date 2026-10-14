@testable import NativeShellMac
import AppKit
import XCTest
import NativeShellShared

// MARK: - Host doubles for examples/watchlist (host.json is the contract)

final class DemoQuoteModule: NSObject, NSHHostModule {
    let name = "demo/quote"
    let methods: [NSHHostMethod] = [.async("quote"), .async("subscribe"), .async("unsubscribe")]
    let events = ["tick"]
    private let lock = NSLock()
    private var _listenerCounts: [String: Int] = [:]
    private var _subscribed: Set<String> = []
    private var _calls: [String] = []
    var listenerCounts: [String: Int] { lock.lock(); defer { lock.unlock() }; return _listenerCounts }
    var subscribed: Set<String> { lock.lock(); defer { lock.unlock() }; return _subscribed }
    var calls: [String] { lock.lock(); defer { lock.unlock() }; return _calls }

    func callSync(_ method: String, args: [Any], context: NSHCallContext) throws -> Any {
        lock.lock(); defer { lock.unlock() }
        _calls.append("\(method)\(JSON.string(args))")
        if method == "__listeners", let event = args.first as? String, let n = args.last as? Int { _listenerCounts[event] = n }
        return NSNull()
    }

    func call(_ method: String, args: [Any], context: NSHCallContext, respond: NSHResponder) {
        lock.lock()
        _calls.append(method)
        let symbols = args.first as? [String] ?? []
        if method == "subscribe" { _subscribed.formUnion(symbols) }
        if method == "unsubscribe" { _subscribed.subtract(symbols) }
        lock.unlock()
        switch method {
        case "quote":
            let symbol = args.first as? String ?? "?"
            DispatchQueue.global().async { respond.resolve(Self.quote(symbol, last: 100.5)) }
        case "subscribe", "unsubscribe":
            respond.resolve(nil)
        default:
            respond.reject("unknown method \(method)")
        }
    }

    /// `Quote {symbol, last, prev_close, change_percent}`.
    static func quote(_ symbol: String, last: Double, prevClose: Double = 100) -> [String: Any] {
        ["symbol": symbol, "last": last, "prev_close": prevClose, "change_percent": (last - prevClose) / prevClose * 100]
    }
}

final class DemoUserModule: NSObject, NSHHostModule {
    let name = "demo/user"
    let methods: [NSHHostMethod] = [.sync("current_user")]
    let requiresPermission = false

    func callSync(_ method: String, args: [Any], context: NSHCallContext) throws -> Any {
        ["id": "u1", "name": "Ada"]
    }
}

/// `demo-stock-cell` props `{symbol, name, last, change_percent}`, event `select`.
final class DemoStockCell: NSObject, NSHHostComponent {
    static let componentName = "demo-stock-cell"
    static let events = ["select"]
    let label = NSTextField(labelWithString: "")
    var eventSink: NSHEventSink?
    private(set) var props: [String: Any] = [:]

    override required init() { super.init() }
    var view: NSView { label }

    func update(props: [String: Any]) {
        self.props = props
        let last = (props["last"] as? NSNumber).map { String(format: "%.2f", $0.doubleValue) } ?? "—"
        label.stringValue = "\(props["name"] as? String ?? "") \(last)"
    }

    func prepareForReuse() { label.stringValue = "" }

    static func measure(props: [String: Any], maxWidth: CGFloat) -> CGSize { CGSize(width: maxWidth, height: 60) }
}

// MARK: - Tests

/// The real Rust core driving the renderer: example programs in a view controller in a window.
final class EndToEndTests: XCTestCase {
    static let repoRoot = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent()

    var window: SheetRecordingWindow!
    var instance: NSHInstance?
    var vc: NSHViewController!
    var errors: [String] = []
    var logs: [String] = []

    override func setUpWithError() throws {
        try XCTSkipUnless(NSHInstance.coreAvailable, "the core xcframework is not built (scripts/build-macos.sh)")
    }

    override func tearDown() {
        instance?.destroy()
        instance = nil
        window?.contentViewController = nil
        window?.close()
        window = nil
    }

    /// `{manifest, files}` of a program directory.
    static func sources(_ dir: URL) throws -> String {
        let manifest = try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("native-shell.json")))
        var files: [String: String] = [:]
        let e = FileManager.default.enumerator(at: dir, includingPropertiesForKeys: nil)
        while let url = e?.nextObject() as? URL {
            guard ["js", "mjs", "json"].contains(url.pathExtension), url.lastPathComponent != "native-shell.json",
                  url.lastPathComponent != "host.json" else { continue }
            let rel = String(url.path.dropFirst(dir.path.count + 1))
            files[rel] = try String(contentsOf: url, encoding: .utf8)
        }
        return JSON.string(["manifest": manifest, "files": files])
    }

    @discardableResult
    func launch(_ sources: String, runtime: NSHRuntime = NSHRuntime(), appId: String) throws -> NSHInstance {
        let config = NSHInstanceConfig(appId: appId)
        config.dataDirectory = FileManager.default.temporaryDirectory.appendingPathComponent("nsh-e2e-\(UUID().uuidString)")
        let inst = try NSHInstance(config: config, runtime: runtime)
        inst.delegate = self
        instance = inst
        window = SheetRecordingWindow(contentRect: CGRect(x: 0, y: 0, width: 390, height: 844), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        vc = NSHViewController(instance: inst, runtime: runtime)
        window.contentViewController = vc
        window.setContentSize(CGSize(width: 390, height: 844))
        vc.view.layoutSubtreeIfNeeded()
        try inst.loadSources(sources)
        return inst
    }

    // MARK: Inspection

    var components: [NSHComponentView] {
        var out: [NSHComponentView] = []
        func walk(_ v: NSView) {
            if let c = v as? NSHComponentView { out.append(c) }
            v.subviews.forEach(walk)
        }
        walk(vc.view)
        for (_, node) in vc.surfaceView.manager.nodes where !out.contains(where: { $0 === node }) { out.append(node) }
        return out
    }

    func view(_ testId: String) -> NSHComponentView? {
        components.first { $0.accessibilityIdentifier() == testId }
    }

    var texts: [String] {
        components.compactMap { ($0 as? NSHTextView)?.parsed?.string.string }
    }

    /// The text inside a test id'd view (its own text or its first descendant's).
    func text(_ testId: String) -> String? {
        guard let v = view(testId) else { return nil }
        if let t = v as? NSHTextView { return t.parsed?.string.string }
        var found: String?
        func walk(_ c: NSHComponentView) {
            if found != nil { return }
            if let t = c as? NSHTextView { found = t.parsed?.string.string; return }
            c.children.forEach(walk)
        }
        walk(v)
        return found
    }

    func waitFor(_ what: String, timeout: TimeInterval = 5, _ cond: @escaping () -> Bool, file: StaticString = #filePath, line: UInt = #line) {
        let ok = wait(timeout, until: cond)
        XCTAssertTrue(ok, "timed out waiting for \(what); errors: \(errors) texts: \(texts.prefix(30))", file: file, line: line)
    }

    func tap(_ testId: String, file: StaticString = #filePath, line: UInt = #line) {
        guard let v = view(testId) else { return XCTFail("no view \(testId)", file: file, line: line) }
        XCTAssertTrue(v.accessibilityPerformPress(), "\(testId) is not clickable", file: file, line: line)
    }

    // MARK: Programs

    func input(_ testId: String) -> NSHTextInputView? { view(testId) as? NSHTextInputView }

    func type(_ text: String, into testId: String, file: StaticString = #filePath, line: UInt = #line) {
        guard let field = input(testId) else { return XCTFail("no input \(testId)", file: file, line: line) }
        field.focus()
        let editor = (field.field?.currentEditor() as? NSTextView) ?? field.textView!
        editor.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
        // Let the core see the change before the next action.
        waitFor("core saw \(testId)", timeout: 5, { (self.view(testId) as? NSHTextInputView)?.text == text }, file: file, line: line)
        spin(0.2)
    }

    /// The id of the task titled `title` (rows carry `title-<id>`).
    func taskId(_ title: String) -> String? {
        components.first { $0.accessibilityIdentifier().hasPrefix("title-") && text($0.accessibilityIdentifier()) == title }
            .map { String($0.accessibilityIdentifier().dropFirst("title-".count)) }
    }

    /// The open-task count in the large title's subtitle ("7 to do · 1 overdue").
    var openCount: Int? {
        texts.lazy.compactMap { t -> Int? in
            guard let r = t.range(of: " to do") else { return nil }
            return Int(t[..<r.lowerBound])
        }.first
    }

    func testTasksAddEditUndoAndComplete() throws {
        try launch(Self.sources(Self.repoRoot.appendingPathComponent("examples/tasks")), appId: "dev.native-shell.tasks")
        waitFor("first frame") { self.texts.contains("Tasks") && self.input("draft") != nil && self.openCount != nil }
        XCTAssertEqual(input("draft")?.field?.placeholderAttributedString?.string, "Add a task")
        let start = openCount!

        type("Buy milk", into: "draft")
        tap("add")
        waitFor("the new task") { self.taskId("Buy milk") != nil }
        waitFor("the field cleared by the script") { self.input("draft")?.text.isEmpty == true }
        waitFor("count +1") { self.openCount == start + 1 }
        // Return adds too and keeps typing going.
        type("Walk the dog", into: "draft")
        (input("draft")!.field!.currentEditor() as! NSTextView).doCommand(by: #selector(NSResponder.insertNewline(_:)))
        waitFor("second task") { self.taskId("Walk the dog") != nil }
        waitFor("count +2") { self.openCount == start + 2 }

        // Edit sheet: delete, then undo.
        let milk = taskId("Buy milk")!
        tap("task-\(milk)")
        waitFor("editor sheet") { self.view("editor") != nil && self.input("editor-title")?.text == "Buy milk" }
        tap("editor-delete")
        waitFor("deleted, undo offered") { self.taskId("Buy milk") == nil && self.view("undo") != nil }
        XCTAssertEqual(text("undo-label"), "Deleted “Buy milk”")
        tap("undo")
        waitFor("restored") { self.taskId("Buy milk") == milk && self.view("undo") == nil }

        // Complete one: the checkbox reports, the task moves under Completed.
        let dog = taskId("Walk the dog")!
        guard let check = view("check-\(dog)") else { return XCTFail("no check-\(dog)") }
        if let box = check as? NSHCheckboxView { box.toggle() } else { XCTAssertTrue(check.accessibilityPerformPress()) }
        waitFor("one fewer open") { self.openCount == start + 1 }
        waitFor("completed header") { self.view("toggle-completed") != nil }
        XCTAssertTrue(errors.isEmpty, "\(errors)")
    }

    func testComponentsPagesPushAndPop() throws {
        try launch(Self.sources(Self.repoRoot.appendingPathComponent("examples/components")), appId: "dev.native-shell.components")
        waitFor("index") { self.view("index") != nil && self.view("open-buttons") != nil }
        var types = Set(components.map(\.viewType))
        for page in ["buttons", "fields", "selection", "progress", "pickers", "overlays"] {
            tap("open-\(page)")
            waitFor("\(page) pushed") { self.vc.stackedPages.count == 1 && self.view("page") != nil }
            types.formUnion(components.map(\.viewType))
            vc.performBack()
            waitFor("\(page) popped") { self.vc.stackedPages.isEmpty }
            waitFor("\(page) gone") { self.view("page") == nil }
        }
        // Native controls the pages use at rest (spinners and progress appear only during an upload).
        for t in [NSHViewType.text, .scroll, .textInput, .switchView, .slider, .segmented, .select, .datePicker, .shape] {
            XCTAssertTrue(types.contains(t), "components mounts \(t)")
        }
        XCTAssertTrue(errors.isEmpty, "\(errors)")
    }

    func testWatchlistModulesAndComponents() throws {
        let rt = NSHRuntime()
        let quote = DemoQuoteModule()
        rt.register(module: quote)
        rt.register(module: DemoUserModule())
        rt.register(component: DemoStockCell.self)
        try launch(Self.sources(Self.repoRoot.appendingPathComponent("examples/watchlist")), runtime: rt, appId: "dev.native-shell.watchlist")
        waitFor("live after async quotes + subscribe") { self.text("status") == "Live" }
        let cells = { self.components.compactMap { ($0 as? NSHHostComponentView)?.component as? DemoStockCell } }
        waitFor("cells with quotes") { cells().count >= 6 && cells().allSatisfy { $0.props["last"] as? Double == 100.5 } }
        XCTAssertEqual(quote.subscribed.count, 6)
        XCTAssertEqual(cells().first { $0.props["symbol"] as? String == "HALO.US" }?.props["name"] as? String, "Halo Systems")
        waitFor("tick listener") { quote.listenerCounts["tick"] == 1 }

        // A pushed tick re-renders that cell.
        rt.emit(module: "demo/quote", event: "tick", payload: DemoQuoteModule.quote("KITE.US", last: 123.4))
        waitFor("tick applied") { cells().contains { $0.props["symbol"] as? String == "KITE.US" && $0.props["last"] as? Double == 123.4 } }

        // A component event opens the detail sheet with that quote.
        cells().first { $0.props["symbol"] as? String == "KITE.US" }?.eventSink?.emit("select", payload: ["symbol": "KITE.US"])
        waitFor("detail sheet") { self.text("detail-price")?.contains("123.4") == true }

        // Pausing drops the last listener; resuming listens again.
        tap("live")
        waitFor("paused") { self.text("status") == "Paused" && quote.listenerCounts["tick"] == 0 }
        rt.emit(module: "demo/quote", event: "tick", payload: DemoQuoteModule.quote("KITE.US", last: 999))
        spin(0.3)
        XCTAssertFalse(cells().contains { $0.props["last"] as? Double == 999 }, "no listener, no update")
        tap("live")
        waitFor("live again") { self.text("status") == "Live" && quote.listenerCounts["tick"] == 1 }
        XCTAssertTrue(errors.isEmpty, "\(errors)")
    }

    func testScriptErrorsAreReported() throws {
        let src = JSON.string(["manifest": ["id": "dev.test.broken", "name": "Broken"],
                               "files": ["main.js": "export default class X { render() { throw new Error('boom') } }"]])
        try launch(src, appId: "dev.test.broken")
        waitFor("an error") { !self.errors.isEmpty || self.logs.contains { $0.contains("boom") } }
    }

    // MARK: Timing

    func bigProgram(rows: Int) -> String {
        let js = """
        import { View, div, v_flex } from "native-shell";
        export default class Big extends View {
          init() { this.n = 0; }
          render(cx) {
            const rows = [];
            for (let i = 0; i < \(rows); i++) {
              rows.push(div().h(20).bg(i % 2 ? "muted" : "surface").child(`Row ${i} · ${this.n}`));
            }
            return v_flex().size_full()
              .child(div().test_id("bump").h(44).bg("primary").on_click((_e, cx) => { this.n++; cx.notify(); }))
              .child(v_flex().test_id("rows").flex_1().overflow_y_scroll().children(rows));
          }
        }
        """
        return JSON.string(["manifest": ["id": "dev.test.big\(rows)", "name": "Big"], "files": ["main.js": js]])
    }

    func testFirstFrameAndUpdateTimings() throws {
        var report: [String] = []
        for nodes in [1000, 3000, 10000] {
            let rows = nodes / 2 // each row: a painted View and its Text
            let t0 = ProcessInfo.processInfo.systemUptime
            try launch(bigProgram(rows: rows), appId: "dev.test.big\(rows)")
            waitFor("first frame of \(nodes)", timeout: 60) { self.vc.surfaceView.mountedViewCount >= nodes }
            let first = ProcessInfo.processInfo.systemUptime - t0
            let applyFirst = vc.surfaceView.manager.lastApplyMs
            let mounted = vc.surfaceView.mountedViewCount

            let t1 = ProcessInfo.processInfo.systemUptime
            tap("bump")
            let lastRow = "Row \(rows - 1) · 1"
            waitFor("update of \(nodes)", timeout: 60) { self.vc.surfaceView.manager.textsContain(lastRow) }
            let update = ProcessInfo.processInfo.systemUptime - t1
            let applyUpdate = vc.surfaceView.manager.lastApplyMs
            report.append(String(format: "%5d nodes (mounted %5d): first frame %7.1f ms (main-thread apply %6.1f ms), update %7.1f ms (apply %6.1f ms)",
                                 nodes, mounted, first * 1000, applyFirst, update * 1000, applyUpdate))
            if let m = instance?.metrics, !m.isEmpty {
                let keys = m.keys.sorted()
                report.append("      core metrics: " + keys.map { "\($0)=\(m[$0]!)" }.joined(separator: " "))
            }
            tearDown()
        }
        print("TIMINGS (real core)\n" + report.joined(separator: "\n"))
    }
}

extension EndToEndTests: NSHInstanceDelegate {
    func instance(_ instance: NSHInstance, didFailWithError message: String) { errors.append(message) }
    func instance(_ instance: NSHInstance, didLog message: String, level: NSHLogLevel) { logs.append(message) }
}

extension MountingManager {
    func textsContain(_ s: String) -> Bool {
        nodes.values.contains { ($0 as? NSHTextView)?.parsed?.string.string == s }
    }
}
