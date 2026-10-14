// What the host app implements to extend scripts: host modules (JS-importable
// modules answered natively) and host components (native views placed by
// scripts). Every type is Objective-C compatible.

import AppKit

@objc public enum NSHMethodKind: Int {
    case sync
    case async
}

@objc public enum NSHAppearance: Int {
    case light
    case dark
}

@objc public enum NSHLogLevel: Int {
    case debug = 0
    case info = 1
    case warn = 2
    case error = 3
}

/// One method of a host module.
@objcMembers
public final class NSHHostMethod: NSObject {
    public let name: String
    public let kind: NSHMethodKind
    /// TypeScript signature for the generated `.d.ts`, e.g. `(symbol: string) => Promise<Quote>`.
    public let typeScript: String?

    public init(name: String, kind: NSHMethodKind, typeScript: String? = nil) {
        self.name = name
        self.kind = kind
        self.typeScript = typeScript
    }

    public static func sync(_ name: String, typeScript: String? = nil) -> NSHHostMethod {
        NSHHostMethod(name: name, kind: .sync, typeScript: typeScript)
    }

    public static func async(_ name: String, typeScript: String? = nil) -> NSHHostMethod {
        NSHHostMethod(name: name, kind: .async, typeScript: typeScript)
    }
}

/// A JS-importable module answered by host code, e.g. `import { quote } from "demo/quote"`.
///
/// Sync methods run on the instance's JS thread and must be fast. Async methods
/// are answered through the responder, from any thread, exactly once. When a
/// script's listener count for an event changes, the sync method `__listeners`
/// is called with `[event, count]` (optional; errors are ignored).
@objc public protocol NSHHostModule: NSObjectProtocol {
    var name: String { get }
    var methods: [NSHHostMethod] { get }
    @objc optional var events: [String] { get }
    /// Default true: the program must list the module in `capabilities.host_modules`.
    @objc optional var requiresPermission: Bool { get }
    @objc optional var typeScriptDeclarations: String { get }
    @objc optional func callSync(_ method: String, args: [Any], context: NSHCallContext) throws -> Any
    @objc optional func call(_ method: String, args: [Any], context: NSHCallContext, respond: NSHResponder)
}

/// Answers one async host-module call. Thread-safe; only the first answer counts.
@objcMembers
public final class NSHResponder: NSObject {
    private let lock = NSLock()
    private var answered = false
    private let deliver: (Bool, String) -> Void

    init(deliver: @escaping (Bool, String) -> Void) {
        self.deliver = deliver
    }

    /// Resolves the script's promise with a JSON-able value.
    public func resolve(_ value: Any?) {
        guard claim() else { return }
        deliver(true, JSON.string(value))
    }

    /// Rejects the script's promise with an error carrying `message`.
    public func reject(_ message: String) {
        guard claim() else { return }
        deliver(false, message)
    }

    public var isAnswered: Bool {
        lock.lock(); defer { lock.unlock() }
        return answered
    }

    private func claim() -> Bool {
        lock.lock(); defer { lock.unlock() }
        if answered { return false }
        answered = true
        return true
    }
}

/// The calling instance, as seen by a host module.
@objcMembers
public final class NSHCallContext: NSObject {
    public let appId: String
    /// The module being called.
    public let module: String
    private weak var instance: NSHInstance?

    init(appId: String, module: String, instance: NSHInstance?) {
        self.appId = appId
        self.module = module
        self.instance = instance
    }

    /// Pushes an event of this module to the calling instance only (`NSHRuntime.emit` broadcasts).
    public func emit(_ event: String, _ payload: Any?) {
        instance?.emit(module: module, event: event, payload: payload)
    }

    /// The instance that made the call, if still alive.
    public var callingInstance: NSHInstance? { instance }
}

/// Handed to a host component so it can send events to the script (`.on(name, fn)`).
@objcMembers
public final class NSHEventSink: NSObject {
    private let handler: (String, Any?) -> Void

    init(_ handler: @escaping (String, Any?) -> Void) {
        self.handler = handler
    }

    public func emit(_ event: String, payload: Any?) {
        handler(event, payload)
    }
}

/// A native view the host registers by name; scripts place it with `host_component(name, props)`.
///
/// Instances are pooled per name: `prepareForReuse` must restore defaults.
/// `measure` is called on the JS thread (when present the component is
/// measurable: an unsized component asks it for its size).
@objc public protocol NSHHostComponent: NSObjectProtocol {
    static var componentName: String { get }
    init()
    var view: NSView { get }
    /// The full props on every change.
    func update(props: [String: Any])
    @objc optional func prepareForReuse()
    @objc optional static var events: [String] { get }
    @objc optional static var requiresPermission: Bool { get }
    /// TypeScript type of the props, e.g. `{ symbol: string; compact?: boolean }`.
    @objc optional static var propsTypeScript: String { get }
    @objc optional static func measure(props: [String: Any], maxWidth: CGFloat) -> CGSize
    @objc optional var eventSink: NSHEventSink? { get set }
}

/// A public key trusted to sign `.nspkg` bundles.
@objcMembers
public final class NSHTrustedKey: NSObject {
    public let id: String
    /// Base64 Ed25519 public key.
    public let publicKey: String

    public init(id: String, publicKey: String) {
        self.id = id
        self.publicKey = publicKey
    }
}
