// Applies decoded batches to a tree of native views: the node table, the view
// pools, frames, props, event masks and commands. Main thread only. The views
// and everything platform-specific (making views, presenting surfaces,
// animations, the theme, root services) are behind `MountedView` and
// `MountingHost`; each renderer has a thin manager that implements the host.
//
// A batch is validated before it is applied (see `BatchDecoder`), and every
// instruction that names an unknown node is skipped with a log line rather
// than trusted: a renderer bug or a hostile buffer must never crash the app.

import CoreGraphics
import Foundation
import QuartzCore

/// Log levels, with the raw values of the renderers' `NSHLogLevel`.
public enum MountLogLevel: Int {
    case debug = 0, info, warn, error
}

/// A native view the core manages: one per node. Children are the native
/// children in order, as the core sees them; how they are placed is the view's
/// business (lists, for instance, host their rows elsewhere).
public protocol MountedView: AnyObject {
    var nodeId: UInt32 { get set }
    var viewType: NSHViewType { get set }
    var mountedParent: MountedView? { get }
    var mountedChildren: [MountedView] { get }
    /// Declared transitions (`Prop.animation`).
    var animation: [String: Any]? { get }
    /// Surfaces may still be animating out when deleted: they are never pooled.
    var isPoolable: Bool { get }
    /// Pages, sheets and dialogs: presented by the host when attached to the root.
    var isSurface: Bool { get }
    /// On screen: frame changes may animate.
    var isInWindow: Bool { get }
    /// For `HostComponent` containers: the registered component it hosts.
    var hostComponentName: String? { get }

    func mountedInsert(_ child: MountedView, at index: Int)
    func mountedRemove(_ child: MountedView)
    func mountedMove(_ child: MountedView, to index: Int)
    /// Child bookkeeping without placing the view (the host places surfaces).
    func mountedInsertRecordOnly(_ child: MountedView, at index: Int)
    /// Places a child's view at `index` (surfaces when no host presents them).
    func mountedPlace(_ child: MountedView, at index: Int)
    /// Takes the view out of its native parent (surfaces when no host presents them).
    func removeFromNativeParent()

    /// Applies one prop. Returns false when this view type does not know it.
    func setProp(_ prop: Prop, _ value: PropValue) -> Bool
    func applyFrame(_ rect: CGRect)
    func setEventMask(_ mask: UInt32)
    func runCommand(_ command: Command, _ payload: [String: Any])
    /// Called after a batch touched this node, once per batch.
    func batchDidEnd()
    /// The node is being deleted (before it is reset and pooled).
    func willDestroy()
    /// Restores a fresh view's state, for the pool.
    func resetForReuse()
    func themeDidChange()
}

/// The renderer side of the core.
public protocol MountingHost: AnyObject {
    func makeView(_ type: NSHViewType) -> MountedView
    /// The view now belongs to a node: give it its context.
    func adopt(_ view: MountedView)
    /// The view left the node table: clear its context.
    func release(_ view: MountedView)

    /// Surfaces attached to the root. False when nothing presents them: the core
    /// then places (or removes) the view itself.
    func attachSurface(_ view: MountedView, at index: Int) -> Bool
    func detachSurface(_ view: MountedView) -> Bool
    /// Batch applied (outside its transaction): present or dismiss what changed.
    func surfacesDidChange(animatedPages: Bool)

    /// `ThemeTokens` arrived (nil: reset). Called before the nodes hear `themeDidChange`.
    func updateTheme(_ tokens: [String: Any]?)
    /// After every node heard `themeDidChange`: what the host draws itself.
    func themeDidChange()

    /// Runs `changes` with the declared animation. `moves`: frames, which Reduce Motion skips.
    func animate(_ animation: [String: Any], moves: Bool, _ changes: @escaping () -> Void)

    /// Loads the registered component `name` into an empty container.
    /// Returns false when no component has that name.
    func attachHostComponent(_ container: MountedView, name: String) -> Bool
    func detachHostComponent(_ container: MountedView)

    /// Toasts, URLs, clipboard and haptics: commands on the root node.
    func runRootCommand(_ command: Command, _ payload: [String: Any])
    func log(_ level: MountLogLevel, _ message: String)
}

public final class MountingCore {
    public static let poolLimit = 128

    /// The view node 0 maps to.
    public let rootView: MountedView
    private unowned let host: MountingHost

