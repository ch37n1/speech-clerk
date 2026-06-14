// swift-tools-version: 6.0

import Foundation
import PackageDescription

let rustLibraryDirectory =
    ProcessInfo.processInfo.environment["SPEECH_CLERK_RUST_TARGET_DIR"] ?? "../../target/debug"
let ffiRuntimeSearchPath =
    ProcessInfo.processInfo.environment["SPEECH_CLERK_FFI_RPATH"] ?? rustLibraryDirectory

let uniffiSwiftSettings: [SwiftSetting] = [
    .unsafeFlags([
        "-I", "Sources/SpeechClerkMac/Generated/UniFFI",
    ])
]

let ffiLinkerSettings: [LinkerSetting] = [
    .unsafeFlags([
        "-L", rustLibraryDirectory,
        "-lspeech_clerk_ffi",
        "-Xlinker", "-rpath",
        "-Xlinker", ffiRuntimeSearchPath,
        "-Xlinker", "-sectcreate",
        "-Xlinker", "__TEXT",
        "-Xlinker", "__info_plist",
        "-Xlinker", "Info.plist",
    ])
]

let package = Package(
    name: "SpeechClerkMac",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "SpeechClerkMac", targets: ["SpeechClerkMac"]),
        .executable(name: "SpeechClerkMacUnitTests", targets: ["SpeechClerkMacUnitTests"]),
        .executable(name: "SpeechClerkMacUITool", targets: ["SpeechClerkMacUITool"]),
    ],
    targets: [
        .target(
            name: "SpeechClerkMacSupport",
            path: "Sources/SpeechClerkMacSupport",
            resources: [
                .copy("Resources")
            ]
        ),
        .executableTarget(
            name: "SpeechClerkMac",
            dependencies: ["SpeechClerkMacSupport"],
            path: "Sources/SpeechClerkMac",
            swiftSettings: uniffiSwiftSettings,
            linkerSettings: ffiLinkerSettings
        ),
        .executableTarget(
            name: "SpeechClerkMacUnitTests",
            dependencies: ["SpeechClerkMacSupport"],
            path: "Tests/SpeechClerkMacTests"
        ),
        .executableTarget(
            name: "SpeechClerkMacUITool",
            path: "Tools/SpeechClerkMacUITool"
        ),
    ]
)
