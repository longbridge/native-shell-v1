// One script instance: a handle on the Rust core plus the host callbacks it
// calls on its JS thread (text and component measurement, host-module calls,
// batch notifications, logs).

import NativeShellShared
import NativeShellFFI
import UIKit

@objc public protocol NSHInstanceDelegate: AnyObject {
    /// Unrecoverable instance error; the instance keeps showing its last good frame.
    @objc optional func instance(_ instance: NSHInstance, didFailWithError message: String)
    @objc optional func instance(_ instance: NSHInstance, didLog message: String, level: NSHLogLevel)
    /// The program's first frame is on screen (hosts hide their loading state here).
    @objc optional func instanceDidRenderFirstFrame(_ instance: NSHInstance)
}

@objcMembers
public final class NSHInstanceConfig: NSObject {
    /// Optional when loading a bundle (taken from its manifest).
    public var appId: String?
    /// Where `localStorage` and `fs/promises` live. Default: Application Support/native-shell/<app id>.
    public var dataDirectory: URL?
    /// Relaxes eval/freeze for debugging only.
    public var dev = false
    /// Capabilities that override the manifest (host grants), as the manifest's JSON object.
    public var capabilities: [String: Any]?
    /// Script limits `{memory_mb, stack_kb, render_ms, event_ms, module_ms}`.
    public var limits: [String: Any]?

    public override init() {}

    public convenience init(appId: String?) {
        self.init()
        self.appId = appId
    }
}

@objcMembers
public final class NSHInstance: NSObject {
    public weak var delegate: NSHInstanceDelegate?
    public let config: NSHInstanceConfig
    public let runtime: NSHRuntime
    /// The surface this instance renders into.
    public internal(set) weak var surface: NSHSurfaceView?

    private(set) var handle: UInt64 = 0
    private var bridgeContext: UnsafeMutableRawPointer?
    var appId: String { config.appId ?? "" }

    /// Batches announced by the core and not yet taken; read on the main thread.
    var batchPending = false

    public init(config: NSHInstanceConfig, runtime: NSHRuntime = .shared) throws {
        self.config = config
        self.runtime = runtime
        super.init()
        let box = CallbackBridge(runtime: runtime)
        box.instance = self
        let context = BridgeRegistry.register(box)
        var callbacks = NSHHostCallbacks(
            context: context,
            measure_text: nshMeasureText,
            measure_text_batch: nshMeasureTextBatch,
            measure_component: nshMeasureComponent,
            batch_ready: nshBatchReady,
            call_sync: nshCallSync,
            call_async: nshCallAsync,
            log: nshLog,
            error: nshError
        )
        let json = Data(JSON.string(makeConfig()).utf8)
        let h = json.withUnsafeBytes { buf in
            nsh_instance_create(&callbacks, buf.bindMemory(to: UInt8.self).baseAddress, buf.count)
        }
        guard h != 0 else {
            BridgeRegistry.unregister(context)
            throw NSHInstance.error(String(cString: nsh_last_error()))
        }
        handle = h
        bridgeContext = context
        runtime.track(self)
    }

    deinit {
        destroy()
    }

