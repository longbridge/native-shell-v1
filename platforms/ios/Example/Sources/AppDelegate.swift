// The example app: a launcher listing the repository's example programs. A
// program opens full screen, pushed on the navigation stack, and the edge
// swipe returns to the list. `-example <name>` on launch opens one directly
// (UI tests use it); `-reset` starts it with empty storage.

import NativeShell
import UIKit

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?

    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        HostDemo.register(in: .shared)
        NSHRuntime.shared.themeProvider = { appearance in
            // An app maps its own design tokens onto the shell's; here, the accent.
            let traits = UITraitCollection(userInterfaceStyle: appearance == .dark ? .dark : .light)
            return ["primary": AppPalette.accent.resolvedColor(with: traits)]
        }
        let w = UIWindow(frame: UIScreen.main.bounds)
        w.tintColor = AppPalette.accent
        let list = ExampleListController()
        w.rootViewController = LauncherNavigationController(rootViewController: list)
        w.makeKeyAndVisible()
        window = w
        let args = ProcessInfo.processInfo.arguments
        if let i = args.firstIndex(of: "-example"), i + 1 < args.count {
            list.open(args[i + 1], animated: false)
        }
        return true
    }
}

/// One example in the launcher.
struct Example {
    let name: String
    let title: String
    let summary: String
    let symbol: String
    let tint: UIColor

    static let sections: [(title: String, examples: [Example])] = [
        ("Apps", [
            Example(name: "tasks", title: "Tasks", summary: "Lists, editing and undo", symbol: "checklist", tint: .systemBlue),
            Example(name: "notes", title: "Notes", summary: "Folders, checklists and search", symbol: "note.text", tint: .systemYellow),
            Example(name: "chat", title: "Chat", summary: "A thread and its composer", symbol: "bubble.left.and.bubble.right.fill", tint: .systemGreen),
            Example(name: "watchlist", title: "Watchlist", summary: "Host modules and a native cell", symbol: "chart.line.uptrend.xyaxis", tint: .systemIndigo),
            Example(name: "settings", title: "Settings", summary: "Grouped preferences and pages", symbol: "gearshape.fill", tint: .systemGray),
        ]),
        ("Reference", [
            Example(name: "components", title: "Components", summary: "Every control, in context", symbol: "square.grid.2x2.fill", tint: .systemOrange),
            Example(name: "gpui-todolist", title: "gpui-shell To-Do List", summary: "A desktop program, unchanged", symbol: "desktopcomputer", tint: .systemTeal),
        ]),
    ]
}

final class ExampleListController: UITableViewController {
    init() {
        super.init(style: .insetGrouped)
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        title = "Examples"
        navigationItem.largeTitleDisplayMode = .always
        navigationController?.navigationBar.prefersLargeTitles = true
        navigationItem.backButtonDisplayMode = .minimal
        tableView.register(UITableViewCell.self, forCellReuseIdentifier: "cell")
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        navigationController?.setNavigationBarHidden(false, animated: animated)
    }

    override func numberOfSections(in tableView: UITableView) -> Int { Example.sections.count }

    override func tableView(_ tableView: UITableView, titleForHeaderInSection section: Int) -> String? { Example.sections[section].title }

    override func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int { Example.sections[section].examples.count }

