// System alerts and toasts.

import AppKit
import NativeShellShared

enum AlertPresenter {
    /// Presents the dialog's alert once, as a sheet on `window`; choosing a button
    /// reports `Action {index}` (the index in the spec's actions).
    ///
    /// macOS orders alert buttons from the trailing edge: the primary (last
    /// non-cancel) action first, so it is the default button, then the other
    /// actions, then Cancel, which Escape triggers.
    static func present(_ spec: [String: Any], for dialog: NSHDialogView, in window: NSWindow?) {
        guard dialog.alert == nil, let window = window else { return }
        let alert = makeAlert(spec)
        dialog.alert = alert
        alert.alert.beginSheetModal(for: window) { [weak dialog] response in
            guard let dialog = dialog, dialog.alert === alert else { return }
            dialog.alert = nil
            let i = response.rawValue - NSApplication.ModalResponse.alertFirstButtonReturn.rawValue
            if i >= 0, i < alert.indices.count, let index = alert.indices[i] {
                dialog.actionChosen(index)
            } else {
                dialog.userDismissed()
            }
        }
    }

    /// The alert and, per button in AppKit order, the action index it reports (nil: plain dismissal).
    static func makeAlert(_ spec: [String: Any]) -> PresentedAlert {
        let alert = NSAlert()
        alert.messageText = spec.string("title") ?? ""
        alert.informativeText = spec.string("message") ?? ""
        let actions = (spec["actions"] as? [[String: Any]] ?? []).enumerated().map { ($0.offset, $0.element) }
        let cancel = actions.filter { $0.1.string("style") == "cancel" }
        let others = actions.filter { $0.1.string("style") != "cancel" }
        var indices: [Int?] = []
        for (i, a) in others.reversed() {
            let b = alert.addButton(withTitle: a.string("label") ?? "OK")
            if a.string("style") == "destructive" { b.hasDestructiveAction = true }
            indices.append(i)
        }
        for (i, a) in cancel {
            let b = alert.addButton(withTitle: a.string("label") ?? "Cancel")
            b.keyEquivalent = "\u{1b}"
            indices.append(i)
        }
        if actions.isEmpty {
            alert.addButton(withTitle: "OK")
            indices.append(nil)
        }
        return PresentedAlert(alert: alert, indices: indices)
    }
}

final class PresentedAlert {
    let alert: NSAlert
    let indices: [Int?]

    init(alert: NSAlert, indices: [Int?]) {
        self.alert = alert
        self.indices = indices
    }
}

extension NSHDialogView {
    private static var alertKey = 0

    /// The system alert on screen for this dialog.
    var alert: PresentedAlert? {
        get { objc_getAssociatedObject(self, &NSHDialogView.alertKey) as? PresentedAlert }
        set { objc_setAssociatedObject(self, &NSHDialogView.alertKey, newValue, .OBJC_ASSOCIATION_RETAIN_NONATOMIC) }
    }

    /// The core closed the alert: take it off screen without reporting.
    func dismissAlert() {
        guard let a = alert else { return }
        alert = nil
        let w = a.alert.window
        w.sheetParent?.endSheet(w)
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

    /// Toasts float above pages too: they go in the window's content view when there is one.
    private func container(_ host: NSView) -> NSView { host.window?.contentView ?? host }

    private func present(_ t: ToastView) {
        guard let host = host else { return }
        t.onAction = { [weak self, weak host, weak t] in
            guard let self = self, let t = t else { return }
            self.retract(t, animated: true)
            host?.manager.send(Wire.rootNode, .toastAction, ["id": t.id])
        }
        visible.append(t)
        container(host).addSubview(t)
        t.prepare(width: host.bounds.width)
        t.alphaValue = 0
        relayout(animated: false)
        NSAnimationContext.runAnimationGroup { ctx in
            ctx.duration = 0.2
            t.animator().alphaValue = 1
        }
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
        NSAnimationContext.runAnimationGroup({ ctx in
            ctx.duration = animated ? 0.2 : 0
            t.animator().alphaValue = 0
        }, completionHandler: { t.removeFromSuperview() })
        if !queued.isEmpty { present(queued.removeFirst()) }
        relayout(animated: animated)
    }

    private func relayout(animated: Bool) {
        guard let host = host else { return }
        let container = container(host)
        var bottom = host.bounds.height - host.toastInset - 16
        for t in visible.reversed() {
            let local = CGRect(x: ((host.bounds.width - t.bounds.width) / 2).rounded(), y: bottom - t.bounds.height,
                               width: t.bounds.width, height: t.bounds.height)
            let f = host.convert(local, to: container)
            if animated { t.animator().frame = f } else { t.frame = f }
            bottom = local.minY - 8
        }
    }
}

/// One toast, styled like gpui-component's Notification: a neutral surface with
/// a border, the level shown by a leading symbol whose shape differs per level
/// (never by color alone).
final class ToastView: NSView {
    let titleLabel = NSTextField(wrappingLabelWithString: "")
    let descriptionLabel = NSTextField(wrappingLabelWithString: "")
    let iconView = NSImageView()
    /// Trailing text button when the payload has `action: {label}`.
    private(set) var actionButton: NSButton?
    var onAction: (() -> Void)?
    let id: String
    let level: String
    /// Seconds; 0 = until dismissed.
    let duration: TimeInterval

