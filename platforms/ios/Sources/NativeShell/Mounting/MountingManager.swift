// Applies decoded batches to UIKit: the node table, the view pool, frames,
// props, event masks and commands. Main thread only.
//
// A batch is validated before it is applied (see `BatchDecoder`), and every
// instruction that names an unknown node is skipped with a log line rather
// than trusted: a renderer bug or a hostile buffer must never crash the app.

import NativeShellShared
import UIKit

/// Presents the surfaces the core attaches to the root node: pages, sheets and
/// dialogs. `NSHViewController` implements it with real view controllers; a bare
/// `NSHSurfaceView` stacks them as subviews.
protocol SurfacePresenter: AnyObject {
    func attachSurface(_ view: NSHComponentView, at index: Int, in manager: MountingManager)
    func detachSurface(_ view: NSHComponentView, in manager: MountingManager)
    /// Batch applied: present or dismiss what changed. `animatedPages` is true when
    /// the batch carried `PresentPage` / `DismissPage`.
    func surfacesDidChange(in manager: MountingManager, animatedPages: Bool)
}

/// Root-level services the renderer offers on node 0.
protocol RootCommandHandler: AnyObject {
    func themeDidChange()
    func showToast(_ payload: [String: Any])
    /// `{"id"}` retracts one toast (visible or queued), `{}` every toast.
    func dismissToast(_ payload: [String: Any])
}

final class MountingManager: ComponentContext {
    let runtime: NSHRuntime
    /// The view node 0 maps to.
    let rootView: NSHComponentView
    weak var presenter: SurfacePresenter?
    let theme = ThemeState()
    weak var rootCommands: RootCommandHandler?
    /// Delivers events to the instance: node, type, JSON payload, seq of the last applied batch.
    var eventSink: ((UInt32, EventType, String, UInt32) -> Void)?
    var logger: ((NSHLogLevel, String) -> Void)?

    private(set) var nodes: [UInt32: NSHComponentView] = [:]
    private var pool: [NSHViewType: [NSHComponentView]] = [:]
    private var hostPool: [String: [NSHHostComponentView]] = [:]
    private(set) var lastSeq: UInt32 = 0
    private(set) var appliedBatches = 0
    /// Decode + apply time of the last batch buffer (diagnostics).
    private(set) var lastApplyMs: Double = 0
    /// Nodes whose `batchDidEnd` runs when the current batch ends.
    private var touched: [UInt32: NSHComponentView] = [:]
    private var batchAnimated = false
    private var batchInitial = false
    private var pagesAnimated = false
    private var surfacesChanged = false
    static let poolLimit = 128

    init(runtime: NSHRuntime, rootView: NSHComponentView) {
        self.runtime = runtime
        self.rootView = rootView
        rootView.nodeId = Wire.rootNode
        rootView.context = self
        nodes[Wire.rootNode] = rootView
    }

    var nodeCount: Int { nodes.count - 1 }

    func component(_ node: UInt32) -> NSHComponentView? { nodes[node] }

    func needsBatchEnd(_ view: NSHComponentView) { touched[view.nodeId] = view }

    func send(_ node: UInt32, _ type: EventType, _ payload: [String: Any]) {
        eventSink?(node, type, JSON.string(payload), lastSeq)
    }

    func log(_ level: NSHLogLevel, _ message: String) {
        if let logger = logger { logger(level, message) } else { runtime.log(level, message) }
    }

    // MARK: Batches

    /// Decodes and applies one batch buffer. Returns false if it was rejected.
    @discardableResult
    func apply(_ bytes: UnsafeRawBufferPointer) -> Bool {
        let start = CACurrentMediaTime()
        let mutations: [Mutation]
        do {
            mutations = try BatchDecoder.decode(bytes)
        } catch {
            log(.error, "native-shell: rejected batch: \(error)")
            return false
        }
        apply(mutations)
        lastApplyMs = (CACurrentMediaTime() - start) * 1000
        return true
    }

    @discardableResult
    func apply(_ data: Data) -> Bool {
        data.withUnsafeBytes { apply($0) }
    }

