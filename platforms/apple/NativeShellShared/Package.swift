// swift-tools-version:5.9
//
// NativeShellShared: the platform-neutral Swift of the Apple renderers — the
// mutation protocol and its batch codec, the controlled-input protocol, JSON
// helpers and the SVG path parser. Foundation and CoreGraphics only; the iOS
// (NativeShell) and macOS (NativeShellMac) packages depend on it.

import PackageDescription

let package = Package(
    name: "NativeShellShared",
    platforms: [.iOS(.v14), .macOS(.v12)],
    products: [
        .library(name: "NativeShellShared", targets: ["NativeShellShared"]),
    ],
    targets: [
        .target(name: "NativeShellShared", path: "Sources/NativeShellShared"),
        .testTarget(name: "NativeShellSharedTests", dependencies: ["NativeShellShared"], path: "Tests/NativeShellSharedTests"),
    ]
)
