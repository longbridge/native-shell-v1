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
}
