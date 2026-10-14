@testable import NativeShellMac
import AppKit
import NativeShellFFI
import XCTest

// MARK: - Test doubles of what a host app registers

final class EchoModule: NSObject, NSHHostModule {
    let name = "test/echo"
    let methods: [NSHHostMethod] = [.sync("echo", typeScript: "(x: any) => any"), .sync("fail"), .async("later"), .async("reject")]
    let events = ["ping"]
    var listenerCalls: [[Any]] = []
    var lastContext: NSHCallContext?

    func callSync(_ method: String, args: [Any], context: NSHCallContext) throws -> Any {
        lastContext = context
        switch method {
        case "echo": return ["args": args, "app": context.appId]
        case "__listeners":
            listenerCalls.append(args)
            return NSNull()
        default: throw NSError(domain: "test", code: 1, userInfo: [NSLocalizedDescriptionKey: "no such method: \(method)"])
        }
    }

    func call(_ method: String, args: [Any], context: NSHCallContext, respond: NSHResponder) {
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.01) {
            if method == "later" { respond.resolve(["got": args]) } else { respond.reject("nope") }
            respond.resolve("ignored: answered twice")
        }
    }
}

final class BadgeComponent: NSObject, NSHHostComponent {
    static let componentName = "test-badge"
    static let events = ["select"]
    static var created = 0
    let label = NSTextField(labelWithString: "")
    var eventSink: NSHEventSink?
    var props: [String: Any] = [:]
    var resets = 0

    override required init() {
        BadgeComponent.created += 1
        super.init()
    }

    var view: NSView { label }

    func update(props: [String: Any]) {
        self.props = props
        label.stringValue = props["text"] as? String ?? ""
    }

    func prepareForReuse() {
        resets += 1
        label.stringValue = ""
        props = [:]
    }

    static func measure(props: [String: Any], maxWidth: CGFloat) -> CGSize {
        let n = CGFloat(((props["text"] as? String) ?? "").count)
        return CGSize(width: min(maxWidth, n * 10 + 16), height: 28)
    }
}

final class PlainComponent: NSObject, NSHHostComponent {
    static let componentName = "test-plain"
    let view = NSView()
    override required init() { super.init() }
    func update(props: [String: Any]) {}
}

// MARK: - Tests

final class HostTests: XCTestCase {
    func testRegistryDescriptors() {
        let rt = NSHRuntime()
        rt.register(module: EchoModule())
        rt.register(component: BadgeComponent.self)
        rt.register(component: PlainComponent.self)
        let reg = rt.registryJSON()
        let modules = reg["modules"] as! [[String: Any]]
        XCTAssertEqual(modules.first?.string("name"), "test/echo")
        XCTAssertEqual(modules.first?.bool("requires_permission"), true)
        let methods = modules.first?["methods"] as! [[String: Any]]
        XCTAssertEqual(methods.map { $0.string("kind") }, ["sync", "sync", "async", "async"])
        XCTAssertEqual(methods.first?.string("ts"), "(x: any) => any")
        XCTAssertEqual(modules.first?["events"] as? [String], ["ping"])
        let comps = reg["components"] as! [[String: Any]]
        XCTAssertEqual(comps.map { $0.string("name") }, ["test-badge", "test-plain"])
        XCTAssertEqual(comps.map { $0.bool("measurable") }, [true, false])
        XCTAssertEqual(comps.first?["events"] as? [String], ["select"])
        // The descriptors are valid JSON for the core.
        XCTAssertNotEqual(JSON.string(reg), "null")

        rt.themeProvider = { $0 == .dark ? ["primary": .white] : ["primary": NSColor(srgbRed: 0.145, green: 0.388, blue: 0.922, alpha: 1)] }
        let theme = rt.themeJSON()!
        XCTAssertEqual((theme["light"] as? [String: String])?["primary"], "#2563eb")
        XCTAssertEqual((theme["dark"] as? [String: String])?["primary"], "#ffffff")
    }

    /// Calls the C `call_sync` callback the way the core does.
    private func callSync(_ bridge: CallbackBridge, _ module: String, _ method: String, _ args: String) -> (Int32, String) {
        let ctx = BridgeRegistry.register(bridge)
        var out = NSHBuffer(ptr: nil, len: 0, handle: 0)
        let m = Array(module.utf8), meth = Array(method.utf8), a = Array(args.utf8)
        let rc = nshCallSync(ctx, 0, m, m.count, meth, meth.count, a, a.count, &out)
        var text = ""
        if let p = out.ptr { text = String(decoding: UnsafeBufferPointer(start: p, count: out.len), as: UTF8.self) }
        if out.handle != 0 { nsh_buffer_free(out) }
        return (rc, text)
    }

