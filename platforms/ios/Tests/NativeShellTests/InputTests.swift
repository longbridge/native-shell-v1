@testable import NativeShell
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

final class TextInputTests: XCTestCase {
    var h: Harness!

    override func setUp() {
        h = Harness()
        h.window.makeKeyAndVisible()
    }

    override func tearDown() { h.tearDown() }

    private func mount(multiline: Bool = false, focus: Bool = true, props: [(Prop, PropValue)] = []) -> NSHTextInputView {
        h.apply { b in
            b.create(1, .textInput, parent: 0, frame: CGRect(x: 0, y: 100, width: 300, height: multiline ? 120 : 44),
                     props: [(.multiline, .bool(multiline))] + props)
            b.mask(1, EventMask.change | EventMask.submit | EventMask.focus | EventMask.blur)
        }
        let v = h.view(1, as: NSHTextInputView.self)
        if focus { XCTAssertTrue(v.input.becomeFirstResponder()) }
        return v
    }

    private func value(_ text: String, _ count: Int) -> PropValue {
        .json(JSON.string(["text": text, "count": count]))
    }

    private func changes() -> [(text: String, count: Int, composing: Bool)] {
        h.events(.change).map { ($0.payload.string("text") ?? "", $0.payload.int("count") ?? -1, $0.payload.bool("composing") ?? false) }
    }

    private func checkTyping(multiline: Bool) {
        let v = mount(multiline: multiline)
        v.input.insertText("h")
        v.input.insertText("i")
        XCTAssertEqual(v.text, "hi")
        XCTAssertEqual(changes().map(\.text), ["h", "hi"])
        XCTAssertEqual(changes().map(\.count), [1, 2])
        XCTAssertEqual(h.events(.change).last?.payload["selection"] as? [Int], [2, 2])

        // The core echoes an older value (count 1): dropped.
        h.apply { $0.prop(1, .textValue, value("h", 1)) }
        XCTAssertEqual(v.text, "hi")
        // The script transforms the latest value (count 2): applied, not reported back.
        h.apply { $0.prop(1, .textValue, value("HI", 2)) }
        XCTAssertEqual(v.text, "HI")
        XCTAssertEqual(changes().count, 2)
        // Same text: nothing happens.
        h.apply { $0.prop(1, .textValue, value("HI", 2)) }
        XCTAssertEqual(v.text, "HI")
    }

    func testTypingAndProgrammaticSetSingleLine() { checkTyping(multiline: false) }
    func testTypingAndProgrammaticSetMultiline() { checkTyping(multiline: true) }

    /// Pinyin: "nihao" composes, then commits "你好". A value set mid-composition must not clobber it.
    private func checkComposition(multiline: Bool, steps: [String], commit: String) {
        let v = mount(multiline: multiline)
        for s in steps {
            v.input.setMarkedText(s, selectedRange: NSRange(location: (s as NSString).length, length: 0))
            XCTAssertTrue(v.isComposing)
            XCTAssertEqual(v.text, s)
            // The script (still at the previous count) pushes a value mid-composition.
            let count = changes().last?.count ?? 0
            h.apply { $0.prop(1, .textValue, value("CLOBBER", count)) }
            XCTAssertEqual(v.text, s, "composition is never overwritten")
        }
        XCTAssertTrue(changes().contains { $0.composing }, "composing edits are reported as composing")
        // Commit.
        v.input.insertText(commit)
        if v.isComposing { v.input.unmarkText() }
        XCTAssertFalse(v.isComposing)
        XCTAssertEqual(v.text, commit, "the deferred stale value was dropped after the commit edit")
        XCTAssertEqual(changes().last?.text, commit)
        XCTAssertEqual(changes().last?.composing, false)
        // Counts strictly increase.
        let counts = changes().map(\.count)
        XCTAssertEqual(counts, counts.sorted())
        XCTAssertEqual(Set(counts).count, counts.count)
        // A value acknowledging the latest count applies again.
        h.apply { $0.prop(1, .textValue, value(commit + "!", counts.last ?? 0)) }
        XCTAssertEqual(v.text, commit + "!")
    }

    func testChinesePinyinComposition() {
        checkComposition(multiline: false, steps: ["n", "ni", "ni h", "ni ha", "ni hao"], commit: "你好")
    }

    func testJapaneseKanaComposition() {
        checkComposition(multiline: false, steps: ["か", "かん", "かんじ"], commit: "漢字")
    }

