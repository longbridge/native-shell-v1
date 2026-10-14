// System alerts and toasts.

import NativeShellShared
import UIKit

enum AlertPresenter {
    /// Presents the dialog's alert once; tapping an action reports `Action {index}`.
    static func present(_ spec: [String: Any], for dialog: NSHDialogView, from host: UIView) {
        guard dialog.alert == nil, let presenter = topViewController(from: host) else { return }
        let alert = UIAlertController(title: spec.string("title"), message: spec.string("message"), preferredStyle: .alert)
        let actions = spec["actions"] as? [[String: Any]] ?? []
        for (i, a) in actions.enumerated() {
            let style: UIAlertAction.Style
            switch a.string("style") {
            case "cancel": style = .cancel
            case "destructive": style = .destructive
            default: style = .default
            }
            alert.addAction(UIAlertAction(title: a.string("label") ?? "OK", style: style) { [weak dialog] _ in
                dialog?.alert = nil
                dialog?.actionChosen(i)
            })
        }
        if actions.isEmpty {
            alert.addAction(UIAlertAction(title: "OK", style: .default) { [weak dialog] _ in
                dialog?.alert = nil
                dialog?.userDismissed()
            })
        }
        dialog.alert = alert
        presenter.present(alert, animated: true)
    }

    static func topViewController(from view: UIView) -> UIViewController? {
        var vc = view.window?.rootViewController
        while let p = vc?.presentedViewController, !p.isBeingDismissed { vc = p }
        return vc
    }
}

extension NSHDialogView {
    private static var alertKey = 0

    /// The system alert on screen for this dialog.
    var alert: UIAlertController? {
        get { objc_getAssociatedObject(self, &NSHDialogView.alertKey) as? UIAlertController }
        set { objc_setAssociatedObject(self, &NSHDialogView.alertKey, newValue, .OBJC_ASSOCIATION_RETAIN_NONATOMIC) }
    }

    /// The core closed the alert: take it off screen without reporting.
    func dismissAlert() {
        guard let a = alert else { return }
        alert = nil
        a.presentingViewController?.dismiss(animated: true)
    }
}

/// Toasts at the bottom of the surface, as gpui-shell shows them: at most three
/// visible (stacked, newest at the bottom), the rest queued in order. A new toast
/// never retracts an earlier one; `duration` 0 keeps a toast until dismissed.
final class ToastCenter {
    static let maxVisible = 3
    private weak var host: NSHSurfaceView?
    private(set) var visible: [ToastView] = []
    private(set) var queued: [ToastView] = []

    init(host: NSHSurfaceView) { self.host = host }

    func themeDidChange() { (visible + queued).forEach { $0.applyColors() } }

    func show(_ payload: [String: Any]) {
        let t = ToastView(payload, runtime: host?.manager.runtime ?? .shared, theme: host?.manager.theme)
        if visible.count < Self.maxVisible { present(t) } else { queued.append(t) }
    }

    /// Retracts the toast with `id` (visible or queued), or every toast when nil.
    func dismiss(id: String?) {
        guard let id = id else {
            queued.removeAll()
            visible.forEach { retract($0, animated: true) }
            return
        }
        queued.removeAll { $0.id == id }
        visible.filter { $0.id == id }.forEach { retract($0, animated: true) }
    }

    private func present(_ t: ToastView) {
        guard let host = host else { return }
        t.onAction = { [weak self, weak host, weak t] in
            guard let self = self, let t = t else { return }
            self.retract(t, animated: true)
            host?.manager.send(Wire.rootNode, .toastAction, ["id": t.id])
        }
        visible.append(t)
        let container = host.window ?? host
        container.addSubview(t)
        t.prepare(width: container.bounds.width)
        t.alpha = 0
        relayout(animated: false)
        UIView.animate(withDuration: 0.2) { t.alpha = 1 }
        if t.duration > 0 {
            DispatchQueue.main.asyncAfter(deadline: .now() + t.duration) { [weak self, weak t] in
                guard let self = self, let t = t else { return }
                self.retract(t, animated: true)
            }
        }
    }

