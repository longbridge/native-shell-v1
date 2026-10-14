// Native transitions declared with `.transition(…)` / `.spring(…)`: the core
// marks a batch ANIMATED, and frames / opacity of nodes that declared a
// transition for that property animate to their new value.

import AppKit

enum Animator {
    /// `Anim = {kind: "timing"|"spring", duration, delay (ms), easing, response, damping}`.
    /// Read on each run so a System Settings change applies at once; tests override it.
    static var reduceMotion: () -> Bool = { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion }

    /// `moves`: the change moves or resizes something. With Reduce Motion on, such
    /// changes apply at once; fades still animate.
    static func run(_ anim: [String: Any], moves: Bool = false, _ changes: @escaping () -> Void) {
        if moves, reduceMotion() {
            changes()
            return
        }
        let delay = (anim.double("delay") ?? 0) / 1000
        let duration: TimeInterval
        let timing: CAMediaTimingFunction
        if anim.string("kind") == "spring" {
            // A critically damped spring settles in about its response; an ease-out
            // curve of that length is its closest timing-function equivalent, and an
            // underdamped one overshoots a little.
            let response = max(0.05, anim.double("response") ?? 0.35)
            let damping = max(0.05, min(1, anim.double("damping") ?? 0.825))
            duration = response
            timing = damping < 0.8 ? CAMediaTimingFunction(controlPoints: 0.2, 1.25, 0.4, 1) : CAMediaTimingFunction(controlPoints: 0.2, 0.9, 0.3, 1)
        } else {
            duration = max(0, (anim.double("duration") ?? 250) / 1000)
            switch anim.string("easing") {
            case "linear": timing = CAMediaTimingFunction(name: .linear)
            case "ease_in": timing = CAMediaTimingFunction(name: .easeIn)
            case "ease_out": timing = CAMediaTimingFunction(name: .easeOut)
            default: timing = CAMediaTimingFunction(name: .easeInEaseOut)
            }
        }
        // Batches are applied with actions disabled; animations need them back on.
        let start = {
            CATransaction.begin()
            CATransaction.setDisableActions(false)
            defer { CATransaction.commit() }
            NSAnimationContext.runAnimationGroup { ctx in
                ctx.duration = duration
                ctx.timingFunction = timing
                ctx.allowsImplicitAnimation = true
                changes()
            }
        }
        if delay > 0 { DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: start) } else { start() }
    }
}

/// Haptics play on Force Touch trackpads only, and only while a finger rests on
/// one; elsewhere they are silently skipped, as the platform intends.
enum Haptics {
    static func play(_ kind: String) {
        let pattern: NSHapticFeedbackManager.FeedbackPattern
        switch kind {
        case "selection": pattern = .alignment
        case "success", "warning", "error": pattern = .levelChange
        default: pattern = .generic
        }
        NSHapticFeedbackManager.defaultPerformer.perform(pattern, performanceTime: .now)
    }
}

enum ComponentRegistry {
    static func make(_ type: NSHViewType) -> NSHComponentView {
        let cls: NSHComponentView.Type
        switch type {
        case .view: cls = NSHComponentView.self
        case .text: cls = NSHTextView.self
        case .image: cls = NSHImageView.self
        case .scroll: cls = NSHScrollView.self
        case .textInput: cls = NSHTextInputView.self
        case .switchView: cls = NSHSwitchView.self
        case .checkbox: cls = NSHCheckboxView.self
        case .slider: cls = NSHSliderView.self
        case .progress: cls = NSHProgressView.self
        case .list: cls = NSHListView.self
        case .listRow: cls = NSHListRowView.self
        case .page: cls = NSHPageView.self
        case .sheet: cls = NSHSheetView.self
        case .dialog: cls = NSHDialogView.self
        case .segmented: cls = NSHSegmentedView.self
        case .datePicker: cls = NSHDatePickerView.self
        case .select: cls = NSHSelectView.self
        case .shape: cls = NSHShapeView.self
        case .spinner: cls = NSHSpinnerView.self
        case .hostComponent: cls = NSHHostComponentView.self
        }
        let v = cls.init(frame: .zero)
        v.viewType = type
        return v
    }
}
