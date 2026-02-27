// swift-tools-version: 5.9
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "PeatTest",
    platforms: [
        .iOS(.v16),
        .macOS(.v13)
    ],
    products: [
        .executable(name: "PeatTest", targets: ["PeatTest"]),
    ],
    dependencies: [],
    targets: [
        // Binary target for the Rust FFI library
        .binaryTarget(
            name: "PeatFFI",
            path: "build/PeatFFI.xcframework"
        ),
        .executableTarget(
            name: "PeatTest",
            dependencies: [
                "PeatFFI",
            ],
            path: "PeatTest",
            exclude: ["Info.plist"],
            linkerSettings: [
                .linkedFramework("CoreBluetooth"),
            ]
        ),
    ]
)