    private func retract(_ t: ToastView, animated: Bool) {
        guard let i = visible.firstIndex(where: { $0 === t }) else { return }
        visible.remove(at: i)
        UIView.animate(withDuration: animated ? 0.2 : 0, animations: { t.alpha = 0 }) { _ in t.removeFromSuperview() }
        if !queued.isEmpty { present(queued.removeFirst()) }
        relayout(animated: animated)
    }

    private func relayout(animated: Bool) {
        guard let host = host else { return }
        let container = host.window ?? host
        var bottom = container.bounds.height - host.toastInset - 16
        for t in visible.reversed() {
            let f = CGRect(x: (container.bounds.width - t.bounds.width) / 2, y: bottom - t.bounds.height, width: t.bounds.width, height: t.bounds.height)
            if animated { UIView.animate(withDuration: 0.2) { t.frame = f } } else { t.frame = f }
            bottom = f.minY - 8
        }
    }
}

/// A transient message at the bottom of the window.
/// One toast, styled like gpui-component's Notification: a neutral surface with
/// a border, the level shown by a leading symbol whose shape differs per level
/// (never by color alone).
final class ToastView: UIView {
    let titleLabel = UILabel()
    let descriptionLabel = UILabel()
    let iconView = UIImageView()
    /// Trailing text button when the payload has `action: {label}`.
    private(set) var actionButton: UIButton?
    var onAction: (() -> Void)?
    let id: String
    let level: String
    /// Seconds; 0 = until dismissed.
    let duration: TimeInterval

    static let symbols = ["info": "info.circle", "success": "checkmark.circle", "warning": "exclamationmark.triangle", "error": "xmark.octagon"]

    private let runtime: NSHRuntime
    private weak var theme: ThemeState?

