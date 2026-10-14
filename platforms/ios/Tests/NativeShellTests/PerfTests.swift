import NativeShellShared
@testable import NativeShell
import XCTest

/// Renderer-only timings: batches shaped like the core's first frame and a
/// full update, decoded and applied on the main thread.
final class PerfTests: XCTestCase {
    func testSyntheticFirstFrameAndUpdate() {
        var report: [String] = []
        for nodes in [1000, 3000, 10000] {
            let h = Harness()
            let rows = nodes / 2
            var first = Batch(seq: 1, flags: BatchFlags.initial)
            first.create(1, .scroll, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 844), props: [(.contentSize, .f32Array([390, Float(rows * 20)]))])
            for i in 0 ..< rows {
                let row = UInt32(10 + 2 * i), text = row + 1
                first.create(row, .view, parent: 1, index: UInt32(i), frame: CGRect(x: 0, y: CGFloat(i) * 20, width: 390, height: 20),
                             props: [(.backgroundColor, .color(i % 2 == 0 ? 0xFFF5_F5F5 : 0xFFFF_FFFF))])
                first.create(text, .text, parent: row, frame: CGRect(x: 16, y: 0, width: 200, height: 20),
                             props: [(.attributedText, .json(textJSON("Row \(i) · 0")))])
            }
            let firstData = first.encoded()
            var update = Batch(seq: 2)
            for i in 0 ..< rows { update.prop(UInt32(11 + 2 * i), .attributedText, .json(textJSON("Row \(i) · 1"))) }
            let updateData = update.encoded()

            var t = CACurrentMediaTime()
            XCTAssertTrue(h.surface.apply(firstData))
            h.surface.layoutIfNeeded()
            CATransaction.flush()
            let firstMs = (CACurrentMediaTime() - t) * 1000
            XCTAssertEqual(h.manager.nodeCount, rows * 2 + 1)

            t = CACurrentMediaTime()
            XCTAssertTrue(h.surface.apply(updateData))
            CATransaction.flush()
            let updateMs = (CACurrentMediaTime() - t) * 1000

            t = CACurrentMediaTime()
            _ = try? BatchDecoder.decode(firstData)
            let decodeMs = (CACurrentMediaTime() - t) * 1000
            report.append(String(format: "%5d nodes: first frame %6.1f ms (decode %5.1f ms, %d KB), update %6.1f ms",
                                 nodes + 1, firstMs, decodeMs, firstData.count / 1024, updateMs))
            h.tearDown()
        }
        print("TIMINGS (renderer only)\n" + report.joined(separator: "\n"))
    }

    /// measure_text cost on the JS thread for distinct strings (cache misses), as in a first frame.
    func testMeasureThroughput() {
        let jsons = (0 ..< 5000).map { textJSON("Row \($0) · measured once", size: 16) }
        var elapsed: Double = 0
        let done = expectation(description: "measured")
        Thread.detachNewThread {
            let measurer = TextMeasurer()
            let t = CACurrentMediaTime()
            for j in jsons { _ = measurer.measure(j, width: 358, mode: 1) }
            elapsed = CACurrentMediaTime() - t
            done.fulfill()
        }
        wait(for: [done], timeout: 60)
        print(String(format: "TIMINGS (measure_text) 5000 distinct strings: %.1f ms total, %.1f µs each", elapsed * 1000, elapsed * 1e6 / 5000))
    }
}
