// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "SpeechClerkMac",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "SpeechClerkMac", targets: ["SpeechClerkMac"])
    ],
    targets: [
        .executableTarget(
            name: "SpeechClerkMac",
            path: "Sources/SpeechClerkMac",
            resources: [
                .copy("Resources")
            ],
            swiftSettings: [
                .unsafeFlags([
                    "-I", "Sources/SpeechClerkMac/Generated/UniFFI",
                ])
            ],
            linkerSettings: [
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
        )
    ]
)
