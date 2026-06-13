// swift-tools-version: 6.0

import PackageDescription

let uniffiSwiftSettings: [SwiftSetting] = [
    .unsafeFlags([
        "-I", "Sources/SpeechClerkMac/Generated/UniFFI",
    ])
]

let ffiLinkerSettings: [LinkerSetting] = [
    .unsafeFlags([
        "-L", "../../target/debug",
        "-lspeech_clerk_ffi",
        "-Xlinker", "-rpath",
        "-Xlinker", "../../target/debug",
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
    ]
)
