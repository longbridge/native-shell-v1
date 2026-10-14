// What a host app registers for examples/watchlist: a quote module that pushes
// ticks while a script listens, the signed-in user, and the app's own stock
// cell. Prices are fictional and move in a random walk.

import AppKit
import NativeShellMac

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

/// The example app's own colors for its native cell, in both appearances. The
/// shell's accent comes from the system accent color (`AppDelegate`).
enum AppPalette {
    static let foreground = dynamic(light: 0x0A0A0A, dark: 0xFAFAFA)
    static let mutedForeground = dynamic(light: 0x737373, dark: 0xA3A3A3)
    static let pressed = dynamic(light: 0xF5F5F5, dark: 0x262626)
    static let gain = dynamic(light: 0x15803D, dark: 0x4ADE80)
    static let loss = dynamic(light: 0xDC2626, dark: 0xF87171)

    private static func dynamic(light: UInt32, dark: UInt32) -> NSColor {
        NSColor(name: nil) { appearance in
            rgb(appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light)
        }
    }

    static func rgb(_ v: UInt32) -> NSColor {
        NSColor(srgbRed: CGFloat((v >> 16) & 0xFF) / 255, green: CGFloat((v >> 8) & 0xFF) / 255, blue: CGFloat(v & 0xFF) / 255, alpha: 1)
    }
}

/// `demo-stock-cell`: the app's own native quote row, placed by scripts.
///
/// Symbol and name lead; price and the day's change trail, right-aligned with
/// tabular digits so rows compare at a glance. The change carries its sign, so
/// gain and loss never depend on color alone.
final class StockCell: NSObject, NSHHostComponent {
    static let componentName = "demo-stock-cell"
    static let events = ["select"]
    static let propsTypeScript = "{ symbol: string; name: string; last?: number | null; change_percent?: number | null }"
    var eventSink: NSHEventSink?

    private static let inset: CGFloat = 16
    private static let lineGap: CGFloat = 2
    private static let verticalPadding: CGFloat = 12
    private static let minHeight: CGFloat = 64

    // The program around the cell uses iOS text sizes; the cell matches them.
    private static let titleFont = NSFont.systemFont(ofSize: 17, weight: .semibold)
    private static let priceFont = NSFont.monospacedDigitSystemFont(ofSize: 17, weight: .medium)
    private static let detailFont = NSFont.systemFont(ofSize: 14)
    private static let changeFont = NSFont.monospacedDigitSystemFont(ofSize: 14, weight: .semibold)

    private static func lineHeight(_ f: NSFont) -> CGFloat { ceil(f.ascender - f.descender + f.leading) }

    private let root = CellView()
    private let symbol = NSTextField(labelWithString: "")
    private let name = NSTextField(labelWithString: "")
    private let last = NSTextField(labelWithString: "")
    private let change = NSTextField(labelWithString: "")
    private var currentSymbol = ""

    override required init() {
        super.init()
        for label in [symbol, name, last, change] { root.addSubview(label) }
        symbol.font = Self.titleFont
        symbol.textColor = AppPalette.foreground
        name.font = Self.detailFont
        name.textColor = AppPalette.mutedForeground
        name.lineBreakMode = .byTruncatingTail
        last.font = Self.priceFont
        last.textColor = AppPalette.foreground
        last.alignment = .right
        change.font = Self.changeFont
        change.alignment = .right
        root.onClick = { [weak self] in self?.clicked() }
        root.onLayout = { [weak self] in self?.layout() }
        root.setAccessibilityElement(true)
        root.setAccessibilityRole(.button)
    }

    var view: NSView { root }

    func update(props: [String: Any]) {
        currentSymbol = props["symbol"] as? String ?? ""
        let l = (props["last"] as? NSNumber)?.doubleValue
        let c = (props["change_percent"] as? NSNumber)?.doubleValue
        symbol.stringValue = currentSymbol
        name.stringValue = props["name"] as? String ?? ""
        last.stringValue = l.map { String(format: "%.2f", $0) } ?? "—"
        change.stringValue = c.map { String(format: "%@%.2f%%", $0 >= 0 ? "+" : "−", abs($0)) } ?? "—"
        change.textColor = c.map { $0 >= 0 ? AppPalette.gain : AppPalette.loss } ?? AppPalette.mutedForeground
        root.setAccessibilityLabel([currentSymbol, name.stringValue, l.map { String(format: "%.2f", $0) },
                                    c.map { String(format: "%@ %.2f percent", $0 >= 0 ? "up" : "down", abs($0)) }]
                .compactMap { $0 }.joined(separator: ", "))
        layout()
    }

    func prepareForReuse() {
        [symbol, name, last, change].forEach { $0.stringValue = "" }
        currentSymbol = ""
    }

    static func measure(props: [String: Any], maxWidth: CGFloat) -> CGSize {
        let height = lineHeight(titleFont) + lineGap + lineHeight(detailFont) + 2 * verticalPadding
        return CGSize(width: maxWidth.isFinite ? maxWidth : 320, height: max(minHeight, ceil(height)))
    }

    private func layout() {
        let b = root.bounds
        guard b.width > 0 else { return }
        let th = Self.lineHeight(Self.titleFont), dh = Self.lineHeight(Self.detailFont)
        let top = max(Self.verticalPadding, (b.height - th - Self.lineGap - dh) / 2)
        let second = top + th + Self.lineGap
        let trailingWidth = ceil(max(last.fittingSize.width, change.fittingSize.width))
        let leadingWidth = b.width - 2 * Self.inset - trailingWidth - 12
        symbol.frame = CGRect(x: Self.inset, y: top, width: leadingWidth, height: th)
        name.frame = CGRect(x: Self.inset, y: second, width: leadingWidth, height: dh)
        last.frame = CGRect(x: b.width - Self.inset - trailingWidth, y: top, width: trailingWidth, height: th)
        change.frame = CGRect(x: b.width - Self.inset - trailingWidth, y: second, width: trailingWidth, height: dh)
    }

    private func clicked() {
        eventSink?.emit("select", payload: ["symbol": currentSymbol])
    }
}

/// The cell's root: lays its labels out whenever the renderer resizes it, and
/// shows the pressed state the rest of the app's rows show.
private final class CellView: NSView {
    var onLayout: (() -> Void)?
    var onClick: (() -> Void)?
    private var pressed = false { didSet { updateBackground() } }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    override var isFlipped: Bool { true }

    override func layout() {
        super.layout()
        onLayout?()
    }

    private func updateBackground() {
        layer?.backgroundColor = pressed ? AppPalette.pressed.cgColor(for: effectiveAppearance) : nil
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateBackground()
    }

    override func mouseDown(with event: NSEvent) { pressed = true }

    override func mouseDragged(with event: NSEvent) {
        pressed = bounds.contains(convert(event.locationInWindow, from: nil))
    }

    override func mouseUp(with event: NSEvent) {
        let inside = bounds.contains(convert(event.locationInWindow, from: nil))
        pressed = false
        if inside { onClick?() }
    }

    override func accessibilityPerformPress() -> Bool {
        onClick?()
        return true
    }
}

private extension NSColor {
    func cgColor(for appearance: NSAppearance) -> CGColor {
        var out = cgColor
        appearance.performAsCurrentDrawingAppearance { out = self.cgColor }
        return out
    }
}
