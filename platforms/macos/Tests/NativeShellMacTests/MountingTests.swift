@testable import NativeShellMac
import AppKit
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
            b.create(4, .view, parent: 1, index: 1, frame: CGRect(x: 10, y: 140, width: 100, height: 50))
        }
        let parent = h.view(1)!
        XCTAssertEqual(parent.children.map(\.nodeId), [2, 4, 3])
        XCTAssertEqual(parent.subviews.compactMap { ($0 as? NSHComponentView)?.nodeId }, [2, 4, 3])
        // Flipped: the core's top-left frames are the views' frames.
        XCTAssertTrue(parent.isFlipped)
        XCTAssertEqual(h.view(2)?.frame, CGRect(x: 10, y: 20, width: 100, height: 50))
        let inWindow = h.view(2)!.convert(CGPoint(x: 0, y: 0), to: nil)
        XCTAssertEqual(inWindow.y, 844 - 20, "AppKit window coordinates grow up: the node sits 20 pt below the top")

        h.apply { b in
            b.move(3, 1, 2, 0)
            b.remove(2, 1, 1)
        }
        XCTAssertEqual(parent.children.map(\.nodeId), [3, 4])
        XCTAssertEqual(parent.subviews.compactMap { ($0 as? NSHComponentView)?.nodeId }, [3, 4])
        XCTAssertNil(h.view(2)?.superview)
    }

    func testUnknownNodesAreSkipped() {
        XCTAssertTrue(h.apply { b in
            b.prop(99, .opacity, .f32(0.5))
            b.insert(98, 0, 0)
            b.delete(97)
            b.create(1, .view, parent: 0)
        })
        XCTAssertNotNil(h.view(1))
    }

    func testViewPropsResetAndPooling() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 100), props: [
                (.backgroundColor, .color(0xFFFF_0000)), (.cornerRadii, .f32Array([8, 8, 8, 8])), (.borderWidths, .f32Array([2])),
                (.borderColor, .color(0xFF00_FF00)), (.opacity, .f32(0.5)), (.hidden, .bool(true)), (.clipsToBounds, .bool(true)),
                (.a11yLabel, .str("Card")), (.testId, .str("card")),
            ])
        }
        let v = h.view(1)!
        XCTAssertEqual(v.layer?.cornerRadius, 8)
        XCTAssertEqual(v.layer?.borderWidth, 2)
        XCTAssertEqual(v.alphaValue, 0.5, accuracy: 0.001)
        XCTAssertTrue(v.isHidden)
        XCTAssertTrue(v.layer?.masksToBounds ?? false)
        XCTAssertNotNil(v.layer?.backgroundColor)
        XCTAssertEqual(v.accessibilityLabel(), "Card")
        XCTAssertEqual(v.accessibilityIdentifier(), "card")

        h.apply { $0.delete(1) }
        XCTAssertEqual(h.manager.pooledCount(.view), 1)
        h.apply { $0.create(2, .view, parent: 0) }
        let reused = h.view(2)!
        XCTAssertTrue(reused === v, "pooled views are reused")
        XCTAssertEqual(reused.layer?.cornerRadius, 0)
        XCTAssertEqual(reused.layer?.borderWidth, 0)
        XCTAssertEqual(reused.alphaValue, 1)
        XCTAssertFalse(reused.isHidden)
        XCTAssertFalse(reused.layer?.masksToBounds ?? true)
        XCTAssertNil(reused.layer?.backgroundColor)
        XCTAssertNil(reused.accessibilityLabel())
    }

    func testPerCornerRadiiAndPerSideBorders() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 60), props: [
                (.backgroundColor, .color(0xFF11_2233)), (.cornerRadii, .f32Array([12, 12, 0, 0])),
                (.borderWidths, .f32Array([0, 0, 1, 0])), (.borderColor, .color(0xFF00_0000)),
            ])
        }
        let layers = h.view(1)!.layer!.sublayers ?? []
        let shapes = layers.compactMap { $0 as? CAShapeLayer }
        XCTAssertEqual(shapes.count, 2, "a fill shape and a border ring")
        XCTAssertNil(h.view(1)!.layer?.backgroundColor)
        XCTAssertEqual(h.view(1)!.layer?.borderWidth, 0)
    }

    func testCornerCurveIsContinuousExceptForCapsules() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 40), props: [(.cornerRadii, .f32Array([10, 10, 10, 10]))])
            b.create(2, .view, parent: 0, frame: CGRect(x: 0, y: 50, width: 100, height: 40), props: [(.cornerRadii, .f32Array([999, 999, 999, 999]))])
        }
        XCTAssertEqual(h.view(1)!.layer?.cornerCurve, .continuous)
        XCTAssertEqual(h.view(2)!.layer?.cornerCurve, .circular)
        XCTAssertEqual(h.view(2)!.layer?.cornerRadius, 20, "clamped to half the height")
    }

    func testClicksCarryCountAndModifiers() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 100))
            b.mask(1, EventMask.click)
        }
        let v = h.view(1)!
        Mouse.click(v, at: CGPoint(x: 10, y: 20), modifiers: [.command, .shift])
        let e = h.events(.click).first
        XCTAssertEqual(e?.payload.double("x"), 10)
        XCTAssertEqual(e?.payload.double("y"), 20, "points are top-left like the core's")
        XCTAssertEqual(e?.payload.int("count"), 1)
        XCTAssertEqual((e?.payload["modifiers"] as? [String: Any])?.bool("cmd"), true)
        XCTAssertEqual((e?.payload["modifiers"] as? [String: Any])?.bool("shift"), true)
        XCTAssertEqual((e?.payload["modifiers"] as? [String: Any])?.bool("alt"), false)
        XCTAssertEqual(e?.seq, h.seq)

        // Releasing outside the view is no click.
        v.mouseDown(with: Mouse.event(.leftMouseDown, at: CGPoint(x: 10, y: 10), in: v))
        v.mouseUp(with: Mouse.event(.leftMouseUp, at: CGPoint(x: 300, y: 300), in: v))
        XCTAssertEqual(h.events(.click).count, 1)

        // A view that does not listen passes the click to its parent.
        h.apply { b in b.create(2, .view, parent: 1, frame: CGRect(x: 0, y: 0, width: 50, height: 50)) }
        Mouse.click(h.view(2)!)
        XCTAssertEqual(h.events(.click).last?.node, 1)
        XCTAssertEqual(h.events(.click).count, 2)

        // Accessibility press performs the click.
        XCTAssertTrue(v.accessibilityPerformPress())
        XCTAssertEqual(h.events(.click).count, 3)
        XCTAssertEqual(v.accessibilityRole(), .button)
    }

    func testDoubleClickLongPressAndSecondaryClick() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 100))
            b.mask(1, EventMask.click | EventMask.doubleClick | EventMask.longPress)
        }
        let v = h.view(1)!
        Mouse.click(v, clicks: 1)
        Mouse.click(v, clicks: 2)
        XCTAssertEqual(h.events(.click).count, 1)
        XCTAssertEqual(h.events(.doubleClick).count, 1)

        // Holding the button: a long press, and no click when released.
        v.mouseDown(with: Mouse.event(.leftMouseDown, at: CGPoint(x: 5, y: 5), in: v))
        XCTAssertTrue(wait(1.5) { !self.h.events(.longPress).isEmpty })
        v.mouseUp(with: Mouse.event(.leftMouseUp, at: CGPoint(x: 5, y: 5), in: v))
        XCTAssertEqual(h.events(.click).count, 1)

        // A secondary click (or Control-click) is the pointer's long press.
        v.rightMouseDown(with: Mouse.event(.rightMouseDown, at: CGPoint(x: 5, y: 5), in: v))
        Mouse.click(v, modifiers: .control)
        XCTAssertEqual(h.events(.longPress).count, 3)
        XCTAssertEqual(h.events(.click).count, 1)
    }

    func testPanAndPinch() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 200))
            b.mask(1, EventMask.pan | EventMask.click)
        }
        let v = h.view(1)!
        let t0 = ProcessInfo.processInfo.systemUptime
        v.mouseDown(with: Mouse.event(.leftMouseDown, at: CGPoint(x: 10, y: 10), in: v, time: t0))
        v.mouseDragged(with: Mouse.event(.leftMouseDragged, at: CGPoint(x: 30, y: 15), in: v, time: t0 + 0.01))
        v.mouseDragged(with: Mouse.event(.leftMouseDragged, at: CGPoint(x: 60, y: 20), in: v, time: t0 + 0.02))
        v.mouseUp(with: Mouse.event(.leftMouseUp, at: CGPoint(x: 60, y: 20), in: v, time: t0 + 0.03))
        XCTAssertEqual(h.events(.pan).map { $0.payload.string("phase") }, ["began", "changed", "ended"])
        XCTAssertEqual(h.events(.pan).last?.payload.double("dx"), 50)
        XCTAssertEqual(h.events(.pan).last?.payload.double("dy"), 10)
        XCTAssertTrue(h.events(.click).isEmpty, "a drag is not a click")
    }

    func testPressFeedbackIsNative() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 100), props: [(.activeOpacity, .f32(0.4)), (.opacity, .f32(0.9))])
            b.mask(1, EventMask.pressIn | EventMask.pressOut)
        }
        let v = h.view(1)!
        v.mouseDown(with: Mouse.event(.leftMouseDown, at: CGPoint(x: 5, y: 5), in: v))
        XCTAssertEqual(v.alphaValue, 0.36, accuracy: 0.001)
        XCTAssertEqual(h.events(.pressIn).count, 1)
        // Dragging out shows the resting state, back in the pressed one, as AppKit buttons do.
        v.mouseDragged(with: Mouse.event(.leftMouseDragged, at: CGPoint(x: 300, y: 5), in: v))
        XCTAssertEqual(v.alphaValue, 0.9, accuracy: 0.001)
        v.mouseDragged(with: Mouse.event(.leftMouseDragged, at: CGPoint(x: 6, y: 5), in: v))
        XCTAssertEqual(v.alphaValue, 0.36, accuracy: 0.001)
        v.mouseUp(with: Mouse.event(.leftMouseUp, at: CGPoint(x: 6, y: 5), in: v))
        XCTAssertEqual(v.alphaValue, 0.9, accuracy: 0.001)
        XCTAssertEqual(h.events(.pressOut).count, 1)
    }

    func testTransformIsAppliedAroundTheCenter() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 40), props: [
                (.transform, .json(#"{"scale":0.5,"translate_y":10}"#)),
            ])
        }
        let v = h.view(1)!
        let t = v.layerTransform
        // The center (50, 20) maps to itself plus the translation.
        let c = CGPoint(x: 50, y: 20).applying(t)
        XCTAssertEqual(c.x, 50, accuracy: 0.001)
        XCTAssertEqual(c.y, 30, accuracy: 0.001)
        XCTAssertEqual(v.frame.size, CGSize(width: 100, height: 40), "the frame is the core's, untransformed")
        h.apply { $0.prop(1, .transform, .null) }
        XCTAssertTrue(h.view(1)!.layerTransform.isIdentity)
    }

    func testEveryViewTypeMountsAndResets() {
        var raw: UInt32 = 1
        var nodes: [UInt32] = []
        while let t = NSHViewType(rawValue: raw) {
            let node = raw + 100
            h.apply { b in
                b.create(node, t, frame: CGRect(x: 0, y: 0, width: 200, height: 44), props: [(.backgroundColor, .color(0xFF00_00FF)), (.a11yLabel, .str("x"))])
                if t != .page, t != .sheet, t != .dialog { b.insert(node, 0, 0) }
            }
            XCTAssertEqual(h.view(node)?.viewType, t)
            nodes.append(node)
            raw += 1
        }
        h.apply { b in for n in nodes { b.delete(n) } }
        XCTAssertEqual(h.manager.nodeCount, 0)
    }

    func testControlsReportChanges() {
        h.apply { b in
            b.create(1, .switchView, parent: 0, frame: CGRect(x: 0, y: 0, width: 60, height: 40), props: [(.checked, .bool(true))])
            b.create(2, .checkbox, parent: 0, frame: CGRect(x: 0, y: 50, width: 30, height: 30))
            b.create(3, .slider, parent: 0, frame: CGRect(x: 0, y: 100, width: 200, height: 30), props: [(.min, .f32(0)), (.max, .f32(10)), (.step, .f32(1))])
            b.create(4, .segmented, parent: 0, frame: CGRect(x: 0, y: 150, width: 240, height: 30), props: [(.items, .json(#"["Day","Week","Month"]"#)), (.selectedIndex, .i32(1))])
            b.create(5, .select, parent: 0, frame: CGRect(x: 0, y: 200, width: 240, height: 30), props: [
                (.menu, .json(#"{"placeholder":"Choose…","items":[{"label":"Name","value":"name"},{"label":"Date","value":"date"}]}"#)),
                (.selectedIndex, .i32(-1)),
            ])
            b.create(6, .datePicker, parent: 0, frame: CGRect(x: 0, y: 250, width: 240, height: 30), props: [(.date, .str("2026-10-14")), (.dateMode, .i32(0))])
            for n: UInt32 in 1 ... 6 { b.mask(n, EventMask.change) }
        }
        let sw = h.view(1, as: NSHSwitchView.self)
        XCTAssertEqual(sw.control.state, .on)
        sw.control.state = .off
        sw.changed()
        XCTAssertEqual(h.events(.change).last?.payload.bool("checked"), false)

        let box = h.view(2, as: NSHCheckboxView.self)
        box.toggle()
        XCTAssertEqual(h.events(.change).last?.payload.bool("checked"), true)
        h.apply { $0.prop(2, .checkStyle, .i32(1)) }
        box.toggle()
        XCTAssertEqual(h.events(.change).count, 2, "a checked radio never unchecks itself")

        let slider = h.view(3, as: NSHSliderView.self)
        // A value sent before its range is kept, not clamped to the default 0…1.
        h.apply { b in
            b.prop(3, .value, .f32(40))
            b.prop(3, .max, .f32(100))
        }
        XCTAssertEqual(slider.control.doubleValue, 40)
        h.apply { $0.prop(3, .max, .f32(10)) }
        slider.control.doubleValue = 6.4
        slider.changed()
        XCTAssertEqual(h.events(.change).last?.payload.double("value"), 6)
        XCTAssertEqual(h.events(.change).last?.payload.bool("final"), true)

        let seg = h.view(4, as: NSHSegmentedView.self)
        XCTAssertEqual(seg.control.segmentCount, 3)
        XCTAssertEqual(seg.control.selectedSegment, 1)
        seg.control.selectedSegment = 2
        seg.changed()
        XCTAssertEqual(h.events(.change).last?.payload.string("value"), "Month")

        let select = h.view(5, as: NSHSelectView.self)
        XCTAssertEqual(select.title, "Choose…", "the placeholder shows until a choice is made")
        select.choose(1)
        XCTAssertEqual(h.events(.change).last?.payload.string("value"), "date")
        XCTAssertEqual(select.title, "Date")

        let date = h.view(6, as: NSHDatePickerView.self)
        XCTAssertEqual(DateFormats.format(date.control.dateValue, mode: 0), "2026-10-14")
        h.apply { $0.prop(6, .dateMode, .i32(2)) }
        XCTAssertEqual(date.control.datePickerElements, [.yearMonthDay, .hourMinute])
        date.changed()
        XCTAssertEqual(h.events(.change).last?.payload.string("date"), "2026-10-14T00:00")
    }

    func testBuiltinControlMeasurement() {
        let short = BuiltinMeasure.measure("nsh/select", ["label": "Day"])!
        let long = BuiltinMeasure.measure("nsh/select", ["label": "A much longer label"])!
        XCTAssertGreaterThan(long.width, short.width)
        XCTAssertGreaterThanOrEqual(short.height, SelectChrome.minHeight)
        // The measured select fits the real pop-up button showing that label.
        let b = NSPopUpButton(frame: .zero, pullsDown: false)
        b.addItem(withTitle: "A much longer label")
        XCTAssertGreaterThanOrEqual(long.width + 1, b.fittingSize.width)
        let date = BuiltinMeasure.measure("nsh/date-picker", ["value": "2026-10-14", "mode": "date"])!
        let both = BuiltinMeasure.measure("nsh/date-picker", ["value": "2026-10-14T09:30", "mode": "datetime"])!
        XCTAssertGreaterThan(both.width, date.width)
        let p = NSDatePicker()
        p.datePickerStyle = .textFieldAndStepper
        p.datePickerElements = .yearMonthDay
        p.dateValue = DateFormats.parse("2026-10-14", mode: 0)!
        XCTAssertGreaterThanOrEqual(date.width + 1, p.fittingSize.width)
        XCTAssertNil(BuiltinMeasure.measure("nsh/unknown", [:]))
    }

    func testShapeIsDrawnInFlippedCoordinates() {
        h.apply { b in
            b.create(1, .shape, parent: 0, frame: CGRect(x: 0, y: 0, width: 24, height: 24), props: [
                (.pathData, .str("M0 0 L24 0 L24 4 L0 4 Z")), (.viewBox, .f32Array([0, 0, 24, 24])), (.fillColor, .color(0xFF00_0000)),
            ])
        }
        let v = h.view(1, as: NSHShapeView.self)
        let box = v.renderedPath!.boundingBox
        XCTAssertEqual(box, CGRect(x: 0, y: 0, width: 24, height: 4))
        // The view's layer is geometry-flipped, so y = 0 is the top of the view on screen.
        XCTAssertTrue(v.layer?.contentsAreFlipped() ?? false)
    }

    func testScrollToAndScrollEvents() {
        h.apply { b in
            b.create(1, .scroll, parent: 0, frame: CGRect(x: 0, y: 0, width: 300, height: 400), props: [(.contentSize, .f32Array([300, 2000]))])
            b.create(2, .view, parent: 1, frame: CGRect(x: 0, y: 1000, width: 300, height: 44))
            b.mask(1, EventMask.scroll)
        }
        let s = h.view(1, as: NSHScrollView.self)
        XCTAssertTrue(h.view(2)!.superview === s.document)
        XCTAssertEqual(s.document.frame.size, CGSize(width: 300, height: 2000))
        h.apply { $0.command(1, .scrollTo, ["y": 5000, "animated": false]) }
        XCTAssertEqual(s.contentOffset.y, 1600, "clamped to the content")
        XCTAssertTrue(wait { !self.h.events(.scroll).isEmpty })
        XCTAssertEqual(h.events(.scroll).last?.payload.double("y"), 1600)
        XCTAssertEqual(h.events(.scroll).last?.payload.double("viewport_height"), 400)
    }

    func testRootCommands() {
        h.apply { $0.command(0, .setClipboard, ["text": "copied from a test"]) }
        XCTAssertEqual(NSPasteboard.general.string(forType: .string), "copied from a test")
        // Haptics are a no-op without a Force Touch trackpad; they must not throw.
        h.apply { $0.command(0, .haptic, ["kind": "selection"]) }
    }

    func testAnimatedBatchAnimatesDeclaredTransitions() {
        h.apply { b in
            b.create(1, .view, parent: 0, frame: CGRect(x: 0, y: 0, width: 100, height: 100), props: [
                (.animation, .json(#"{"frame":{"kind":"timing","duration":200},"opacity":{"kind":"spring","response":0.3}}"#)),
            ])
        }
        Animator.reduceMotion = { false }
        defer { Animator.reduceMotion = { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion } }
        h.apply({ b in
            b.frame(1, CGRect(x: 50, y: 0, width: 100, height: 100))
            b.prop(1, .opacity, .f32(0.2))
        }, flags: BatchFlags.animated)
        let v = h.view(1)!
        // The model values are final at once; Core Animation animates the presentation.
        XCTAssertEqual(v.frame.minX, 50)
        XCTAssertEqual(v.alphaValue, 0.2, accuracy: 0.001)
        XCTAssertFalse(v.layer?.animationKeys()?.isEmpty ?? true, "an animation is running")

        Animator.reduceMotion = { true }
        h.apply({ b in b.frame(1, CGRect(x: 0, y: 0, width: 100, height: 100)) }, flags: BatchFlags.animated)
        XCTAssertEqual(v.frame.minX, 0)
    }

    func testThemeTokensPropDrivesRendererDrawnColors() {
        h.apply { b in
            b.prop(0, .themeTokens, .json(#"{"appearance":"dark","primary":4294901760,"background":4278190080}"#))
            b.create(1, .slider, parent: 0, frame: CGRect(x: 0, y: 0, width: 200, height: 30))
        }
        XCTAssertEqual(h.manager.theme.appearance, "dark")
        let slider = h.view(1, as: NSHSliderView.self)
        XCTAssertEqual(Colors.toHex(slider.control.trackFillColor!), "#ff0000")
        XCTAssertEqual(Colors.toHex(ThemeTokens.color("background", runtime: .shared, theme: h.manager.theme)), "#000000")
    }

    func testToastsStackQueueAndDismiss() {
        for i in 0 ..< 5 { h.apply { $0.command(0, .showToast, ["id": "t\(i)", "title": "Saved \(i)", "duration": 0]) } }
        let t = h.surface.toasts
        XCTAssertEqual(t.visible.map(\.id), ["t0", "t1", "t2"])
        XCTAssertEqual(t.queued.map(\.id), ["t3", "t4"])
        // Newest at the bottom, all above the bottom edge of the surface.
        let frames = t.visible.map { h.surface.convert($0.frame, from: $0.superview) }
        XCTAssertLessThan(frames[0].minY, frames[2].minY)
        XCTAssertLessThanOrEqual(frames[2].maxY, h.surface.bounds.height - 16 + 0.5)
        h.apply { $0.command(0, .dismissToast, ["id": "t1"]) }
        XCTAssertEqual(t.visible.map(\.id), ["t0", "t2", "t3"])
        h.apply { $0.command(0, .dismissToast, [:]) }
        XCTAssertTrue(t.visible.isEmpty && t.queued.isEmpty)
    }

    func testToastActionDismissesAndReports() {
        h.apply { $0.command(0, .showToast, ["id": "undo-1", "title": "Deleted", "action": ["label": "Undo"], "level": "success", "duration": 0]) }
        let toast = h.surface.toasts.visible.first!
        XCTAssertEqual(toast.actionButton?.title, "Undo")
        XCTAssertEqual(toast.accessibilityLabel(), "Success. Deleted")
        toast.actionTapped()
        XCTAssertEqual(h.events(.toastAction).first?.payload.string("id"), "undo-1")
        XCTAssertEqual(h.events(.toastAction).first?.node, 0)
        XCTAssertTrue(h.surface.toasts.visible.isEmpty)
    }

    func testEnvironmentReportsMacOS() {
        let env = h.surface.environment
        XCTAssertEqual(env["platform"] as? String, "macos")
        XCTAssertEqual(env["width"] as? Double, 390)
        XCTAssertEqual(env["height"] as? Double, 844)
        XCTAssertEqual(env["keyboard_height"] as? Double, 0)
        XCTAssertEqual(env["clipboard_feedback"] as? Bool, false)
        XCTAssertEqual(env["safe_area"] as? [Double], [0, 0, 0, 0])
        h.surface.appearance = NSAppearance(named: .darkAqua)
        XCTAssertEqual(h.surface.environment["appearance"] as? String, "dark")
        h.surface.appearance = NSAppearance(named: .aqua)
        XCTAssertEqual(h.surface.environment["appearance"] as? String, "light")
    }

    func testImageLoadsAbsoluteFilePathsOnly() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("nsh-test-\(UUID().uuidString).png")
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 4, pixelsHigh: 4, bitsPerSample: 8, samplesPerPixel: 4,
                                   hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        try rep.representation(using: .png, properties: [:])!.write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        h.apply { b in
            b.create(1, .image, parent: 0, frame: CGRect(x: 0, y: 0, width: 40, height: 40), props: [(.imageSource, .str(url.path))])
            b.create(2, .image, parent: 0, frame: CGRect(x: 0, y: 0, width: 40, height: 40), props: [(.imageSource, .str("relative.png"))])
        }
        XCTAssertTrue(wait { self.h.view(1, as: NSHImageView.self).loaded != nil })
        spin(0.1)
        XCTAssertNil(h.view(2, as: NSHImageView.self).loaded)
    }
}
