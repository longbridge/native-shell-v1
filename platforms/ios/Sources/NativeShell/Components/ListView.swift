// `List` / `ListRow`: a UICollectionView laid out from the core's `RowOffsets`.
// The core renders only a window of rows (`ListRow` children carrying their
// `RowIndex`); cells host the row for their index, or stay empty until the
// core renders it. The visible range plus `Prefetch` is reported with
// `RangeChange` whenever it changes, which moves the core's window.

import NativeShellShared
import UIKit

final class NSHListRowView: NSHComponentView {
    private(set) var rowIndex: Int = -1

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        guard prop == .rowIndex else { return super.applyProp(prop, value) }
        let old = rowIndex
        rowIndex = value.int ?? -1
        if old != rowIndex { (parentComponent as? NSHListView)?.rowIndexChanged(self, from: old) }
        return true
    }
}

final class NSHListView: NSHComponentView, UICollectionViewDataSource, UICollectionViewDelegate {
    private let layout = OffsetsLayout()
    private(set) var collectionView: UICollectionView!
    private(set) var itemCount = 0
    private var prefetch = 0
    private var rowsByIndex: [Int: NSHListRowView] = [:]
    private(set) var reportedRange: (first: Int, last: Int)?
    private var needsReload = false
    private var lastRangeEvent: TimeInterval = 0
    private var rangeTimerPending = false
    /// The last `ScrollToIndex`, re-aimed when new row offsets arrive: rows the
    /// core had not measured yet were placed at estimated heights.
    private var pendingScroll: (index: Int, position: String?, y: CGFloat, corrections: Int)?
    private var offsetsChanged = false
    private var scrollAnimating = false
    private static let maxScrollCorrections = 8

    override func commonInit() {
        collectionView = UICollectionView(frame: .zero, collectionViewLayout: layout)
        collectionView.backgroundColor = .clear
        collectionView.dataSource = self
        collectionView.delegate = self
        collectionView.contentInsetAdjustmentBehavior = .never
        collectionView.alwaysBounceVertical = true
        collectionView.register(RowCell.self, forCellWithReuseIdentifier: "row")
        addSubview(collectionView)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        if collectionView.frame != bounds {
            collectionView.frame = bounds
            layout.invalidateLayout()
            collectionView.layoutIfNeeded()
            // A shrinking viewport (the keyboard rising) keeps a pinned end in view.
            keepEndInView()
            hostVisibleRows()
            reportRange()
        }
    }

    // Rows are not subviews: cells host them.
    override func mountChildView(_ child: NSHComponentView, at index: Int) {
        guard let row = child as? NSHListRowView else { return }
        if row.rowIndex >= 0 { rowsByIndex[row.rowIndex] = row }
        needsReload = true
        context?.needsBatchEnd(self)
    }

    override func unmountChildView(_ child: NSHComponentView) {
        if let row = child as? NSHListRowView, rowsByIndex[row.rowIndex] === row { rowsByIndex[row.rowIndex] = nil }
        child.removeFromSuperview()
        needsReload = true
    }