    static func error(_ message: String) -> NSError {
        NSError(domain: "dev.nativeshell", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
    }

    private func makeConfig() -> [String: Any] {
        var c: [String: Any] = ["threading": 0, "dev": config.dev]
        if let id = config.appId { c["app_id"] = id }
        let dir = config.dataDirectory ?? NSHInstance.defaultDataDirectory(config.appId ?? "default")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        c["data_dir"] = dir.path
        if let caps = config.capabilities { c["capabilities"] = caps }
        if let limits = config.limits { c["limits"] = limits }
        c["registry"] = runtime.registryJSON()
        if let theme = runtime.themeJSON() { c["theme"] = theme }
        c["trusted_keys"] = runtime.trustedKeys.map { ["id": $0.id, "public_key": $0.publicKey] }
        c["revoked"] = runtime.revokedBundles
        c["environment"] = Environment.current(size: UIScreen.main.bounds.size, safeArea: .zero, traits: UIScreen.main.traitCollection, keyboard: 0)
        return c
    }

    static func defaultDataDirectory(_ appId: String) -> URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSTemporaryDirectory())
        let safe = appId.map { $0.isLetter || $0.isNumber || $0 == "." || $0 == "-" || $0 == "_" ? $0 : "_" }
        return base.appendingPathComponent("native-shell").appendingPathComponent(String(safe))
    }

    public var isAlive: Bool { handle != 0 }

    // MARK: Loading

    /// Loads plain sources: `{"manifest": {…}, "files": {"main.js": "…"}}`.
    public func loadSources(_ json: String) throws {
        guard handle != 0 else { throw NSHInstance.error("instance destroyed") }
        let data = Data(json.utf8)
        let rc = data.withUnsafeBytes { nsh_instance_load_sources(handle, $0.bindMemory(to: UInt8.self).baseAddress, $0.count) }
        if rc != 0 { throw NSHInstance.error(String(cString: nsh_last_error())) }
    }

    /// Loads a manifest and files given as objects.
    public func load(manifest: [String: Any], files: [String: String]) throws {
        try loadSources(JSON.string(["manifest": manifest, "files": files]))
    }

    /// Loads a program directory (`native-shell.json` and its files), e.g. one shipped
    /// in the app bundle. Text files go in as they are; anything that is not UTF-8
    /// (images, fonts) as a `data:<mime>;base64,…` URL, as `native-shell dev` sends
    /// them, so program-relative assets such as `image("assets/photo.jpg")` resolve.
    public func loadProgram(directory: URL) throws {
        try loadSources(NSHInstance.programSources(directory: directory))
    }

    /// `{"manifest", "files"}` JSON for a program directory (hidden entries and
    /// `node_modules` skipped).
    public static func programSources(directory: URL) throws -> String {
        try ProgramDirectory.sourcesJSON(directory)
    }

    /// Loads a signed `.nspkg` bundle; its signature is checked against `NSHRuntime.trustedKeys`.
    public func loadBundle(_ data: Data) throws {
        guard handle != 0 else { throw NSHInstance.error("instance destroyed") }
        let rc = data.withUnsafeBytes { nsh_instance_load_bundle(handle, $0.bindMemory(to: UInt8.self).baseAddress, $0.count) }
        if rc != 0 { throw NSHInstance.error(String(cString: nsh_last_error())) }
    }

    public func destroy() {
        guard handle != 0 else { return }
        let h = handle
        handle = 0
        runtime.untrack(self)
        nsh_instance_destroy(h)
        if let c = bridgeContext { BridgeRegistry.unregister(c) }
        bridgeContext = nil
    }

    /// Pauses timers (app in background) or resumes them.
    public func setActive(_ active: Bool) {
        guard handle != 0 else { return }
        nsh_instance_set_active(handle, active ? 1 : 0)
    }

    public func trimMemory() {
        guard handle != 0 else { return }
        nsh_instance_trim_memory(handle)
        TextMeasurer.shared.clear()
    }

    // MARK: Core calls

    func setEnvironment(_ env: [String: Any]) {
        guard handle != 0 else { return }
        let data = Data(JSON.string(env).utf8)
        data.withUnsafeBytes { nsh_instance_set_environment(handle, $0.bindMemory(to: UInt8.self).baseAddress, $0.count) }
    }

    func dispatchEvent(node: UInt32, type: EventType, payload: String, seq: UInt32) {
        guard handle != 0 else { return }
        let data = Data(payload.utf8)
        data.withUnsafeBytes {
            nsh_instance_dispatch_event(handle, node, type.rawValue, $0.bindMemory(to: UInt8.self).baseAddress, $0.count, seq)
        }
    }

    /// Pushes a host-module event to this instance.
    public func emit(module: String, event: String, payload: Any?) {
        guard handle != 0 else { return }
        let m = Data(module.utf8), e = Data(event.utf8), p = Data(JSON.string(payload).utf8)
        m.withUnsafeBytes { mb in
            e.withUnsafeBytes { eb in
                p.withUnsafeBytes { pb in
                    nsh_instance_host_emit(handle, mb.bindMemory(to: UInt8.self).baseAddress, mb.count,
                                          eb.bindMemory(to: UInt8.self).baseAddress, eb.count,
                                          pb.bindMemory(to: UInt8.self).baseAddress, pb.count)
                }
            }
        }
    }

    func respond(_ requestId: UInt64, ok: Bool, json: String) {
        guard handle != 0 else { return }
        let data = Data(json.utf8)
        data.withUnsafeBytes {
            nsh_instance_host_respond(handle, requestId, ok ? 1 : 0, $0.bindMemory(to: UInt8.self).baseAddress, $0.count)
        }
    }

    /// Takes every pending batch and hands each buffer to `apply`. Main thread.
    func drainBatches(_ apply: (UnsafeRawBufferPointer) -> Void) {
        batchPending = false
        guard handle != 0 else { return }
        while true {
            let buf = nsh_instance_take_batch(handle)
            guard buf.len > 0, let ptr = buf.ptr else {
                if buf.handle != 0 { nsh_buffer_free(buf) }
                break
            }
            apply(UnsafeRawBufferPointer(start: ptr, count: buf.len))
            nsh_buffer_free(buf)
        }
    }

    // MARK: Callbacks (main thread)

    func batchReady() {
        batchPending = true
        surface?.scheduleBatches()
    }

    func reportError(_ message: String) {
        runtime.log(.error, message)
        delegate?.instance?(self, didFailWithError: message)
    }

    func reportLog(_ level: NSHLogLevel, _ message: String) {
        runtime.log(level, message)
        delegate?.instance?(self, didLog: message, level: level)
    }

    /// Runtime counters (renders, phase timings in ns, measure calls/time, native nodes…).
    public var metrics: [String: Any] {
        guard handle != 0 else { return [:] }
        let buf = nsh_instance_metrics(handle)
        defer { if buf.handle != 0 { nsh_buffer_free(buf) } }
        guard let p = buf.ptr, buf.len > 0 else { return [:] }
        return JSON.object(String(decoding: UnsafeBufferPointer(start: p, count: buf.len), as: UTF8.self)) ?? [:]
    }

    /// The core's ABI hash ("stub" when the core is not built).
    public static var abiHash: String { String(cString: nsh_abi_hash()) }
    public static var protocolVersion: UInt32 { nsh_protocol_version() }
    /// Whether the real core is linked (false with the development stub).
    public static var coreAvailable: Bool { abiHash != "stub" }
}

