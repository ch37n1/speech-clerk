# Speech Clerk macOS

Native SwiftUI/AppKit shell for Phase 1 fake dictation.

Swift conventions and tooling are documented in `../../docs/SWIFT_GUIDE.md`.

Run from the repository root:

```sh
cargo build -p ffi
cd apps/macos
swift run SpeechClerkMac
```

Quality checks from the repository root:

```sh
make swift-fmt-check
make swift-lint
make swift-build
make swift-test
make swift-check
make c
```

`make swift-lint` runs SwiftLint only when `swiftlint` is installed. The
formatter uses Apple `swift-format`, usually available as `xcrun swift-format`
with Xcode 16 or newer.

The app loads the bundled `fake-local` model pack, captures microphone audio
with `AVAudioEngine`, sends interleaved `f32` frames through the generated
UniFFI `DictationController`, receives the fake post-processed transcript, and
inserts it into the previously active macOS app with the V1 clipboard paste
flow.

Manual Phase 1 check:

1. Launch the app with the commands above.
2. Allow microphone access and paste control when prompted.
3. Open a text editor or browser text field, then return to Speech Clerk.
4. Load `Fake Local Model`, click Record, speak briefly, then click Stop.
5. If macOS opens Privacy & Security for paste control, grant Speech Clerk
   permission, relaunch the app if macOS requires it, and repeat the paste step.
6. Confirm the fake transcript is pasted into the previously active text field.
7. Change the replacement fields, click Apply, repeat recording, and confirm the
   pasted fake transcript uses the new replacement.
