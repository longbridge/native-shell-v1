import CoreGraphics
import NativeShellShared
import XCTest

final class SVGPathTests: XCTestCase {
    func testEveryCommandParses() {
        let p = SVGPath.parse("M2 2 L22 2 H 22 V22 h-20 v-20 C 2 2 4 4 6 6 S 8 8 10 10 Q 12 12 14 14 T 16 16 A 4 4 0 0 1 20 20 Z")
        let box = p!.boundingBoxOfPath
        XCTAssertEqual(box.minX, 2, accuracy: 0.01)
        XCTAssertEqual(box.maxX, 22, accuracy: 0.5)
        XCTAssertEqual(box.minY, 2, accuracy: 0.01)
    }

    func testMalformedDataNeverCrashes() {
        XCTAssertNil(SVGPath.parse("12 34"), "data must start with a command")
        XCTAssertNotNil(SVGPath.parse("M0,0l10-10z"))
        XCTAssertNotNil(SVGPath.parse("M 0 0 A 0 0 0 1 1 5 5"), "a zero radius arc is a line")
        _ = SVGPath.parse("M1e2 1.5.5L-.5-2e-1")
        _ = SVGPath.parse("M0 0 C 1")
        _ = SVGPath.parse("")
    }

    func testZeroLengthSegmentIsKept() {
        // Icon sets draw dots as `h.01`; the path must keep that segment for round caps to show.
        let p = SVGPath.parse("M12 12h.01")!
        XCTAssertFalse(p.isEmpty)
        XCTAssertEqual(p.currentPoint.x, 12.01, accuracy: 0.0001)
    }
}

final class JSONHelpersTests: XCTestCase {
    func testColorsParseIntegersAndHex() {
        XCTAssertEqual(ColorValue.argb(NSNumber(value: 0xFF11_2233 as UInt32)), 0xFF11_2233)
        XCTAssertEqual(ColorValue.argb("#123"), 0xFF11_2233)
        XCTAssertEqual(ColorValue.argb("#1234"), 0x4411_2233)
        XCTAssertEqual(ColorValue.argb("#112233"), 0xFF11_2233)
        XCTAssertEqual(ColorValue.argb("#11223344"), 0x4411_2233)
        XCTAssertNil(ColorValue.argb("112233"))
        XCTAssertNil(ColorValue.argb("#12"))
        XCTAssertNil(ColorValue.argb("#gggggg"))
        XCTAssertNil(ColorValue.argb(NSNumber(value: -1)))
        XCTAssertNil(ColorValue.argb(["r": 1]))
    }

    func testAccessorsTolerateMistypedFields() {
        let o = JSON.object(##"{"n": 2.5, "s": "x", "b": true, "o": {"k": 1}, "c": "#fff", "bad": [1]}"##)!
        XCTAssertEqual(o.double("n"), 2.5)
        XCTAssertEqual(o.int("n"), 2)
        XCTAssertEqual(o.string("s"), "x")
        XCTAssertEqual(o.bool("b"), true)
        XCTAssertEqual(o.object("o")?.int("k"), 1)
        XCTAssertEqual(o.argb("c"), 0xFFFF_FFFF)
        XCTAssertNil(o.double("s"))
        XCTAssertNil(o.string("n"))
        XCTAssertNil(o.object("bad"))
        XCTAssertNil(o.argb("missing"))
        XCTAssertNil(JSON.object("not json"))
        XCTAssertNil(JSON.object("[1]"))
        XCTAssertEqual(JSON.array("[1,2]")?.count, 2)
    }

    func testStringSerializesFragmentsAndRejectsTheRest() {
        XCTAssertEqual(JSON.string(nil), "null")
        XCTAssertEqual(JSON.string(NSNull()), "null")
        XCTAssertEqual(JSON.string("é"), "\"é\"")
        XCTAssertEqual(JSON.string(3), "3")
        XCTAssertEqual(JSON.string(["a": [1, 2]]), #"{"a":[1,2]}"#)
        XCTAssertEqual(JSON.string(Date()), "null")
    }

    func testPropValueAccessors() {
        XCTAssertEqual(PropValue.f32(1.5).number, 1.5)
        XCTAssertEqual(PropValue.i32(-3).int, -3)
        XCTAssertEqual(PropValue.bool(true).number, 1)
        XCTAssertNil(PropValue.null.number)
        XCTAssertEqual(PropValue.str("a").string, "a")
        XCTAssertEqual(PropValue.json("{}").string, "{}")
        XCTAssertEqual(PropValue.color(0xFF00_00FF).argb, 0xFF00_00FF)
        XCTAssertNil(PropValue.str("#fff").argb)
        XCTAssertEqual(PropValue.f32Array([1, 2]).floats, [1, 2])
        XCTAssertEqual(PropValue.json(#"{"a":1}"#).jsonObject?.int("a"), 1)
        XCTAssertEqual(PropValue.json("[1]").jsonArray?.count, 1)
        XCTAssertNil(PropValue.str(#"{"a":1}"#).jsonObject, "only Json values parse")
    }

    func testFrameRect() {
        XCTAssertEqual(Frame(x: 1, y: 2, width: 3, height: 4).cgRect, CGRect(x: 1, y: 2, width: 3, height: 4))
    }
}
