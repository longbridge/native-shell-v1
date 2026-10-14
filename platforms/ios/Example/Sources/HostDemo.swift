// What a host app registers for examples/watchlist: a quote module that pushes
// ticks while a script listens, the signed-in user, and the app's own stock
// cell. Prices are fictional and move in a random walk.

import NativeShell
import UIKit

enum HostDemo {
    static func register(in runtime: NSHRuntime) {
        runtime.register(module: QuoteModule())
        runtime.register(module: UserModule())
        runtime.register(component: StockCell.self)
    }
}

/// `demo/quote`: `quote(symbol)`, `subscribe(symbols)`, `unsubscribe(symbols)`, event `tick`.
/// Ticks flow only while a script listens: the SDK reports listener counts through `__listeners`.
final class QuoteModule: NSObject, NSHHostModule {
    let name = "demo/quote"
    let methods: [NSHHostMethod] = [
        .async("quote", typeScript: "(symbol: string) => Promise<Quote>"),
        .async("subscribe", typeScript: "(symbols: string[]) => Promise<void>"),
        .async("unsubscribe", typeScript: "(symbols: string[]) => Promise<void>"),
    ]
    let events = ["tick"]
    let typeScriptDeclarations = "interface Quote { symbol: string; last: number; prev_close: number; change_percent: number }"

    /// Previous close and today's opening move of each fictional instrument.
    private static let instruments: [String: (close: Double, move: Double)] = [
        "HALO.US": (182.40, 0.0124), "MAPL.US": (64.15, -0.0062), "KITE.US": (27.80, 0.0310), "ORCA.US": (311.05, -0.0185),
        "FERN.US": (18.92, 0.0040), "PINE.HK": (46.30, -0.0015), "TIDE.HK": (9.87, 0.0205), "LUMA.US": (121.60, -0.0090),
    ]

    private var prices: [String: Double] = QuoteModule.instruments.mapValues { $0.close * (1 + $0.move) }
    private var subscribed: [String] = []
    private var timer: Timer?
    private var listening = false

    private func quote(_ symbol: String) -> [String: Any]? {
        guard let close = Self.instruments[symbol]?.close, let last = prices[symbol] else { return nil }
        return ["symbol": symbol, "last": (last * 100).rounded() / 100, "prev_close": close,
                "change_percent": ((last - close) / close * 10000).rounded() / 100]
    }

    /// `__listeners [event, count]`: start the feed on the first listener, stop it after the last.
    func callSync(_ method: String, args: [Any], context: NSHCallContext) throws -> Any {
        guard method == "__listeners", args.first as? String == "tick", let count = args.last as? Int else { return NSNull() }
        DispatchQueue.main.async {
            self.listening = count > 0
            self.updateTicking()
        }
        return NSNull()
    }

    func call(_ method: String, args: [Any], context: NSHCallContext, respond: NSHResponder) {
        // A short delay stands in for the network round trip.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) {
            let symbols = args.first as? [String] ?? []
            switch method {
            case "quote":
                let symbol = args.first as? String ?? ""
                if let q = self.quote(symbol) { respond.resolve(q) } else { respond.reject("No quote for \(symbol)") }
            case "subscribe":
                self.subscribed += symbols.filter { !self.subscribed.contains($0) }
                self.updateTicking()
                respond.resolve(nil)
            case "unsubscribe":
                self.subscribed.removeAll { symbols.contains($0) }
                self.updateTicking()
                respond.resolve(nil)
            default:
                respond.reject("demo/quote has no method \(method)")
            }
        }
    }

    private func updateTicking() {
        guard listening, !subscribed.isEmpty else {
            timer?.invalidate()
            timer = nil
            return
        }
        guard timer == nil else { return }
        timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            guard let self = self, let symbol = self.subscribed.randomElement(), let old = self.prices[symbol] else { return }
            self.prices[symbol] = old * (1 + Double.random(in: -0.004 ... 0.004))
            if let q = self.quote(symbol) { NSHRuntime.shared.emit(module: self.name, event: "tick", payload: q) }
        }
    }
}