    func testCompositionInMultiline() {
        checkComposition(multiline: true, steps: ["z", "zh", "zho", "zhon", "zhong"], commit: "中")
    }

    func testMaxLengthAndAllowedCharsNatively() {
        let v = mount(props: [(.maxLength, .i32(4)), (.allowedChars, .str("0123456789"))])
        v.input.insertText("12")
        v.input.insertText("a")
        v.input.insertText("345")
        XCTAssertEqual(v.text, "1234")
        XCTAssertEqual(changes().last?.text, "1234")
        // IME commits bypass shouldChange: clamped when composition ends.
        h.apply { $0.prop(1, .textValue, value("", changes().last?.count ?? 0)) }
        XCTAssertEqual(v.text, "")
        v.input.setMarkedText("12345678", selectedRange: NSRange(location: 8, length: 0))
        v.input.unmarkText()
        XCTAssertEqual(v.text, "1234")
    }

    func testSubmitFocusBlurAndCommands() {
        let v = mount(props: [(.returnKey, .i32(1))])
        XCTAssertFalse(h.events(.focus).isEmpty)
        v.input.insertText("x")
        _ = v.textFieldShouldReturn(v.field!)
        XCTAssertEqual(h.events(.submit).count, 1)
        XCTAssertFalse(v.input.isFirstResponder, "return key `done` blurs")
        XCTAssertFalse(h.events(.blur).isEmpty)
        h.apply { $0.command(1, .focus) }
        XCTAssertTrue(v.input.isFirstResponder)
        h.apply { $0.command(1, .blur) }
        XCTAssertFalse(v.input.isFirstResponder)
    }

    /// ContentInsets (the style padding) inset the text exactly once.
    func testContentInsetsApplyOnce() {
        let v = mount(focus: false, props: [(.contentInsets, .f32Array([4, 12, 4, 12])), (.placeholder, .str("Your name"))])
        v.layoutIfNeeded()
        let f = v.field!
        // The field is inset by exactly ContentInsets; its own text, placeholder and caret start at its edge.
        XCTAssertEqual(f.frame, CGRect(x: 12, y: 4, width: 276, height: 36))
        XCTAssertEqual(f.textRect(forBounds: f.bounds).minX, 0, accuracy: 0.5)
        XCTAssertEqual(f.placeholderRect(forBounds: f.bounds).minX, 0, accuracy: 0.5)
        XCTAssertTrue(f.becomeFirstResponder())
        f.insertText("Buy")
        f.layoutIfNeeded()
        XCTAssertEqual(f.convert(f.caretRect(for: f.beginningOfDocument), to: v).minX, 12, accuracy: 2)
        h.apply { $0.prop(1, .multiline, .bool(true)) }
        XCTAssertEqual(v.textView?.textContainerInset, UIEdgeInsets(top: 4, left: 12, bottom: 4, right: 12))
    }

    func testInputPropsAndReset() {
        let v = mount(focus: false, props: [
            (.placeholder, .str("Search")), (.secure, .bool(true)), (.keyboardType, .i32(3)), (.returnKey, .i32(4)),
            (.contentType, .i32(1)), (.autoCapitalize, .i32(0)), (.editable, .bool(false)),
            (.textStyle, .json(#"{"size":20,"weight":700,"color":4294901760,"align":"center"}"#)),
        ])
        let f = v.field!
        XCTAssertEqual(f.placeholder, "Search")
        XCTAssertTrue(f.isSecureTextEntry)
        XCTAssertEqual(f.keyboardType, .emailAddress)
        XCTAssertEqual(f.returnKeyType, .search)
        XCTAssertEqual(f.textContentType, .oneTimeCode)
        XCTAssertEqual(f.autocapitalizationType, .none)
        XCTAssertFalse(f.isEnabled)
        XCTAssertEqual(f.font?.pointSize, 20)
        XCTAssertEqual(f.textAlignment, .center)
        h.apply { $0.delete(1) }
        h.apply { $0.create(2, .textInput, parent: 0) }
        let reused = h.view(2, as: NSHTextInputView.self)
        XCTAssertTrue(reused === v)
        XCTAssertNil(reused.field?.placeholder)
        XCTAssertFalse(reused.field!.isSecureTextEntry)
        XCTAssertEqual(reused.field!.keyboardType, .default)
        XCTAssertTrue(reused.field!.isEnabled)
        XCTAssertEqual(reused.text, "")
        XCTAssertEqual(reused.state.eventCount, 0)
    }
}
