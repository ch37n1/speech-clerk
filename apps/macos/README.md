# Speech Clerk macOS

Native SwiftUI/AppKit shell for local dictation.

Swift conventions and tooling are documented in `../../docs/SWIFT_GUIDE.md`.
App inspection and control are documented in `../../docs/MACOS_APP_ACCESS.md`.
E2E smoke and manual verification are documented in
`../../docs/MACOS_E2E_TESTING.md`.
Release-candidate install, benchmark, and known-limitations checks are
documented in `../../docs/RELEASE_CANDIDATE.md`.

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
make macos-ui CALL_ARGS="tree --max-depth 8"
make macos-e2e-smoke
make c
```

`make swift-lint` runs SwiftLint only when `swiftlint` is installed. The
formatter uses Apple `swift-format`, usually available as `xcrun swift-format`
with Xcode 16 or newer.

The app reads model packs from:

```text
~/Library/Application Support/SpeechClerk/ModelPacks
```

Bundled development packs are copied there on launch when missing. A real
Parakeet ONNX pack should be installed as a directory under that root with a
valid `manifest.json`, split ONNX/tokenizer/config assets, and matching SHA-256
checksums.

For split Parakeet ONNX exports with:

```text
encoder-model.int8.onnx
decoder_joint-model.int8.onnx
nemo128.onnx
vocab.txt
config.json
```

create the app model pack with:

```sh
cargo run -p model-packager -- parakeet-split \
  --source ./exports/parakeet-tdt-0.6b-v3-int8 \
  --output "$HOME/Library/Application Support/SpeechClerk/ModelPacks"
```

Real ONNX execution dynamically loads ONNX Runtime. Set `ORT_DYLIB_PATH` before
launching the app or benchmark CLI, for example:

```sh
export ORT_DYLIB_PATH=/path/to/libonnxruntime.dylib
```

Current implementation note: the ONNX backend loads manifest-declared Parakeet
preprocessor, encoder, and decoder/joint sessions, then runs greedy Parakeet TDT
token/duration decoding. Real Parakeet packs require a compatible ONNX Runtime
dylib through `ORT_DYLIB_PATH`. Fixture ONNX packs are still usable for app and
benchmark plumbing tests.

The app captures microphone audio with `AVAudioEngine`, sends interleaved
`f32` frames through the generated UniFFI `DictationController`, receives the
post-processed transcript, and inserts it into the previously active macOS app
with the V1 clipboard paste flow.

Benchmark CLI:

```sh
cargo run -p dictation-bench -- transcribe \
  --model "$HOME/Library/Application Support/SpeechClerk/ModelPacks/parakeet-tdt-0.6b-v3-int8" \
  --audio ./fixtures/ru_10s.wav \
  --runs 30 \
  --output bench.json
```

Manual macOS check:

1. Run `make macos-e2e-smoke` or launch the app with the commands above.
2. Allow microphone access and paste control when prompted.
3. Open a text editor or browser text field, then return to Speech Clerk.
4. Load either the development fixture pack or an installed Parakeet ONNX model,
   click Record, dictate for 5-15 seconds, then click Stop.
5. If macOS opens Privacy & Security for paste control, grant Speech Clerk
   permission, relaunch the app if macOS requires it, and repeat the paste step.
6. Confirm the transcript is pasted into the previously active text field. For a
   real Parakeet pack, use a normal 5-15 second phrase and verify the inserted
   text matches the dictated speech closely enough for the installed model.
7. Open the Benchmark section, run the selected model pack, and confirm latency,
   RTF, and memory values are shown.
