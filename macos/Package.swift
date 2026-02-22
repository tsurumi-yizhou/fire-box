// swift-tools-version: 6.1

import PackageDescription

let package = Package(
    name: "Firebox",
    defaultLocalization: "en",
    platforms: [
        .macOS(.v15)
    ],
    targets: [
        .executableTarget(name: "App", path: "Sources/App"),
        .executableTarget(
            name: "Helper",
            path: "Sources/Helper",
            resources: [
                .process("Resources")
            ]
        )
    ]
)