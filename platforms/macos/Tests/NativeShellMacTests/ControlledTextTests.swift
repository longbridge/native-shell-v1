@testable import NativeShellMac
import XCTest

final class ControlledTextTests: XCTestCase {
    func testStaleValuesAreDropped() {
        var s = ControlledText()
        XCTAssertEqual(s.incoming(text: "a", count: 0, current: "", composing: false), .apply("a"))
        _ = s.didEdit() // 1
        _ = s.didEdit() // 2
        XCTAssertEqual(s.incoming(text: "old", count: 1, current: "ab", composing: false), .skip, "older than the latest edit")
        XCTAssertEqual(s.incoming(text: "ab", count: 2, current: "ab", composing: false), .skip, "equal text")
        XCTAssertEqual(s.incoming(text: "AB", count: 2, current: "ab", composing: false), .apply("AB"))
        XCTAssertEqual(s.incoming(text: "later", count: 9, current: "ab", composing: false), .apply("later"))
    }

    func testCompositionDefersAndRevalidates() {
        var s = ControlledText()
        _ = s.didEdit() // marked text "n"
        XCTAssertEqual(s.incoming(text: "X", count: 1, current: "n", composing: true), .deferred)
        // Composition ends with a commit edit: the deferred value is now stale.
        _ = s.didEdit()
        XCTAssertEqual(s.compositionEnded(current: "你"), .skip)
        XCTAssertNil(s.pending)

        // Deferred value that is still current after composition (no further edit) applies.
        XCTAssertEqual(s.incoming(text: "Y", count: 2, current: "你", composing: true), .deferred)
        XCTAssertEqual(s.compositionEnded(current: "你"), .apply("Y"))
    }

    func testFilterMaxLengthCountsUTF16AndKeepsCharactersWhole() {
        let f = InputFilter(maxLength: 5, allowed: nil)
        XCTAssertEqual(f.admit(current: "abc", range: NSRange(location: 3, length: 0), replacement: "de"), "de")
        XCTAssertEqual(f.admit(current: "abc", range: NSRange(location: 3, length: 0), replacement: "defg"), "de")
        XCTAssertNil(f.admit(current: "abcde", range: NSRange(location: 5, length: 0), replacement: "f"))
        XCTAssertEqual(f.admit(current: "abcde", range: NSRange(location: 4, length: 1), replacement: ""), "", "deleting is always allowed")
        // 😀 is two UTF-16 units: with 1 unit left it does not fit.
        XCTAssertNil(f.admit(current: "abcd", range: NSRange(location: 4, length: 0), replacement: "😀"))
        XCTAssertEqual(f.admit(current: "abc", range: NSRange(location: 3, length: 0), replacement: "😀x"), "😀")
        XCTAssertEqual(f.clamp("1234567"), "12345")
    }

    func testFilterAllowedChars() {
        let f = InputFilter(maxLength: 0, allowed: Set("0123456789."))
        XCTAssertEqual(f.admit(current: "", range: NSRange(location: 0, length: 0), replacement: "12.5"), "12.5")
        XCTAssertEqual(f.admit(current: "", range: NSRange(location: 0, length: 0), replacement: "1a2"), "12")
        XCTAssertNil(f.admit(current: "", range: NSRange(location: 0, length: 0), replacement: "abc"))
        XCTAssertEqual(f.clamp("4x2"), "42")
    }
}
