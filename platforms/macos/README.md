# NativeShellMac

The AppKit renderer of native-shell and its host integration API, as a Swift
Package (`Package.swift`). macOS 12+, every public type usable from
Objective-C. The same programs that run on iOS and Android run here unchanged;
see [docs/platform-macos.md](../../docs/platform-macos.md) for how each part of
the protocol maps to AppKit.

The protocol, batch codec, controlled-input protocol, JSON helpers and SVG
path parser come from `../apple/NativeShellShared`, the Swift package shared
with the iOS renderer.

Build the Rust core first: `scripts/build-macos.sh` writes
`Frameworks/NativeShellFFI.xcframework` (arm64 + x86_64). Without it, the
package compiles against a stub of the C ABI (`Sources/NativeShellFFIStub`) so
the renderer and its tests still build; end-to-end tests then skip. SwiftPM
caches the evaluated manifest by its contents: after the xcframework appears,
clear `~/Library/Caches/org.swift.swiftpm/manifests` (or edit `Package.swift`)
so the binary target is picked up.

## Embedding

```swift
import NativeShellMac

// Once, before creating instances.
NSHRuntime.shared.register(module: QuoteModule())        // NSHHostModule
NSHRuntime.shared.register(component: StockCell.self)    // NSHHostComponent
NSHRuntime.shared.themeProvider = { _ in ["primary": .controlAccentColor] }

let instance = try NSHInstance(config: NSHInstanceConfig(appId: "com.example.todo"))
let controller = NSHViewController(instance: instance)   // pages, window sheets, alerts
try instance.loadSources(sourcesJSON)                   // or loadBundle(signedPackage)
window.contentViewController = controller
```

The names match the iOS package (`NSHRuntime`, `NSHInstance`,
`NSHViewController`, `NSHSurfaceView`, `NSHHostModule`, `NSHHostComponent`), so
host code ports by swapping UIKit types for AppKit ones (`view: NSView`,
`NSColor`, `NSImage`). Add an Edit menu (Cut, Copy, Paste, Select All) to the
app so text fields get their standard shortcuts, and a Go menu item with the
`goBack:` action (Command-[) for page navigation; the example app shows both.

## Layout

| Path | What |
| --- | --- |
| `Sources/NativeShellMac/Mounting` | Node table, view pool, base view (`View` type), animations, key routing |
| `Sources/NativeShellMac/Components` | One view per view type; controlled text input; offset-placed list |
| `Sources/NativeShellMac/Text` | `AttributedText` → TextKit 1, measurement shared by layout and drawing |
| `Sources/NativeShellMac/Public` | `NSHRuntime`, `NSHInstance`, `NSHSurfaceView`, `NSHViewController`, host API |
| `Tests/NativeShellMacTests` | XCTest suite, including end-to-end runs of `examples/` against the real core |
| `Example` | Example app (XcodeGen): a sidebar of the examples, the host-demo module and component |
| `screenshots` | Light and dark captures of every example, pages and overlays |

## Tests

```sh
scripts/build-macos.sh
cd platforms/macos && swift test
cd platforms/apple/NativeShellShared && swift test   # the shared protocol code

cd Example && ./capture-screenshots.sh   # builds the app, writes ../screenshots/
```

The package tests never put a window on screen (sheets go to a recording
window), so they run without taking focus. The example app accepts
`-example <name>`, `-appearance light|dark`, `-reset` and
`-actions "tap:<test id>;wait:<s>;back;capture:<png>;quit"`.
