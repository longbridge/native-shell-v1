@testable import NativeShellMac
import AppKit
import XCTest
import NativeShellShared

/// Measurement and rendering go through the same TextKit configuration: a Text
/// view given the measured frame draws at the measured size.
final class TextTests: XCTestCase {
    var h: Harness!

    override func setUp() { h = Harness() }
    override func tearDown() { h.tearDown() }

    static let samples: [(String, String)] = [
        ("latin", "The quick brown fox jumps over the lazy dog, again and again until the line wraps."),
        ("cjk", "敏捷的棕色狐狸跳过了懒狗。日本語のテキストも折り返します。한국어 문장도 있습니다."),
        ("emoji", "Party 🎉🎉 time 👩‍👩‍👧‍👦 family 🇯🇵 flags ✨ and more ✨✨✨"),
        ("mixed", "Price 價格 ¥1,234.50 (+3.2%) 📈 updated 10:42 — Ünïcödé"),
        ("short", "Hi"),
        ("empty", ""),
    ]

    /// Measures with the measurer (as the core would on the JS thread), mounts a Text
    /// with that frame, and compares with what the view lays out for drawing.
    /// What the core does: measure, snap the width up to the pixel grid, measure again at it.
    private func check(_ json: String, width: CGFloat, mode: Int, label: String, file: StaticString = #filePath, line: UInt = #line) -> TextMeasurer.Metrics {
        var m = TextMeasurer.Metrics(width: 0, height: 0, baseline: 0)
        let done = expectation(description: "measured")
        Thread.detachNewThread {
            let first = TextMeasurer.shared.measure(json, width: width, mode: mode)
            let s = TextMeasurer.scale
            let snapped = (first.width * s - 0.001).rounded(.up) / s
            m = TextMeasurer.shared.measure(json, width: snapped, mode: 2)
            done.fulfill()
        }
        wait(for: [done], timeout: 5)
        h.apply { b in
            b.create(1, .text, parent: 0, frame: CGRect(x: 0, y: 0, width: m.width, height: m.height), props: [(.attributedText, .json(json))])
        }
        let v = h.view(1, as: NSHTextView.self)
        v.layoutSubtreeIfNeeded()
        let drawn = v.drawnSize
        XCTAssertLessThanOrEqual(abs(drawn.width - m.width), 1, "\(label) width drawn \(drawn.width) measured \(m.width)", file: file, line: line)
        XCTAssertLessThanOrEqual(abs(drawn.height - m.height), 1, "\(label) height drawn \(drawn.height) measured \(m.height)", file: file, line: line)
        // Rendering must not clip: the used size fits the frame.
        XCTAssertLessThanOrEqual(drawn.height, m.height + 0.01, label, file: file, line: line)
        h.apply { $0.delete(1) }
        return m
    }

    func testMeasureMatchesRenderAcrossScriptsAndLineHeights() {
        var report: [String] = []
        for (name, text) in Self.samples {
            for lh in [nil, 1.0, 1.3, 2.0] as [Double?] {
                for size in [13.0, 16.0, 22.0] {
                    let json = textJSON(text, size: size, lineHeight: lh)
                    let wrapped = check(json, width: 200, mode: 1, label: "\(name) lh=\(lh.map { "\($0)" } ?? "normal") size=\(size) at-most 200")
                    let single = check(json, width: .infinity, mode: 0, label: "\(name) lh=\(lh.map { "\($0)" } ?? "normal") size=\(size) unconstrained")
                    if text.isEmpty {
                        XCTAssertEqual(single.width, 0)
                        XCTAssertGreaterThan(single.height, 0, "an empty string is one line tall")
                    } else if name != "short" {
                        XCTAssertGreaterThan(wrapped.height, single.height, "\(name) wraps at 200")
                        XCTAssertLessThanOrEqual(wrapped.width, 200)
                    }
                    if let lh = lh, !text.isEmpty {
                        // Forced line height: every line is exactly lh * size.
                        let lines = (single.height / CGFloat(lh * size)).rounded()
                        XCTAssertEqual(single.height, CGFloat(lh * size) * lines, accuracy: 1, "\(name) lh=\(lh)")
                    }
                    if size == 16 { report.append("\(name) lh=\(lh.map { "\($0)" } ?? "-"): \(single.width)x\(single.height) / wrapped \(wrapped.width)x\(wrapped.height)") }
                }
            }
        }
        print("TEXT GOLDEN\n" + report.joined(separator: "\n"))
    }

    func testFontScaleChangesKeepMeasureAndRenderInSync() {
        // A font scale change arrives as new sizes in the attributed JSON.
        for scale in [0.85, 1.0, 1.35, 2.0] {
            for (name, text) in Self.samples where !text.isEmpty {
                _ = check(textJSON(text, size: 16 * scale, lineHeight: 1.3), width: 240, mode: 1, label: "\(name) scale \(scale)")
            }
        }
    }

