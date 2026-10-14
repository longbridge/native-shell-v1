// swift-tools-version:5.9
//
// NativeShell: the UIKit renderer and host integration of native-shell.
//
// The Rust core ships as `Frameworks/NativeShellFFI.xcframework`, built by
// `scripts/build-ios.sh`. Until it has been built, a stub with the same C ABI
// stands in (every entry point fails), so the renderer and its tests still
// compile and run; end-to-end tests skip themselves in that case.

import Foundation
import PackageDescription

let packageDir = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let hasCore = FileManager.default.fileExists(atPath: packageDir + "/Frameworks/NativeShellFFI.xcframework/Info.plist")

let ffiTarget: Target = hasCore
    ? .binaryTarget(name: "NativeShellFFI", path: "Frameworks/NativeShellFFI.xcframework")
    : .target(name: "NativeShellFFI", path: "Sources/NativeShellFFIStub")

let package = Package(
    name: "NativeShell",
    platforms: [.iOS(.v14)],
    products: [
        .library(name: "NativeShell", targets: ["NativeShell"]),
    ],
    targets: [
        ffiTarget,
        .target(
            name: "NativeShell",
            dependencies: ["NativeShellFFI"],
            path: "Sources/NativeShell",
            linkerSettings: [.linkedLibrary("c++"), .linkedFramework("Security"), .linkedFramework("SystemConfiguration")]
        ),
        .testTarget(
            name: "NativeShellTests",
            dependencies: ["NativeShell"],
            path: "Tests/NativeShellTests"
        ),
    ]
)
