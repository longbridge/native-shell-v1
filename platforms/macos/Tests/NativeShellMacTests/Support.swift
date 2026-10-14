@testable import NativeShellMac
import AppKit
import XCTest
import NativeShellShared

/// Builds batches the way the core would, for driving the renderer without it.
struct Batch {
    private(set) var mutations: [Mutation]

    init(seq: UInt32 = 1, flags: UInt32 = 0) {
        mutations = [.beginBatch(seq: seq, flags: flags)]
    }

    mutating func create(_ node: UInt32, _ type: NSHViewType, parent: UInt32? = nil, index: UInt32 = 0,
                         frame: CGRect? = nil, props: [(Prop, PropValue)] = []) {
        mutations.append(.create(node: node, type: type))
        for (p, v) in props { mutations.append(.setProp(node: node, prop: p, value: v)) }
        if let f = frame { self.frame(node, f) }
        if let parent = parent { mutations.append(.insert(node: node, parent: parent, index: index)) }
    }

    mutating func prop(_ node: UInt32, _ p: Prop, _ v: PropValue) { mutations.append(.setProp(node: node, prop: p, value: v)) }
    mutating func frame(_ node: UInt32, _ f: CGRect) {
        mutations.append(.setFrame(node: node, frame: Frame(x: Float(f.minX), y: Float(f.minY), width: Float(f.width), height: Float(f.height))))
    }
    mutating func mask(_ node: UInt32, _ m: UInt32) { mutations.append(.setEventMask(node: node, mask: m)) }
    mutating func insert(_ node: UInt32, _ parent: UInt32, _ index: UInt32) { mutations.append(.insert(node: node, parent: parent, index: index)) }
    mutating func remove(_ node: UInt32, _ parent: UInt32, _ index: UInt32) { mutations.append(.remove(node: node, parent: parent, index: index)) }
    mutating func delete(_ node: UInt32) { mutations.append(.delete(node: node)) }
    mutating func move(_ node: UInt32, _ parent: UInt32, _ from: UInt32, _ to: UInt32) {
        mutations.append(.move(node: node, parent: parent, from: from, to: to))
    }
    mutating func command(_ node: UInt32, _ c: Command, _ payload: [String: Any] = [:]) {
        mutations.append(.command(node: node, command: c, payload: JSON.string(payload)))
    }

    func encoded() -> Data {
        BatchEncoder.encode(mutations + [.endBatch])
    }
}

struct SentEvent {
    let node: UInt32
    let type: EventType
    let payload: [String: Any]
    let seq: UInt32
}

/// A surface view in a real (never ordered in) window, with captured events.
/// Windows stay off screen so the tests never take focus from the user.
final class Harness {
    let window: NSWindow
    let surface: NSHSurfaceView
    var events: [SentEvent] = []
    var seq: UInt32 = 0

    init(size: CGSize = CGSize(width: 390, height: 844), runtime: NSHRuntime = .shared) {
        window = NSWindow(contentRect: CGRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        surface = NSHSurfaceView(runtime: runtime)
        surface.frame = CGRect(origin: .zero, size: size)
        surface.autoresizingMask = [.width, .height]
        window.contentView = surface
        surface.eventSink = { [weak self] node, type, payload, seq in
            let obj = (JSON.any(payload) as? [String: Any]) ?? [:]
            self?.events.append(SentEvent(node: node, type: type, payload: obj, seq: seq))
        }
        surface.layoutSubtreeIfNeeded()
    }

    var manager: MountingManager { surface.manager }

    @discardableResult
    func apply(_ build: (inout Batch) -> Void, flags: UInt32 = 0) -> Bool {
        seq += 1
        var b = Batch(seq: seq, flags: flags)
        build(&b)
        let ok = surface.apply(b.encoded())
        surface.layoutSubtreeIfNeeded()
        return ok
    }

    func view(_ node: UInt32) -> NSHComponentView? { manager.component(node) }
    func view<T: NSHComponentView>(_ node: UInt32, as: T.Type) -> T { manager.component(node) as! T }

    func events(_ type: EventType) -> [SentEvent] { events.filter { $0.type == type } }

    func tearDown() {
        window.contentView = nil
        window.close()
    }
}

/// Synthetic pointer events at a point in `view`.
enum Mouse {
    static func event(_ type: NSEvent.EventType, at p: CGPoint, in view: NSView, clicks: Int = 1, modifiers: NSEvent.ModifierFlags = [],
                      time: TimeInterval = ProcessInfo.processInfo.systemUptime) -> NSEvent {
        let w = view.convert(p, to: nil)
        return NSEvent.mouseEvent(with: type, location: w, modifierFlags: modifiers, timestamp: time, windowNumber: view.window?.windowNumber ?? 0,
                                  context: nil, eventNumber: 0, clickCount: clicks, pressure: 1)!
    }

    /// A click as AppKit delivers it: down and up on the hit view.
    static func click(_ view: NSView, at p: CGPoint? = nil, clicks: Int = 1, modifiers: NSEvent.ModifierFlags = []) {
        let point = p ?? CGPoint(x: view.bounds.midX, y: view.bounds.midY)
        view.mouseDown(with: event(.leftMouseDown, at: point, in: view, clicks: clicks, modifiers: modifiers))
        view.mouseUp(with: event(.leftMouseUp, at: point, in: view, clicks: clicks, modifiers: modifiers))
    }
}

extension XCTestCase {
    /// Spins the main run loop until `condition` holds or `timeout` passes.
    @discardableResult
    func wait(_ timeout: TimeInterval = 2, until condition: () -> Bool) -> Bool {
        let end = Date().addingTimeInterval(timeout)
        while !condition() && Date() < end {
            RunLoop.main.run(until: Date().addingTimeInterval(0.01))
        }
        return condition()
    }

    func spin(_ seconds: TimeInterval) {
        RunLoop.main.run(until: Date().addingTimeInterval(seconds))
    }
}

func textJSON(_ text: String, size: Double = 16, weight: Int = 400, lineHeight: Double? = nil, lines: Int? = nil,
              truncate: Bool = false, align: String? = nil, url: String? = nil) -> String {
    var run: [String: Any] = ["text": text, "size": size, "weight": weight, "color": 0xFF00_0000]
    // Line heights are points on the wire (the core resolves multiples).
    if let lh = lineHeight { run["line_height"] = lh * size }
    if let u = url { run["url"] = u }
    var o: [String: Any] = ["runs": [run], "truncate": truncate]
    if let l = lines { o["lines"] = l }
    if let a = align { o["align"] = a }
    return JSON.string(o)
}