    public private(set) var nodes: [UInt32: MountedView] = [:]
    private var pool: [NSHViewType: [MountedView]] = [:]
    private var hostPool: [String: [MountedView]] = [:]
    public private(set) var lastSeq: UInt32 = 0
    public private(set) var appliedBatches = 0
    /// Decode + apply time of the last batch buffer (diagnostics).
    public private(set) var lastApplyMs: Double = 0
    /// Nodes whose `batchDidEnd` runs when the current batch ends.
    private var touched: [UInt32: MountedView] = [:]
    private var batchAnimated = false
    private var batchInitial = false
    private var pagesAnimated = false
    private var surfacesChanged = false

    /// `host` owns the core; the core never outlives it.
    public init(host: MountingHost, rootView: MountedView) {
        self.host = host
        self.rootView = rootView
        rootView.nodeId = Wire.rootNode
        nodes[Wire.rootNode] = rootView
    }

    public var nodeCount: Int { nodes.count - 1 }

    public func component(_ node: UInt32) -> MountedView? { nodes[node] }

    /// Asks for `batchDidEnd` on `view` at the end of the current batch.
    public func needsBatchEnd(_ view: MountedView) { touched[view.nodeId] = view }

    public func pooledCount(_ type: NSHViewType) -> Int { pool[type]?.count ?? 0 }

    /// Memory warning: drop the pools.
    public func trimPools() {
        pool.removeAll()
        hostPool.removeAll()
    }

    // MARK: Batches

    /// Decodes and applies one batch buffer. Returns false if it was rejected.
    @discardableResult
    public func apply(_ bytes: UnsafeRawBufferPointer) -> Bool {
        let start = CACurrentMediaTime()
        let mutations: [Mutation]
        do {
            mutations = try BatchDecoder.decode(bytes)
        } catch {
            host.log(.error, "native-shell: rejected batch: \(error)")
            return false
        }
        apply(mutations)
        lastApplyMs = (CACurrentMediaTime() - start) * 1000
        return true
    }

    @discardableResult
    public func apply(_ data: Data) -> Bool {
        data.withUnsafeBytes { apply($0) }
    }

    public func apply(_ mutations: [Mutation]) {
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
        if surfacesChanged { host.surfacesDidChange(animatedPages: pagesAnimated && !batchInitial) }
    }

