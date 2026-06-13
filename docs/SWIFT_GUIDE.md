# Swift Guide

This guide defines how Swift is used in Speech Clerk. It complements
`docs/TECHNICAL_ARCHITECTURE.md` and `docs/DDD_GUIDE.md`; those documents stay
canonical for product architecture and domain ownership.

## Scope

Swift is the native macOS shell under `apps/macos`.

Swift owns:

- SwiftUI views and AppKit integration.
- macOS permissions and user-facing permission state.
- Microphone capture through `AVAudioEngine`.
- Clipboard paste insertion through `NSPasteboard`, Accessibility trust, and
  synthetic Cmd+V events.
- Thin local settings needed to drive the platform shell.
- A small adapter around the generated UniFFI API.

Swift does not own:

- Dictation session rules.
- Audio normalization, resampling, VAD, or chunking policy.
- ASR backend selection or model runtime access.
- Model-pack validation beyond local app discovery for display.
- Transcript post-processing rules.
- Language selection policy.

Those behaviors belong in Rust crates and cross the platform boundary through
the product-level UniFFI API.

## Package Layout

The macOS app is a Swift Package Manager executable package:

```text
apps/macos/Package.swift
apps/macos/Sources/SpeechClerkMac
apps/macos/Sources/SpeechClerkMacSupport
apps/macos/Tests/SpeechClerkMacTests
```

Keep app-shell Swift code in feature-sized files under `Sources/SpeechClerkMac`.
Pure platform-edge helpers that need isolated tests can live in
`Sources/SpeechClerkMacSupport`. Do not add broad `Utilities`, `Common`, or
`Types` buckets. If platform code grows a new responsibility, name it after the
macOS behavior it owns.

Generated UniFFI files live under:

```text
apps/macos/Sources/SpeechClerkMac/Generated/UniFFI
```

Do not edit generated UniFFI files manually. Tooling must exclude this directory
from formatting and linting.

## UniFFI Boundary

Swift should call Rust through `RustDictationBridge`, not through scattered
direct calls to generated UniFFI types.

The bridge should stay product-level:

- Load a model by id.
- Start, stop, or cancel recording.
- Push captured PCM frames.
- Set deterministic replacement rules.
- Return transcript text to the view model.

Swift must not call ONNX Runtime, load concrete model sessions, or duplicate
Rust-owned product decisions.

After changing `crates/ffi/src/speech_clerk.udl` or exported FFI Rust types,
regenerate the UniFFI Swift bindings and build the Rust FFI library before
running the macOS app:

```sh
cargo build -p ffi
swift build --package-path apps/macos
```

## Concurrency And Audio

Keep UI state on the main actor. View models that publish SwiftUI state should
be annotated with `@MainActor`.

The `AVAudioEngine` capture callback must stay small:

- Copy audio frames into an owned buffer.
- Forward interleaved `f32` samples to Rust.
- Do not update SwiftUI state directly from the callback.
- Do not run inference, post-processing, file I/O, or blocking work in the
  callback.

If an audio callback needs to report UI state, hop back to the main actor.

## Permissions And Insertion

Microphone and paste-control permissions are platform behavior and can live in
Swift. The resulting dictation behavior must still be driven by Rust state.

The V1 insertion path is clipboard paste:

1. Save the current string clipboard value.
2. Write the transcript to the pasteboard.
3. Activate the previous external app where possible.
4. Send Cmd+V.
5. Restore the previous string clipboard when the paste appears unchanged.

Accessibility-native insertion is out of scope until the architecture document
is updated.

## Tooling

Routine work uses `make`.

```sh
make swift-fmt
make swift-fmt-check
make swift-lint
make swift-build
make swift-test
make swift-check
make macos-agent-smoke
make c
```

The repo uses Apple `swift-format` through `xcrun swift-format` by default.
Configuration lives in `.swift-format`.

SwiftLint is optional locally. When it is installed, `make swift-lint` and
`make c` run it with `.swiftlint.yml`. When it is missing, the target prints a
skip message so contributors can still run the rest of the gate.

SwiftPM build artifacts and caches should stay under `.build/` and must not be
committed.

## Tests

Prefer Rust tests for product behavior. Swift tests use a small SwiftPM
executable harness so they work with the command line toolchain even when
`Testing` or `XCTest` modules are unavailable. They should cover platform-edge
logic that can be tested without real microphone, Accessibility, or app focus
permissions.

Good Swift test subjects:

- Model-pack discovery for the macOS picker.
- Settings adapters that translate Swift UI state into FFI inputs.
- Clipboard or permission helpers only when behavior can be isolated without
  requiring global macOS permissions.

Manual verification remains required for microphone capture, paste control,
global focus behavior, and the full visible dictation workflow.

Use `docs/MACOS_AGENT_TESTING.md` for the agent-facing macOS smoke and manual
workflow evidence contract.