// MARK: - C callbacks

/// What the C callbacks reach through their context.
final class CallbackBridge {
    let runtime: NSHRuntime
    weak var instance: NSHInstance?

    init(runtime: NSHRuntime) { self.runtime = runtime }
}

/// The callbacks' context is an id, not a pointer: a callback that arrives after
/// `destroy` (or races it) finds nothing instead of touching freed memory.
enum BridgeRegistry {
    private static let lock = NSLock()
    private static var next: UInt = 1
    private static var bridges: [UInt: CallbackBridge] = [:]

    static func register(_ bridge: CallbackBridge) -> UnsafeMutableRawPointer {
        lock.lock(); defer { lock.unlock() }
        let id = next
        next += 1
        bridges[id] = bridge
        return UnsafeMutableRawPointer(bitPattern: id)!
    }

    static func unregister(_ context: UnsafeMutableRawPointer) {
        lock.lock(); defer { lock.unlock() }
        bridges[UInt(bitPattern: context)] = nil
    }

    static func lookup(_ context: UnsafeMutableRawPointer?) -> CallbackBridge? {
        guard let c = context else { return nil }
        lock.lock(); defer { lock.unlock() }
        return bridges[UInt(bitPattern: c)]
    }
}

private func box(_ ctx: UnsafeMutableRawPointer?) -> CallbackBridge? {
    BridgeRegistry.lookup(ctx)
}

private func str(_ p: UnsafePointer<UInt8>?, _ n: Int) -> String {
    guard let p = p, n > 0 else { return "" }
    return String(decoding: UnsafeBufferPointer(start: p, count: n), as: UTF8.self)
}

private func onMain(_ f: @escaping () -> Void) {
    if Thread.isMainThread { f() } else { DispatchQueue.main.async(execute: f) }
}

let nshMeasureText: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, Int, Float, Int32) -> NSHTextMetrics = { _, json, len, width, mode in
    let m = TextMeasurer.shared.measure(str(json, len), width: CGFloat(width), mode: Int(mode))
    return NSHTextMetrics(width: Float(m.width), height: Float(m.height), baseline: Float(m.baseline))
}

/// Shapes every text of a layout pass in parallel; each worker thread uses its own TextKit stack.
let nshMeasureTextBatch: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<NSHMeasureRequest>?, Int, UnsafeMutablePointer<NSHTextMetrics>?) -> Void = { _, requests, count, out in
    guard let requests = requests, let out = out, count > 0 else { return }
    let measure = { (i: Int) in
        let r = requests[i]
        let m = TextMeasurer.shared.measure(str(r.attributed_json, r.len), width: CGFloat(r.width), mode: Int(r.width_mode))
        out[i] = NSHTextMetrics(width: Float(m.width), height: Float(m.height), baseline: Float(m.baseline))
    }
    if count < 8 {
        (0 ..< count).forEach(measure)
    } else {
        DispatchQueue.concurrentPerform(iterations: count, execute: measure)
    }
}

let nshMeasureComponent: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, Int, UnsafePointer<UInt8>?, Int, Float, Int32) -> NSHSize = { ctx, name, nameLen, props, propsLen, width, mode in
    let componentName = str(name, nameLen)
    let p = JSON.object(str(props, propsLen)) ?? [:]
    if componentName.hasPrefix("nsh/") {
        guard let s = BuiltinMeasure.measure(componentName, p) else { return NSHSize(width: 0, height: 0, has_value: 0) }
        let w = mode == 0 || !width.isFinite ? s.width : min(s.width, CGFloat(width))
        return NSHSize(width: Float(w), height: Float(s.height), has_value: 1)
    }
    guard let b = box(ctx), let type = b.runtime.componentType(componentName) else {
        return NSHSize(width: 0, height: 0, has_value: 0)
    }
    let maxWidth: CGFloat = mode == 0 || !width.isFinite ? .greatestFiniteMagnitude : CGFloat(width)
    guard let size = type.measure?(props: p, maxWidth: maxWidth) else { return NSHSize(width: 0, height: 0, has_value: 0) }
    return NSHSize(width: Float(size.width), height: Float(size.height), has_value: 1)
}