    override func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let example = Example.sections[indexPath.section].examples[indexPath.row]
        let cell = tableView.dequeueReusableCell(withIdentifier: "cell", for: indexPath)
        var content = UIListContentConfiguration.subtitleCell()
        content.text = example.title
        content.textProperties.font = .preferredFont(forTextStyle: .body)
        content.secondaryText = example.summary
        content.secondaryTextProperties.color = .secondaryLabel
        content.secondaryTextProperties.font = .preferredFont(forTextStyle: .subheadline)
        content.textToSecondaryTextVerticalPadding = 2
        content.image = Self.tile(example)
        content.imageToTextPadding = 14
        content.directionalLayoutMargins = NSDirectionalEdgeInsets(top: 10, leading: 0, bottom: 10, trailing: 0)
        cell.contentConfiguration = content
        cell.accessoryType = .disclosureIndicator
        cell.accessibilityIdentifier = "example-\(example.name)"
        return cell
    }

    override func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
        tableView.deselectRow(at: indexPath, animated: true)
        open(Example.sections[indexPath.section].examples[indexPath.row].name, animated: true)
    }

    /// The example's symbol, white on a rounded tile of its color, as Settings draws its rows.
    private static func tile(_ example: Example) -> UIImage {
        let size = CGSize(width: 32, height: 32)
        return UIGraphicsImageRenderer(size: size).image { _ in
            example.tint.setFill()
            UIBezierPath(roundedRect: CGRect(origin: .zero, size: size), cornerRadius: 8).fill()
            let config = UIImage.SymbolConfiguration(pointSize: 15, weight: .semibold)
            if let symbol = UIImage(systemName: example.symbol, withConfiguration: config)?.withTintColor(.white, renderingMode: .alwaysOriginal) {
                let s = symbol.size
                symbol.draw(in: CGRect(x: (size.width - s.width) / 2, y: (size.height - s.height) / 2, width: s.width, height: s.height))
            }
        }
    }

    func open(_ name: String, animated: Bool) {
        do {
            let (directory, appId) = try Programs.locate(name)
            let config = NSHInstanceConfig(appId: appId)
            let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("programs").appendingPathComponent(appId)
            if ProcessInfo.processInfo.arguments.contains("-reset") { try? FileManager.default.removeItem(at: dir) }
            config.dataDirectory = dir
            let instance = try NSHInstance(config: config)
            let vc = ProgramController(instance: instance)
            vc.title = Example.sections.flatMap(\.examples).first { $0.name == name }?.title
            try instance.loadProgram(directory: directory)
            navigationController?.pushViewController(vc, animated: animated)
        } catch {
            let alert = UIAlertController(title: "Couldn’t open \(name)", message: error.localizedDescription, preferredStyle: .alert)
            alert.addAction(UIAlertAction(title: "OK", style: .default))
            present(alert, animated: true)
        }
    }
}

/// Keeps the edge swipe working for programs, which hide the bar and draw their own header.
final class LauncherNavigationController: UINavigationController, UIGestureRecognizerDelegate {
    override func viewDidLoad() {
        super.viewDidLoad()
        interactivePopGestureRecognizer?.delegate = self
    }

    func gestureRecognizerShouldBegin(_ g: UIGestureRecognizer) -> Bool {
        viewControllers.count > 1
    }
}

/// Hosts a program full screen. Its pages push inside it; at its first page the
/// edge swipe pops the program itself and the instance is destroyed.
final class ProgramController: NSHViewController, NSHInstanceDelegate {
    override init(instance: NSHInstance?, runtime: NSHRuntime = .shared) {
        super.init(instance: instance, runtime: runtime)
        instance?.delegate = self
    }

    required init?(coder: NSCoder) { fatalError("not supported") }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        navigationController?.setNavigationBarHidden(true, animated: animated)
    }

    override func didMove(toParent parent: UIViewController?) {
        super.didMove(toParent: parent)
        if parent == nil { instance?.destroy() }
    }

    func instance(_ instance: NSHInstance, didFailWithError message: String) {
        NSLog("native-shell error: %@", message)
    }
}

enum Programs {
    /// A bundled example's directory and the program's id from its manifest.
    static func locate(_ name: String) throws -> (URL, String) {
        guard let dir = Bundle.main.url(forResource: name, withExtension: nil, subdirectory: "examples") else {
            throw NSError(domain: "example", code: 1, userInfo: [NSLocalizedDescriptionKey: "example \(name) is not bundled"])
        }
        let manifest = try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("native-shell.json")))
        let id = (manifest as? [String: Any])?["id"] as? String ?? name
        return (dir, id)
    }
}