/// `demo/user`: `current_user()`, answered synchronously from memory.
final class UserModule: NSObject, NSHHostModule {
    let name = "demo/user"
    let methods: [NSHHostMethod] = [.sync("current_user", typeScript: "() => { id: string; name: string } | null")]
    let requiresPermission = false

    func callSync(_ method: String, args: [Any], context: NSHCallContext) throws -> Any {
        ["id": "u-1", "name": "Ada Lovelace"]
    }
}

/// The example app's own colors. The shell's theme takes its accent from here
/// (`AppDelegate`), and the native stock cell draws with the rest, so both
/// sides of the screen come from one palette in either appearance.
enum AppPalette {
    static let accent = UIColor { $0.userInterfaceStyle == .dark ? UIColor(rgb: 0x60A5FA) : UIColor(rgb: 0x2563EB) }
    static let foreground = UIColor { $0.userInterfaceStyle == .dark ? UIColor(rgb: 0xFAFAFA) : UIColor(rgb: 0x0A0A0A) }
    static let mutedForeground = UIColor { $0.userInterfaceStyle == .dark ? UIColor(rgb: 0xA3A3A3) : UIColor(rgb: 0x737373) }
    static let pressed = UIColor { $0.userInterfaceStyle == .dark ? UIColor(rgb: 0x262626) : UIColor(rgb: 0xF5F5F5) }
    static let gain = UIColor { $0.userInterfaceStyle == .dark ? UIColor(rgb: 0x4ADE80) : UIColor(rgb: 0x15803D) }
    static let loss = UIColor { $0.userInterfaceStyle == .dark ? UIColor(rgb: 0xF87171) : UIColor(rgb: 0xDC2626) }
}

extension UIColor {
    convenience init(rgb: UInt32) {
        self.init(red: CGFloat((rgb >> 16) & 0xFF) / 255, green: CGFloat((rgb >> 8) & 0xFF) / 255, blue: CGFloat(rgb & 0xFF) / 255, alpha: 1)
    }
}

/// `demo-stock-cell`: the app's own native quote row, placed by scripts.
///
/// Symbol and name lead; price and the day's change trail, right-aligned with
/// tabular digits so rows compare at a glance. The change carries its sign, so
/// gain and loss never depend on color alone. Text follows Dynamic Type.
final class StockCell: NSObject, NSHHostComponent {
    static let componentName = "demo-stock-cell"
    static let events = ["select"]
    static let propsTypeScript = "{ symbol: string; name: string; last?: number | null; change_percent?: number | null }"
    var eventSink: NSHEventSink?

    private static let inset: CGFloat = 16
    private static let lineGap: CGFloat = 2
    private static let verticalPadding: CGFloat = 12
    private static let minHeight: CGFloat = 64

    private static var titleFont: UIFont { UIFontMetrics(forTextStyle: .body).scaledFont(for: .systemFont(ofSize: 17, weight: .semibold)) }
    private static var priceFont: UIFont { UIFontMetrics(forTextStyle: .body).scaledFont(for: .monospacedDigitSystemFont(ofSize: 17, weight: .medium)) }
    private static var detailFont: UIFont { UIFontMetrics(forTextStyle: .subheadline).scaledFont(for: .systemFont(ofSize: 14)) }
    private static var changeFont: UIFont { UIFontMetrics(forTextStyle: .subheadline).scaledFont(for: .monospacedDigitSystemFont(ofSize: 14, weight: .semibold)) }

    private let root = CellView()
    private let symbol = UILabel()
    private let name = UILabel()
    private let last = UILabel()
    private let change = UILabel()
    private var currentSymbol = ""

