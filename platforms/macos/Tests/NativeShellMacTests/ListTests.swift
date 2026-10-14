@testable import NativeShellMac
import AppKit
import XCTest
import NativeShellShared

final class ListTests: XCTestCase {
    var h: Harness!

    override func setUp() { h = Harness() }
    override func tearDown() { h.tearDown() }

    private let rowHeight: Float = 44
    private func offsets(_ n: Int) -> [Float] { (0 ... n).map { Float($0) * rowHeight } }

    /// What the core does when the window moves: render rows [first, end) as ListRows.
    private func renderWindow(_ first: Int, _ end: Int, existing: inout Set<Int>, b: inout Batch) {
        let wanted = Set(first ..< end)
        for i in existing.subtracting(wanted) { b.delete(UInt32(100_000 + i)) }
        for i in wanted.subtracting(existing).sorted() {
            let row = UInt32(100_000 + i), text = UInt32(200_000 + i)
            b.create(row, .listRow, props: [(.rowIndex, .i32(Int32(i)))])
            b.frame(row, CGRect(x: 0, y: 0, width: 390, height: 44))
            b.create(text, .text, parent: row, frame: CGRect(x: 16, y: 12, width: 200, height: 20),
                     props: [(.attributedText, .json(textJSON("Row \(i)")))])
            b.insert(row, 1, 0)
        }
        existing = wanted
    }

    private func scroll(_ list: NSHListView, to y: CGFloat) {
        list.scroll(toY: y, animated: false)
    }

    func testTenThousandRowsReportRangesWhileScrolling() {
        let count = 10_000
        var rendered = Set<Int>()
        h.apply { b in
            b.create(1, .list, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 844), props: [
                (.itemCount, .i32(Int32(count))), (.rowOffsets, .f32Array(offsets(count))), (.prefetch, .i32(5)),
            ])
            renderWindow(0, 20, existing: &rendered, b: &b)
        }
        let list = h.view(1, as: NSHListView.self)
        XCTAssertEqual(list.document.frame.height, CGFloat(count) * 44)
        // Initial range: 844 / 44 → rows 0..<20, plus prefetch 5 at the end.
        let first = h.events(.rangeChange).last?.payload
        XCTAssertEqual(first?.int("first"), 0)
        XCTAssertEqual(first?.int("end"), 25)
        // Rows sit at their offsets, whatever frame the core gave them.
        let row5 = h.view(100_005, as: NSHListRowView.self)
        XCTAssertEqual(row5.frame, CGRect(x: 0, y: 220, width: 390, height: 44))

        var ranges: [(Int, Int)] = []
        for y in stride(from: 0, through: 400_000, by: 20_000) {
            scroll(list, to: CGFloat(y))
            spin(0.03)
            guard let r = h.events(.rangeChange).last?.payload, let f = r.int("first"), let e = r.int("end") else { continue }
            ranges.append((f, e))
            h.apply { b in renderWindow(f, e, existing: &rendered, b: &b) }
        }
        XCTAssertGreaterThan(ranges.count, 10)
        let (f, e) = ranges.last!
        XCTAssertEqual(f, 400_000 / 44 - 5)
        XCTAssertEqual(e, min(count, Int((400_000 + 844) / 44) + 1 + 5))
        XCTAssertLessThanOrEqual(h.manager.nodeCount, 1 + 2 * 40, "only the window is mounted")
        for i in f ..< e {
            XCTAssertEqual(h.view(UInt32(100_000 + i))?.frame.minY, CGFloat(i) * 44)
        }

