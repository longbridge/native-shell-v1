import NativeShellShared
@testable import NativeShell
import XCTest

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
        list.layoutIfNeeded()
        XCTAssertEqual(list.collectionView.contentSize.height, CGFloat(count) * 44)
        // Initial range: 844 / 44 → rows 0..<20, plus prefetch 5 at the end.
        let first = h.events(.rangeChange).last?.payload
        XCTAssertEqual(first?.int("first"), 0)
        XCTAssertEqual(first?.int("end"), 25)

        // Visible cells host the rendered rows.
        let hosted = list.collectionView.visibleCells.compactMap { $0.contentView.subviews.first as? NSHListRowView }
        XCTAssertEqual(hosted.count, list.collectionView.visibleCells.count)
        XCTAssertEqual(Set(hosted.map(\.rowIndex)), Set(0 ..< hosted.count))

        // Scroll in steps; each new range moves the window the way the core would.
        var ranges: [(Int, Int)] = []
        for y in stride(from: 0, through: 400_000, by: 20_000) {
            list.collectionView.setContentOffset(CGPoint(x: 0, y: y), animated: false)
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
        list.layoutIfNeeded()
        let visible = list.collectionView.visibleCells.compactMap { $0.contentView.subviews.first as? NSHListRowView }
        XCTAssertFalse(visible.isEmpty)
        for row in visible {
            let ip = list.collectionView.indexPath(for: row.superview!.superview as! UICollectionViewCell)!
            XCTAssertEqual(row.rowIndex, ip.item, "cells host the row of their index")
        }

        // Scroll to the end; the range is clamped to the item count.
        list.collectionView.setContentOffset(CGPoint(x: 0, y: CGFloat(count) * 44 - 844), animated: false)
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
        XCTAssertEqual(list.collectionView.contentOffset.y, 4400)
        h.apply { $0.command(1, .scrollToIndex, ["index": 100, "position": "center", "animated": false]) }
        XCTAssertEqual(list.collectionView.contentOffset.y, 4400 - (440 - 44) / 2)
        h.apply { $0.command(1, .scrollToIndex, ["index": 100, "position": "bottom", "animated": false]) }
        XCTAssertEqual(list.collectionView.contentOffset.y, 4400 + 44 - 440)
        h.apply { $0.command(1, .scrollToIndex, ["index": 99_999, "animated": false]) }
        XCTAssertEqual(list.collectionView.contentOffset.y, 44_000 - 440, "clamped")
    }

    /// Unmeasured rows sit at estimated heights; as the core measures the rows
    /// around the target, ScrollToIndex re-aims until the row is at the edge.
    func testScrollToIndexCorrectsAsRowsAreMeasured() {
        // Rows 0..<n measured at `measured` points, the rest still estimated at 44.
        func offsets(measuredUpTo n: Int, _ measured: Float = 90) -> [Float] {
            var o: [Float] = [0]
            for i in 0 ..< 200 { o.append(o.last! + (i < n ? measured : rowHeight)) }
            return o
        }
        for position in ["top", "center", "bottom"] {
            let h = Harness()
            defer { h.tearDown() }
            h.apply { b in
                b.create(1, .list, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 440), props: [
                    (.itemCount, .i32(200)), (.rowOffsets, .f32Array(offsets(measuredUpTo: 10))),
                ])
                b.command(1, .scrollToIndex, ["index": 150, "position": position, "animated": false])
            }
            let list = h.view(1, as: NSHListView.self)
            func expected(_ offs: [Float]) -> CGFloat {
                let top = CGFloat(offs[150]), rowH = CGFloat(offs[151] - offs[150]), total = CGFloat(offs.last!)
                let y: CGFloat = position == "center" ? top - (440 - rowH) / 2 : position == "bottom" ? top + rowH - 440 : top
                return max(0, min(y, total - 440))
            }
            XCTAssertEqual(list.collectionView.contentOffset.y, expected(offsets(measuredUpTo: 10)), position)
            // The window around the estimated position gets measured, then the next one.
            for n in [140, 155, 200] {
                let offs = offsets(measuredUpTo: n)
                h.apply { $0.prop(1, .rowOffsets, .f32Array(offs)) }
                XCTAssertEqual(list.collectionView.contentOffset.y, expected(offs), "\(position) after \(n) measured")
            }
            // Stable offsets end the correction: later changes no longer move the list.
            h.apply { $0.prop(1, .rowOffsets, .f32Array(offsets(measuredUpTo: 200))) }
            let settled = list.collectionView.contentOffset.y
            h.apply { $0.prop(1, .rowOffsets, .f32Array(offsets(measuredUpTo: 200, 100))) }
            XCTAssertEqual(list.collectionView.contentOffset.y, settled, position)
        }
    }

    func testDraggingCancelsScrollCorrection() {
        h.apply { b in
            b.create(1, .list, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 440), props: [
                (.itemCount, .i32(100)), (.rowOffsets, .f32Array(offsets(100))),
            ])
            b.command(1, .scrollToIndex, ["index": 80, "animated": false])
        }
        let list = h.view(1, as: NSHListView.self)
        XCTAssertEqual(list.collectionView.contentOffset.y, 80 * 44)
        list.scrollViewWillBeginDragging(list.collectionView)
        h.apply { $0.prop(1, .rowOffsets, .f32Array((0 ... 100).map { Float($0) * 50 })) }
        XCTAssertEqual(list.collectionView.contentOffset.y, 80 * 44, "the user's scroll position wins")
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
        list.layoutIfNeeded()
        // RowOffsets has ItemCount + 1 entries (the last is the total): never an extra cell.
        XCTAssertEqual(offs.count, 6)
        XCTAssertEqual(list.collectionView.numberOfItems(inSection: 0), 5)
        XCTAssertEqual(list.collectionView.visibleCells.count, 5)
        XCTAssertEqual(list.collectionView.contentSize.height, CGFloat(offs.last!))
        let cells = list.collectionView.visibleCells.sorted { $0.frame.minY < $1.frame.minY }
        XCTAssertEqual(cells.map(\.frame.height), heights.map { CGFloat($0) })
        // Only row 1 is rendered: the others are empty placeholders.
        XCTAssertEqual(cells.map { $0.contentView.subviews.count }, [0, 1, 0, 0, 0])
        // The row moves to index 3 (RowIndex change, e.g. after an insert above).
        h.apply { $0.prop(10, .rowIndex, .i32(3)) }
        let after = list.collectionView.visibleCells.sorted { $0.frame.minY < $1.frame.minY }
        XCTAssertEqual(after.map { $0.contentView.subviews.count }, [0, 0, 0, 1, 0])
    }

    func testItemCountChangesReload() {
        h.apply { b in
            b.create(1, .list, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 844), props: [
                (.itemCount, .i32(3)), (.rowOffsets, .f32Array(offsets(3))),
            ])
        }
        let list = h.view(1, as: NSHListView.self)
        list.layoutIfNeeded()
        XCTAssertEqual(list.collectionView.numberOfItems(inSection: 0), 3)
        h.apply { b in
            b.prop(1, .itemCount, .i32(50))
            b.prop(1, .rowOffsets, .f32Array(offsets(50)))
        }
        list.layoutIfNeeded()
        XCTAssertEqual(list.collectionView.numberOfItems(inSection: 0), 50)
        XCTAssertEqual(list.collectionView.contentSize.height, 50 * 44)
        // Range events are throttled to one per 16 ms; the latest always goes out.
        XCTAssertTrue(wait { self.h.events(.rangeChange).last?.payload.int("end") == 20 })
    }

    /// A viewport change relayouts the cells; every visible cell still shows its
    /// rendered row (the last row of a chat thread once went missing).
    func testRowsStayHostedAcrossViewportChanges() {
        let heights: [Float] = [114, 51, 138, 242, 81, 60, 90, 44, 120, 70, 51, 138, 242, 81]
        var offs: [Float] = [0]
        for x in heights { offs.append(offs.last! + x) }
        h.apply { b in
            b.create(1, .list, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 627), props: [
                (.listAlignment, .i32(1)), (.itemCount, .i32(Int32(heights.count))), (.rowOffsets, .f32Array(offs)),
            ])
            for (i, hgt) in heights.enumerated() {
                let row = UInt32(100 + i)
                b.create(row, .listRow, props: [(.rowIndex, .i32(Int32(i)))])
                b.frame(row, CGRect(x: 0, y: 0, width: 390, height: CGFloat(hgt)))
                b.insert(row, 1, UInt32(i))
            }
        }
        let list = h.view(1, as: NSHListView.self)
        func unhosted() -> [Int] {
            list.collectionView.visibleCells.compactMap { c in
                guard let i = list.collectionView.indexPath(for: c)?.item else { return nil }
                return c.contentView.subviews.contains { $0 === h.view(UInt32(100 + i)) } ? nil : i
            }.sorted()
        }
        XCTAssertEqual(unhosted(), [])
        for height in [534.0, 300, 627, 534] as [CGFloat] {
            h.apply { $0.frame(1, CGRect(x: 0, y: 0, width: 390, height: height)) }
            list.layoutIfNeeded()
            XCTAssertEqual(list.collectionView.contentOffset.y, CGFloat(offs.last!) - height, "pinned end at \(height)")
            XCTAssertEqual(unhosted(), [], "every visible row hosted at \(height)")
        }
    }

    func testBottomAlignedListOpensAtTheEndAndFollowsIt() {
        h.apply { b in
            b.create(1, .list, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 440), props: [
                (.listAlignment, .i32(1)), (.itemCount, .i32(100)), (.rowOffsets, .f32Array(offsets(100))), (.prefetch, .i32(2)),
            ])
        }
        let list = h.view(1, as: NSHListView.self)
        let cv = list.collectionView!
        XCTAssertEqual(cv.contentOffset.y, 100 * 44 - 440, "opens at its end")
        let first = h.events(.rangeChange).first?.payload
        XCTAssertEqual(first?.int("end"), 100, "first range covers the last rows")
        XCTAssertGreaterThan(first?.int("first") ?? 0, 80)

        // New rows while pinned: the end stays in view.
        h.apply { b in b.prop(1, .itemCount, .i32(103)); b.prop(1, .rowOffsets, .f32Array(offsets(103))) }
        XCTAssertEqual(cv.contentOffset.y, 103 * 44 - 440)
        // The viewport shrinks (keyboard): still at the end.
        h.apply { $0.frame(1, CGRect(x: 0, y: 0, width: 390, height: 300)) }
        list.layoutIfNeeded()
        XCTAssertEqual(cv.contentOffset.y, 103 * 44 - 300)

        // Rows change height with the same count (an image loads, estimates get measured): the end stays in view.
        var grown = offsets(103)
        for i in 90 ..< grown.count { grown[i] += Float(i - 89) * 120 }
        h.apply { $0.prop(1, .rowOffsets, .f32Array(grown)) }
        XCTAssertEqual(cv.contentOffset.y, CGFloat(grown.last!) - 300, "taller rows")
        h.apply { $0.prop(1, .rowOffsets, .f32Array(offsets(103))) }
        XCTAssertEqual(cv.contentOffset.y, 103 * 44 - 300, "shorter rows")
        XCTAssertTrue(list.pinnedToEnd)

        // The user scrolls away: the position is kept as rows arrive.
        cv.setContentOffset(CGPoint(x: 0, y: 1000), animated: false)
        list.userScrolled()
        XCTAssertFalse(list.pinnedToEnd)
        h.apply { b in b.prop(1, .itemCount, .i32(110)); b.prop(1, .rowOffsets, .f32Array(offsets(110))) }
        XCTAssertEqual(cv.contentOffset.y, 1000)
        // Back at the end: pinned again.
        cv.setContentOffset(CGPoint(x: 0, y: 110 * 44 - 300 - 4), animated: false)
        list.userScrolled()
        XCTAssertTrue(list.pinnedToEnd)

        // Top-aligned lists (default) open at the top.
        h.apply { b in
            b.create(2, .list, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 440), props: [
                (.itemCount, .i32(100)), (.rowOffsets, .f32Array(offsets(100))),
            ])
        }
        XCTAssertEqual(h.view(2, as: NSHListView.self).collectionView.contentOffset.y, 0)
    }
}