    func testMeasurementHeightsArePixelAlignedAndCached() {
        let json = textJSON("Pixel aligned measurement", size: 15)
        let m = TextMeasurer.shared.measure(json, width: 300, mode: 1)
        let s = TextMeasurer.scale
        XCTAssertEqual((m.height * s).rounded(), m.height * s, accuracy: 0.001)
        XCTAssertGreaterThan(m.baseline, 0)
        XCTAssertLessThan(m.baseline, m.height)
        XCTAssertEqual(TextMeasurer.shared.measure(json, width: 300, mode: 1), m)
        // Exactly mode reports the given width.
        XCTAssertEqual(TextMeasurer.shared.measure(json, width: 300, mode: 2).width, 300)
    }

    func testLinesAndTruncation() {
        let long = String(repeating: "word ", count: 80)
        let two = TextMeasurer.shared.measure(textJSON(long, lines: 2, truncate: true), width: 200, mode: 1)
        let one = TextMeasurer.shared.measure(textJSON(long, lines: 1, truncate: true), width: 200, mode: 1)
        XCTAssertEqual(two.height, one.height * 2, accuracy: 1)
        _ = check(textJSON(long, lines: 3, truncate: true), width: 180, mode: 1, label: "3 lines")
    }

    /// max-content W, then at most ceil(W): the same lines, so the same height.
    func testMaxContentWidthDoesNotWrap() {
        let s = TextMeasurer.scale
        for (name, text) in Self.samples where !text.isEmpty {
            for lh in [nil, 1.3] as [Double?] {
                let json = textJSON(text, size: 17, lineHeight: lh)
                let free = TextMeasurer.shared.measure(json, width: .infinity, mode: 0)
                let snapped = (free.width * s - 0.001).rounded(.up) / s
                let atMost = TextMeasurer.shared.measure(json, width: snapped, mode: 1)
                XCTAssertEqual(atMost.height, free.height, "\(name) re-wrapped at its own width")
                XCTAssertEqual(atMost.width, free.width, accuracy: 0.001, name)
                XCTAssertGreaterThan(free.width, 0, name)
            }
        }
    }

    func testCenteredTextMeasuresItsOwnWidth() {
        let m = TextMeasurer.shared.measure(textJSON("Centered", align: "center"), width: .infinity, mode: 0)
        let left = TextMeasurer.shared.measure(textJSON("Centered"), width: .infinity, mode: 0)
        XCTAssertEqual(m.width, left.width, accuracy: 0.5)
    }

    func testMeasurementIsThreadSafe() {
        let group = DispatchGroup()
        var results = [CGFloat](repeating: 0, count: 64)
        let lock = NSLock()
        for i in 0 ..< 64 {
            group.enter()
            DispatchQueue.global().async {
                let m = TextMeasurer().measure(textJSON("Concurrent \(i % 4) " + String(repeating: "x", count: i % 4)), width: 120, mode: 1)
                lock.lock(); results[i] = m.width; lock.unlock()
                group.leave()
            }
        }
        XCTAssertEqual(group.wait(timeout: .now() + 10), .success)
        for i in 0 ..< 64 { XCTAssertEqual(results[i], results[i % 4], accuracy: 0.001) }
    }

    func testLinksReportOrOpen() {
        let json = JSON.string(["runs": [["text": "Read the ", "size": 16], ["text": "docs", "size": 16, "url": "https://example.org/docs"]]])
        let m = TextMeasurer.shared.measure(json, width: 300, mode: 1)
        h.apply { b in
            b.create(1, .text, parent: 0, frame: CGRect(x: 0, y: 0, width: m.width, height: m.height), props: [(.attributedText, .json(json))])
            b.mask(1, EventMask.link)
        }
        let v = h.view(1, as: NSHTextView.self)
        XCTAssertTrue(v.parsed?.hasLinks ?? false)
        XCTAssertEqual(v.accessibilityLabel(), "Read the docs")
        // A click on the link run reports it; a click on plain text does not.
        Mouse.click(v, at: CGPoint(x: m.width - 8, y: m.height / 2))
        XCTAssertEqual(h.events(.link).first?.payload.string("url"), "https://example.org/docs")
        Mouse.click(v, at: CGPoint(x: 4, y: m.height / 2))
        XCTAssertEqual(h.events(.link).count, 1)
    }

    func testSelectableTextUsesTextKit1TextView() {
        let json = textJSON("Selectable text", size: 17)
        h.apply { b in
            b.create(1, .text, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 30), props: [(.attributedText, .json(json)), (.selectable, .bool(true))])
        }
        let v = h.view(1, as: NSHTextView.self)
        let tv = v.subviews.compactMap { $0 as? NSTextView }.first
        XCTAssertNotNil(tv)
        XCTAssertNil(tv?.textLayoutManager, "TextKit 1, like the measurer")
        XCTAssertEqual(tv?.textContainer?.lineFragmentPadding, 0)
        XCTAssertEqual(tv?.textContainerInset, .zero)
        XCTAssertEqual(tv?.string, "Selectable text")
        h.apply { $0.prop(1, .selectable, .null) }
        XCTAssertNil(v.subviews.compactMap { $0 as? NSTextView }.first)
    }
}