    init(_ payload: [String: Any], runtime: NSHRuntime = .shared, theme: ThemeState? = nil) {
        self.runtime = runtime
        self.theme = theme
        id = payload["id"].map { "\($0)" } ?? UUID().uuidString
        let ms = payload.double("duration") ?? 3000
        duration = ms <= 0 ? 0 : max(0.5, ms / 1000)
        let lvl = payload.string("level") ?? "info"
        level = Self.symbols[lvl] == nil ? "info" : lvl
        super.init(frame: .zero)
        layer.cornerRadius = ThemeTokens.radius
        layer.borderWidth = 1
        layer.shadowColor = UIColor.black.cgColor
        layer.shadowOpacity = 0.08
        layer.shadowRadius = 8
        layer.shadowOffset = CGSize(width: 0, height: 2)

        let scale = FontScale.value
        iconView.image = UIImage(systemName: Self.symbols[level]!, withConfiguration: UIImage.SymbolConfiguration(pointSize: 17 * scale, weight: .medium))
        iconView.contentMode = .center
        titleLabel.text = payload.string("title")
        titleLabel.font = .systemFont(ofSize: 15 * scale, weight: .medium)
        titleLabel.numberOfLines = 2
        descriptionLabel.text = payload.string("description")
        descriptionLabel.font = .systemFont(ofSize: 13 * scale)
        descriptionLabel.numberOfLines = 3
        [iconView, titleLabel, descriptionLabel].forEach(addSubview)
        if let label = (payload["action"] as? [String: Any])?.string("label"), !label.isEmpty {
            let b = UIButton(type: .system)
            b.setTitle(label, for: .normal)
            b.titleLabel?.font = .systemFont(ofSize: 15 * scale, weight: .medium)
            b.addTarget(self, action: #selector(actionTapped), for: .touchUpInside)
            b.accessibilityIdentifier = "nsh-toast-action"
            addSubview(b)
            actionButton = b
            // The card itself is one element; the action stays separately reachable.
            isAccessibilityElement = false
            accessibilityElements = [AccessibleSummary(self), b]
        }
        if actionButton == nil { isAccessibilityElement = true }
        let levelName = ["info": "Info", "success": "Success", "warning": "Warning", "error": "Error"][level]!
        accessibilityLabel = ([levelName] + [payload.string("title"), payload.string("description")].compactMap { $0 }).joined(separator: ". ")
        accessibilityTraits = .staticText
        accessibilityIdentifier = "nsh-toast"
        applyColors()
    }

    private var borderColorToken: UIColor = .separator

    /// Colors from the theme tokens; redone when the theme changes.
    func applyColors() {
        let token = { ThemeTokens.color($0, runtime: self.runtime, theme: self.theme) }
        backgroundColor = token("background")
        borderColorToken = token("border")
        layer.borderColor = borderColorToken.resolvedColor(with: traitCollection).cgColor
        switch level {
        case "error": iconView.tintColor = token("destructive")
        case "success": iconView.tintColor = .systemGreen
        case "warning": iconView.tintColor = .systemOrange
        default: iconView.tintColor = .systemBlue
        }
        titleLabel.textColor = token("foreground")
        descriptionLabel.textColor = token("muted_foreground")
        actionButton?.setTitleColor(token("foreground"), for: .normal)
    }

    @objc func actionTapped() { onAction?() }

    override func traitCollectionDidChange(_ previous: UITraitCollection?) {
        super.traitCollectionDidChange(previous)
        // CGColors do not follow the appearance by themselves.
        layer.borderColor = borderColorToken.resolvedColor(with: traitCollection).cgColor
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    /// Sizes the toast for a container `width` wide.
    func prepare(width containerWidth: CGFloat) {
        let width = min(containerWidth - 32, 420)
        let iconSide = 20 * FontScale.value
        let left = 14 + iconSide + 10
        var right: CGFloat = 16
        if let b = actionButton {
            // At least a 44 pt touch target, text width plus padding.
            let w = max(44, (b.titleLabel?.intrinsicContentSize.width ?? 0) + 24)
            right = w + 4
        }
        let inner = width - left - right
        let th = titleLabel.text == nil ? 0 : titleLabel.sizeThatFits(CGSize(width: inner, height: 200)).height
        let dh = descriptionLabel.text == nil ? 0 : descriptionLabel.sizeThatFits(CGSize(width: inner, height: 300)).height
        titleLabel.frame = CGRect(x: left, y: 12, width: inner, height: th)
        descriptionLabel.frame = CGRect(x: left, y: 12 + th + (th > 0 && dh > 0 ? 2 : 0), width: inner, height: dh)
        let height = max(descriptionLabel.frame.maxY, titleLabel.frame.maxY) + 12
        // The icon aligns with the first line.
        let firstLine = th > 0 ? titleLabel.font.lineHeight : descriptionLabel.font.lineHeight
        iconView.frame = CGRect(x: 14, y: 12 + (firstLine - iconSide) / 2, width: iconSide, height: iconSide)
        bounds = CGRect(x: 0, y: 0, width: width, height: max(height, 44))
        if let b = actionButton {
            let w = right - 4
            b.frame = CGRect(x: width - w - 4, y: (bounds.height - 44) / 2, width: w, height: 44)
        }
        layer.shadowPath = UIBezierPath(roundedRect: bounds, cornerRadius: ThemeTokens.radius).cgPath
    }
}

/// The toast's text as one accessibility element when the toast also has an action.
private final class AccessibleSummary: UIAccessibilityElement {
    private weak var toast: ToastView?

    init(_ toast: ToastView) {
        self.toast = toast
        super.init(accessibilityContainer: toast)
        accessibilityTraits = .staticText
    }

    override var accessibilityLabel: String? {
        get { toast?.accessibilityLabel }
        set {}
    }

    override var accessibilityFrameInContainerSpace: CGRect {
        get { toast?.bounds ?? .zero }
        set {}
    }
}
