// `List` / `ListRow`: an NSScrollView whose document is as tall as the core's
// `RowOffsets`. The core renders only a window of rows (`ListRow` children
// carrying their `RowIndex`) and recycles their nodes as the window moves, so
// the list places each row at its offset; rows not rendered yet leave their
// space empty. The visible range plus `Prefetch` is reported with
// `RangeChange` whenever it changes, which moves the core's window.

import AppKit
import NativeShellShared

final class NSHListRowView: NSHComponentView {
    private(set) var rowIndex: Int = -1
    /// The frame the core gave: its height counts; the list decides where the row goes.
    private(set) var coreFrame: CGRect = .zero

    override func applyFrame(_ rect: CGRect) {
        coreFrame = rect
        if let list = parentComponent as? NSHListView { list.setNeedsPlacement() } else { super.applyFrame(rect) }
    }

    /// Puts the row at its place in the list's document.
    func place(_ rect: CGRect) { super.applyFrame(rect) }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        guard prop == .rowIndex else { return super.applyProp(prop, value) }
        let old = rowIndex
        rowIndex = value.int ?? -1
        if old != rowIndex { (parentComponent as? NSHListView)?.rowIndexChanged(self) }
        return true
    }
}

final class NSHListView: NSHComponentView {
    let scrollView = NSScrollView()
    let document = FlippedView()
    let offsets = RowOffsets()
    private(set) var itemCount = 0
    private var prefetch = 0
    /// GPUI's `ListAlignment::Bottom`: open at the end and keep the end in view while the user is there.
    private(set) var bottomAligned = false
    /// Whether the end was in view before the current change (rows added, viewport resized).
    private var pinnedToEnd = true
    private(set) var reportedRange: (first: Int, last: Int)?
    private var needsPlacement = false
    private var lastRangeEvent: TimeInterval = 0
    private var rangeTimerPending = false

    override func commonInit() {
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.horizontalScrollElasticity = .none
        scrollView.documentView = document
        scrollView.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(self, selector: #selector(didScroll), name: NSView.boundsDidChangeNotification, object: scrollView.contentView)
        document.setAccessibilityRole(.list)
        addSubview(scrollView)
    }

    override var childContainer: NSView { document }

    override func layout() {
        super.layout()
        if scrollView.frame != bounds {
            let pinned = bottomAligned && isAtEnd
            scrollView.frame = bounds
            placeRows()
            if pinned { scrollToEnd() }
            reportRange()
        }
    }

    override func mountChildView(_ child: NSHComponentView, at index: Int) {
        // Rows are placed by offset, so subview order does not matter.
        document.addSubview(child)
        setNeedsPlacement()
    }

    func rowIndexChanged(_ row: NSHListRowView) { setNeedsPlacement() }

    func setNeedsPlacement() {
        // Remember where the user was before this batch changes the geometry.
        if !needsPlacement { pinnedToEnd = isAtEnd }
        needsPlacement = true
        context?.needsBatchEnd(self)
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .itemCount:
            itemCount = max(0, value.int ?? 0)
            offsets.count = itemCount
        case .rowOffsets:
            offsets.offsets = (value.floats ?? []).map { CGFloat($0) }
        case .prefetch:
            prefetch = max(0, value.int ?? 0)
        case .listAlignment:
            bottomAligned = value.int == 1
        case .showsIndicators:
            scrollView.hasVerticalScroller = value.bool ?? true
        default:
            return super.applyProp(prop, value)
        }
        setNeedsPlacement()
        return true
    }

    override func batchDidEnd() {
        guard needsPlacement else { return }
        needsPlacement = false
        placeRows()
        if bottomAligned, pinnedToEnd { scrollToEnd() }
        reportRange()
    }

    /// Within a few points of the last row (an empty or unscrolled list counts).
    private var isAtEnd: Bool {
        let clip = scrollView.contentView.bounds
        return clip.maxY >= document.frame.height - 4 || document.frame.height <= clip.height
    }

    private func scrollToEnd() {
        scroll(toY: offsets.total - scrollView.contentView.bounds.height, animated: false)
    }

