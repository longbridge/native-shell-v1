// `Scroll`: a UIScrollView whose content the core lays out (`ContentSize`),
// with throttled scroll events and pull to refresh.

import NativeShellShared
import UIKit

final class NSHScrollView: NSHComponentView, UIScrollViewDelegate {
    let scrollView = UIScrollView()
    private var refresh: UIRefreshControl?
    private var horizontal = false
    private var lastScrollEvent: TimeInterval = 0
    private var pendingScroll = false

    override func commonInit() {
        scrollView.delegate = self
        scrollView.contentInsetAdjustmentBehavior = .never
        scrollView.alwaysBounceVertical = true
        scrollView.delaysContentTouches = true
        scrollView.keyboardDismissMode = .interactive
        addSubview(scrollView)
    }

    override var childContainer: UIView { scrollView }

    override func layoutSubviews() {
        super.layoutSubviews()
        if scrollView.frame != bounds { scrollView.frame = bounds }
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .contentSize:
            let a = value.floats ?? []
            scrollView.contentSize = a.count == 2 ? CGSize(width: CGFloat(a[0]), height: CGFloat(a[1])) : .zero
        case .horizontal:
            horizontal = value.bool ?? false
            scrollView.alwaysBounceVertical = !horizontal
            scrollView.alwaysBounceHorizontal = horizontal
        case .showsIndicators:
            let on = value.bool ?? true
            scrollView.showsVerticalScrollIndicator = on
            scrollView.showsHorizontalScrollIndicator = on
        case .refreshEnabled:
            setRefreshEnabled(value.bool ?? false)
        case .refreshing:
            let on = value.bool ?? false
            if on, refresh?.isRefreshing == false { refresh?.beginRefreshing() }
            if !on, refresh?.isRefreshing == true { refresh?.endRefreshing() }
        default:
            return super.applyProp(prop, value)
        }
        return true
    }

    private func setRefreshEnabled(_ on: Bool) {
        if on, refresh == nil {
            let r = UIRefreshControl()
            r.addTarget(self, action: #selector(onRefresh), for: .valueChanged)
            scrollView.refreshControl = r
            refresh = r
        } else if !on, refresh != nil {
            scrollView.refreshControl = nil
            refresh = nil
        }
    }

    @objc private func onRefresh() {
        send(.refresh)
    }

    override func runCommand(_ command: Command, _ payload: [String: Any]) {
        guard command == .scrollTo else { return }
        layoutIfNeeded()
        let maxX = max(0, scrollView.contentSize.width - scrollView.bounds.width)
        let maxY = max(0, scrollView.contentSize.height - scrollView.bounds.height)
        let x = min(max(0, payload.cgFloat("x") ?? scrollView.contentOffset.x), maxX)
        let y = min(max(0, payload.cgFloat("y") ?? scrollView.contentOffset.y), maxY)
        scrollView.setContentOffset(CGPoint(x: x, y: y), animated: payload.bool("animated") ?? true)
    }

    // Scroll events at most every 16 ms; the last position is always delivered.
    func scrollViewDidScroll(_ sv: UIScrollView) {
        guard listens(EventMask.scroll) else { return }
        let now = ProcessInfo.processInfo.systemUptime
        if now - lastScrollEvent >= 0.016 {
            emitScroll()
        } else if !pendingScroll {
            pendingScroll = true
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.016) { [weak self] in
                guard let self = self, self.pendingScroll else { return }
                self.emitScroll()
            }
        }
    }

    private func emitScroll() {
        pendingScroll = false
        lastScrollEvent = ProcessInfo.processInfo.systemUptime
        let o = scrollView.contentOffset, c = scrollView.contentSize, b = scrollView.bounds.size
        send(.scroll, ["x": Double(o.x), "y": Double(o.y), "content_width": Double(c.width), "content_height": Double(c.height),
                       "viewport_width": Double(b.width), "viewport_height": Double(b.height)])
    }

    override func resetForReuse() {
        super.resetForReuse()
        pendingScroll = false
        scrollView.setContentOffset(.zero, animated: false)
    }
}
