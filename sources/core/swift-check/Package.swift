// swift-tools-version: 6.0
// Checks that Swift can call the core. tools/build-core-swift.sh builds the XCFramework and the Swift bindings.
import PackageDescription

let package = Package(
    name: "CoreCheck",
    platforms: [.macOS(.v14)],
    targets: [
        .binaryTarget(name: "specboard_ffiFFI", path: "Frameworks/SpecBoardCoreFFI.xcframework"),
        .target(name: "SpecBoardCore", dependencies: ["specboard_ffiFFI"], path: "Generated"),
        .executableTarget(name: "core-check", dependencies: ["SpecBoardCore"], path: "Check"),
    ],
    swiftLanguageModes: [.v5]
)
