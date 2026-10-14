// The sidebar listing the examples and the pane hosting the selected program.

import AppKit
import NativeShellMac

/// One example in the sidebar.
struct Example {
    let name: String
    let title: String
    let summary: String
    let symbol: String

    static let sections: [(title: String, examples: [Example])] = [
        ("Apps", [
            Example(name: "tasks", title: "Tasks", summary: "Lists, editing and undo", symbol: "checklist"),
            Example(name: "chat", title: "Chat", summary: "A thread and its composer", symbol: "bubble.left.and.bubble.right"),
            Example(name: "watchlist", title: "Watchlist", summary: "Host modules and a native cell", symbol: "chart.line.uptrend.xyaxis"),
            Example(name: "settings", title: "Settings", summary: "Grouped preferences and pages", symbol: "gearshape"),
        ]),
        ("Reference", [
            Example(name: "components", title: "Components", summary: "Every control, in context", symbol: "square.grid.2x2"),
            Example(name: "gpui-todolist", title: "gpui-shell To-Do List", summary: "A desktop program, unchanged", symbol: "desktopcomputer"),
        ]),
    ]

    static var all: [Example] { sections.flatMap(\.examples) }
}

final class LauncherSplitController: NSSplitViewController {
    private let sidebar = SidebarController()
    private let detail = DetailController()
    private let reset: Bool

    init(reset: Bool) {
        self.reset = reset
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    var program: ProgramController? { detail.program }

    override func viewDidLoad() {
        super.viewDidLoad()
        let side = NSSplitViewItem(sidebarWithViewController: sidebar)
        side.minimumThickness = 200
        side.maximumThickness = 280
        side.canCollapse = true
        let main = NSSplitViewItem(viewController: detail)
        main.minimumThickness = 520
        addSplitViewItem(side)
        addSplitViewItem(main)
        sidebar.onSelect = { [weak self] in self?.open($0) }
    }

    func select(_ name: String) {
        sidebar.select(name)
        open(name)
    }

    private func open(_ name: String) {
        guard let example = Example.all.first(where: { $0.name == name }) else { return }
        view.window?.title = example.title
        detail.show(example, reset: reset)
    }
}

/// A source list of the examples, grouped like the iOS launcher.
final class SidebarController: NSViewController, NSTableViewDataSource, NSTableViewDelegate {
    var onSelect: ((String) -> Void)?
    private let table = NSTableView()
    private var selecting = false

    /// Rows: a section title, or an example.
    private let rows: [Any] = Example.sections.flatMap { [$0.title as Any] + $0.examples.map { $0 as Any } }

    override func loadView() {
        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        table.style = .sourceList
        table.headerView = nil
        table.rowSizeStyle = .default
        table.addTableColumn(NSTableColumn(identifier: .init("example")))
        table.dataSource = self
        table.delegate = self
        table.setAccessibilityLabel("Examples")
        scroll.documentView = table
        view = scroll
    }

    func numberOfRows(in tableView: NSTableView) -> Int { rows.count }

    func tableView(_ tableView: NSTableView, isGroupRow row: Int) -> Bool { rows[row] is String }

    func tableView(_ tableView: NSTableView, shouldSelectRow row: Int) -> Bool { rows[row] is Example }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        if let title = rows[row] as? String {
            let label = NSTextField(labelWithString: title)
            let cell = NSTableCellView()
            cell.textField = label
            cell.addSubview(label)
            label.translatesAutoresizingMaskIntoConstraints = false
            NSLayoutConstraint.activate([
                label.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 2),
                label.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            ])
            return cell
        }
        guard let example = rows[row] as? Example else { return nil }
        let cell = NSTableCellView()
        let image = NSImageView(image: NSImage(systemSymbolName: example.symbol, accessibilityDescription: nil) ?? NSImage())
        image.symbolConfiguration = NSImage.SymbolConfiguration(textStyle: .body)
        let label = NSTextField(labelWithString: example.title)
        label.lineBreakMode = .byTruncatingTail
        cell.imageView = image
        cell.textField = label
        cell.toolTip = example.summary
        cell.setAccessibilityIdentifier("example-\(example.name)")
        for v in [image, label] {
            v.translatesAutoresizingMaskIntoConstraints = false
            cell.addSubview(v)
        }
        NSLayoutConstraint.activate([
            image.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 2),
            image.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            image.widthAnchor.constraint(equalToConstant: 20),
            label.leadingAnchor.constraint(equalTo: image.trailingAnchor, constant: 6),
            label.trailingAnchor.constraint(lessThanOrEqualTo: cell.trailingAnchor, constant: -2),
            label.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
        ])
        return cell
    }

    func select(_ name: String) {
        guard let row = rows.firstIndex(where: { ($0 as? Example)?.name == name }) else { return }
        selecting = true
        table.selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
        selecting = false
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        guard !selecting, let example = rows[safe: table.selectedRow] as? Example else { return }
        onSelect?(example.name)
    }
}

