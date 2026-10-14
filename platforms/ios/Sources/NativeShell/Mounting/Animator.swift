// Native transitions declared with `.transition(…)` / `.spring(…)`: the core
// marks a batch ANIMATED, and frames / opacity of nodes that declared a
// transition for that property animate to their new value.

import NativeShellShared
import UIKit

enum Animator {
    /// `Anim = {kind: "timing"|"spring", duration, delay (ms), easing, response, damping}`.
    /// Read on each run so a Settings change applies at once; tests override it.
    static var reduceMotion: () -> Bool = { UIAccessibility.isReduceMotionEnabled }

    /// `moves`: the change moves or resizes something. With Reduce Motion on, such
    /// changes apply at once; fades still animate.
    static func run(_ anim: [String: Any], moves: Bool = false, _ changes: @escaping () -> Void) {
        if moves, reduceMotion() {
            changes()
            return
        }
        let delay = (anim.double("delay") ?? 0) / 1000
        if anim.string("kind") == "spring" {
            let response = max(0.05, anim.double("response") ?? 0.35)
            let damping = max(0.05, min(1, anim.double("damping") ?? 0.825))
            let timing = UISpringTimingParameters(dampingRatio: CGFloat(damping))
            let animator = UIViewPropertyAnimator(duration: response, timingParameters: timing)
            animator.addAnimations(changes)
            start(animator, delay)
            return
        }
        let duration = max(0, (anim.double("duration") ?? 250) / 1000)
        let curve: UIView.AnimationCurve
        switch anim.string("easing") {
        case "linear": curve = .linear
        case "ease_in": curve = .easeIn
        case "ease_out": curve = .easeOut
        default: curve = .easeInOut
        }
        let animator = UIViewPropertyAnimator(duration: duration, curve: curve, animations: changes)
        start(animator, delay)
    }

    /// Batches are applied with actions disabled; animations need them back on.
    private static func start(_ animator: UIViewPropertyAnimator, _ delay: TimeInterval) {
        CATransaction.begin()
        CATransaction.setDisableActions(false)
        animator.startAnimation(afterDelay: delay)
        CATransaction.commit()
    }
}

enum Haptics {
    static func play(_ kind: String) {
        switch kind {
        case "success": UINotificationFeedbackGenerator().notificationOccurred(.success)
        case "warning": UINotificationFeedbackGenerator().notificationOccurred(.warning)
        case "error": UINotificationFeedbackGenerator().notificationOccurred(.error)
        case "selection": UISelectionFeedbackGenerator().selectionChanged()
        case "medium": UIImpactFeedbackGenerator(style: .medium).impactOccurred()
        case "heavy": UIImpactFeedbackGenerator(style: .heavy).impactOccurred()
        default: UIImpactFeedbackGenerator(style: .light).impactOccurred()
        }
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
