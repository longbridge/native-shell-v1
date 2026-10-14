// The process-wide registry: host modules and components, theme provider and
// platform hooks. Register before creating instances; each instance receives
// the registry as it was at its creation.

import os.log
import UIKit

@objcMembers
public final class NSHRuntime: NSObject {
    public static let shared = NSHRuntime()

    private let lock = NSLock()
    private var modules: [String: NSHHostModule] = [:]
    private var components: [String: NSHHostComponent.Type] = [:]
    private let instances = NSHashTable<NSHInstance>.weakObjects()

    /// Maps the app's design tokens to the 17 shell color tokens
    /// (`background`, `foreground`, `primary`…) for one appearance.
    public var themeProvider: ((NSHAppearance) -> [String: UIColor])?
    /// Loads a network image downsampled to `size` (points). Default: URLSession + memory cache.
    /// The completion may be called on any thread.
    public var imageLoader: ((URL, CGSize, @escaping (UIImage?) -> Void) -> Void)?
    /// Opens a URL the script asked for (after its capability check). Default: `UIApplication.open`.
    public var urlOpener: ((URL) -> Void)?
    /// Receives renderer and script logs. Default: os_log.
    public var logger: ((NSHLogLevel, String) -> Void)?
    /// Keys that may sign `.nspkg` bundles.
    public var trustedKeys: [NSHTrustedKey] = []
    /// SHA-256 (hex) of bundles that must not load.
    public var revokedBundles: [String] = []

    public override init() {
        super.init()
        if Thread.isMainThread { TextMeasurer.scale = UIScreen.main.scale } else {
            DispatchQueue.main.async { TextMeasurer.scale = UIScreen.main.scale }
        }
    }

    // MARK: Registration

    public func register(module: NSHHostModule) {
        lock.lock(); defer { lock.unlock() }
        modules[module.name] = module
    }

    public func register(component: NSHHostComponent.Type) {
        lock.lock(); defer { lock.unlock() }
        components[component.componentName] = component
    }

    public func unregister(module name: String) {
        lock.lock(); defer { lock.unlock() }
        modules[name] = nil
    }

    public func unregister(component name: String) {
        lock.lock(); defer { lock.unlock() }
        components[name] = nil
    }

    func module(_ name: String) -> NSHHostModule? {
        lock.lock(); defer { lock.unlock() }
        return modules[name]
    }

    func componentType(_ name: String) -> NSHHostComponent.Type? {
        lock.lock(); defer { lock.unlock() }
        return components[name]
    }

    /// Pushes a host-module event to every live instance (scripts without listeners ignore it).
    public func emit(module: String, event: String, payload: Any?) {
        for i in liveInstances() { i.emit(module: module, event: event, payload: payload) }
    }

    func track(_ instance: NSHInstance) {
        lock.lock(); defer { lock.unlock() }
        instances.add(instance)
    }

    func untrack(_ instance: NSHInstance) {
        lock.lock(); defer { lock.unlock() }
        instances.remove(instance)
    }

    func liveInstances() -> [NSHInstance] {
        lock.lock(); defer { lock.unlock() }
        return instances.allObjects
    }

    // MARK: Descriptors

    /// `HostRegistry` JSON for an instance's config.
    func registryJSON() -> [String: Any] {
        lock.lock()
        let mods = modules.values.sorted { $0.name < $1.name }
        let comps = components.sorted { $0.key < $1.key }
        lock.unlock()
        let m: [[String: Any]] = mods.map { mod in
            var d: [String: Any] = [
                "name": mod.name,
                "methods": mod.methods.map { meth -> [String: Any] in
                    var o: [String: Any] = ["name": meth.name, "kind": meth.kind == .sync ? "sync" : "async"]
                    if let ts = meth.typeScript { o["ts"] = ts }
                    return o
                },
                "events": mod.events ?? [],
                "requires_permission": mod.requiresPermission ?? true,
            ]
            if let dts = mod.typeScriptDeclarations { d["dts"] = dts }
            return d
        }
        let c: [[String: Any]] = comps.map { name, type in
            var d: [String: Any] = [
                "name": name,
                "events": type.events ?? [],
                "measurable": type.measure != nil,
                "requires_permission": type.requiresPermission ?? true,
            ]
            if let ts = type.propsTypeScript { d["props_ts"] = ts }
            return d
        }
        return ["modules": m, "components": c]
    }

    /// `{light: {token: "#hex"}, dark: {…}}`, or nil without a provider.
    func themeJSON() -> [String: Any]? {
        guard let provider = themeProvider else { return nil }
        func map(_ a: NSHAppearance) -> [String: String] {
            provider(a).mapValues { Colors.toHex($0) }
        }
        return ["light": map(.light), "dark": map(.dark)]
    }

    // MARK: Hooks

    func log(_ level: NSHLogLevel, _ message: String) {
        if let logger = logger { return logger(level, message) }
        let type: OSLogType
        switch level {
        case .debug: type = .debug
        case .info: type = .info
        case .warn: type = .default
        case .error: type = .error
        }
        os_log("%{public}@", log: NSHRuntime.osLog, type: type, message)
    }

    private static let osLog = OSLog(subsystem: "dev.nativeshell", category: "runtime")

    func openURL(_ url: URL) {
        if let opener = urlOpener { return opener(url) }
        DispatchQueue.main.async { UIApplication.shared.open(url) }
    }

    func loadImage(_ url: URL, _ size: CGSize, _ completion: @escaping (UIImage?) -> Void) {
        if let loader = imageLoader { return loader(url, size, completion) }
        DefaultImageLoader.shared.load(url, size: size, completion: completion)
    }
}