let nshBatchReady: @convention(c) (UnsafeMutableRawPointer?, UInt64) -> Void = { ctx, _ in
    guard let b = box(ctx) else { return }
    onMain { b.instance?.batchReady() }
}

let nshCallSync: @convention(c) (UnsafeMutableRawPointer?, UInt64, UnsafePointer<UInt8>?, Int, UnsafePointer<UInt8>?, Int, UnsafePointer<UInt8>?, Int, UnsafeMutablePointer<NSHBuffer>?) -> Int32 = { ctx, _, module, moduleLen, method, methodLen, args, argsLen, out in
    func answer(_ s: String, _ rc: Int32) -> Int32 {
        let data = Data(s.utf8)
        out?.pointee = data.withUnsafeBytes { nsh_string_new($0.bindMemory(to: UInt8.self).baseAddress, $0.count) }
        return rc
    }
    guard let b = box(ctx) else { return answer("instance is gone", 1) }
    let name = str(module, moduleLen), meth = str(method, methodLen)
    // Built-in: `cx.read_from_clipboard()`.
    if name == "nsh/clipboard" {
        guard meth == "read" else { return answer("nsh/clipboard has no method `\(meth)`", 1) }
        return answer(UIPasteboard.general.string.map { JSON.string($0) } ?? "null", 0)
    }
    guard let mod = b.runtime.module(name) else { return answer("host module `\(name)` is not registered", 1) }
    guard mod.responds(to: #selector(NSHHostModule.callSync(_:args:context:))) else {
        return answer("host module `\(name)` has no sync methods", 1)
    }
    let a = JSON.array(str(args, argsLen)) ?? []
    let context = NSHCallContext(appId: b.instance?.appId ?? "", module: name, instance: b.instance)
    do {
        let result = try mod.callSync!(meth, args: a, context: context)
        return answer(JSON.string(result), 0)
    } catch {
        return answer((error as NSError).localizedDescription, 1)
    }
}

let nshCallAsync: @convention(c) (UnsafeMutableRawPointer?, UInt64, UInt64, UnsafePointer<UInt8>?, Int, UnsafePointer<UInt8>?, Int, UnsafePointer<UInt8>?, Int) -> Void = { ctx, handle, requestId, module, moduleLen, method, methodLen, args, argsLen in
    guard let b = box(ctx) else { return }
    let name = str(module, moduleLen), meth = str(method, methodLen)
    let respond = { (ok: Bool, json: String) in
        let data = Data(json.utf8)
        data.withUnsafeBytes { nsh_instance_host_respond(handle, requestId, ok ? 1 : 0, $0.bindMemory(to: UInt8.self).baseAddress, $0.count) }
    }
    let responder = NSHResponder { ok, json in
        // The instance may be gone: then the VM is too and the promise never settles.
        guard let inst = b.instance, inst.isAlive else { return }
        respond(ok, json)
    }
    guard let mod = b.runtime.module(name), mod.responds(to: #selector(NSHHostModule.call(_:args:context:respond:))) else {
        return responder.reject("host module `\(name)` has no async method `\(meth)`")
    }
    let a = JSON.array(str(args, argsLen)) ?? []
    let context = NSHCallContext(appId: b.instance?.appId ?? "", module: name, instance: b.instance)
    mod.call!(meth, args: a, context: context, respond: responder)
}

let nshLog: @convention(c) (UnsafeMutableRawPointer?, UInt64, Int32, UnsafePointer<UInt8>?, Int) -> Void = { ctx, _, level, msg, len in
    guard let b = box(ctx) else { return }
    let message = str(msg, len)
    let l = NSHLogLevel(rawValue: Int(level)) ?? .info
    onMain {
        if let i = b.instance { i.reportLog(l, message) } else { b.runtime.log(l, message) }
    }
}

let nshError: @convention(c) (UnsafeMutableRawPointer?, UInt64, UnsafePointer<UInt8>?, Int) -> Void = { ctx, _, msg, len in
    guard let b = box(ctx) else { return }
    let message = str(msg, len)
    onMain {
        if let i = b.instance { i.reportError(message) } else { b.runtime.log(.error, message) }
    }
}