    private func node(_ id: UInt32, _ op: String) -> MountedView? {
        if let n = nodes[id] { return n }
        host.log(.warn, "native-shell: \(op) on unknown node \(id)")
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
            guard id != Wire.rootNode else { return host.log(.warn, "native-shell: Create on the root node") }
            if let old = nodes[id] {
                host.log(.warn, "native-shell: Create on live node \(id); replacing")
                destroy(old)
            }
            let view = dequeue(type)
            view.nodeId = id
            view.viewType = type
            host.adopt(view)
            nodes[id] = view
        case let .insert(id, parentId, index):
            guard let child = node(id, "Insert"), let parent = node(parentId, "Insert") else { return }
            if let old = child.mountedParent { detach(child, from: old) }
            if parent === rootView, child.isSurface {
                parent.mountedInsertRecordOnly(child, at: Int(index))
                if !host.attachSurface(child, at: Int(index)) { parent.mountedPlace(child, at: Int(index)) }
                surfacesChanged = true
            } else {
                parent.mountedInsert(child, at: Int(index))
            }
            touched[parentId] = parent
        case let .remove(id, parentId, _):
            guard let child = node(id, "Remove"), let parent = node(parentId, "Remove") else { return }
            detach(child, from: parent)
            touched[parentId] = parent
        case let .delete(id):
            guard id != Wire.rootNode, let view = node(id, "Delete") else { return }
            if let parent = view.mountedParent {
                detach(view, from: parent)
                touched[parent.nodeId] = parent
            }
            destroy(view)
        case let .move(id, parentId, _, to):
            guard let child = node(id, "Move"), let parent = node(parentId, "Move") else { return }
            parent.mountedMove(child, to: Int(to))
            touched[parentId] = parent
        case let .setProp(id, prop, value):
            guard let view = node(id, "SetProp") else { return }
            if prop == .themeTokens {
                host.updateTheme(value.jsonObject)
                for (_, n) in nodes { n.themeDidChange() }
                host.themeDidChange()
            } else if prop == .hostComponent, view.viewType == .hostComponent {
                attachHostComponent(view, name: value.string)
            } else if prop == .opacity, batchAnimated, let anim = view.animation?["opacity"] as? [String: Any] {
                host.animate(anim, moves: false) { _ = view.setProp(prop, value) }
            } else if !view.setProp(prop, value) {
                host.log(.debug, "native-shell: prop \(prop) ignored by \(view.viewType)")
            }
            touched[id] = view
        case let .setFrame(id, frame):
            guard let view = node(id, "SetFrame") else { return }
            let rect = frame.cgRect
            guard rect.origin.x.isFinite, rect.origin.y.isFinite, rect.width.isFinite, rect.height.isFinite else {
                return host.log(.warn, "native-shell: non-finite frame for node \(id)")
            }
            if batchAnimated, let anim = view.animation?["frame"] as? [String: Any], view.isInWindow {
                host.animate(anim, moves: true) { view.applyFrame(rect) }
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

    private func detach(_ child: MountedView, from parent: MountedView) {
        if parent === rootView, child.isSurface {
            parent.mountedRemove(child)
            if !host.detachSurface(child) { child.removeFromNativeParent() }
            surfacesChanged = true
        } else {
            parent.mountedRemove(child)
        }
    }

    /// Drops a node and everything below it, recycling views into the pool.
    private func destroy(_ view: MountedView) {
        // A deleted page, sheet or dialog may still be animating out: its views stay
        // as they are (and are released with it); only the node table forgets them.
        if !view.isPoolable {
            forget(view)
            view.willDestroy()
            return
        }
        for child in view.mountedChildren.reversed() {
            view.mountedRemove(child)
            destroy(child)
        }
        if nodes[view.nodeId] === view { nodes[view.nodeId] = nil }
        touched[view.nodeId] = nil
        view.willDestroy()
        recycle(view)
    }

    private func forget(_ view: MountedView) {
        view.mountedChildren.forEach(forget)
        if nodes[view.nodeId] === view { nodes[view.nodeId] = nil }
        touched[view.nodeId] = nil
        host.release(view)
    }

    // MARK: Pool

    private func dequeue(_ type: NSHViewType) -> MountedView {
        if var list = pool[type], let v = list.popLast() {
            pool[type] = list
            return v
        }
        return host.makeView(type)
    }

    private func recycle(_ view: MountedView) {
        // Host component containers are pooled per component name, keeping their component.
        if view.viewType == .hostComponent {
            view.resetForReuse()
            if let name = view.hostComponentName, (hostPool[name]?.count ?? 0) < Self.poolLimit {
                hostPool[name, default: []].append(view)
            }
            return
        }
        guard view.isPoolable else { return }
        view.resetForReuse()
        host.release(view)
        if (pool[view.viewType]?.count ?? 0) < Self.poolLimit {
            pool[view.viewType, default: []].append(view)
        }
    }

    private func attachHostComponent(_ placeholder: MountedView, name: String?) {
        guard let name = name else { return host.detachHostComponent(placeholder) }
        if placeholder.hostComponentName == name { return }
        // A pooled container that already hosts this component replaces the fresh placeholder.
        if placeholder.hostComponentName == nil, var list = hostPool[name], let pooled = list.popLast() {
            hostPool[name] = list
            pooled.nodeId = placeholder.nodeId
            pooled.viewType = .hostComponent
            host.adopt(pooled)
            nodes[placeholder.nodeId] = pooled
            touched[placeholder.nodeId] = pooled
            host.release(placeholder)
            if (pool[.hostComponent]?.count ?? 0) < Self.poolLimit { pool[.hostComponent, default: []].append(placeholder) }
            return
        }
        if !host.attachHostComponent(placeholder, name: name) {
            host.log(.error, "native-shell: host component `\(name)` is not registered")
        }
    }

    // MARK: Commands

    private func runCommand(_ id: UInt32, _ command: Command, _ payloadString: String) {
        let payload = JSON.object(payloadString) ?? [:]
        switch command {
        case .showToast, .dismissToast, .openUrl, .setClipboard, .haptic:
            host.runRootCommand(command, payload)
        case .presentPage, .dismissPage:
            if payload.bool("animated") ?? true { pagesAnimated = true }
            surfacesChanged = true
            node(id, "\(command)")?.runCommand(command, payload)
        default:
            node(id, "\(command)")?.runCommand(command, payload)
        }
    }
}