    /// Sizes the document and puts every rendered row at its offset.
    private func placeRows() {
        // Batches end before AppKit's layout pass: size the scroll view now so the range is right.
        if scrollView.frame != bounds { scrollView.frame = bounds }
        let width = scrollView.contentSize.width
        document.setFrameSize(CGSize(width: width, height: offsets.total))
        for case let row as NSHListRowView in children where row.rowIndex >= 0 {
            let top = offsets.top(of: row.rowIndex)
            let h = row.coreFrame.height > 0 ? row.coreFrame.height : offsets.top(of: row.rowIndex + 1) - top
            row.place(CGRect(x: 0, y: top, width: width, height: h))
        }
    }

    @objc private func didScroll() {
        reportRange()
        // Scroll events from a list carry the same payload as a scroll view's.
        if listens(EventMask.scroll) {
            let o = scrollView.contentView.bounds.origin, c = document.frame.size, b = scrollView.contentView.bounds.size
            send(.scroll, ["x": Double(o.x), "y": Double(o.y), "content_width": Double(c.width), "content_height": Double(c.height),
                           "viewport_width": Double(b.width), "viewport_height": Double(b.height)])
        }
    }

    // MARK: Range

    /// Visible rows `[first, last)` widened by `Prefetch` on both sides.
    var visibleRange: (first: Int, last: Int) {
        guard itemCount > 0 else { return (0, 0) }
        let clip = scrollView.contentView.bounds
        let top = max(0, clip.minY)
        let bottom = top + max(clip.height, 1)
        let first = offsets.row(at: top)
        let last = min(itemCount, offsets.row(at: bottom - 0.5) + 1)
        return (max(0, first - prefetch), min(itemCount, last + prefetch))
    }

    /// Sends `RangeChange` when the window moves, at most every 16 ms (the last one always goes out).
    func reportRange() {
        guard itemCount > 0, bounds.height > 0 else { return }
        let r = visibleRange
        if let old = reportedRange, old == r { return }
        let now = ProcessInfo.processInfo.systemUptime
        if now - lastRangeEvent < 0.016 {
            if !rangeTimerPending {
                rangeTimerPending = true
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.016) { [weak self] in
                    self?.rangeTimerPending = false
                    self?.reportRange()
                }
            }
            return
        }
        lastRangeEvent = now
        reportedRange = r
        send(.rangeChange, ["first": r.first, "end": r.last])
    }

    func scroll(toY y: CGFloat, animated: Bool) {
        let clip = scrollView.contentView
        let p = CGPoint(x: 0, y: max(0, min(y, max(0, offsets.total - clip.bounds.height))))
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
        layoutSubtreeIfNeeded()
        let animated = payload.bool("animated") ?? true
        switch command {
        case .scrollToIndex:
            guard itemCount > 0, let index = payload.int("index") else { return }
            let i = max(0, min(itemCount - 1, index))
            let rowTop = offsets.top(of: i), rowH = offsets.top(of: i + 1) - rowTop
            let h = scrollView.contentView.bounds.height
            switch payload.string("position") {
            case "center": scroll(toY: rowTop - (h - rowH) / 2, animated: animated)
            case "bottom": scroll(toY: rowTop + rowH - h, animated: animated)
            default: scroll(toY: rowTop, animated: animated)
            }
        case .scrollTo:
            scroll(toY: payload.cgFloat("y") ?? 0, animated: animated)
        default:
            break
        }
    }

    override func resetForReuse() {
        super.resetForReuse()
        reportedRange = nil
        pinnedToEnd = true
        scroll(toY: 0, animated: false)
    }
}

/// Row geometry from the core's offsets (`offsets[i]` is row i's top; the last
/// element is the total height).
final class RowOffsets {
    var offsets: [CGFloat] = []
    var count = 0

    var total: CGFloat { top(of: count) }

    /// Top of row `i`; rows missing from the offsets fall back to the last known row height.
    func top(of i: Int) -> CGFloat {
        if i < offsets.count { return offsets[i] }
        guard let last = offsets.last else { return CGFloat(i) * 44 }
        return last + CGFloat(i - (offsets.count - 1)) * 44
    }

    /// The row containing `y` (binary search).
    func row(at y: CGFloat) -> Int {
        guard count > 0 else { return 0 }
        var lo = 0, hi = count - 1
        while lo < hi {
            let mid = (lo + hi + 1) / 2
            if top(of: mid) <= y { lo = mid } else { hi = mid - 1 }
        }
        return lo
    }
}
