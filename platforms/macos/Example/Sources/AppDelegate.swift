// The example app: one window with a sidebar listing the repository's example
// programs; the selected program runs in the detail pane. Switching programs
// destroys the previous instance.
//
// Launch arguments (used to capture screenshots):
//   -example <name>          select a program at launch
//   -appearance light|dark   force an appearance
//   -reset                   start programs with empty storage
//   -actions "<a>;<b>;…"     after launch: tap:<test id>, wait:<seconds>, back,
//                            capture:<png path> (the window), quit

import AppKit
import NativeShellMac

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var window: NSWindow!
    private var split: LauncherSplitController!

    func applicationDidFinishLaunching(_ notification: Notification) {
        let args = ProcessInfo.processInfo.arguments
        if let a = Self.argument("-appearance") { NSApp.appearance = NSAppearance(named: a == "dark" ? .darkAqua : .aqua) }
        HostDemo.register(in: .shared)
        // The shell's accent is the user's accent color, resolved per appearance.
        NSHRuntime.shared.themeProvider = { appearance in
            var accent = NSColor.controlAccentColor
            NSAppearance(named: appearance == .dark ? .darkAqua : .aqua)?.performAsCurrentDrawingAppearance {
                accent = NSColor.controlAccentColor.usingColorSpace(.sRGB) ?? .systemBlue
            }
            return ["primary": accent]
        }
        NSApp.mainMenu = MainMenu.make()

        split = LauncherSplitController(reset: args.contains("-reset"))
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1040, height: 760),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                          backing: .buffered, defer: false)
        window.title = "native-shell"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.contentViewController = split
        window.setContentSize(NSSize(width: 1040, height: 760))
        window.contentMinSize = NSSize(width: 760, height: 540)
        window.setFrameAutosaveName("Examples")
        window.tabbingMode = .disallowed
        if !window.setFrameUsingName("Examples") { window.center() }
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)

        split.select(Self.argument("-example") ?? Example.all[0].name)
        if let actions = Self.argument("-actions") { Automation(window: window, split: split).run(actions) }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }

    static func argument(_ name: String) -> String? {
        let args = ProcessInfo.processInfo.arguments
        guard let i = args.firstIndex(of: name), i + 1 < args.count else { return nil }
        return args[i + 1]
    }
}

/// The standard macOS menus, plus Go for program navigation.
enum MainMenu {
    static func make() -> NSMenu {
        let main = NSMenu()
        let name = "native-shell"
        func submenu(_ title: String, _ items: [NSMenuItem]) {
            let m = NSMenu(title: title)
            items.forEach(m.addItem)
            let host = NSMenuItem(title: title, action: nil, keyEquivalent: "")
            host.submenu = m
            main.addItem(host)
        }
        func item(_ title: String, _ action: Selector?, _ key: String = "", _ mods: NSEvent.ModifierFlags = .command) -> NSMenuItem {
            let i = NSMenuItem(title: title, action: action, keyEquivalent: key)
            i.keyEquivalentModifierMask = mods
            return i
        }
        submenu(name, [
            item("About \(name)", #selector(NSApplication.orderFrontStandardAboutPanel(_:))),
            .separator(),
            item("Hide \(name)", #selector(NSApplication.hide(_:)), "h"),
            item("Hide Others", #selector(NSApplication.hideOtherApplications(_:)), "h", [.command, .option]),
            item("Show All", #selector(NSApplication.unhideAllApplications(_:))),
            .separator(),
            item("Quit \(name)", #selector(NSApplication.terminate(_:)), "q"),
        ])
        submenu("File", [item("Close Window", #selector(NSWindow.performClose(_:)), "w")])
        submenu("Edit", [
            item("Undo", Selector(("undo:")), "z"),
            item("Redo", Selector(("redo:")), "z", [.command, .shift]),
            .separator(),
            item("Cut", #selector(NSText.cut(_:)), "x"),
            item("Copy", #selector(NSText.copy(_:)), "c"),
            item("Paste", #selector(NSText.paste(_:)), "v"),
            item("Select All", #selector(NSText.selectAll(_:)), "a"),
        ])
        submenu("View", [
            item("Show Sidebar", #selector(NSSplitViewController.toggleSidebar(_:)), "s", [.command, .control]),
            item("Enter Full Screen", #selector(NSWindow.toggleFullScreen(_:)), "f", [.command, .control]),
        ])
        submenu("Go", [item("Back", #selector(NSHViewController.goBack(_:)), "[")])
        let windowMenu = NSMenu(title: "Window")
        windowMenu.addItem(item("Minimize", #selector(NSWindow.performMiniaturize(_:)), "m"))
        windowMenu.addItem(item("Zoom", #selector(NSWindow.performZoom(_:))))
        windowMenu.addItem(.separator())
        windowMenu.addItem(item("Bring All to Front", #selector(NSApplication.arrangeInFront(_:))))
        let windowItem = NSMenuItem(title: "Window", action: nil, keyEquivalent: "")
        windowItem.submenu = windowMenu
        main.addItem(windowItem)
        NSApp.windowsMenu = windowMenu
        let help = NSMenu(title: "Help")
        let helpItem = NSMenuItem(title: "Help", action: nil, keyEquivalent: "")
        helpItem.submenu = help
        main.addItem(helpItem)
        NSApp.helpMenu = help
        return main
    }
}

/// Scripted steps after launch, for screenshots.
final class Automation {
    private let window: NSWindow
    private let split: LauncherSplitController

    init(window: NSWindow, split: LauncherSplitController) {
        self.window = window
        self.split = split
    }

    func run(_ script: String) {
        var steps = script.split(separator: ";").map { String($0).trimmingCharacters(in: .whitespaces) }
        func next() {
            guard !steps.isEmpty else { return }
            let step = steps.removeFirst()
            let (verb, arg) = step.firstIndex(of: ":").map { (String(step[..<$0]), String(step[step.index(after: $0)...])) } ?? (step, "")
            var delay = 0.3
            switch verb {
            case "wait": delay = Double(arg) ?? 1
            case "tap": split.program?.tap(arg)
            case "back": split.program?.performBack()
            case "capture": capture(arg)
            case "quit": NSApp.terminate(nil)
            default: NSLog("unknown step %@", step)
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: next)
        }
        // Let the first frame and the window's appearance settle.
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.5, execute: next)
        // A script never keeps the app running: a step that cannot finish (a locked
        // screen blocks window captures) must not stall a batch of captures.
        DispatchQueue.main.asyncAfter(deadline: .now() + 45) { NSApp.terminate(nil) }
    }

    /// The window (with its shadow trimmed) as a PNG, through the system screenshot tool.
    private func capture(_ path: String) {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
        p.arguments = ["-x", "-o", "-l", String(window.windowNumber), path]
        try? p.run()
        p.waitUntilExit()
    }
}
