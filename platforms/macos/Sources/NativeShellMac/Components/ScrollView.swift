// `Scroll`: an NSScrollView whose content the core lays out (`ContentSize`),
// with throttled scroll events. Children live in a flipped document view.

import AppKit
import NativeShellShared

/// A flipped plain view: the document of scroll and list views.
final class FlippedView: NSView {
    override var isFlipped: Bool { true }
}

final class NSHScrollView: NSHComponentView {
    let scrollView = NSScrollView()
    let document = FlippedView()
    private var horizontal = false
    private var lastScrollEvent: TimeInterval = 0
    private var pendingScroll = false

    override func commonInit() {
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.verticalScrollElasticity = .allowed
        scrollView.horizontalScrollElasticity = .none
        scrollView.documentView = document
        scrollView.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(self, selector: #selector(didScroll), name: NSView.boundsDidChangeNotification, object: scrollView.contentView)
        addSubview(scrollView)
    }

    override var childContainer: NSView { document }

    override func layout() {
        super.layout()
        if scrollView.frame != bounds { scrollView.frame = bounds }
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .contentSize:
            let a = value.floats ?? []
            let size = a.count == 2 ? CGSize(width: CGFloat(a[0]), height: CGFloat(a[1])) : .zero
            document.setFrameSize(size)
        case .horizontal:
            horizontal = value.bool ?? false
            scrollView.hasHorizontalScroller = horizontal
            scrollView.hasVerticalScroller = !horizontal
            scrollView.verticalScrollElasticity = horizontal ? .none : .allowed
            scrollView.horizontalScrollElasticity = horizontal ? .allowed : .none
        case .showsIndicators:
            let on = value.bool ?? true
            scrollView.hasVerticalScroller = on && !horizontal
            scrollView.hasHorizontalScroller = on && horizontal
        case .refreshEnabled, .refreshing:
            // There is no pull to refresh with a pointer; programs offer their own refresh action.
            break
        default:
            return super.applyProp(prop, value)
        }
        return true
    }

    /// The visible origin of the content.
    var contentOffset: CGPoint { scrollView.contentView.bounds.origin }

    func setContentOffset(_ p: CGPoint, animated: Bool) {
        let clip = scrollView.contentView
        if animated, window != nil, !Animator.reduceMotion() {
            NSAnimationContext.runAnimationGroup { ctx in
                ctx.duration = 0.3
                clip.animator().setBoundsOrigin(p)
            } completionHandler: { [weak self] in
                self?.scrollView.reflectScrolledClipView(clip)
            }
        } else {
            clip.setBoundsOrigin(p)
            scrollView.reflectScrolledClipView(clip)
        }
    }

    override func runCommand(_ command: Command, _ payload: [String: Any]) {
        guard command == .scrollTo else { return }
        layoutSubtreeIfNeeded()
        let viewport = scrollView.contentView.bounds.size
        let maxX = max(0, document.frame.width - viewport.width)
        let maxY = max(0, document.frame.height - viewport.height)
        let x = min(max(0, payload.cgFloat("x") ?? contentOffset.x), maxX)
        let y = min(max(0, payload.cgFloat("y") ?? contentOffset.y), maxY)
        setContentOffset(CGPoint(x: x, y: y), animated: payload.bool("animated") ?? true)
    }

    // Scroll events at most every 16 ms; the last position is always delivered.
    @objc private func didScroll() {
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
        let o = contentOffset, c = document.frame.size, b = scrollView.contentView.bounds.size
        send(.scroll, ["x": Double(o.x), "y": Double(o.y), "content_width": Double(c.width), "content_height": Double(c.height),
                       "viewport_width": Double(b.width), "viewport_height": Double(b.height)])
    }

    override func resetForReuse() {
        super.resetForReuse()
        pendingScroll = false
        setContentOffset(.zero, animated: false)
    }
}
