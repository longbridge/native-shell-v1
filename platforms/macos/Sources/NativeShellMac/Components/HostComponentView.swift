// `HostComponent`: a container for a native view the host app registered by
// name. Containers are pooled per component name, so a component instance is
// reused only for the same name, after `prepareForReuse`.

import AppKit
import NativeShellShared

final class NSHHostComponentView: NSHComponentView {
    private(set) var component: NSHHostComponent?
    private(set) var componentName: String?
    override var hostComponentName: String? { componentName }
    private(set) var props: [String: Any] = [:]
    private var sink: NSHEventSink?

    func attachComponent(named name: String, runtime: NSHRuntime) {
        detachComponent()
        guard let type = runtime.componentType(name) else { return }
        let c = type.init()
        component = c
        componentName = name
        let s = NSHEventSink { [weak self] event, payload in self?.emit(event, payload) }
        sink = s
        // Optional protocol property: set through its Objective-C setter.
        if c.responds(to: NSSelectorFromString("setEventSink:")) { (c as? NSObject)?.setValue(s, forKey: "eventSink") }
        let v = c.view
        v.frame = bounds
        v.autoresizingMask = [.width, .height]
        addSubview(v)
    }

    func detachComponent() {
        component?.view.removeFromSuperview()
        component = nil
        componentName = nil
        sink = nil
    }

    private func emit(_ event: String, _ payload: Any?) {
        let deliver = { [weak self] in
            guard let self = self, self.listens(EventMask.hostEvent) else { return }
            self.send(.hostEvent, ["name": event, "payload": payload ?? NSNull()])
        }
        if Thread.isMainThread { deliver() } else { DispatchQueue.main.async(execute: deliver) }
    }

    override func layout() {
        super.layout()
        component?.view.frame = bounds
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .hostComponent:
            return true // handled by the mounting manager (pool by name)
        case .hostProps:
            props = value.jsonObject ?? [:]
            // A reset (pooling) clears the props; `prepareForReuse` is the component's reset.
            if case .null = value { return true }
            component?.update(props: props)
            return true
        default:
            return super.applyProp(prop, value)
        }
    }

    override func resetForReuse() {
        super.resetForReuse()
        props = [:]
        component?.prepareForReuse?()
    }
}
