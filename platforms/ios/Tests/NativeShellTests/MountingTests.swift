import NativeShellShared
@testable import NativeShell
import NativeShellFFI
import XCTest

final class MountingTests: XCTestCase {
    var h: Harness!

    override func setUp() { h = Harness() }
    override func tearDown() { h.tearDown() }

    func testCreateInsertFrameAndOrder() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 844))
            b.create(2, .view, parent: 1, index: 0, frame: CGRect(x: 10, y: 20, width: 100, height: 50))
            b.create(3, .view, parent: 1, index: 1, frame: CGRect(x: 10, y: 80, width: 100, height: 50))
            b.create(4, .view, parent: 1, index: 0, frame: CGRect(x: 0, y: 0, width: 5, height: 5))
        }
        let root = h.view(1)!
        XCTAssertEqual(root.children.map(\.nodeId), [4, 2, 3])
        XCTAssertEqual(root.subviews.compactMap { ($0 as? NSHComponentView)?.nodeId }, [4, 2, 3])
        XCTAssertEqual(h.view(2)!.frame, CGRect(x: 10, y: 20, width: 100, height: 50))

        h.apply { $0.move(4, 1, 0, 2) }
        XCTAssertEqual(root.subviews.compactMap { ($0 as? NSHComponentView)?.nodeId }, [2, 3, 4])
        h.apply { $0.remove(3, 1, 1) }
        XCTAssertEqual(root.children.map(\.nodeId), [2, 4])
        XCTAssertNil(h.view(3)!.superview)
        h.apply { $0.insert(3, 1, 0) }
        XCTAssertEqual(root.subviews.compactMap { ($0 as? NSHComponentView)?.nodeId }, [3, 2, 4])
        h.apply { $0.delete(1) }
        XCTAssertNil(h.view(1))
        XCTAssertNil(h.view(2))
        XCTAssertEqual(h.manager.nodeCount, 0)
    }

    func testUnknownNodesAreSkipped() {
        XCTAssertTrue(h.apply { b in
            b.insert(99, 0, 0)
            b.prop(98, .opacity, .f32(0.5))
            b.frame(97, .zero)
            b.delete(96)
            b.command(95, .focus)
            b.create(1, .view, parent: 0)
            b.frame(1, CGRect(x: 0, y: 0, width: CGFloat.nan, height: 1))
        })
        XCTAssertNotNil(h.view(1))
    }

    func testFrameUsesBoundsAndCenterUnderTransform() {
        h.apply { b in
            b.create(1, .view, parent: 0, props: [(.transform, .json(#"{"rotate":45,"scale":2}"#))])
            b.frame(1, CGRect(x: 10, y: 10, width: 100, height: 40))
        }
        let v = h.view(1)!
        XCTAssertEqual(v.bounds.size, CGSize(width: 100, height: 40))
        XCTAssertEqual(v.center, CGPoint(x: 60, y: 30))
        XCTAssertNotEqual(v.transform, .identity)
        h.apply { $0.prop(1, .transform, .null) }
        XCTAssertEqual(v.transform, .identity)
        XCTAssertEqual(v.frame, CGRect(x: 10, y: 10, width: 100, height: 40))
    }

    /// Every View prop restores its default on Null, and pooling replays those resets.
    func testViewPropsResetAndPooling() {
        let props: [(Prop, PropValue)] = [
            (.backgroundColor, .color(0xFFFF_0000)),
            (.cornerRadii, .f32Array([8, 8, 8, 8])),
            (.borderWidths, .f32Array([2, 2, 2, 2])),
            (.borderColor, .color(0xFF00_FF00)),
            (.opacity, .f32(0.5)),
            (.shadow, .json(#"{"color":2147483648,"x":0,"y":2,"blur":8,"spread":0}"#)),
            (.gradient, .json(#"{"angle":90,"stops":[[4294901760,0],[4278190335,1]]}"#)),
            (.clipsToBounds, .bool(true)),
            (.hidden, .bool(true)),
            (.a11yLabel, .str("Card")),
            (.a11yRole, .i32(1)),
            (.a11yHint, .str("Opens")),
            (.a11yValue, .str("3")),
            (.testId, .str("card")),
            (.hitSlop, .f32Array([10, 10, 10, 10])),
            (.activeOpacity, .f32(0.6)),
        ]
        h.apply { b in b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 100), props: props) }
        let v = h.view(1)!
        v.layoutIfNeeded()
        XCTAssertEqual(v.layer.backgroundColor.map { UIColor(cgColor: $0) }, UIColor(red: 1, green: 0, blue: 0, alpha: 1))
        XCTAssertEqual(v.layer.cornerRadius, 8)
        XCTAssertEqual(v.layer.borderWidth, 2)
        XCTAssertEqual(v.alpha, 0.5)
        XCTAssertTrue(v.clipsToBounds)
        XCTAssertTrue(v.isHidden)
        XCTAssertEqual(v.accessibilityLabel, "Card")
        XCTAssertTrue(v.accessibilityTraits.contains(.button))
        XCTAssertEqual(v.accessibilityIdentifier, "card")
        XCTAssertNotNil(v.layer.shadowPath)
        XCTAssertTrue(v.layer.sublayers?.contains { $0 is CAGradientLayer } ?? false)
        XCTAssertTrue(v.point(inside: CGPoint(x: -5, y: -5), with: nil))
        XCTAssertTrue(v.gestureRecognizers?.contains { $0 is PressGestureRecognizer } ?? false)

        func assertDefault(_ v: NSHComponentView, _ file: StaticString = #filePath, _ line: UInt = #line) {
            v.layoutIfNeeded()
            XCTAssertNil(v.layer.backgroundColor, file: file, line: line)
            XCTAssertEqual(v.layer.cornerRadius, 0, file: file, line: line)
            XCTAssertEqual(v.layer.borderWidth, 0, file: file, line: line)
            XCTAssertEqual(v.alpha, 1, file: file, line: line)
            XCTAssertFalse(v.clipsToBounds, file: file, line: line)
            XCTAssertFalse(v.isHidden, file: file, line: line)
            XCTAssertNil(v.accessibilityLabel, file: file, line: line)
            XCTAssertNil(v.accessibilityHint, file: file, line: line)
            XCTAssertNil(v.accessibilityIdentifier, file: file, line: line)
            XCTAssertEqual(v.accessibilityTraits, [], file: file, line: line)
            XCTAssertEqual(v.layer.shadowOpacity, 0, file: file, line: line)
            XCTAssertFalse(v.layer.sublayers?.contains { $0 is CAGradientLayer } ?? false, file: file, line: line)
            XCTAssertFalse(v.point(inside: CGPoint(x: -5, y: -5), with: nil), file: file, line: line)
            XCTAssertFalse(v.gestureRecognizers?.contains { $0 is PressGestureRecognizer } ?? false, file: file, line: line)
        }

        // Explicit resets.
        h.apply { b in for (p, _) in props { b.prop(1, p, .null) } }
        assertDefault(v)

        // Pooling: set everything again, delete, create a new node of the same type.
        h.apply { b in for (p, val) in props { b.prop(1, p, val) }; b.mask(1, EventMask.click | EventMask.longPress) }
        XCTAssertFalse(v.gestureRecognizers?.isEmpty ?? true)
        h.apply { $0.delete(1) }
        XCTAssertEqual(h.manager.pooledCount(.view), 1)
        h.apply { $0.create(2, .view, parent: 0) }
        let reused = h.view(2)!
        XCTAssertTrue(reused === v, "the pooled view is reused")
        XCTAssertEqual(reused.eventMask, 0)
        XCTAssertEqual(reused.gestureRecognizers?.count ?? 0, 0)
        assertDefault(reused)
    }

    func testPerCornerRadiiAndPerSideBorders() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 60), props: [
                (.backgroundColor, .color(0xFF00_00FF)),
                (.cornerRadii, .f32Array([12, 0, 12, 0])),
            ])
            b.create(2, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 60), props: [
                (.borderWidths, .f32Array([0, 0, 1, 0])),
                (.borderColor, .color(0xFF00_0000)),
            ])
        }
        let a = h.view(1)!, b = h.view(2)!
        a.layoutIfNeeded(); b.layoutIfNeeded()
        XCTAssertNil(a.layer.backgroundColor)
        let shape = a.layer.sublayers?.first { $0 is CAShapeLayer } as? CAShapeLayer
        XCTAssertNotNil(shape?.path)
        XCTAssertNotNil(shape?.fillColor)
        // Bottom-only border: a 1-pt band along the bottom edge, nothing elsewhere.
        let px = pixels(b)
        XCTAssertGreaterThan(px(50, 59.5).3, 200)
        XCTAssertLessThan(px(50, 58).3, 10)
        XCTAssertLessThan(px(50, 0.5).3, 10)
        XCTAssertLessThan(px(0.5, 30).3, 10)
    }

    func testEventMaskControlsGesturesAndEvents() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 100))
        }
        let v = h.view(1)!
        XCTAssertEqual(v.gestureRecognizers?.count ?? 0, 0)
        h.apply { $0.mask(1, EventMask.click | EventMask.doubleClick | EventMask.longPress | EventMask.pan | EventMask.pinch) }
        let kinds = Set((v.gestureRecognizers ?? []).map { String(describing: type(of: $0)) })
        XCTAssertTrue(kinds.isSuperset(of: ["UITapGestureRecognizer", "UILongPressGestureRecognizer", "UIPanGestureRecognizer", "UIPinchGestureRecognizer"]))
        let taps = v.gestureRecognizers!.compactMap { $0 as? UITapGestureRecognizer }
        XCTAssertEqual(Set(taps.map(\.numberOfTapsRequired)), [1, 2])
        // Simulate the recognizer action: a click with its count, tagged with the last applied seq.
        v.onTap(taps.first { $0.numberOfTapsRequired == 1 }!)
        v.onTap(taps.first { $0.numberOfTapsRequired == 1 }!)
        let clicks = h.events(.click)
        XCTAssertEqual(clicks.count, 2)
        XCTAssertEqual(clicks.map { $0.payload.int("count") }, [1, 2])
        XCTAssertEqual(clicks.first?.seq, h.seq)
        XCTAssertEqual(clicks.first?.node, 1)

        h.apply { $0.mask(1, 0) }
        XCTAssertEqual(v.gestureRecognizers?.count ?? 0, 0)
    }

    func testPressFeedbackIsNative() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 100), props: [
                (.activeOpacity, .f32(0.4)), (.opacity, .f32(0.8)), (.backgroundColor, .color(0xFFFF_FFFF)),
                (.activeBackground, .color(0xFF00_0000)),
            ])
            b.mask(1, EventMask.pressIn | EventMask.pressOut)
        }
        let v = h.view(1)!
        v.pressed = true
        XCTAssertEqual(v.alpha, 0.8 * 0.4, accuracy: 0.001)
        XCTAssertEqual(v.layer.backgroundColor.map { UIColor(cgColor: $0) }, UIColor(red: 0, green: 0, blue: 0, alpha: 1))
        v.pressed = false
        XCTAssertEqual(v.alpha, 0.8, accuracy: 0.001)
        XCTAssertTrue(v.gestureRecognizers!.contains { $0 is PressGestureRecognizer })
        v.pressChanged(.began)
        XCTAssertEqual(v.alpha, 0.8 * 0.4, accuracy: 0.001)
        v.pressChanged(.ended)
        XCTAssertEqual(h.events.map(\.type), [.pressIn, .pressOut])
    }

    func testEveryViewTypeMountsAndResets() {
        var node: UInt32 = 1
        var raw: UInt32 = 1
        var created: [(UInt32, NSHViewType)] = []
        h.apply { b in
            b.create(1000, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 390, height: 844))
            while let t = NSHViewType(rawValue: raw) {
                raw += 1
                if [.page, .sheet, .dialog, .listRow].contains(t) { continue }
                b.create(node, t, parent: 1000, index: UInt32(created.count), frame: CGRect(x: 0, y: CGFloat(node) * 10, width: 200, height: 40))
                created.append((node, t))
                node += 1
            }
        }
        for (id, t) in created {
            let v = h.view(id)!
            XCTAssertEqual(v.viewType, t)
            XCTAssertTrue(type(of: v) == type(of: ComponentRegistry.make(t)), "\(t)")
            XCTAssertEqual(v.frame.size, CGSize(width: 200, height: 40))
        }
        h.apply { b in for (id, _) in created { b.delete(id) } }
        for (_, t) in created where t != .hostComponent {
            XCTAssertEqual(h.manager.pooledCount(t), 1, "\(t) pooled")
        }
    }

    func testTypeSpecificPropsResetOnReuse() {
        h.apply { b in
            b.create(1, .switchView, parent: 0, frame: CGRect(x: 0, y: 0, width: 60, height: 40), props: [(.checked, .bool(true)), (.enabled, .bool(false))])
            b.create(2, .slider, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 40), props: [(.min, .f32(10)), (.max, .f32(20)), (.value, .f32(15))])
            b.create(3, .image, parent: 0, frame: CGRect(x: 0, y: 0, width: 20, height: 20), props: [(.contentMode, .i32(1))])
            b.create(4, .scroll, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 100), props: [(.contentSize, .f32Array([100, 500])), (.horizontal, .bool(true))])
            b.create(5, .segmented, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 32), props: [(.items, .json(#"["A","B"]"#)), (.selectedIndex, .i32(1))])
            b.create(6, .checkbox, parent: 0, frame: CGRect(x: 0, y: 0, width: 24, height: 24), props: [(.checked, .bool(true)), (.checkStyle, .i32(1))])
        }
        let sw = h.view(1, as: NSHSwitchView.self), sl = h.view(2, as: NSHSliderView.self)
        let im = h.view(3, as: NSHImageView.self), sc = h.view(4, as: NSHScrollView.self)
        let seg = h.view(5, as: NSHSegmentedView.self), cb = h.view(6, as: NSHCheckboxView.self)
        XCTAssertTrue(sw.control.isOn)
        XCTAssertFalse(sw.control.isEnabled)
        XCTAssertEqual(sl.control.value, 15)
        XCTAssertEqual(im.imageView.contentMode, .scaleAspectFit)
        XCTAssertEqual(sc.scrollView.contentSize, CGSize(width: 100, height: 500))
        XCTAssertEqual(seg.control.selectedSegmentIndex, 1)
        XCTAssertTrue(cb.checked)
        h.apply { b in (1 ... 6).forEach { b.delete($0) } }
        h.apply { b in
            b.create(11, .switchView, parent: 0); b.create(12, .slider, parent: 0); b.create(13, .image, parent: 0)
            b.create(14, .scroll, parent: 0); b.create(15, .segmented, parent: 0); b.create(16, .checkbox, parent: 0)
        }
        XCTAssertTrue(h.view(11) === sw)
        XCTAssertFalse(sw.control.isOn)
        XCTAssertTrue(sw.control.isEnabled)
        XCTAssertEqual(sl.control.minimumValue, 0)
        XCTAssertEqual(sl.control.maximumValue, 1)
        XCTAssertEqual(sl.control.value, 0)
        XCTAssertEqual(im.imageView.contentMode, .scaleAspectFill)
        XCTAssertEqual(sc.scrollView.contentSize, .zero)
        XCTAssertEqual(seg.control.numberOfSegments, 0)
        XCTAssertFalse(cb.checked)
    }

    func testControlsReportChanges() {
        h.apply { b in
            b.create(1, .switchView, parent: 0, frame: CGRect(x: 0, y: 0, width: 60, height: 40))
            b.create(2, .checkbox, parent: 0, frame: CGRect(x: 0, y: 0, width: 24, height: 24))
            b.create(3, .segmented, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 32), props: [(.items, .json(#"["A","B"]"#))])
            b.create(4, .select, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 32), props: [
                (.menu, .json(#"{"title":"Pick","items":[{"label":"One","value":1},{"label":"Two","value":"two"}]}"#)),
                (.placeholder, .str("Choose")),
            ])
            b.create(5, .datePicker, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 40), props: [(.date, .str("2026-03-04"))])
            for n in 1 ... 5 { b.mask(UInt32(n), EventMask.change) }
        }
        let sw = h.view(1, as: NSHSwitchView.self)
        sw.control.setOn(true, animated: false)
        sw.control.fire(.valueChanged)
        h.view(2, as: NSHCheckboxView.self).toggle()
        let seg = h.view(3, as: NSHSegmentedView.self)
        seg.control.selectedSegmentIndex = 1
        seg.control.fire(.valueChanged)
        let sel = h.view(4, as: NSHSelectView.self)
        XCTAssertEqual(sel.control.title(for: .normal), "Choose")
        XCTAssertEqual(sel.control.menu?.children.count, 2)
        sel.choose(1)
        XCTAssertEqual(sel.control.title(for: .normal), "Two")
        let dp = h.view(5, as: NSHDatePickerView.self)
        XCTAssertEqual(DateFormats.format(dp.control.date, mode: 0), "2026-03-04")
        dp.control.fire(.valueChanged)

        let changes = h.events(.change)
        guard changes.count == 5 else { return XCTFail("expected 5 changes, got \(changes.count)") }
        XCTAssertEqual(changes[0].payload.bool("checked"), true)
        XCTAssertEqual(changes[1].payload.bool("checked"), true)
        XCTAssertEqual(changes[2].payload.int("index"), 1)
        XCTAssertEqual(changes[2].payload.string("value"), "B")
        XCTAssertEqual(changes[3].payload.string("value"), "two")
        XCTAssertEqual(changes[4].payload.string("date"), "2026-03-04")
    }

    func testSliderSendsFinalValue() {
        h.apply { b in
            b.create(1, .slider, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 40), props: [(.max, .f32(10)), (.step, .f32(1))])
            b.mask(1, EventMask.change)
        }
        let s = h.view(1, as: NSHSliderView.self)
        s.control.value = 3.4
        s.control.fire(.touchUpInside)
        XCTAssertEqual(h.events(.change).last?.payload.double("value"), 3)
        XCTAssertEqual(h.events(.change).last?.payload.bool("final"), true)
    }

    func testShapeParsesAndScalesPaths() {
        h.apply { b in
            b.create(1, .shape, parent: 0, frame: CGRect(x: 0, y: 0, width: 48, height: 48), props: [
                (.pathData, .str("M2 2 L22 2 H 22 V22 h-20 v-20 C 2 2 4 4 6 6 S 8 8 10 10 Q 12 12 14 14 T 16 16 A 4 4 0 0 1 20 20 Z")),
                (.viewBox, .f32Array([0, 0, 24, 24])),
                (.fillColor, .color(0xFFFF_0000)),
                (.strokeWidth, .f32(2)),
            ])
        }
        let s = h.view(1, as: NSHShapeView.self)
        s.layoutIfNeeded()
        let box = s.renderedPath!.boundingBoxOfPath
        XCTAssertEqual(box.minX, 4, accuracy: 0.01)
        XCTAssertEqual(box.maxX, 44, accuracy: 0.5)
        XCTAssertNil(SVGPath.parse("12 34"))
        XCTAssertNotNil(SVGPath.parse("M0,0l10-10z"))
        XCTAssertNotNil(SVGPath.parse("M 0 0 A 0 0 0 1 1 5 5"))
        _ = SVGPath.parse("M1e2 1.5.5L-.5-2e-1")
    }

    func testScrollToAndScrollEvents() {
        h.apply { b in
            b.create(1, .scroll, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 100), props: [(.contentSize, .f32Array([100, 1000]))])
            b.mask(1, EventMask.scroll)
            b.command(1, .scrollTo, ["x": 0, "y": 300, "animated": false])
        }
        let s = h.view(1, as: NSHScrollView.self)
        XCTAssertEqual(s.scrollView.contentOffset.y, 300)
        XCTAssertTrue(wait { !self.h.events(.scroll).isEmpty })
        XCTAssertEqual(h.events(.scroll).last?.payload.double("y"), 300)
        XCTAssertEqual(h.events(.scroll).last?.payload.double("content_height"), 1000)
        h.apply { $0.command(1, .scrollTo, ["y": 5000, "animated": false]) }
        XCTAssertEqual(s.scrollView.contentOffset.y, 900)
    }

    func testRootCommands() {
        // Reading the pasteboard would prompt for paste permission: check its change count instead.
        let before = UIPasteboard.general.changeCount
        h.apply { b in
            b.command(0, .setClipboard, ["text": "copied"])
            b.command(0, .showToast, ["id": "t1", "title": "Saved", "description": "All good", "duration": 300])
            b.command(0, .haptic, ["kind": "success"])
        }
        XCTAssertGreaterThan(UIPasteboard.general.changeCount, before)
        XCTAssertTrue(UIPasteboard.general.hasStrings)
        XCTAssertEqual(h.surface.toasts.visible.first?.titleLabel.text, "Saved")
        XCTAssertTrue(wait(3) { self.h.surface.toasts.visible.isEmpty })

        var opened: URL?
        let runtime = NSHRuntime()
        runtime.urlOpener = { opened = $0 }
        let h2 = Harness(runtime: runtime)
        defer { h2.tearDown() }
        h2.apply { $0.command(0, .openUrl, ["url": "https://example.com/a"]) }
        XCTAssertEqual(opened?.absoluteString, "https://example.com/a")
    }

    func testAnimatedBatchAnimatesDeclaredTransitions() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 50, height: 50), props: [
                (.animation, .json(#"{"frame":{"kind":"timing","duration":300,"delay":0,"easing":"linear"},"opacity":{"kind":"spring","response":0.3,"damping":0.8}}"#)),
            ])
            b.create(2, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 50, height: 50))
        }
        h.apply({ b in
            b.frame(1, CGRect(x: 100, y: 0, width: 50, height: 50))
            b.prop(1, .opacity, .f32(0.2))
            b.frame(2, CGRect(x: 100, y: 0, width: 50, height: 50))
        }, flags: BatchFlags.animated)
        let a = h.view(1)!, b = h.view(2)!
        XCTAssertFalse(a.layer.animationKeys()?.isEmpty ?? true, "declared transition animates")
        XCTAssertTrue(b.layer.animationKeys()?.isEmpty ?? true, "no transition: jumps")
        XCTAssertEqual(a.frame.minX, 100) // model value is final immediately
    }

    func testReduceMotionSkipsMovementButKeepsFades() {
        let saved = Animator.reduceMotion
        Animator.reduceMotion = { true }
        defer { Animator.reduceMotion = saved }
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 50, height: 50), props: [
                (.animation, .json(#"{"frame":{"kind":"spring","response":0.4,"damping":0.8},"opacity":{"kind":"timing","duration":300}}"#)),
            ])
        }
        h.apply({ b in
            b.frame(1, CGRect(x: 100, y: 0, width: 50, height: 50))
        }, flags: BatchFlags.animated)
        let v = h.view(1)!
        XCTAssertTrue(v.layer.animationKeys()?.isEmpty ?? true, "no movement with Reduce Motion")
        XCTAssertEqual(v.frame.minX, 100)
        h.apply({ $0.prop(1, .opacity, .f32(0.3)) }, flags: BatchFlags.animated)
        XCTAssertFalse(v.layer.animationKeys()?.isEmpty ?? true, "fades still animate")
    }

    func testInitialBatchAppliesSynchronouslyOnFirstSchedule() {
        // Without an instance the surface still applies what it is given right away.
        XCTAssertEqual(h.manager.appliedBatches, 0)
        h.apply { $0.create(1, .view, parent: 0) }
        XCTAssertEqual(h.manager.appliedBatches, 1)
        XCTAssertNotNil(h.view(1)?.superview)
    }

    func testToastsStackQueueAndDismiss() {
        h.apply { b in
            for i in 1 ... 5 { b.command(0, .showToast, ["id": "t\(i)", "title": "Toast \(i)", "level": "info", "duration": 0]) }
        }
        let toasts = h.surface.toasts
        XCTAssertEqual(toasts.visible.map(\.id), ["t1", "t2", "t3"], "at most three visible, none retracted by newer ones")
        XCTAssertEqual(toasts.queued.map(\.id), ["t4", "t5"])
        // Stacked: newest lowest, no overlap.
        let frames = toasts.visible.map(\.frame)
        XCTAssertLessThan(frames[0].maxY, frames[1].minY + 0.5)
        XCTAssertLessThan(frames[1].maxY, frames[2].minY + 0.5)
        // Sticky (duration 0) stays.
        spin(0.4)
        XCTAssertEqual(toasts.visible.count, 3)
        // Dismiss a queued one, then a visible one: the queue moves up.
        h.apply { $0.command(0, .dismissToast, ["id": "t5"]) }
        XCTAssertEqual(toasts.queued.map(\.id), ["t4"])
        h.apply { $0.command(0, .dismissToast, ["id": "t2"]) }
        XCTAssertEqual(toasts.visible.map(\.id), ["t1", "t3", "t4"])
        XCTAssertTrue(toasts.queued.isEmpty)
        h.apply { $0.command(0, .dismissToast, ["id": "nope"]) }
        XCTAssertEqual(toasts.visible.count, 3)
        h.apply { $0.command(0, .dismissToast, [:]) }
        XCTAssertTrue(toasts.visible.isEmpty)
        XCTAssertTrue(wait { self.h.window.subviews.allSatisfy { !($0 is ToastView) } })
    }

    /// Neutral surface, level by symbol shape (not color alone), token colors in both appearances.
    func testToastStylingLightAndDark() {
        let levels = ["info", "success", "warning", "error"]
        h.apply { b in
            for (i, l) in levels.enumerated() {
                b.command(0, .showToast, ["id": "s\(i)", "title": "\(l.capitalized) title", "description": "Something about \(l) that may wrap onto a second line.", "level": l, "duration": 0])
            }
        }
        let toasts = h.surface.toasts
        let all = toasts.visible + toasts.queued
        XCTAssertEqual(all.count, 4)
        let symbolNames = Set(all.map { ToastView.symbols[$0.level]! })
        XCTAssertEqual(symbolNames.count, 4, "a distinct shape per level")
        for t in all {
            XCTAssertNotNil(t.iconView.image, t.level)
            XCTAssertEqual(t.layer.borderWidth, 1)
            XCTAssertEqual(t.layer.cornerRadius, ThemeTokens.radius)
            XCTAssertEqual(t.titleLabel.font.fontDescriptor.symbolicTraits.contains(.traitBold), false, "title is medium, not bold")
            XCTAssertTrue(t.accessibilityLabel?.hasPrefix(["info": "Info", "success": "Success", "warning": "Warning", "error": "Error"][t.level]!) ?? false)
        }
        let error = all.first { $0.level == "error" }!
        for style in [UIUserInterfaceStyle.light, .dark] {
            let traits = UITraitCollection(userInterfaceStyle: style)
            let bg = error.backgroundColor!.resolvedColor(with: traits)
            XCTAssertEqual(bg, Colors.hex(style == .dark ? "#0a0a0a" : "#ffffff"), "background token, not a level color")
            XCTAssertEqual(error.iconView.tintColor.resolvedColor(with: traits), Colors.hex(style == .dark ? "#f87171" : "#ef4444"), "destructive token")
            XCTAssertEqual(error.descriptionLabel.textColor.resolvedColor(with: traits), Colors.hex(style == .dark ? "#a3a3a3" : "#737373"))
        }

        // Retract the info toast so the queued error one shows in the captures.
        h.apply { $0.command(0, .dismissToast, ["id": "s0"]) }
        XCTAssertEqual(toasts.visible.map(\.level), ["success", "warning", "error"])
        // Capture both appearances (attached to the result and written to /tmp for review).
        for style in [UIUserInterfaceStyle.light, .dark] {
            h.window.overrideUserInterfaceStyle = style
            h.window.backgroundColor = style == .dark ? .black : .white
            h.window.layoutIfNeeded()
            // layer.render: there is no render server to snapshot in a hostless test process.
            let image = UIGraphicsImageRenderer(bounds: h.window.bounds).image { ctx in
                h.window.layer.render(in: ctx.cgContext)
            }
            let name = "toasts-\(style == .dark ? "dark" : "light")"
            let attachment = XCTAttachment(image: image)
            attachment.name = name
            attachment.lifetime = .keepAlways
            add(attachment)
            try? image.pngData()?.write(to: URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("\(name).png"))
            print("TOAST CAPTURE \(NSTemporaryDirectory())\(name).png")
        }
        h.apply { $0.command(0, .dismissToast, [:]) }
    }

    func testToastActionDismissesAndReports() {
        h.apply { $0.command(0, .showToast, ["id": "undo-1", "title": "Deleted “Groceries”", "level": "info", "duration": 0, "action": ["label": "Undo"]]) }
        guard let t = h.surface.toasts.visible.first, let b = t.actionButton else { return XCTFail("no action button") }
        XCTAssertEqual(b.title(for: .normal), "Undo")
        XCTAssertGreaterThanOrEqual(b.frame.height, 44)
        XCTAssertGreaterThanOrEqual(b.frame.width, 44)
        XCTAssertEqual(b.titleLabel?.font.fontDescriptor.symbolicTraits.contains(.traitBold), false)
        XCTAssertLessThanOrEqual(t.titleLabel.frame.maxX, b.frame.minX, "text never runs under the button")
        XCTAssertEqual(t.accessibilityElements?.count, 2, "summary and action are separate elements")
        b.fire(.touchUpInside)
        XCTAssertTrue(h.surface.toasts.visible.isEmpty)
        let e = h.events(.toastAction)
        XCTAssertEqual(e.map(\.node), [0])
        XCTAssertEqual(e.first?.payload.string("id"), "undo-1")
        // Without an action there is no button.
        h.apply { $0.command(0, .showToast, ["id": "plain", "title": "Saved", "duration": 0]) }
        XCTAssertNil(h.surface.toasts.visible.first?.actionButton)
    }

    func testShapeStrokesUseRoundCapsAndDrawDots() {
        h.apply { b in
            b.create(1, .shape, parent: 0, frame: CGRect(x: 0, y: 0, width: 24, height: 24), props: [
                (.pathData, .str("M12 12h.01")), (.viewBox, .f32Array([0, 0, 24, 24])),
                (.strokeColor, .color(0xFF00_0000)), (.strokeWidth, .f32(2)),
            ])
        }
        let v = h.view(1, as: NSHShapeView.self)
        v.layoutIfNeeded()
        let shape = v.layer.sublayers!.compactMap { $0 as? CAShapeLayer }.first!
        XCTAssertEqual(shape.lineCap, .round)
        XCTAssertEqual(shape.lineJoin, .round)
        // The zero-length segment renders a dot at the center.
        let image = UIGraphicsImageRenderer(size: CGSize(width: 24, height: 24), format: {
            let f = UIGraphicsImageRendererFormat(); f.scale = 1; f.opaque = false; return f
        }()).image { ctx in v.layer.render(in: ctx.cgContext) }
        let cg = image.cgImage!
        let data = CFDataGetBytePtr(cg.dataProvider!.data)!
        let alpha = { (x: Int, y: Int) -> UInt8 in data[y * cg.bytesPerRow + x * 4 + 3] }
        XCTAssertGreaterThan(alpha(12, 12), 100, "a dot at the zero-length segment")
        XCTAssertEqual(alpha(2, 2), 0)
    }

    /// Renders a view at 4x and returns a sampler of (r, g, b, a) at a point in view coordinates.
    private func pixels(_ v: UIView) -> (CGFloat, CGFloat) -> (Int, Int, Int, Int) {
        let scale: CGFloat = 4
        let w = Int(v.bounds.width * scale), hgt = Int(v.bounds.height * scale)
        var buf = [UInt8](repeating: 0, count: w * hgt * 4)
        buf.withUnsafeMutableBytes { raw in
            let ctx = CGContext(data: raw.baseAddress, width: w, height: hgt, bitsPerComponent: 8, bytesPerRow: w * 4,
                                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
            // UIKit coordinates: origin top-left.
            ctx.translateBy(x: 0, y: CGFloat(hgt))
            ctx.scaleBy(x: scale, y: -scale)
            v.layer.render(in: ctx)
        }
        return { x, y in
            let px = min(w - 1, max(0, Int(x * scale))), py = min(hgt - 1, max(0, Int(y * scale)))
            let o = (py * w + px) * 4
            return (Int(buf[o]), Int(buf[o + 1]), Int(buf[o + 2]), Int(buf[o + 3]))
        }
    }

    func testRoundedFullBorderIsACircle() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 24, height: 24), props: [
                (.cornerRadii, .f32Array([9999, 9999, 9999, 9999])), (.borderWidths, .f32Array([2, 2, 2, 2])),
                (.borderColor, .color(0xFFFF_0000)),
            ])
        }
        let v = h.view(1)!
        v.layoutIfNeeded()
        let px = pixels(v)
        let c: CGFloat = 12
        for deg in [0.0, 45, 90, 135, 180, 225, 270, 315] {
            let a = CGFloat(deg) * .pi / 180
            let on = px(c + cos(a) * 11, c + sin(a) * 11)
            XCTAssertGreaterThan(on.3, 200, "ring at \(deg)°")
            XCTAssertGreaterThan(on.0, 200, "border color at \(deg)°")
            if Int(deg) % 90 != 0 {
                // Just outside the circle on the diagonals (axis points would leave the box).
                let outside = px(c + cos(a) * 12.9, c + sin(a) * 12.9)
                XCTAssertLessThan(outside.3, 90, "clear just outside at \(deg)°")
            }
            let inside = px(c + cos(a) * 8.5, c + sin(a) * 8.5)
            XCTAssertLessThan(inside.3, 10, "hollow inside at \(deg)°")
        }
        // Corners of the box are empty: it is a circle, not a rounded square.
        XCTAssertLessThan(px(1.5, 1.5).3, 10)
        XCTAssertLessThan(px(22.5, 22.5).3, 10)
    }

    func testCornerCurveIsContinuousExceptForCapsules() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 60), props: [(.cornerRadii, .f32Array([12, 12, 12, 12])), (.backgroundColor, .color(0xFFEE_EEEE))])
            b.create(2, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 24, height: 24), props: [(.cornerRadii, .f32Array([9999, 9999, 9999, 9999]))])
            b.create(3, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 120, height: 32), props: [(.cornerRadii, .f32Array([16, 16, 16, 16]))])
        }
        for n: UInt32 in 1 ... 3 { h.view(n)!.layoutIfNeeded() }
        XCTAssertEqual(h.view(1)!.layer.cornerCurve, .continuous, "12 pt card keeps the iOS curve")
        XCTAssertEqual(h.view(2)!.layer.cornerCurve, .circular, "circle")
        XCTAssertEqual(h.view(3)!.layer.cornerCurve, .circular, "capsule")
        XCTAssertTrue(Paths.isCapsule([12, 12, 12, 12], in: CGSize(width: 40, height: 24)))
        XCTAssertFalse(Paths.isCapsule([12, 12, 12, 12], in: CGSize(width: 100, height: 60)))
    }

    func testNonUniformRadiiAndPerSideBordersShareTheClampedShape() {
        // CSS clamping: 30 + 30 on a 40-wide side scales every radius by 40/60.
        let r = Paths.clampedRadii([30, 30, 0, 10], in: CGSize(width: 40, height: 100))
        XCTAssertEqual(r[0], 20, accuracy: 0.001)
        XCTAssertEqual(r[1], 20, accuracy: 0.001)
        XCTAssertEqual(r[3], 10 * 40 / 60, accuracy: 0.001)
        for v in Paths.clampedRadii([9999, 9999, 9999, 9999], in: CGSize(width: 24, height: 40)) { XCTAssertEqual(v, 12, accuracy: 0.001) }

        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 40, height: 40), props: [
                (.cornerRadii, .f32Array([20, 0, 20, 0])), (.borderWidths, .f32Array([4, 1, 4, 1])),
                (.borderColor, .color(0xFF00_00FF)), (.backgroundColor, .color(0xFF00_FF00)), (.clipsToBounds, .bool(true)),
            ])
            b.create(2, .view, parent: 1, frame: CGRect(x: 0, y: 0, width: 40, height: 40), props: [(.backgroundColor, .color(0xFFFF_0000))])
        }
        let v = h.view(1)!
        v.layoutIfNeeded()
        h.view(2)!.isHidden = true
        var px = pixels(v)
        // Rounded top-left corner: clear outside the arc, fill inside.
        XCTAssertLessThan(px(1, 1).3, 10, "outside the top-left arc")
        XCTAssertGreaterThan(px(20, 20).1, 200, "green fill in the middle")
        // Square top-right corner: border color right at the corner.
        XCTAssertGreaterThan(px(39, 1).2, 200, "blue border at the square corner")
        // Top border is 4 pt, side border 1 pt.
        XCTAssertGreaterThan(px(20, 3).2, 200, "4 pt top border")
        XCTAssertGreaterThan(px(20, 6).1, 200, "fill below the top border")
        XCTAssertGreaterThan(px(39.5, 20).2, 200, "1 pt right border")
        XCTAssertGreaterThan(px(37.5, 20).1, 200, "fill inside the right border")
        // Clipping uses the same shape: a child filling the box is cut at the rounded corner.
        h.view(2)!.isHidden = false
        px = pixels(v)
        XCTAssertLessThan(px(1, 1).3, 10, "child clipped by the rounded corner")
        XCTAssertGreaterThan(px(20, 20).0, 200, "child visible inside")
        XCTAssertNotNil(v.layer.mask)
        h.apply { $0.prop(1, .clipsToBounds, .null) }
        v.layoutIfNeeded()
        XCTAssertNil(v.layer.mask)
    }

    func testDatePickerWireFormatsByMode() {
        h.apply { b in
            b.create(1, .datePicker, parent: 0, frame: CGRect(x: 0, y: 0, width: 240, height: 44), props: [(.dateMode, .i32(1)), (.date, .str("09:30"))])
            b.create(2, .datePicker, parent: 0, frame: CGRect(x: 0, y: 0, width: 300, height: 44), props: [(.dateMode, .i32(2)), (.date, .str("2026-10-14T18:05"))])
            b.create(3, .datePicker, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 44), props: [(.date, .str("2026-10-14"))])
            for n in 1 ... 3 { b.mask(UInt32(n), EventMask.change) }
        }
        for n in 1 ... 3 { h.view(UInt32(n), as: NSHDatePickerView.self).control.fire(.valueChanged) }
        XCTAssertEqual(h.events(.change).map { $0.payload.string("date") }, ["09:30", "2026-10-14T18:05", "2026-10-14"])
        XCTAssertEqual(h.view(1, as: NSHDatePickerView.self).control.datePickerMode, .time)
        // A value in another form still parses after a mode change.
        h.apply { $0.prop(3, .dateMode, .i32(2)) }
        h.view(3, as: NSHDatePickerView.self).control.fire(.valueChanged)
        XCTAssertEqual(h.events(.change).last?.payload.string("date"), "2026-10-14T00:00")
    }

    func testBuiltinControlMeasurement() {
        func measure(_ name: String, _ props: [String: Any], width: Float = .infinity, mode: Int32 = 0) -> NSHSize {
            var out = NSHSize(width: 0, height: 0, has_value: 0)
            let n = Array(name.utf8), p = Array(JSON.string(props).utf8)
            let done = expectation(description: name)
            Thread.detachNewThread { // the JS thread, as in the core
                out = nshMeasureComponent(nil, n, n.count, p, p.count, width, mode)
                done.fulfill()
            }
            wait(for: [done], timeout: 2)
            return out
        }
        let short = measure("nsh/select", ["label": "Weekly"])
        let long = measure("nsh/select", ["label": "Every other Wednesday"])
        let placeholder = measure("nsh/select", ["label": "", "placeholder": "Choose…"])
        XCTAssertEqual(short.has_value, 1)
        XCTAssertGreaterThan(long.width, short.width)
        XCTAssertGreaterThan(placeholder.width, 40)
        XCTAssertGreaterThanOrEqual(short.height, 44, "touch target")
        XCTAssertLessThanOrEqual(measure("nsh/select", ["label": "Every other Wednesday"], width: 100, mode: 1).width, 100)
        // Rendered at its measured size the label is not truncated.
        h.apply { b in
            b.create(1, .select, parent: 0, frame: CGRect(x: 0, y: 0, width: CGFloat(long.width), height: CGFloat(long.height)), props: [
                (.menu, .json(#"{"items":[{"label":"Every other Wednesday","value":1}]}"#)), (.selectedIndex, .i32(0)),
            ])
        }
        let sel = h.view(1, as: NSHSelectView.self)
        sel.layoutIfNeeded()
        sel.control.layoutIfNeeded()
        let label = sel.control.titleLabel!
        XCTAssertGreaterThanOrEqual(label.frame.width + 0.5, label.intrinsicContentSize.width, "label fits")
        XCTAssertNotNil(sel.subviews.compactMap { $0 as? UIImageView }.first?.image, "chevron")

        // Date picker: wider for date+time than for either; at least the native intrinsic size.
        let date = measure("nsh/date-picker", ["value": "2026-10-14", "mode": "date"])
        let time = measure("nsh/date-picker", ["value": "09:30", "mode": "time"])
        let both = measure("nsh/date-picker", ["value": "2026-10-14T09:30", "mode": "datetime"])
        XCTAssertGreaterThan(both.width, date.width)
        XCTAssertGreaterThan(both.width, time.width)
        XCTAssertGreaterThanOrEqual(date.height, 44)
        let picker = UIDatePicker()
        picker.preferredDatePickerStyle = .compact
        picker.datePickerMode = .date
        picker.date = DateFormats.parse("2026-10-14", mode: 0)!
        let native = picker.sizeThatFits(.zero)
        print("DATE MEASURE measured \(date.width)x\(date.height) native \(native)")
        XCTAssertGreaterThanOrEqual(CGFloat(date.width) + 1, native.width, "never narrower than the native control")
        XCTAssertLessThanOrEqual(CGFloat(date.width), native.width + 16, "close to the native control")
        XCTAssertEqual(measure("nsh/unknown", [:]).has_value, 0)
    }

    func testShapeFillRule() {
        // Outer and inner squares in the same direction: nonzero fills both, even-odd leaves a hole.
        let ring = "M0 0H24V24H0Z M8 8H16V16H8Z"
        h.apply { b in
            b.create(1, .shape, parent: 0, frame: CGRect(x: 0, y: 0, width: 24, height: 24), props: [
                (.pathData, .str(ring)), (.viewBox, .f32Array([0, 0, 24, 24])), (.fillColor, .color(0xFF00_0000)), (.fillRule, .i32(1)),
            ])
        }
        let v = h.view(1, as: NSHShapeView.self)
        v.layoutIfNeeded()
        var px = pixels(v)
        XCTAssertGreaterThan(px(4, 4).3, 200, "ring filled")
        XCTAssertLessThan(px(12, 12).3, 10, "even-odd hole")
        h.apply { $0.prop(1, .fillRule, .null) }
        v.layoutIfNeeded()
        px = pixels(v)
        XCTAssertGreaterThan(px(12, 12).3, 200, "nonzero (default) fills the center")
    }

    func testImageLoadsAbsoluteFilePathsOnly() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("nsh-test-\(UUID().uuidString).png")
        let png = UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8)).image { ctx in
            UIColor.red.setFill(); ctx.fill(CGRect(x: 0, y: 0, width: 8, height: 8))
        }.pngData()!
        try png.write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        h.apply { b in
            b.create(1, .image, parent: 0, frame: CGRect(x: 0, y: 0, width: 16, height: 16), props: [(.imageSource, .str(url.path))])
            b.create(2, .image, parent: 0, frame: CGRect(x: 0, y: 0, width: 16, height: 16), props: [(.imageSource, .str(url.absoluteString))])
            b.create(3, .image, parent: 0, frame: CGRect(x: 0, y: 0, width: 16, height: 16), props: [(.imageSource, .str(url.lastPathComponent))])
        }
        XCTAssertTrue(wait { self.h.view(1, as: NSHImageView.self).imageView.image != nil }, "absolute path")
        XCTAssertTrue(wait { self.h.view(2, as: NSHImageView.self).imageView.image != nil }, "file URL")
        spin(0.2)
        XCTAssertNil(h.view(3, as: NSHImageView.self).imageView.image, "relative paths are not resolved against the cwd")
    }

    func testControlsUsePrimaryTintButSwitchStaysGreen() {
        h.apply { b in
            b.create(1, .slider, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 44))
            b.create(2, .progress, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 8))
            b.create(3, .spinner, parent: 0, frame: CGRect(x: 0, y: 0, width: 20, height: 20))
            b.create(4, .segmented, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 32), props: [(.items, .json(#"["A","B"]"#))])
            b.create(5, .switchView, parent: 0, frame: CGRect(x: 0, y: 0, width: 60, height: 40), props: [(.tintColor, .color(0xFFFF_0000))])
            b.create(6, .select, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 44))
        }
        func primary(_ c: UIColor?, _ style: UIUserInterfaceStyle) -> Bool {
            c?.resolvedColor(with: UITraitCollection(userInterfaceStyle: style)) == Colors.hex(style == .dark ? "#fafafa" : "#171717")
        }
        for style in [UIUserInterfaceStyle.light, .dark] {
            XCTAssertTrue(primary(h.view(1, as: NSHSliderView.self).control.minimumTrackTintColor, style), "slider")
            XCTAssertTrue(primary(h.view(2, as: NSHProgressView.self).control.progressTintColor, style), "progress")
            XCTAssertTrue(primary(h.view(3, as: NSHSpinnerView.self).control.color, style), "spinner")
            XCTAssertTrue(primary(h.view(4, as: NSHSegmentedView.self).control.selectedSegmentTintColor, style), "segmented")
            XCTAssertTrue(primary(h.view(6, as: NSHSelectView.self).subviews.compactMap { $0 as? UIImageView }.first?.tintColor, style), "select chevron")
        }
        XCTAssertNil(h.view(5, as: NSHSwitchView.self).control.onTintColor, "switch keeps the system green")
        // An explicit TintColor overrides the theme; resetting it brings primary back.
        h.apply { $0.prop(1, .tintColor, .color(0xFF00_FF00)) }
        XCTAssertEqual(h.view(1, as: NSHSliderView.self).control.minimumTrackTintColor, Colors.argb(0xFF00_FF00))
        h.apply { $0.prop(1, .tintColor, .null) }
        XCTAssertTrue(primary(h.view(1, as: NSHSliderView.self).control.minimumTrackTintColor, .light))
        // The theme provider maps primary for the host app.
        let rt = NSHRuntime()
        rt.themeProvider = { _ in ["primary": .systemPurple] }
        XCTAssertEqual(ThemeTokens.color("primary", runtime: rt).resolvedColor(with: UITraitCollection(userInterfaceStyle: .light)),
                       UIColor.systemPurple.resolvedColor(with: UITraitCollection(userInterfaceStyle: .light)))
    }

    func testThemeTokensPropDrivesRendererDrawnColors() {
        func tokens(primary: UInt32, background: UInt32) -> PropValue {
            .json(JSON.string(["appearance": "light", "primary": primary, "primary_foreground": 0xFFFF_FFFF, "background": background,
                               "foreground": 0xFF00_0000, "muted_foreground": 0xFF77_7777, "border": 0xFFDD_DDDD, "destructive": 0xFFEF_4444]))
        }
        h.apply { b in
            b.prop(0, .themeTokens, tokens(primary: 0xFF25_63EB, background: 0xFF11_2233))
            b.create(1, .slider, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 44))
            b.create(2, .checkbox, parent: 0, frame: CGRect(x: 0, y: 0, width: 24, height: 24), props: [(.checked, .bool(true))])
            b.command(0, .showToast, ["id": "t", "title": "Hi", "duration": 0])
        }
        let slider = h.view(1, as: NSHSliderView.self)
        XCTAssertEqual(slider.control.minimumTrackTintColor, Colors.argb(0xFF25_63EB), "set_theme primary reaches the control")
        let toast = h.surface.toasts.visible.first!
        XCTAssertEqual(toast.backgroundColor, Colors.argb(0xFF11_2233))
        // A theme change re-resolves what is already on screen.
        h.apply { $0.prop(0, .themeTokens, tokens(primary: 0xFFDC_2626, background: 0xFF00_0000)) }
        XCTAssertEqual(slider.control.minimumTrackTintColor, Colors.argb(0xFFDC_2626))
        XCTAssertEqual(toast.backgroundColor, Colors.argb(0xFF00_0000))
        let box = h.view(2)!.layer.sublayers!.compactMap { $0 as? CAShapeLayer }.first!
        XCTAssertEqual(box.fillColor.map { UIColor(cgColor: $0) }, Colors.argb(0xFFDC_2626), "drawn checkbox")
        // Explicit TintColor still wins over the theme.
        h.apply { $0.prop(1, .tintColor, .color(0xFF00_FF00)) }
        XCTAssertEqual(slider.control.minimumTrackTintColor, Colors.argb(0xFF00_FF00))
        // Before any ThemeTokens, defaults/provider stand in.
        XCTAssertEqual(ThemeTokens.color("primary", runtime: NSHRuntime(), theme: ThemeState()).resolvedColor(with: UITraitCollection(userInterfaceStyle: .light)), Colors.hex("#171717"))
    }

    /// Renderer-drawn glyphs follow the frame the core gives (it grows with the font scale).
    func testDrawnGlyphsScaleWithTheirFrame() {
        h.apply { b in
            b.create(1, .checkbox, parent: 0, frame: CGRect(x: 0, y: 0, width: 22, height: 22), props: [(.checked, .bool(true))])
            b.create(2, .checkbox, parent: 0, frame: CGRect(x: 0, y: 0, width: 35, height: 35), props: [(.checked, .bool(true))])
            b.create(3, .shape, parent: 0, frame: CGRect(x: 0, y: 0, width: 35, height: 35), props: [
                (.pathData, .str("M2 2H22V22H2Z")), (.viewBox, .f32Array([0, 0, 24, 24])), (.strokeWidth, .f32(2)), (.strokeColor, .color(0xFF00_0000)),
            ])
        }
        func box(_ n: UInt32) -> CAShapeLayer {
            let v = h.view(n)!
            v.layoutIfNeeded()
            return v.layer.sublayers!.compactMap { $0 as? CAShapeLayer }.first!
        }
        let small = box(1), large = box(2)
        XCTAssertEqual(large.path!.boundingBoxOfPath.width / small.path!.boundingBoxOfPath.width, 35.0 / 22.0, accuracy: 0.05)
        XCTAssertEqual(large.lineWidth / small.lineWidth, 35.0 / 22.0, accuracy: 0.05)
        // Shapes map their view box onto the frame, strokes included.
        let shape = h.view(3, as: NSHShapeView.self)
        shape.layoutIfNeeded()
        let layer = shape.layer.sublayers!.compactMap { $0 as? CAShapeLayer }.first!
        XCTAssertEqual(shape.renderedPath!.boundingBoxOfPath.width, 20 * 35.0 / 24.0, accuracy: 0.1)
        XCTAssertEqual(layer.lineWidth, 2 * 35.0 / 24.0, accuracy: 0.01)
    }

    func testHardwareKeysRouteToNearestListenerOrRoot() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 100))
            b.create(2, .view, parent: 1, frame: CGRect(x: 0, y: 0, width: 50, height: 50))
            b.mask(1, EventMask.key)
        }
        let p = KeyEvents.payload(keyCode: .keyboardA, characters: "A", ignoringModifiers: "a", modifiers: [.shift, .command], phase: "down")
        XCTAssertEqual(p?.string("key"), "a")
        XCTAssertEqual(p?.string("key_char"), "A")
        XCTAssertEqual((p?["modifiers"] as? [String: Any])?.bool("cmd"), true)
        XCTAssertEqual((p?["modifiers"] as? [String: Any])?.bool("ctrl"), false)
        XCTAssertEqual(KeyEvents.name(.keyboardReturnOrEnter, "\r"), "enter")
        XCTAssertEqual(KeyEvents.name(.keyboardUpArrow, ""), "up")
        XCTAssertEqual(KeyEvents.name(.keyboardF5, ""), "f5")
        XCTAssertEqual(KeyEvents.name(.keyboardEscape, ""), "escape")
        XCTAssertNil(KeyEvents.name(.keyboardLeftShift, ""), "pure modifiers are not sent")
        // Routing: the deepest view without KEY passes; the nearest listener takes it once.
        let inner = h.view(2)!, outer = h.view(1)!
        XCTAssertFalse(inner.listens(EventMask.key))
        XCTAssertTrue(outer.listens(EventMask.key))
        XCTAssertTrue(h.surface.canBecomeFirstResponder)
        // The responder chain from the inner view reaches the listener, then the surface.
        var chain: [UIResponder] = []
        var r: UIResponder? = inner
        while let x = r { chain.append(x); r = x.next }
        XCTAssertTrue(chain.contains { $0 === outer })
        XCTAssertTrue(chain.contains { $0 === h.surface })
        XCTAssertLessThan(chain.firstIndex { $0 === outer }!, chain.firstIndex { $0 === h.surface }!)
    }

    func testCommonVisualPropsOnEveryType() {
        let visual: [(Prop, PropValue)] = [
            (.backgroundColor, .color(0xFFEE_EEEE)), (.cornerRadii, .f32Array([6, 6, 6, 6])),
            (.borderWidths, .f32Array([1, 1, 1, 1])), (.borderColor, .color(0xFF00_0000)), (.opacity, .f32(0.9)),
            (.testId, .str("styled")),
        ]
        var node: UInt32 = 1
        var made: [(UInt32, NSHViewType)] = []
        h.apply { b in
            for t in [NSHViewType.textInput, .switchView, .slider, .image, .text, .scroll, .segmented, .select, .datePicker, .hostComponent, .checkbox, .progress, .spinner, .shape] {
                b.create(node, t, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 44), props: visual)
                made.append((node, t))
                node += 1
            }
        }
        for (id, t) in made {
            let v = h.view(id)!
            v.layoutIfNeeded()
            XCTAssertEqual(v.layer.cornerRadius, 6, "\(t)")
            XCTAssertEqual(v.layer.borderWidth, 1, "\(t)")
            XCTAssertNotNil(v.layer.backgroundColor, "\(t)")
            XCTAssertEqual(v.alpha, 0.9, accuracy: 0.001, "\(t)")
            XCTAssertEqual(v.accessibilityIdentifier, "styled", "\(t)")
        }
        // A bordered, rounded TextInput keeps its field reachable and resets on reuse.
        let input = h.view(1, as: NSHTextInputView.self)
        XCTAssertEqual(input.field?.accessibilityIdentifier, "styled")
        XCTAssertFalse(input.isAccessibilityElement)
        h.apply { $0.delete(1) }
        h.apply { $0.create(100, .textInput, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 44)) }
        let reused = h.view(100, as: NSHTextInputView.self)
        XCTAssertTrue(reused === input)
        reused.layoutIfNeeded()
        XCTAssertEqual(reused.layer.cornerRadius, 0)
        XCTAssertEqual(reused.layer.borderWidth, 0)
        XCTAssertNil(reused.layer.backgroundColor)
        XCTAssertNil(reused.field?.accessibilityIdentifier)
    }
}