    static let symbols = ["info": "info.circle", "success": "checkmark.circle", "warning": "exclamationmark.triangle", "error": "xmark.octagon"]

    private let runtime: NSHRuntime
    private weak var theme: ThemeState?
    private var actionLabel: String?

    init(_ payload: [String: Any], runtime: NSHRuntime = .shared, theme: ThemeState? = nil) {
        self.runtime = runtime
        self.theme = theme
        id = payload["id"].map { "\($0)" } ?? UUID().uuidString
        let ms = payload.double("duration") ?? 3000
        duration = ms <= 0 ? 0 : max(0.5, ms / 1000)
        let lvl = payload.string("level") ?? "info"
        level = Self.symbols[lvl] == nil ? "info" : lvl
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = ThemeTokens.radius
        layer?.cornerCurve = .continuous
        layer?.borderWidth = 1
        shadow = {
            let s = NSShadow()
            s.shadowColor = NSColor.black.withAlphaComponent(0.12)
            s.shadowBlurRadius = 12
            s.shadowOffset = NSSize(width: 0, height: -2)
            return s
        }()

        let scale = FontScale.value
        iconView.image = NSImage(systemSymbolName: Self.symbols[level]!, accessibilityDescription: nil)?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 17 * scale, weight: .medium))
        iconView.imageScaling = .scaleNone
        titleLabel.stringValue = payload.string("title") ?? ""
        titleLabel.isHidden = payload.string("title") == nil
        titleLabel.font = .systemFont(ofSize: 15 * scale, weight: .medium)
        titleLabel.maximumNumberOfLines = 2
        descriptionLabel.stringValue = payload.string("description") ?? ""
        descriptionLabel.isHidden = payload.string("description") == nil
        descriptionLabel.font = .systemFont(ofSize: 13 * scale)
        descriptionLabel.maximumNumberOfLines = 3
        [iconView, titleLabel, descriptionLabel].forEach(addSubview)
        if let label = (payload["action"] as? [String: Any])?.string("label"), !label.isEmpty {
            let b = NSButton(title: label, target: self, action: #selector(actionTapped))
            b.isBordered = false
            b.setAccessibilityIdentifier("nsh-toast-action")
            actionLabel = label
            addSubview(b)
            actionButton = b
        }
        let levelName = ["info": "Info", "success": "Success", "warning": "Warning", "error": "Error"][level]!
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel(([levelName] + [payload.string("title"), payload.string("description")].compactMap { $0 }).joined(separator: ". "))
        setAccessibilityIdentifier("nsh-toast")
        applyColors()
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    override var isFlipped: Bool { true }

    /// Colors from the theme tokens; redone when the theme or the appearance changes.
    func applyColors() {
        let token = { ThemeTokens.color($0, runtime: self.runtime, theme: self.theme) }
        layer?.backgroundColor = token("background").cgColor(in: effectiveAppearance)
        layer?.borderColor = token("border").cgColor(in: effectiveAppearance)
        switch level {
        case "error": iconView.contentTintColor = token("destructive")
        case "success": iconView.contentTintColor = .systemGreen
        case "warning": iconView.contentTintColor = .systemOrange
        default: iconView.contentTintColor = .systemBlue
        }
        titleLabel.textColor = token("foreground")
        descriptionLabel.textColor = token("muted_foreground")
        if let b = actionButton, let label = actionLabel {
            b.attributedTitle = NSAttributedString(string: label, attributes: [
                .foregroundColor: token("foreground"), .font: NSFont.systemFont(ofSize: 15 * FontScale.value, weight: .medium),
            ])
        }
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyColors()
    }

    @objc func actionTapped() { onAction?() }

    /// Sizes the toast for a container `width` wide.
    func prepare(width containerWidth: CGFloat) {
        let width = min(containerWidth - 32, 420)
        let iconSide = 20 * FontScale.value
        let left = 14 + iconSide + 10
        var right: CGFloat = 16
        if let b = actionButton {
            right = b.fittingSize.width + 24
        }
        let inner = width - left - right
        let th = titleLabel.isHidden ? 0 : titleLabel.sizeThatFits(CGSize(width: inner, height: 200)).height
        let dh = descriptionLabel.isHidden ? 0 : descriptionLabel.sizeThatFits(CGSize(width: inner, height: 300)).height
        titleLabel.frame = CGRect(x: left, y: 12, width: inner, height: th)
        descriptionLabel.frame = CGRect(x: left, y: 12 + th + (th > 0 && dh > 0 ? 2 : 0), width: inner, height: dh)
        let height = max(max(descriptionLabel.frame.maxY, titleLabel.frame.maxY) + 12, 44)
        // The icon aligns with the first line.
        let firstLine = th > 0 ? Fonts.lineHeight(titleLabel.font!) : Fonts.lineHeight(descriptionLabel.font!)
        iconView.frame = CGRect(x: 14, y: 12 + (firstLine - iconSide) / 2, width: iconSide, height: iconSide)
        setFrameSize(CGSize(width: width, height: height))
        if let b = actionButton {
            let s = b.fittingSize
            b.frame = CGRect(x: width - s.width - 16, y: ((height - s.height) / 2).rounded(), width: s.width, height: s.height)
        }
    }
}