    func testSyncCallsRouteToModules() throws {
        try XCTSkipUnless(NSHInstance.coreAvailable, "nsh_string_new needs the core")
        let rt = NSHRuntime()
        let echo = EchoModule()
        rt.register(module: echo)
        let bridge = CallbackBridge(runtime: rt)
        let (rc, json) = callSync(bridge, "test/echo", "echo", #"[1,"two",{"three":3}]"#)
        XCTAssertEqual(rc, 0)
        let obj = JSON.object(json)
        XCTAssertEqual((obj?["args"] as? [Any])?.count, 3)
        XCTAssertEqual(echo.lastContext?.module, "test/echo")

        let (rc2, err) = callSync(bridge, "test/echo", "fail", "[]")
        XCTAssertEqual(rc2, 1)
        XCTAssertEqual(err, "no such method: fail")
        let (rc3, missing) = callSync(bridge, "test/missing", "x", "[]")
        XCTAssertEqual(rc3, 1)
        XCTAssertTrue(missing.contains("not registered"))
        _ = callSync(bridge, "test/echo", "__listeners", #"["ping", 1]"#)
        XCTAssertEqual(echo.listenerCalls.first?.first as? String, "ping")
        // Built-in clipboard read (no paste prompt in tests: only the call shape is checked).
        let (rc4, _) = callSync(bridge, "nsh/clipboard", "nope", "[]")
        XCTAssertEqual(rc4, 1)
    }

    func testCallbacksAfterDestroyFindNothing() {
        let bridge = CallbackBridge(runtime: NSHRuntime())
        let ctx = BridgeRegistry.register(bridge)
        XCTAssertTrue(BridgeRegistry.lookup(ctx) === bridge)
        BridgeRegistry.unregister(ctx)
        XCTAssertNil(BridgeRegistry.lookup(ctx))
        // A late log or error callback is a no-op, not a use-after-free.
        let msg = Array("late".utf8)
        nshLog(ctx, 0, 1, msg, msg.count)
        nshError(ctx, 0, msg, msg.count)
        nshBatchReady(ctx, 0)
        spin(0.05)
    }

    func testResponderAnswersExactlyOnce() {
        var answers: [(Bool, String)] = []
        let lock = NSLock()
        let r = NSHResponder { ok, json in lock.lock(); answers.append((ok, json)); lock.unlock() }
        DispatchQueue.concurrentPerform(iterations: 16) { i in
            if i % 2 == 0 { r.resolve(["i": i]) } else { r.reject("e\(i)") }
        }
        XCTAssertEqual(answers.count, 1)
        XCTAssertTrue(r.isAnswered)
    }

    func testAsyncCallRoutesToModuleAndRejectsUnknown() {
        let rt = NSHRuntime()
        rt.register(module: EchoModule())
        let bridge = CallbackBridge(runtime: rt)
        let ctx = BridgeRegistry.register(bridge)
        let m = Array("test/missing".utf8), meth = Array("x".utf8), a = Array("[]".utf8)
        // No instance: the responder drops the answer (the VM is gone), and nothing crashes.
        nshCallAsync(ctx, 0, 1, m, m.count, meth, meth.count, a, a.count)
        let m2 = Array("test/echo".utf8), meth2 = Array("later".utf8)
        nshCallAsync(ctx, 0, 2, m2, m2.count, meth2, meth2.count, a, a.count)
        spin(0.1)
    }

    func testHostComponentPropsEventsPoolingAndMeasure() {
        let rt = NSHRuntime()
        rt.register(component: BadgeComponent.self)
        let h = Harness(runtime: rt)
        defer { h.tearDown() }
        BadgeComponent.created = 0
        h.apply { b in
            b.create(1, .hostComponent, props: [(.hostComponent, .str("test-badge")), (.hostProps, .json(#"{"text":"AAPL","n":1}"#))])
            b.frame(1, CGRect(x: 0, y: 0, width: 120, height: 28))
            b.mask(1, EventMask.hostEvent)
            b.insert(1, 0, 0)
        }
        let v = h.view(1, as: NSHHostComponentView.self)
        let c = v.component as! BadgeComponent
        XCTAssertEqual(c.label.stringValue, "AAPL")
        XCTAssertEqual(c.props["n"] as? Int, 1)
        XCTAssertTrue(c.label.superview === v)
        v.layoutSubtreeIfNeeded()
        XCTAssertEqual(c.label.frame, CGRect(x: 0, y: 0, width: 120, height: 28))
        XCTAssertNotNil(c.eventSink)
        c.eventSink?.emit("select", payload: ["symbol": "AAPL"])
        let e = h.events(.hostEvent).first
        XCTAssertEqual(e?.payload.string("name"), "select")
        XCTAssertEqual((e?.payload["payload"] as? [String: Any])?.string("symbol"), "AAPL")
        // Events from another thread are delivered on the main thread.
        DispatchQueue.global().async { c.eventSink?.emit("select", payload: nil) }
        XCTAssertTrue(wait { h.events(.hostEvent).count == 2 })

        // Pooled per name: deleting and re-creating reuses the component after prepareForReuse.
        h.apply { $0.delete(1) }
        XCTAssertEqual(c.resets, 1)
        XCTAssertEqual(c.label.stringValue, "")
        h.apply { b in
            b.create(2, .hostComponent, props: [(.hostComponent, .str("test-badge")), (.hostProps, .json(#"{"text":"TSLA"}"#))])
            b.insert(2, 0, 0)
        }
        let v2 = h.view(2, as: NSHHostComponentView.self)
        XCTAssertTrue(v2.component === c)
        XCTAssertEqual(c.label.stringValue, "TSLA")
        XCTAssertEqual(BadgeComponent.created, 1)

        // Unknown component: an empty container, an error log, no crash.
        h.apply { b in b.create(3, .hostComponent, parent: 0, props: [(.hostComponent, .str("nope"))]) }
        XCTAssertNil(h.view(3, as: NSHHostComponentView.self).component)

        // measure_component routes to the static measure on the calling (JS) thread.
        let bridge = CallbackBridge(runtime: rt)
        let ctx = BridgeRegistry.register(bridge)
        let name = Array("test-badge".utf8), props = Array(#"{"text":"ABCD"}"#.utf8)
        var size = NSHSize(width: 0, height: 0, has_value: 0)
        let done = expectation(description: "measured")
        Thread.detachNewThread {
            size = nshMeasureComponent(ctx, name, name.count, props, props.count, 300, 1)
            done.fulfill()
        }
        wait(for: [done], timeout: 2)
        XCTAssertEqual(size.has_value, 1)
        XCTAssertEqual(size.width, 56)
        XCTAssertEqual(size.height, 28)
        let unknown = Array("nope".utf8)
        XCTAssertEqual(nshMeasureComponent(ctx, unknown, unknown.count, props, props.count, 300, 1).has_value, 0)
    }

    func testMeasureTextCallback() {
        let json = Array(textJSON("Hello from the core", size: 16).utf8)
        let m = nshMeasureText(nil, json, json.count, .infinity, 0)
        XCTAssertGreaterThan(m.width, 50)
        XCTAssertGreaterThan(m.height, 10)
        XCTAssertGreaterThan(m.baseline, 0)
    }

    func testMeasureTextBatchEqualsSingleMeasurements() {
        let jsons = (0 ..< 64).map { Array(textJSON("Batch \($0) " + String(repeating: "word ", count: $0 % 7), size: 14 + Double($0 % 5)).utf8) }
        var out = [NSHTextMetrics](repeating: NSHTextMetrics(width: 0, height: 0, baseline: 0), count: jsons.count)
        withExtendedLifetime(jsons) {
            let requests = jsons.map { j in
                j.withUnsafeBufferPointer { NSHMeasureRequest(attributed_json: $0.baseAddress, len: j.count, width: 120, width_mode: 1) }
            }
            TextMeasurer.shared.clear()
            nshMeasureTextBatch(nil, requests, requests.count, &out)
        }
        for (i, j) in jsons.enumerated() {
            let single = TextMeasurer().measure(String(decoding: j, as: UTF8.self), width: 120, mode: 1)
            XCTAssertEqual(CGFloat(out[i].width), single.width, accuracy: 0.001)
            XCTAssertEqual(CGFloat(out[i].height), single.height, accuracy: 0.001)
        }
    }
}
