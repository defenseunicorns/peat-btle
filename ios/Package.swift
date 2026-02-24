// swift-tools-version: 5.9
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "EcheTest",
    platforms: [
        .iOS(.v16),
        .macOS(.v13)
    ],
    products: [
        .executable(name: "EcheTest", targets: ["EcheTest"]),
    ],
    dependencies: [],
    targets: [
        // Binary target for the Rust FFI library
        .binaryTarget(
            name: "EcheFFI",
            path: "build/EcheFFI.xcframework"
        ),
        .executableTarget(
            name: "EcheTest",
            dependencies: [
                "EcheFFI",
            ],
            path: "EcheTest",
            exclude: ["Info.plist"],
            linkerSettings: [
                .linkedFramework("CoreBluetooth"),
            ]
        ),
    ]
)