    func rowIndexChanged(_ row: NSHListRowView, from old: Int) {
        if rowsByIndex[old] === row { rowsByIndex[old] = nil }
        if row.rowIndex >= 0 { rowsByIndex[row.rowIndex] = row }
        needsReload = true
        context?.needsBatchEnd(self)
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .itemCount:
            let n = max(0, value.int ?? 0)
            if n != itemCount {
                itemCount = n
                collectionView.reloadData()
            }
        case .rowOffsets:
            layout.offsets = (value.floats ?? []).map { CGFloat($0) }
            layout.invalidateLayout()
            offsetsChanged = true
        case .prefetch:
            prefetch = max(0, value.int ?? 0)
        case .listAlignment:
            bottomAligned = value.int == 1
            pinnedToEnd = bottomAligned
        case .showsIndicators:
            collectionView.showsVerticalScrollIndicator = value.bool ?? true
        default:
            return super.applyProp(prop, value)
        }
        needsReload = true
        return true
    }

    override func batchDidEnd() {
        guard needsReload else { return }
        needsReload = false
        // Re-host rows in the visible cells; cells that come later pick them up in cellForItemAt.
        layoutIfNeeded()
        collectionView.layoutIfNeeded()
        keepEndInView()
        if offsetsChanged {
            offsetsChanged = false
            correctPendingScroll()
        }
        hostVisibleRows()
        reportRange()
    }

    /// Puts each rendered row in the visible cell for its index. A relayout can
    /// briefly show two cells for one index; the one UIKit drops takes the row
    /// with it when reused, so rows are re-hosted after every relayout.
    private func hostVisibleRows() {
        for case let cell as RowCell in collectionView.visibleCells {
            guard let ip = collectionView.indexPath(for: cell) else { continue }
            cell.host(rowsByIndex[ip.item])
        }
    }

    // MARK: Data source

    func collectionView(_ cv: UICollectionView, numberOfItemsInSection section: Int) -> Int { itemCount }

    func collectionView(_ cv: UICollectionView, cellForItemAt indexPath: IndexPath) -> UICollectionViewCell {
        let cell = cv.dequeueReusableCell(withReuseIdentifier: "row", for: indexPath) as! RowCell
        cell.host(rowsByIndex[indexPath.item])
        return cell
    }

    func collectionView(_ cv: UICollectionView, didEndDisplaying cell: UICollectionViewCell, forItemAt indexPath: IndexPath) {
        (cell as? RowCell)?.host(nil)
        // Another cell may be showing this index now (see hostVisibleRows).
        (cv.cellForItem(at: indexPath) as? RowCell)?.host(rowsByIndex[indexPath.item])
    }

    func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
        pendingScroll = nil
    }

    func scrollViewDidEndScrollingAnimation(_ scrollView: UIScrollView) {
        scrollAnimating = false
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        // Only the user moves the pin; the list's own scrolling to the end does not.
        if scrollView.isTracking || scrollView.isDragging || scrollView.isDecelerating { userScrolled() }
        reportRange()
        // Scroll events from a list carry the same payload as a scroll view's.
        if listens(EventMask.scroll) {
            let o = scrollView.contentOffset, c = scrollView.contentSize, b = scrollView.bounds.size
            send(.scroll, ["x": Double(o.x), "y": Double(o.y), "content_width": Double(c.width), "content_height": Double(c.height),
                           "viewport_width": Double(b.width), "viewport_height": Double(b.height)])
        }
    }

    // MARK: Bottom alignment

    /// GPUI's `ListAlignment::Bottom` (a chat thread): opens at the end and
    /// follows it while the user is there.
    private(set) var bottomAligned = false
    /// The user is at the end (within a few points), so new rows and a shorter
    /// viewport keep the end in view.
    private(set) var pinnedToEnd = false

    private var maxOffset: CGFloat {
        max(0, layout.total - collectionView.bounds.height)
    }

    func userScrolled() {
        guard bottomAligned else { return }
        pinnedToEnd = collectionView.contentOffset.y >= maxOffset - 8
    }

    private func keepEndInView() {
        guard bottomAligned, pinnedToEnd, itemCount > 0, collectionView.bounds.height > 0 else { return }
        let y = maxOffset
        if abs(collectionView.contentOffset.y - y) > 0.5 {
            collectionView.setContentOffset(CGPoint(x: 0, y: y), animated: false)
        }
    }

    // MARK: Range

    /// Visible rows `[first, last)` widened by `Prefetch` on both sides.
    var visibleRange: (first: Int, last: Int) {
        guard itemCount > 0 else { return (0, 0) }
        let top = collectionView.contentOffset.y
        let bottom = top + max(collectionView.bounds.height, 1)
        let first = layout.row(at: top)
        let last = min(itemCount, layout.row(at: bottom - 0.5) + 1)
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

    override func runCommand(_ command: Command, _ payload: [String: Any]) {
        layoutIfNeeded()
        switch command {
        case .scrollToIndex:
            guard itemCount > 0, let index = payload.int("index") else { return }
            let i = max(0, min(itemCount - 1, index)), position = payload.string("position")
            let y = scrollOffset(toRow: i, position)
            pendingScroll = (i, position, y, 0)
            offsetsChanged = false
            setScrollOffset(y, animated: payload.bool("animated") ?? true)
        case .scrollTo:
            pendingScroll = nil
            let y = max(0, min(payload.cgFloat("y") ?? 0, max(0, layout.total - collectionView.bounds.height)))
            collectionView.setContentOffset(CGPoint(x: 0, y: y), animated: payload.bool("animated") ?? true)
        default:
            break
        }
    }

    /// The content offset that puts row `i` at the top, center or bottom of the viewport.
    private func scrollOffset(toRow i: Int, _ position: String?) -> CGFloat {
        let rowTop = layout.top(of: i), rowH = layout.top(of: i + 1) - rowTop
        let h = collectionView.bounds.height
        let y: CGFloat
        switch position {
        case "center": y = rowTop - (h - rowH) / 2
        case "bottom": y = rowTop + rowH - h
        default: y = rowTop
        }
        return max(0, min(y, max(0, layout.total - h)))
    }

    private func setScrollOffset(_ y: CGFloat, animated: Bool) {
        let animates = animated && abs(collectionView.contentOffset.y - y) > 0.5
        if animates { scrollAnimating = true }
        collectionView.setContentOffset(CGPoint(x: 0, y: y), animated: animates)
    }

    /// Measured rows replaced estimates, so the target row may have moved: aim
    /// again until it stays put (bounded, since every scroll measures new rows).
    private func correctPendingScroll() {
        guard let p = pendingScroll, p.index < itemCount else { return pendingScroll = nil }
        let y = scrollOffset(toRow: p.index, p.position)
        guard abs(y - p.y) > 0.5, p.corrections < Self.maxScrollCorrections else { return pendingScroll = nil }
        pendingScroll = (p.index, p.position, y, p.corrections + 1)
        // An animated scroll still under way is retargeted rather than cut short.
        setScrollOffset(y, animated: scrollAnimating)
    }

    override func resetForReuse() {
        super.resetForReuse()
        pendingScroll = nil
        offsetsChanged = false
        scrollAnimating = false
        rowsByIndex.removeAll()
        reportedRange = nil
        bottomAligned = false
        pinnedToEnd = false
        collectionView.setContentOffset(.zero, animated: false)
    }
}

