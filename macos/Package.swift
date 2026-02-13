// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "FireBox",
    platforms: [.macOS(.v15)],
    products: [
        .executable(
            name: "FireBox",
            targets: ["FireBox"]
        )
    ],
    targets: [
        // The main macOS native executable (SwiftUI menu-bar app).
        .executableTarget(
            name: "FireBox",
            path: "Sources/FireBox",
            exclude: ["AGENTS.md", "Views/AGENTS.md"],
            linkerSettings: [
                // Rust core static library (produced by `cargo build` in core/ and copied by core/build.rs).
                .unsafeFlags([
                    "-L", "../generated",
                    "-I", "../generated",
                ]),
                .linkedLibrary("core"),
                // System libraries needed by the Rust static library's deps.
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("CoreFoundation"),
                .linkedLibrary("resolv"),
                .linkedLibrary("iconv"),
            ]
        )
    ]
)
