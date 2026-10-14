import CoreGraphics
import NativeShellShared
import XCTest

/// A view that records what the core asks of it.
final class FakeView: MountedView {
    var nodeId: UInt32 = 0
    var viewType: NSHViewType
    weak var parent: FakeView?
    var children: [FakeView] = []
    /// Native placement order (what `mountedPlace` and inserts produce).
    var placed: [FakeView] = []
    weak var nativeParent: FakeView?
    var props: [Prop: PropValue] = [:]
    var frame: CGRect = .zero
    var mask: UInt32 = 0
    var commands: [Command] = []
    var batchEnds = 0
    var destroyed = 0
    var resets = 0
    var themeChanges = 0
    var animation: [String: Any]?
    var inWindow = true
    var hostComponentName: String?
    var context: AnyObject?

    init(_ type: NSHViewType) { viewType = type }

    var mountedParent: MountedView? { parent }
    var mountedChildren: [MountedView] { children }
    var isSurface: Bool { viewType == .page || viewType == .sheet || viewType == .dialog }
    var isPoolable: Bool { !isSurface }
    var isInWindow: Bool { inWindow }

    func mountedInsert(_ child: MountedView, at index: Int) {
        mountedInsertRecordOnly(child, at: index)
        mountedPlace(child, at: index)
    }

    func mountedInsertRecordOnly(_ child: MountedView, at index: Int) {
        let c = child as! FakeView
        c.parent = self
        children.insert(c, at: max(0, min(index, children.count)))
    }

    func mountedRemove(_ child: MountedView) {
        let c = child as! FakeView
        children.removeAll { $0 === c }
        c.parent = nil
        c.removeFromNativeParent()
    }

    func mountedMove(_ child: MountedView, to index: Int) {
        let c = child as! FakeView
        children.removeAll { $0 === c }
        children.insert(c, at: max(0, min(index, children.count)))
        c.removeFromNativeParent()
        mountedPlace(c, at: index)
    }

    /// Placement relative to siblings, as the platform views do.
    func mountedPlace(_ child: MountedView, at index: Int) {
        let c = child as! FakeView
        guard let i = children.firstIndex(where: { $0 === c }) else { return }
        if i + 1 < children.count, let next = placed.firstIndex(where: { $0 === children[i + 1] }) {
            placed.insert(c, at: next)
        } else if i > 0, let prev = placed.firstIndex(where: { $0 === children[i - 1] }) {
            placed.insert(c, at: prev + 1)
        } else {
            placed.append(c)
        }
        c.nativeParent = self
    }

    func removeFromNativeParent() {
        nativeParent?.placed.removeAll { $0 === self }
        nativeParent = nil
    }

    func setProp(_ prop: Prop, _ value: PropValue) -> Bool {
        guard prop != .placeholder else { return false }
        if case .null = value { props[prop] = nil } else { props[prop] = value }
        if prop == .animation { animation = value.jsonObject }
        return true
    }

    func applyFrame(_ rect: CGRect) { frame = rect }
    func setEventMask(_ mask: UInt32) { self.mask = mask }
    func runCommand(_ command: Command, _ payload: [String: Any]) { commands.append(command) }
    func batchDidEnd() { batchEnds += 1 }
    func willDestroy() { destroyed += 1 }

    func resetForReuse() {
        resets += 1
        props = [:]
        mask = 0
        frame = .zero
        animation = nil
    }

    func themeDidChange() { themeChanges += 1 }
}

final class FakeHost: MountingHost {
    var made: [NSHViewType] = []
    var presents = true
    var attached: [UInt32] = []
    var detached: [UInt32] = []
    var surfaceChanges: [Bool] = []
    var theme: [String: Any]?
    var log: [String] = []
    var calls: [String] = []
    var animations: [(moves: Bool, changes: () -> Void)] = []
    var registered: Set<String> = ["demo-cell"]
    var rootCommands: [Command] = []

    func makeView(_ type: NSHViewType) -> MountedView {
        made.append(type)
        return FakeView(type)
    }

