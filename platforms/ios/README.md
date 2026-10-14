# NativeShell for iOS

The UIKit renderer of native-shell and its host integration API, as a Swift
Package (`Package.swift`) and a CocoaPods pod (`NativeShell.podspec`).
iOS 14+, every public type usable from Objective-C.

Build the Rust core first: `scripts/build-ios.sh` writes
`Frameworks/NativeShellFFI.xcframework`. Without it, the package compiles
against a stub of the C ABI (`Sources/NativeShellFFIStub`) so the renderer and
its tests still build; end-to-end tests then skip. SwiftPM caches the evaluated manifest by its
contents: after the xcframework appears, clear `~/Library/Caches/org.swift.swiftpm/manifests`
(or edit `Package.swift`) so the binary target is picked up.

## Embedding

```swift
import NativeShell

// Once, before creating instances.
NSHRuntime.shared.register(module: QuoteModule())        // NSHHostModule
NSHRuntime.shared.register(component: StockCell.self)    // NSHHostComponent
NSHRuntime.shared.themeProvider = { appearance in ["primary": .systemBlue] }

let instance = try NSHInstance(config: NSHInstanceConfig(appId: "com.example.todo"))
let controller = NSHViewController(instance: instance)   // pages, sheets, dialogs, alerts
try instance.loadSources(sourcesJSON)                   // or loadBundle(signedMsp)
present(controller, animated: true)
```

`NSHSurfaceView` renders an instance into any view hierarchy when a view
controller with navigation is not wanted.

## Layout

| Path | What |
| --- | --- |
| `Sources/NativeShell/Protocol` | Wire enums, validating batch decoder, test encoder |
| `Sources/NativeShell/Mounting` | Node table, view pool, base view (`View` type), animations |
| `Sources/NativeShell/Components` | One view per view type; controlled text input; collection-view list |
| `Sources/NativeShell/Text` | `AttributedText` → TextKit 1, measurement shared by layout and drawing |
| `Sources/NativeShell/Public` | `NSHRuntime`, `NSHInstance`, `NSHSurfaceView`, `NSHViewController`, host API |
| `Tests/NativeShellTests` | XCTest suite (simulator), including end-to-end runs of `examples/` |
| `Example` | Example app (XcodeGen) with the host-demo module and component, UI tests, screenshots |

## Tests

```sh
cd platforms/ios
xcodebuild test -scheme NativeShell -destination 'platform=iOS Simulator,name=iPhone 17 Pro'

cd Example && xcodegen generate
xcodebuild test -project NativeShellExample.xcodeproj -scheme NativeShellExample \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro'   # writes screenshots/
```

The package tests run without a `UIApplication` (no host app), so UIKit never
completes modal transitions or routes control actions there; those tests check
what the renderer asks UIKit to do, and the Example UI tests cover the same
flows in a real app.
