@testable import NativeShellMac
import NativeShellShared
import XCTest

/// The codec itself is tested in NativeShellShared; this checks the renderer's side.
final class RejectedBatchTests: XCTestCase {
    func testRejectedBatchLeavesTreeUntouched() {
        let h = Harness()
        defer { h.tearDown() }
        h.apply { $0.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 10, height: 10)) }
        var b = Batch(seq: 2)
        b.create(2, .view, parent: 1)
        var data = b.encoded()
        data.removeLast(4) // drop EndBatch's word and break the header count
        XCTAssertFalse(h.surface.apply(data))
        XCTAssertNil(h.view(2))
        XCTAssertEqual(h.view(1)?.children.count, 0)
    }
}