/// A cell showing one row view. The row stays owned by the list: hosting only moves it between cells.
private final class RowCell: UICollectionViewCell {
    private weak var row: NSHListRowView?

    func host(_ new: NSHListRowView?) {
        if row === new, new?.superview === contentView { return }
        if let old = row, old.superview === contentView { old.removeFromSuperview() }
        row = new
        if let r = new {
            contentView.addSubview(r)
            r.applyFrame(CGRect(origin: .zero, size: CGSize(width: contentView.bounds.width, height: r.nodeFrame.height > 0 ? r.nodeFrame.height : contentView.bounds.height)))
        }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        if let r = row, r.superview === contentView, r.bounds.width != contentView.bounds.width {
            r.applyFrame(CGRect(x: 0, y: 0, width: contentView.bounds.width, height: r.bounds.height))
        }
    }

    override func prepareForReuse() {
        super.prepareForReuse()
        host(nil)
    }
}

/// Vertical layout straight from row offsets (`offsets[i]` is row i's top; the last element is the total height).
final class OffsetsLayout: UICollectionViewLayout {
    var offsets: [CGFloat] = []

    var count: Int { collectionView?.numberOfItems(inSection: 0) ?? 0 }
    var total: CGFloat { top(of: count) }

    /// Top of row `i`; rows missing from the offsets fall back to the last known row height.
    func top(of i: Int) -> CGFloat {
        if i < offsets.count { return offsets[i] }
        guard let last = offsets.last else { return CGFloat(i) * 44 }
        return last + CGFloat(i - (offsets.count - 1)) * 44
    }

    /// The row containing `y` (binary search).
    func row(at y: CGFloat) -> Int {
        let n = count
        guard n > 0 else { return 0 }
        var lo = 0, hi = n - 1
        while lo < hi {
            let mid = (lo + hi + 1) / 2
            if top(of: mid) <= y { lo = mid } else { hi = mid - 1 }
        }
        return lo
    }

    override var collectionViewContentSize: CGSize {
        CGSize(width: collectionView?.bounds.width ?? 0, height: total)
    }

    private func attributes(_ i: Int) -> UICollectionViewLayoutAttributes {
        let a = UICollectionViewLayoutAttributes(forCellWith: IndexPath(item: i, section: 0))
        let y = top(of: i)
        a.frame = CGRect(x: 0, y: y, width: collectionView?.bounds.width ?? 0, height: max(0, top(of: i + 1) - y))
        return a
    }

    override func layoutAttributesForElements(in rect: CGRect) -> [UICollectionViewLayoutAttributes]? {
        let n = count
        guard n > 0 else { return [] }
        var out: [UICollectionViewLayoutAttributes] = []
        var i = row(at: max(0, rect.minY))
        while i < n, top(of: i) < rect.maxY {
            out.append(attributes(i))
            i += 1
        }
        return out
    }

    override func layoutAttributesForItem(at indexPath: IndexPath) -> UICollectionViewLayoutAttributes? {
        attributes(indexPath.item)
    }

    override func shouldInvalidateLayout(forBoundsChange newBounds: CGRect) -> Bool {
        newBounds.width != collectionView?.bounds.width
    }
}