    override required init() {
        super.init()
        for label in [symbol, name, last, change] {
            label.adjustsFontForContentSizeCategory = true
            root.addSubview(label)
        }
        symbol.textColor = AppPalette.foreground
        name.textColor = AppPalette.mutedForeground
        name.lineBreakMode = .byTruncatingTail
        last.textColor = AppPalette.foreground
        last.textAlignment = .right
        change.textAlignment = .right
        root.addTarget(self, action: #selector(tapped), for: .touchUpInside)
        root.onLayout = { [weak self] in self?.layout() }
        root.isAccessibilityElement = true
        root.accessibilityTraits = .button
    }

    var view: UIView { root }

    func update(props: [String: Any]) {
        currentSymbol = props["symbol"] as? String ?? ""
        let l = (props["last"] as? NSNumber)?.doubleValue
        let c = (props["change_percent"] as? NSNumber)?.doubleValue
        symbol.text = currentSymbol
        name.text = props["name"] as? String
        last.text = l.map { String(format: "%.2f", $0) } ?? "—"
        change.text = c.map { String(format: "%@%.2f%%", $0 >= 0 ? "+" : "−", abs($0)) } ?? "—"
        change.textColor = c.map { $0 >= 0 ? AppPalette.gain : AppPalette.loss } ?? AppPalette.mutedForeground
        root.accessibilityLabel = [currentSymbol, name.text, l.map { String(format: "%.2f", $0) },
                                   c.map { String(format: "%@ %.2f percent", $0 >= 0 ? "up" : "down", abs($0)) }]
            .compactMap { $0 }.joined(separator: ", ")
        layout()
    }

    func prepareForReuse() {
        [symbol, name, last, change].forEach { $0.text = nil }
        currentSymbol = ""
    }

    static func measure(props: [String: Any], maxWidth: CGFloat) -> CGSize {
        let height = titleFont.lineHeight + lineGap + detailFont.lineHeight + 2 * verticalPadding
        return CGSize(width: maxWidth.isFinite ? maxWidth : 320, height: max(minHeight, ceil(height)))
    }

    private func layout() {
        symbol.font = Self.titleFont
        last.font = Self.priceFont
        name.font = Self.detailFont
        change.font = Self.changeFont
        let b = root.bounds
        guard b.width > 0 else { return }
        let top = max(Self.verticalPadding, (b.height - Self.titleFont.lineHeight - Self.lineGap - Self.detailFont.lineHeight) / 2)
        let second = top + Self.titleFont.lineHeight + Self.lineGap
        let trailingWidth = max(last.intrinsicContentSize.width, change.intrinsicContentSize.width)
        let leadingWidth = b.width - 2 * Self.inset - trailingWidth - 12
        symbol.frame = CGRect(x: Self.inset, y: top, width: leadingWidth, height: Self.titleFont.lineHeight)
        name.frame = CGRect(x: Self.inset, y: second, width: leadingWidth, height: Self.detailFont.lineHeight)
        last.frame = CGRect(x: b.width - Self.inset - trailingWidth, y: top, width: trailingWidth, height: Self.titleFont.lineHeight)
        change.frame = CGRect(x: b.width - Self.inset - trailingWidth, y: second, width: trailingWidth, height: Self.detailFont.lineHeight)
    }

    @objc private func tapped() {
        eventSink?.emit("select", payload: ["symbol": currentSymbol])
    }
}

/// The cell's root: lays its labels out whenever the renderer resizes it, and
/// shows the pressed state the rest of the app's rows show.
private final class CellView: UIControl {
    var onLayout: (() -> Void)?

    override var isHighlighted: Bool {
        didSet { backgroundColor = isHighlighted ? AppPalette.pressed : .clear }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        onLayout?()
    }

    override func traitCollectionDidChange(_ previous: UITraitCollection?) {
        super.traitCollectionDidChange(previous)
        if previous?.preferredContentSizeCategory != traitCollection.preferredContentSizeCategory { onLayout?() }
    }
}