private extension Array {
    subscript(safe i: Int) -> Element? { i >= 0 && i < count ? self[i] : nil }
}

/// Hosts the selected program, or says why it could not open.
final class DetailController: NSViewController {
    private(set) var program: ProgramController?
    private let message = NSTextField(wrappingLabelWithString: "")

    override func loadView() {
        let v = NSView()
        message.alignment = .center
        message.textColor = .secondaryLabelColor
        message.font = .systemFont(ofSize: NSFont.systemFontSize)
        message.isHidden = true
        message.translatesAutoresizingMaskIntoConstraints = false
        v.addSubview(message)
        NSLayoutConstraint.activate([
            message.centerXAnchor.constraint(equalTo: v.centerXAnchor),
            message.centerYAnchor.constraint(equalTo: v.centerYAnchor),
            message.widthAnchor.constraint(lessThanOrEqualToConstant: 360),
        ])
        view = v
    }

    func show(_ example: Example, reset: Bool) {
        if let old = program {
            old.view.removeFromSuperview()
            old.removeFromParent()
            old.instance?.destroy()
            program = nil
        }
        do {
            let (sources, appId) = try Programs.sources(example.name)
            let config = NSHInstanceConfig(appId: appId)
            let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("native-shell-examples").appendingPathComponent(appId)
            if reset { try? FileManager.default.removeItem(at: dir) }
            config.dataDirectory = dir
            let instance = try NSHInstance(config: config)
            let vc = ProgramController(instance: instance)
            addChild(vc)
            vc.view.frame = view.bounds
            vc.view.autoresizingMask = [.width, .height]
            view.addSubview(vc.view, positioned: .below, relativeTo: message)
            try instance.loadSources(sources)
            program = vc
            message.isHidden = true
        } catch {
            message.stringValue = "Couldn’t open \(example.title).\n\(error.localizedDescription)"
            message.isHidden = false
        }
    }
}

/// One running program.
final class ProgramController: NSHViewController, NSHInstanceDelegate {
    override init(instance: NSHInstance?, runtime: NSHRuntime = .shared) {
        super.init(instance: instance, runtime: runtime)
        instance?.delegate = self
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    func instance(_ instance: NSHInstance, didFailWithError message: String) {
        NSLog("native-shell error: %@", message)
    }

    /// Presses the view with accessibility identifier `id` (automation).
    func tap(_ id: String) {
        var roots: [NSView] = [view]
        roots += view.window?.sheets.compactMap(\.contentView) ?? []
        for root in roots {
            if let v = find(id, in: root) {
                _ = v.accessibilityPerformPress()
                return
            }
        }
        NSLog("no view %@", id)
    }

    private func find(_ id: String, in v: NSView) -> NSView? {
        if v.accessibilityIdentifier() == id { return v }
        for s in v.subviews { if let f = find(id, in: s) { return f } }
        return nil
    }
}

enum Programs {
    /// `{manifest, files}` for a bundled example directory, and the program's id.
    static func sources(_ name: String) throws -> (String, String) {
        guard let dir = Bundle.main.url(forResource: name, withExtension: nil, subdirectory: "examples") else {
            throw NSError(domain: "example", code: 1, userInfo: [NSLocalizedDescriptionKey: "The example “\(name)” is not bundled with the app."])
        }
        let manifest = try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("native-shell.json")))
        var files: [String: String] = [:]
        let e = FileManager.default.enumerator(at: dir, includingPropertiesForKeys: nil)
        while let url = e?.nextObject() as? URL {
            guard url.pathExtension == "js" else { continue }
            files[String(url.path.dropFirst(dir.path.count + 1))] = try String(contentsOf: url, encoding: .utf8)
        }
        let data = try JSONSerialization.data(withJSONObject: ["manifest": manifest, "files": files])
        let id = (manifest as? [String: Any])?["id"] as? String ?? name
        return (String(decoding: data, as: UTF8.self), id)
    }
}