    func apply(_ mutations: [Mutation]) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        batchAnimated = false
        pagesAnimated = false
        surfacesChanged = false
        for m in mutations { applyOne(m) }
        for (_, view) in touched { view.batchDidEnd() }
        touched.removeAll(keepingCapacity: true)
        appliedBatches += 1
        CATransaction.commit()
        // Presentation runs outside the batch's transaction, where transitions animate normally.
        // Pages present in the INITIAL batch appear without a transition.
        if surfacesChanged { presenter?.surfacesDidChange(in: self, animatedPages: pagesAnimated && !batchInitial) }
    }

    private func node(_ id: UInt32, _ op: String) -> NSHComponentView? {
        if let n = nodes[id] { return n }
        log(.warn, "native-shell: \(op) on unknown node \(id)")
        return nil
    }

    private func applyOne(_ m: Mutation) {
        switch m {
        case let .beginBatch(seq, flags):
            lastSeq = seq
            batchAnimated = flags & BatchFlags.animated != 0
            batchInitial = flags & BatchFlags.initial != 0
        case .endBatch:
            break
        case let .create(id, type):
            guard id != Wire.rootNode else { return log(.warn, "native-shell: Create on the root node") }
            if let old = nodes[id] {
                log(.warn, "native-shell: Create on live node \(id); replacing")
                destroy(old)
            }
            let view = dequeue(type)
            view.nodeId = id
            view.viewType = type
            view.context = self
            nodes[id] = view
        case let .insert(id, parentId, index):
            guard let child = node(id, "Insert"), let parent = node(parentId, "Insert") else { return }
            if let old = child.parentComponent { detach(child, from: old) }
            if parent === rootView, child.isSurface {
                parent.insertChildRecordOnly(child, at: Int(index))
                presenter?.attachSurface(child, at: Int(index), in: self) ?? parent.mountChildView(child, at: Int(index))
                surfacesChanged = true
            } else {
                parent.insertChild(child, at: Int(index))
            }
            touched[parentId] = parent
        case let .remove(id, parentId, _):
            guard let child = node(id, "Remove"), let parent = node(parentId, "Remove") else { return }
            detach(child, from: parent)
            touched[parentId] = parent
        case let .delete(id):
            guard id != Wire.rootNode, let view = node(id, "Delete") else { return }
            if let parent = view.parentComponent {
                detach(view, from: parent)
                touched[parent.nodeId] = parent
            }
            destroy(view)
        case let .move(id, parentId, _, to):
            guard let child = node(id, "Move"), let parent = node(parentId, "Move") else { return }
            parent.moveChild(child, to: Int(to))
            touched[parentId] = parent
        case let .setProp(id, prop, value):
            guard let view = node(id, "SetProp") else { return }
            if prop == .themeTokens {
                if let o = value.jsonObject { theme.update(o) } else { theme.reset() }
                for (_, n) in nodes { n.themeDidChange() }
                rootCommands?.themeDidChange()
                (presenter as? RootCommandHandler)?.themeDidChange()
            } else if prop == .hostComponent, let host = view as? NSHHostComponentView {
                attachHostComponent(host, name: value.string)
            } else if prop == .opacity, batchAnimated, let anim = view.animation?["opacity"] as? [String: Any] {
                Animator.run(anim) { _ = view.setProp(prop, value) }
            } else if !view.setProp(prop, value) {
                log(.debug, "native-shell: prop \(prop) ignored by \(view.viewType)")
            }
            touched[id] = view
        case let .setFrame(id, frame):
            guard let view = node(id, "SetFrame") else { return }
            let rect = frame.cgRect
            guard rect.origin.x.isFinite, rect.origin.y.isFinite, rect.width.isFinite, rect.height.isFinite else {
                return log(.warn, "native-shell: non-finite frame for node \(id)")
            }
            if batchAnimated, let anim = view.animation?["frame"] as? [String: Any], view.window != nil {
                Animator.run(anim, moves: true) {
                    view.applyFrame(rect)
                    view.layoutIfNeeded()
                }
            } else {
                view.applyFrame(rect)
            }
            touched[id] = view
        case let .setEventMask(id, mask):
            node(id, "SetEventMask")?.setEventMask(mask)
        case let .command(id, command, payload):
            runCommand(id, command, payload)
        }
    }

    private func detach(_ child: NSHComponentView, from parent: NSHComponentView) {
        if parent === rootView, child.isSurface {
            parent.removeChildRecordOnly(child)
            if let presenter = presenter { presenter.detachSurface(child, in: self) } else { child.removeFromSuperview() }
            surfacesChanged = true
        } else {
            parent.removeChild(child)
        }
    }

    /// Drops a node and everything below it, recycling views into the pool.
    private func destroy(_ view: NSHComponentView) {
        // A deleted page, sheet or dialog may still be animating out: its views stay
        // as they are (and are released with it); only the node table forgets them.
        if !view.isPoolable {
            forget(view)
            view.willDestroy()
            return
        }
        for child in view.children.reversed() {
            view.removeChild(child)
            destroy(child)
        }
        if nodes[view.nodeId] === view { nodes[view.nodeId] = nil }
        touched[view.nodeId] = nil
        view.willDestroy()
        recycle(view)
    }

    private func forget(_ view: NSHComponentView) {
        view.children.forEach(forget)
        if nodes[view.nodeId] === view { nodes[view.nodeId] = nil }
        touched[view.nodeId] = nil
        view.context = nil
    }

    // MARK: Pool

    private func dequeue(_ type: NSHViewType) -> NSHComponentView {
        if var list = pool[type], let v = list.popLast() {
            pool[type] = list
            return v
        }
        return ComponentRegistry.make(type)
    }

    private func recycle(_ view: NSHComponentView) {
        if let host = view as? NSHHostComponentView {
            host.prepareForReuse()
            if let name = host.componentName, (hostPool[name]?.count ?? 0) < Self.poolLimit {
                hostPool[name, default: []].append(host)
            }
            return
        }
        guard view.isPoolable else { return }
        view.prepareForReuse()
        view.context = nil
        if (pool[view.viewType]?.count ?? 0) < Self.poolLimit {
            pool[view.viewType, default: []].append(view)
        }
    }

    func pooledCount(_ type: NSHViewType) -> Int { pool[type]?.count ?? 0 }

    /// Memory warning: drop the pools.
    func trimPools() {
        pool.removeAll()
        hostPool.removeAll()
    }

    private func attachHostComponent(_ placeholder: NSHHostComponentView, name: String?) {
        guard let name = name else { return placeholder.detachComponent() }
        if placeholder.componentName == name { return }
        // A pooled container that already hosts this component replaces the fresh placeholder.
        if placeholder.componentName == nil, var list = hostPool[name], let pooled = list.popLast() {
            hostPool[name] = list
            pooled.nodeId = placeholder.nodeId
            pooled.viewType = .hostComponent
            pooled.context = self
            nodes[placeholder.nodeId] = pooled
            touched[placeholder.nodeId] = pooled
            placeholder.context = nil
            if (pool[.hostComponent]?.count ?? 0) < Self.poolLimit { pool[.hostComponent, default: []].append(placeholder) }
            return
        }
        placeholder.attachComponent(named: name, runtime: runtime)
        if placeholder.componentName == nil { log(.error, "native-shell: host component `\(name)` is not registered") }
    }

    // MARK: Commands

    private func runCommand(_ id: UInt32, _ command: Command, _ payloadString: String) {
        let payload = JSON.object(payloadString) ?? [:]
        switch command {
        case .showToast:
            rootCommands?.showToast(payload)
        case .dismissToast:
            rootCommands?.dismissToast(payload)
        case .openUrl:
            if let s = payload.string("url"), let url = URL(string: s) { runtime.openURL(url) }
        case .setClipboard:
            UIPasteboard.general.string = payload.string("text") ?? ""
        case .haptic:
            Haptics.play(payload.string("kind") ?? "light")
        case .presentPage, .dismissPage:
            if payload.bool("animated") ?? true { pagesAnimated = true }
            surfacesChanged = true
            node(id, "\(command)")?.runCommand(command, payload)
        default:
            node(id, "\(command)")?.runCommand(command, payload)
        }
    }
}

extension NSHComponentView {
    /// Pages, sheets and dialogs: presented by the surface presenter when attached to the root.
    var isSurface: Bool {
        viewType == .page || viewType == .sheet || viewType == .dialog
    }

    /// Child bookkeeping without placing the view (surfaces are placed by the presenter).
    func insertChildRecordOnly(_ child: NSHComponentView, at index: Int) {
        insertChild(child, at: index)
        child.removeFromSuperview()
    }

    func removeChildRecordOnly(_ child: NSHComponentView) {
        removeChild(child)
    }
}