    func adopt(_ view: MountedView) { (view as! FakeView).context = self }
    func release(_ view: MountedView) { (view as! FakeView).context = nil }

    func attachSurface(_ view: MountedView, at index: Int) -> Bool {
        attached.append(view.nodeId)
        return presents
    }

    func detachSurface(_ view: MountedView) -> Bool {
        detached.append(view.nodeId)
        return presents
    }

    func surfacesDidChange(animatedPages: Bool) { surfaceChanges.append(animatedPages) }

    func updateTheme(_ tokens: [String: Any]?) {
        theme = tokens
        calls.append("updateTheme")
    }

    func themeDidChange() { calls.append("themeDidChange") }

    func animate(_ animation: [String: Any], moves: Bool, _ changes: @escaping () -> Void) {
        animations.append((moves, changes))
        changes()
    }

    func attachHostComponent(_ container: MountedView, name: String) -> Bool {
        guard registered.contains(name) else { return false }
        (container as! FakeView).hostComponentName = name
        return true
    }

    func detachHostComponent(_ container: MountedView) { (container as! FakeView).hostComponentName = nil }
    func runRootCommand(_ command: Command, _ payload: [String: Any]) { rootCommands.append(command) }
    func log(_ level: MountLogLevel, _ message: String) { log.append(message) }
}

final class MountingCoreTests: XCTestCase {
    var host: FakeHost!
    var root: FakeView!
    var core: MountingCore!
    var seq: UInt32 = 0

    override func setUp() {
        host = FakeHost()
        root = FakeView(.view)
        core = MountingCore(host: host, rootView: root)
    }

    @discardableResult
    private func apply(flags: UInt32 = 0, _ ms: [Mutation]) -> Bool {
        seq += 1
        return core.apply(BatchEncoder.encode([.beginBatch(seq: seq, flags: flags)] + ms + [.endBatch]))
    }

    private func view(_ id: UInt32) -> FakeView? { core.component(id) as? FakeView }

    func testCreateInsertMoveRemoveKeepOrderAndPlacement() {
        apply([
            .create(node: 1, type: .view), .insert(node: 1, parent: 0, index: 0),
            .create(node: 2, type: .text), .insert(node: 2, parent: 1, index: 0),
            .create(node: 3, type: .text), .insert(node: 3, parent: 1, index: 1),
            .create(node: 4, type: .text), .insert(node: 4, parent: 1, index: 1),
            .setFrame(node: 2, frame: Frame(x: 1, y: 2, width: 3, height: 4)),
            .setEventMask(node: 2, mask: EventMask.click),
        ])
        let p = view(1)!
        XCTAssertEqual(p.children.map(\.nodeId), [2, 4, 3])
        XCTAssertEqual(p.placed.map(\.nodeId), [2, 4, 3])
        XCTAssertEqual(view(2)?.frame, CGRect(x: 1, y: 2, width: 3, height: 4))
        XCTAssertEqual(view(2)?.mask, EventMask.click)
        XCTAssertTrue(view(2)?.context === host, "adopted")
        XCTAssertEqual(core.nodeCount, 4)
        XCTAssertEqual(core.lastSeq, 1)
        XCTAssertEqual(core.appliedBatches, 1)
        XCTAssertEqual(p.batchEnds, 1, "once per batch, however many ops touched it")

        // Move places by neighbours, so native order follows the record.
        apply([.move(node: 3, parent: 1, from: 2, to: 0), .remove(node: 2, parent: 1, index: 1)])
        XCTAssertEqual(p.children.map(\.nodeId), [3, 4])
        XCTAssertEqual(p.placed.map(\.nodeId), [3, 4])
        XCTAssertNil(view(2)?.parent)
    }

    func testUnknownNodesAreSkippedAndLogged() {
        XCTAssertTrue(apply([.setProp(node: 99, prop: .opacity, value: .f32(0.5)), .insert(node: 98, parent: 0, index: 0),
                             .delete(node: 97), .create(node: 0, type: .view), .create(node: 1, type: .view)]))
        XCTAssertNotNil(view(1))
        XCTAssertEqual(host.log.filter { $0.contains("unknown node") }.count, 3)
        XCTAssertTrue(host.log.contains { $0.contains("Create on the root node") })
    }

