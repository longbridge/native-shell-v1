// Surface nodes: `Page`, `Sheet` and `Dialog`. They hold their props; the
// surface presenter (`NSHViewController`, or the bare surface view) decides how
// they appear.

import UIKit

/// Something that wants to hear when a surface's props change.
protocol SurfaceObserver: AnyObject {
    func surfacePropsChanged(_ surface: NSHComponentView)
}

class NSHSurfaceNodeView: NSHComponentView {
    weak var observer: SurfaceObserver?
    private(set) var dismissable = true

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        if prop == .dismissable {
            dismissable = value.bool ?? true
            observer?.surfacePropsChanged(self)
            return true
        }
        return super.applyProp(prop, value)
    }

    /// Dismissed by the user (swipe, back, backdrop): tell the core.
    func userDismissed() { send(.dismiss) }
}

/// A full-screen page. Programs draw their own headers; `Title` only names the
/// page for accessibility and the back-history menu. `NavBarHidden` is reserved.
final class NSHPageView: NSHSurfaceNodeView {
    private(set) var title = ""
    private(set) var canGoBack = true
    /// `DismissPage` arrived: the page leaves the stack now; its `Delete` may come later.
    private(set) var dismissing = false

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .title:
            title = value.string ?? ""
            accessibilityLabel = title.isEmpty ? nil : title
        case .canGoBack: canGoBack = value.bool ?? true
        case .navBarHidden: break
        default: return super.applyProp(prop, value)
        }
        observer?.surfacePropsChanged(self)
        return true
    }

    override func runCommand(_ command: Command, _ payload: [String: Any]) {
        if command == .dismissPage { dismissing = true }
        if command == .presentPage { dismissing = false }
    }

    func backPressed() { send(.backPressed) }
}

enum SheetDetent: Equatable {
    case medium
    case large
    case height(CGFloat)
}

final class NSHSheetView: NSHSurfaceNodeView {
    /// `["medium", "large"]` and/or `{"height": 320}` entries; empty: fit the content.
    private(set) var detents: [SheetDetent] = []

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        if prop == .detents {
            detents = (value.jsonArray ?? []).compactMap { d in
                if let s = d as? String { return s == "medium" ? .medium : (s == "large" ? .large : nil) }
                if let h = (d as? [String: Any])?.cgFloat("height"), h > 0 { return .height(h) }
                return nil
            }
            observer?.surfacePropsChanged(self)
            return true
        }
        return super.applyProp(prop, value)
    }
}

final class NSHDialogView: NSHSurfaceNodeView {
    /// `{title, message, actions: [{label, style}]}`: a system alert instead of custom content.
    private(set) var alertSpec: [String: Any]?

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        if prop == .alertSpec {
            alertSpec = value.jsonObject
            observer?.surfacePropsChanged(self)
            return true
        }
        return super.applyProp(prop, value)
    }

    func actionChosen(_ index: Int) { send(.action, ["index": index]) }

    override func willDestroy() { dismissAlert() }
}
