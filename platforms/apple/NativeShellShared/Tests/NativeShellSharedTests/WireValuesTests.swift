import NativeShellShared
import XCTest

/// Values added after the first protocol version, pinned to `crates/core/src/protocol.rs`.
final class WireValuesTests: XCTestCase {
    func testSheetResize() {
        XCTAssertEqual(EventType.resize.rawValue, 24)
    }

    func testListAlignment() {
        XCTAssertEqual(Prop.listAlignment.rawValue, 94)
    }

    func testPointerAdditions() {
        XCTAssertEqual(Prop.hoverOpacity.rawValue, 144)
        XCTAssertEqual(Prop.hoverBackground.rawValue, 145)
        XCTAssertEqual(Prop.cursor.rawValue, 146)
        XCTAssertEqual(Prop.tooltip.rawValue, 147)
        XCTAssertEqual(EventType.hover.rawValue, 23)
        XCTAssertEqual(EventMask.hover, 1 << 18)
        // GPUI's CursorStyle, 21 cases in order.
        XCTAssertEqual(CursorStyle.pointingHand.rawValue, 5)
        XCTAssertEqual(CursorStyle.resizeRow.rawValue, 15)
        XCTAssertEqual(CursorStyle.contextualMenu.rawValue, 20)
        XCTAssertNil(CursorStyle(rawValue: 21))
    }
}
