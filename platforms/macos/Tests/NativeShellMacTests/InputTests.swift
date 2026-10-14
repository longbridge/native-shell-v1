@testable import NativeShellMac
import AppKit
import XCTest

final class TextInputTests: XCTestCase {
    var h: Harness!

    override func setUp() { h = Harness() }
    override func tearDown() { h.tearDown() }

    private let none = NSRange(location: NSNotFound, length: 0)

    private func mount(multiline: Bool = false, props: [(Prop, PropValue)] = []) -> NSHTextInputView {
        h.apply { b in
            b.create(1, .textInput, parent: 0, frame: CGRect(x: 0, y: 0, width: 300, height: multiline ? 120 : 44),
                     props: [(.multiline, .bool(multiline)), (.placeholder, .str("Add a task"))] + props)
            b.mask(1, EventMask.change | EventMask.submit | EventMask.focus | EventMask.blur)
        }
        return h.view(1, as: NSHTextInputView.self)
    }

    /// The editor that takes typing once the input is focused.
    private func editor(_ v: NSHTextInputView) -> NSTextView {
        v.focus()
        return (v.field?.currentEditor() as? NSTextView) ?? v.textView!
    }

    private func checkTyping(multiline: Bool) {
        let v = mount(multiline: multiline)
        let ed = editor(v)
        XCTAssertEqual(h.events(.focus).count, 1)
        ed.insertText("he", replacementRange: none)
        ed.insertText("y", replacementRange: none)
        let changes = h.events(.change)
        XCTAssertEqual(changes.map { $0.payload.string("text") }, ["he", "hey"])
        XCTAssertEqual(changes.last?.payload.int("count"), 2)
        XCTAssertEqual(changes.last?.payload["selection"] as? [Int], [3, 3])
        XCTAssertEqual(changes.last?.payload.bool("composing"), false)

        // The script's value written back: applied without echoing, stale ones dropped.
        h.apply { $0.prop(1, .textValue, .json(#"{"text":"HEY","count":2}"#)) }
        XCTAssertEqual(v.text, "HEY")
        XCTAssertEqual(h.events(.change).count, 2, "programmatic writes do not echo")
        h.apply { $0.prop(1, .textValue, .json(#"{"text":"old","count":1}"#)) }
        XCTAssertEqual(v.text, "HEY")
        // Clearing (what a script does after submit) leaves the field empty and the caret at 0.
        h.apply { $0.prop(1, .textValue, .json(#"{"text":"","count":2}"#)) }
        XCTAssertEqual(v.text, "")
    }

    func testTypingAndProgrammaticSetSingleLine() { checkTyping(multiline: false) }
    func testTypingAndProgrammaticSetMultiline() { checkTyping(multiline: true) }

    private func checkComposition(multiline: Bool, marked: [String], committed: String) {
        let v = mount(multiline: multiline)
        let ed = editor(v)
        for m in marked {
            ed.setMarkedText(m, selectedRange: NSRange(location: (m as NSString).length, length: 0), replacementRange: none)
        }
        XCTAssertTrue(v.isComposing)
        XCTAssertTrue(wait { self.h.events(.change).last?.payload.string("text") == marked.last })
        XCTAssertEqual(h.events(.change).last?.payload.bool("composing"), true)
        let count = h.events(.change).last?.payload.int("count") ?? 0
        // A script value mid-composition waits; the commit is a newer edit, so it is dropped.
        h.apply { $0.prop(1, .textValue, .json("{\"text\":\"X\",\"count\":\(count)}")) }
        XCTAssertEqual(v.text, marked.last)
        ed.insertText(committed, replacementRange: none)
        XCTAssertFalse(v.isComposing)
        XCTAssertEqual(v.text, committed)
        XCTAssertEqual(h.events(.change).last?.payload.string("text"), committed)
        XCTAssertEqual(h.events(.change).last?.payload.bool("composing"), false)
        spin(0.05)
        XCTAssertEqual(h.events(.change).filter { $0.payload.string("text") == committed }.count, 1, "each edit is reported once")
    }

    func testChinesePinyinComposition() { checkComposition(multiline: false, marked: ["n", "ni", "ni h", "ni hao"], committed: "你好") }
    func testJapaneseKanaComposition() { checkComposition(multiline: false, marked: ["に", "にほ", "にほん"], committed: "日本") }
    func testCompositionInMultiline() { checkComposition(multiline: true, marked: ["z", "zh", "zho"], committed: "中") }

    func testMaxLengthAndAllowedCharsNatively() {
        let v = mount(props: [(.maxLength, .i32(4)), (.allowedChars, .str("0123456789"))])
        let ed = editor(v)
        ed.insertText("12a", replacementRange: none)
        XCTAssertEqual(v.text, "12")
        ed.insertText("3456", replacementRange: none)
        XCTAssertEqual(v.text, "1234")
        XCTAssertEqual(h.events(.change).last?.payload.string("text"), "1234")
    }

    func testSubmitFocusBlurAndCommands() {
        let v = mount(props: [(.returnKey, .i32(1))])
        let ed = editor(v)
        ed.insertText("milk", replacementRange: none)
        ed.doCommand(by: #selector(NSResponder.insertNewline(_:)))
        XCTAssertEqual(h.events(.submit).first?.payload.string("text"), "milk")
        // Return key "done" ends editing.
        XCTAssertTrue(wait { !self.h.events(.blur).isEmpty })
        h.apply { $0.command(1, .focus) }
        XCTAssertTrue(v.isEditing)
        h.apply { $0.command(1, .blur) }
        XCTAssertFalse(v.isEditing)
    }

    func testContentInsetsCenterTheLine() {
        let v = mount(props: [(.contentInsets, .f32Array([0, 16, 0, 12]))])
        v.layoutSubtreeIfNeeded()
        let f = v.field!.frame
        XCTAssertEqual(f.minX, 12)
        XCTAssertEqual(f.width, 300 - 28)
        XCTAssertEqual(f.midY, 22, accuracy: 1, "the line is centered like UIKit's")
        // Insets that leave less than a line never clip it.
        h.apply { $0.prop(1, .contentInsets, .f32Array([16, 16, 16, 12])) }
        v.layoutSubtreeIfNeeded()
        XCTAssertGreaterThanOrEqual(v.field!.frame.height, ceil(Fonts.lineHeight(v.field!.font!)))
        XCTAssertEqual(v.field!.frame.midY, 22, accuracy: 1)
    }

    func testInputPropsAndReset() {
        let v = mount(props: [(.secure, .bool(true)), (.textStyle, .json(#"{"size":20,"weight":600,"color":4294901760}"#)), (.contentType, .i32(3))])
        XCTAssertTrue(v.field is NSSecureTextField)
        XCTAssertEqual(v.field?.font?.pointSize, 20)
        XCTAssertEqual(v.field?.contentType, .password)
        XCTAssertEqual(v.field?.placeholderAttributedString?.string, "Add a task")
        let ed = editor(v)
        ed.insertText("secret", replacementRange: none)
        XCTAssertEqual(v.text, "secret")
        h.apply { $0.prop(1, .secure, .null) }
        XCTAssertFalse(v.field is NSSecureTextField)
        XCTAssertEqual(v.text, "secret", "switching the field kind keeps the text")
        h.apply { $0.delete(1) }
        h.apply { b in b.create(2, .textInput, parent: 0, frame: CGRect(x: 0, y: 0, width: 300, height: 44)) }
        let reused = h.view(2, as: NSHTextInputView.self)
        XCTAssertTrue(reused === v)
        XCTAssertEqual(reused.text, "")
        XCTAssertEqual(reused.state.eventCount, 0)
        XCTAssertNil(reused.field?.placeholderAttributedString)
    }
}

final class KeyTests: XCTestCase {
    var h: Harness!

    override func setUp() { h = Harness() }
    override func tearDown() { h.tearDown() }

    private func key(_ chars: String, code: UInt16, down: Bool = true, modifiers: NSEvent.ModifierFlags = []) -> NSEvent {
        NSEvent.keyEvent(with: down ? .keyDown : .keyUp, location: .zero, modifierFlags: modifiers, timestamp: 0,
                         windowNumber: h.window.windowNumber, context: nil, characters: chars, charactersIgnoringModifiers: chars,
                         isARepeat: false, keyCode: code)!
    }

    func testNamesAndPayloads() {
        XCTAssertEqual(KeyEvents.name(36, "\r"), "enter")
        XCTAssertEqual(KeyEvents.name(53, "\u{1b}"), "escape")
        XCTAssertEqual(KeyEvents.name(126, "\u{F700}"), "up")
        XCTAssertEqual(KeyEvents.name(122, "\u{F704}"), "f1")
        XCTAssertEqual(KeyEvents.name(0, "A"), "a")
        XCTAssertNil(KeyEvents.name(56, ""), "modifiers alone are never sent")
        let p = KeyEvents.payload(keyCode: 1, characters: "ß", ignoringModifiers: "s", modifiers: [.command, .option], phase: "down")!
        XCTAssertEqual(p.string("key"), "s")
        XCTAssertEqual(p.string("key_char"), "ß")
        XCTAssertEqual((p["modifiers"] as? [String: Any])?.bool("cmd"), true)
        XCTAssertEqual((p["modifiers"] as? [String: Any])?.bool("alt"), true)
        XCTAssertEqual(KeyEvents.payload(keyCode: 125, characters: "\u{F701}", ignoringModifiers: "\u{F701}", modifiers: [], phase: "up")?.string("key_char"), "")
    }

    func testKeysRouteToNearestListenerOrRoot() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 300, height: 300))
            b.mask(1, EventMask.key)
            b.create(2, .textInput, parent: 1, frame: CGRect(x: 0, y: 0, width: 300, height: 44))
            b.create(3, .textInput, parent: 0, frame: CGRect(x: 0, y: 100, width: 300, height: 44))
        }
        // Nothing focused: the root.
        XCTAssertTrue(h.surface.route(key("a", code: 0)))
        XCTAssertEqual(h.events(.key).last?.node, 0)
        // Focus inside node 1 (its input's field editor): node 1 listens.
        h.view(2, as: NSHTextInputView.self).focus()
        h.surface.route(key("b", code: 11))
        h.surface.route(key("b", code: 11, down: false))
        XCTAssertEqual(h.events(.key).suffix(2).map(\.node), [1, 1])
        XCTAssertEqual(h.events(.key).suffix(2).map { $0.payload.string("phase") }, ["down", "up"])
        // An input outside any listener: the root.
        h.view(3, as: NSHTextInputView.self).focus()
        h.surface.route(key("c", code: 8, modifiers: .command))
        XCTAssertEqual(h.events(.key).last?.node, 0)
        XCTAssertEqual(h.events(.key).last?.payload.string("key"), "c")
        // Other windows' keys are not ours.
        let other = NSWindow(contentRect: .zero, styleMask: [.titled], backing: .buffered, defer: true)
        let foreign = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: other.windowNumber,
                                       context: nil, characters: "x", charactersIgnoringModifiers: "x", isARepeat: false, keyCode: 7)!
        XCTAssertFalse(h.surface.route(foreign))
    }
}