    func testRejectedBatchesLeaveTheTreeUntouched() {
        apply([.create(node: 1, type: .view), .insert(node: 1, parent: 0, index: 0)])
        var data = BatchEncoder.encode([.beginBatch(seq: 2, flags: 0), .create(node: 2, type: .view), .insert(node: 2, parent: 1, index: 0), .endBatch])
        data.removeLast(4)
        XCTAssertFalse(core.apply(data))
        XCTAssertNil(view(2))
        XCTAssertTrue(view(1)!.children.isEmpty)
        XCTAssertTrue(host.log.contains { $0.contains("rejected batch") })
    }

    func testDeletedSubtreesArePooledResetAndReused() {
        apply([
            .create(node: 1, type: .view), .insert(node: 1, parent: 0, index: 0),
            .create(node: 2, type: .text), .insert(node: 2, parent: 1, index: 0),
            .setProp(node: 2, prop: .opacity, value: .f32(0.5)),
        ])
        let text = view(2)!
        apply([.delete(node: 1)])
        XCTAssertEqual(core.nodeCount, 0)
        XCTAssertEqual(core.pooledCount(.view), 1)
        XCTAssertEqual(core.pooledCount(.text), 1)
        XCTAssertEqual(text.resets, 1)
        XCTAssertEqual(text.destroyed, 1)
        XCTAssertNil(text.context, "released")
        apply([.create(node: 5, type: .text)])
        XCTAssertTrue(view(5) === text)
        XCTAssertEqual(host.made, [.view, .text], "reused, not made again")
        core.trimPools()
        XCTAssertEqual(core.pooledCount(.view), 0)
    }