        // Scroll to the end; the range is clamped to the item count.
        scroll(list, to: CGFloat(count) * 44 - 844)
        XCTAssertTrue(wait { self.h.events(.rangeChange).last?.payload.int("end") == count })
    }

    func testScrollToIndexPositions() {
        h.apply { b in
            b.create(1, .list, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 440), props: [
                (.itemCount, .i32(1000)), (.rowOffsets, .f32Array(offsets(1000))),
            ])
            b.command(1, .scrollToIndex, ["index": 100, "position": "top", "animated": false])
        }
        let list = h.view(1, as: NSHListView.self)
        let y = { list.scrollView.contentView.bounds.minY }
        XCTAssertEqual(y(), 4400)
        h.apply { $0.command(1, .scrollToIndex, ["index": 100, "position": "center", "animated": false]) }
        XCTAssertEqual(y(), 4400 - (440 - 44) / 2)
        h.apply { $0.command(1, .scrollToIndex, ["index": 100, "position": "bottom", "animated": false]) }
        XCTAssertEqual(y(), 4400 + 44 - 440)
        h.apply { $0.command(1, .scrollToIndex, ["index": 99_999, "animated": false]) }
        XCTAssertEqual(y(), 44_000 - 440, "clamped")
    }

    func testVariableRowHeightsAndPlaceholders() {
        let heights: [Float] = [30, 80, 44, 120, 44]
        var offs: [Float] = [0]
        for h in heights { offs.append(offs.last! + h) }
        h.apply { b in
            b.create(1, .list, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 844), props: [
                (.itemCount, .i32(5)), (.rowOffsets, .f32Array(offs)),
            ])
            b.create(10, .listRow, parent: 1, index: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 80), props: [(.rowIndex, .i32(1))])
        }
        let list = h.view(1, as: NSHListView.self)
        XCTAssertEqual(list.document.frame.height, CGFloat(offs.last!))
        XCTAssertEqual(h.view(10)?.frame, CGRect(x: 0, y: 30, width: 390, height: 80))
        XCTAssertEqual(list.document.subviews.count, 1, "rows not rendered leave their space empty")
        // The row moves to index 3 (RowIndex change, e.g. after an insert above).
        h.apply { $0.prop(10, .rowIndex, .i32(3)) }
        XCTAssertEqual(h.view(10)?.frame.minY, 154)
    }

    func testItemCountChangesReportNewRange() {
        h.apply { b in
            b.create(1, .list, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 844), props: [
                (.itemCount, .i32(3)), (.rowOffsets, .f32Array(offsets(3))),
            ])
        }
        let list = h.view(1, as: NSHListView.self)
        XCTAssertEqual(list.document.frame.height, 3 * 44)
        h.apply { b in
            b.prop(1, .itemCount, .i32(50))
            b.prop(1, .rowOffsets, .f32Array(offsets(50)))
        }
        XCTAssertEqual(list.document.frame.height, 50 * 44)
        // Range events are throttled to one per 16 ms; the latest always goes out.
        XCTAssertTrue(wait { self.h.events(.rangeChange).last?.payload.int("end") == 20 })
    }

    func testBottomAlignedListOpensAtTheEndAndFollowsIt() {
        h.apply { b in
            b.create(1, .list, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 440), props: [
                (.itemCount, .i32(100)), (.rowOffsets, .f32Array(offsets(100))), (.listAlignment, .i32(1)),
            ])
        }
        let list = h.view(1, as: NSHListView.self)
        let y = { list.scrollView.contentView.bounds.minY }
        XCTAssertEqual(y(), 4400 - 440, "opens at its end")
        XCTAssertEqual(h.events(.rangeChange).first?.payload.int("end"), 100, "the first range covers the last rows")
        // A row arrives while the user is at the end: the end stays in view.
        h.apply { b in
            b.prop(1, .itemCount, .i32(101))
            b.prop(1, .rowOffsets, .f32Array(offsets(101)))
        }
        XCTAssertEqual(y(), 4444 - 440)
        // The viewport shrinks (a panel opens below): still pinned.
        h.apply { $0.frame(1, CGRect(x: 0, y: 0, width: 390, height: 300)) }
        XCTAssertEqual(y(), 4444 - 300)
        // Scrolled away, the position is kept as for any list.
        list.scroll(toY: 1000, animated: false)
        h.apply { b in
            b.prop(1, .itemCount, .i32(102))
            b.prop(1, .rowOffsets, .f32Array(offsets(102)))
        }
        XCTAssertEqual(y(), 1000)
        // A top-aligned list opens at the top.
        h.apply { b in
            b.create(2, .list, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 440), props: [
                (.itemCount, .i32(100)), (.rowOffsets, .f32Array(offsets(100))),
            ])
        }
        XCTAssertEqual(h.view(2, as: NSHListView.self).scrollView.contentView.bounds.minY, 0)
    }
}