    func testDeletedSurfacesAreForgottenNotRecycled() {
        apply([
            .create(node: 10, type: .page), .create(node: 11, type: .text), .insert(node: 11, parent: 10, index: 0),
            .insert(node: 10, parent: 0, index: 1), .command(node: 10, command: .presentPage, payload: #"{"animated":true}"#),
        ])
        XCTAssertEqual(host.attached, [10])
        XCTAssertEqual(host.surfaceChanges, [true])
        let page = view(10)!, text = view(11)!
        apply([.command(node: 10, command: .dismissPage, payload: "{}"), .delete(node: 10)])
        XCTAssertEqual(host.detached, [10])
        XCTAssertNil(view(10))
        XCTAssertNil(view(11))
        XCTAssertTrue(page.children.first === text, "content stays for the exit animation")
        XCTAssertEqual(text.resets, 0)
        XCTAssertNil(text.context)
        XCTAssertEqual(page.destroyed, 1)
        XCTAssertEqual(core.pooledCount(.text), 0)
        XCTAssertEqual(page.commands, [.presentPage, .dismissPage])
    }

    func testInitialAndUnanimatedPageBatchesDoNotAnimate() {
        apply(flags: BatchFlags.initial, [.create(node: 10, type: .page), .insert(node: 10, parent: 0, index: 1),
                                          .command(node: 10, command: .presentPage, payload: #"{"animated":true}"#)])
        apply([.command(node: 10, command: .dismissPage, payload: #"{"animated":false}"#)])
        XCTAssertEqual(host.surfaceChanges, [false, false])
    }

    func testWithoutAPresenterSurfacesArePlacedDirectly() {
        host.presents = false
        apply([.create(node: 10, type: .sheet), .insert(node: 10, parent: 0, index: 0)])
        XCTAssertTrue(root.placed.first === view(10))
        apply([.remove(node: 10, parent: 0, index: 0)])
        XCTAssertTrue(root.placed.isEmpty)
    }

    func testThemeTokensFanOutToEveryNodeThenTheHost() {
        apply([.create(node: 1, type: .view), .create(node: 2, type: .slider)])
        apply([.setProp(node: 0, prop: .themeTokens, value: .json(#"{"appearance":"dark","primary":4294901760}"#))])
        XCTAssertEqual(host.theme?["appearance"] as? String, "dark")
        XCTAssertEqual([root, view(1)!, view(2)!].map(\.themeChanges), [1, 1, 1])
        XCTAssertEqual(host.calls, ["updateTheme", "themeDidChange"])
        apply([.setProp(node: 0, prop: .themeTokens, value: .null)])
        XCTAssertNil(host.theme)
    }

    func testAnimatedBatchesAnimateDeclaredTransitions() {
        apply([.create(node: 1, type: .view),
               .setProp(node: 1, prop: .animation, value: .json(#"{"frame":{"kind":"spring"},"opacity":{"kind":"timing"}}"#))])
        apply(flags: BatchFlags.animated, [.setFrame(node: 1, frame: Frame(x: 9, y: 0, width: 1, height: 1)),
                                           .setProp(node: 1, prop: .opacity, value: .f32(0.3))])
        XCTAssertEqual(host.animations.map(\.moves), [true, false], "frames move, opacity only fades")
        XCTAssertEqual(view(1)?.frame.minX, 9)
        // Off screen, or a batch not flagged animated: applied at once.
        view(1)!.inWindow = false
        apply(flags: BatchFlags.animated, [.setFrame(node: 1, frame: Frame(x: 1, y: 0, width: 1, height: 1))])
        apply([.setProp(node: 1, prop: .opacity, value: .f32(1))])
        XCTAssertEqual(host.animations.count, 2)
        // Non-finite frames are refused.
        apply([.setFrame(node: 1, frame: Frame(x: .nan, y: 0, width: 1, height: 1))])
        XCTAssertEqual(view(1)?.frame.minX, 1)
    }

    func testHostComponentContainersArePooledPerName() {
        apply([.create(node: 1, type: .hostComponent), .setProp(node: 1, prop: .hostComponent, value: .str("demo-cell"))])
        let cell = view(1)!
        XCTAssertEqual(cell.hostComponentName, "demo-cell")
        apply([.delete(node: 1)])
        XCTAssertEqual(cell.resets, 1)
        XCTAssertTrue(cell.context === host, "pooled containers keep their component and context")
        // A fresh placeholder gives way to the pooled container hosting the same component.
        apply([.create(node: 2, type: .hostComponent), .setProp(node: 2, prop: .hostComponent, value: .str("demo-cell"))])
        XCTAssertTrue(view(2) === cell)
        XCTAssertEqual(cell.nodeId, 2)
        XCTAssertEqual(core.pooledCount(.hostComponent), 1, "the placeholder went back to the pool")
        // Unknown names leave an empty container and a log line.
        apply([.create(node: 3, type: .hostComponent), .setProp(node: 3, prop: .hostComponent, value: .str("nope"))])
        XCTAssertNil(view(3)?.hostComponentName)
        XCTAssertTrue(host.log.contains { $0.contains("`nope` is not registered") })
    }

    func testRootCommandsGoToTheHostOthersToTheirNode() {
        apply([.create(node: 1, type: .scroll)])
        apply([.command(node: 0, command: .showToast, payload: "{}"), .command(node: 0, command: .setClipboard, payload: "{}"),
               .command(node: 0, command: .haptic, payload: "{}"), .command(node: 0, command: .openUrl, payload: "{}"),
               .command(node: 0, command: .dismissToast, payload: "{}"), .command(node: 1, command: .scrollTo, payload: "{}")])
        XCTAssertEqual(host.rootCommands, [.showToast, .setClipboard, .haptic, .openUrl, .dismissToast])
        XCTAssertEqual(view(1)?.commands, [.scrollTo])
    }

    func testUnknownPropsAreLoggedNotFatal() {
        apply([.create(node: 1, type: .view), .setProp(node: 1, prop: .placeholder, value: .str("x"))])
        XCTAssertTrue(host.log.contains { $0.contains("ignored") })
    }
}
