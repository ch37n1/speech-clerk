# V1 Release Candidate Guide

This guide is the repeatable Phase 4 handoff for a local-only Speech Clerk
release candidate. It complements `docs/ROADMAP.md`, `apps/macos/README.md`,
and `apps/android/README.md`.

## Local-Only Contract

- Model packs are installed from local directories.
- ONNX Runtime is loaded from a local dynamic library:
  `ORT_DYLIB_PATH` on macOS and the packaged `libonnxruntime.so` on Android.
- Android declares no `INTERNET` permission and disables cleartext traffic.
- V1 has no account system, sync, cloud inference, semantic correction, or
  token-level streaming.

## Model Pack Setup

Create the default Parakeet INT8 pack from split ONNX export assets:

```sh
cargo run -p model-packager -- parakeet-split \
  --source ./exports/parakeet-tdt-0.6b-v3-int8 \
  --output "$HOME/Library/Application Support/SpeechClerk/ModelPacks"
```

The release-candidate validator requires ONNX packs to declare:

- `runtime: "onnx"`
- `quantization: "int8"`
- SHA-256 checksums for every manifest-declared file
- either `model` and `tokenizer` assets, or split Parakeet
  `encoder`, `decoder_joint`, `preprocessor`, `vocab`, and `config` assets
- total ONNX pack size no larger than 6 GiB

Checksum, missing-file, invalid-quantization, and over-limit package-size
failures are reported as model-pack validation errors before recording starts.

Validate a pack explicitly before installing it:

```sh
cargo run -p model-packager -- validate \
  --pack "$HOME/Library/Application Support/SpeechClerk/ModelPacks/parakeet-tdt-0.6b-v3-int8"
```

The repository gate can also validate static local-only expectations and, when
provided, the real pack path:

```sh
make rc-check MODEL_PACK="$HOME/Library/Application Support/SpeechClerk/ModelPacks/parakeet-tdt-0.6b-v3-int8"
```

## macOS Build And Install

Set the local ONNX Runtime dylib before launching a real ONNX model:

```sh
export ORT_DYLIB_PATH=/path/to/libonnxruntime.dylib
make macos-e2e-build
cd apps/macos
swift run SpeechClerkMac
```

Manual workflow:

1. Open a real text field in another app.
2. Return to Speech Clerk.
3. Confirm a model is loaded and microphone and paste-control states are
   allowed.
4. Click `Record`, dictate for 5-15 seconds, then click `Stop`.
5. Confirm the transcript is inserted into the previously active field.
6. Run the Benchmark section and record model load, finalization P50/P95, RTF,
   and peak memory.

## Android Build And Install

Build the Rust library, package ONNX Runtime, then build the APK:

```sh
make android-rust ANDROID_ABI=arm64-v8a
cp /path/to/libonnxruntime.so apps/android/app/src/main/jniLibs/arm64-v8a/
make android-build ANDROID_ABI=arm64-v8a
```

Install and enable the IME:

```sh
adb install -r apps/android/app/build/outputs/apk/debug/app-debug.apk
adb shell pm grant app.speechclerk.android android.permission.RECORD_AUDIO
adb shell ime enable app.speechclerk.android/.SpeechClerkImeService
adb shell ime set app.speechclerk.android/.SpeechClerkImeService
```

Install the model pack into app-private storage:

```sh
adb push ./models/parakeet-tdt-0.6b-v3-int8 /data/local/tmp/speech-clerk-model
adb shell run-as app.speechclerk.android mkdir -p files/ModelPacks
adb shell run-as app.speechclerk.android cp -R /data/local/tmp/speech-clerk-model files/ModelPacks/parakeet-tdt-0.6b-v3-int8
```

Manual workflow:

1. Select Speech Clerk as the active keyboard.
2. Open two unrelated apps with text fields.
3. Tap `Mic`, dictate a short phrase, tap `Stop`, and confirm committed text.
4. Switch the active input subtype or language where Android exposes it.
5. Repeat dictation and confirm language context still follows the Rust-owned
   priority order.

## Runtime Tuning

Release-candidate Rust defaults:

- audio queue capacity: 64 captured frames
- minimum speech: 700 ms
- target chunk window: 5-12 seconds
- hard chunk maximum: 15 seconds
- pre-roll: 250 ms
- post-roll: 500 ms
- speech threshold: `0.002`
- ONNX graph optimization: Level 3
- ONNX intra-op threads: min(available CPU parallelism, 4)
- ONNX inter-op threads: 1
- ONNX execution mode: sequential
- ONNX memory pattern: disabled for variable chunk sizes

Optional ONNX overrides:

```sh
export SPEECH_CLERK_ORT_INTRA_THREADS=4
export SPEECH_CLERK_ORT_INTER_THREADS=1
export SPEECH_CLERK_ORT_PARALLEL=false
export SPEECH_CLERK_ORT_MEMORY_PATTERN=false
```

Opt-in Rust tracing from the native app process:

```sh
export SPEECH_CLERK_LOG=info
```

Accepted levels are `error`, `warn`, `info`, `debug`, and `trace`. The variable
is off by default; `off`, `false`, `0`, and `none` keep tracing disabled.

## Verification Checklist

Run before handing off release-candidate work:

```sh
make c
make rc-check MODEL_PACK=/path/to/parakeet-tdt-0.6b-v3-int8
make macos-e2e-smoke
```

Record blockers instead of treating them as success when the local environment
lacks Accessibility, Screen Recording, microphone, Android SDK, Gradle,
`cargo-ndk`, a device, or ONNX Runtime.

Manual evidence to record:

- macOS build result and screenshot path from `.build/e2e/macos/`.
- Android ABI, device/emulator, and APK build result.
- Model pack id, quantization, total size, and checksum validation result.
- Dictation result in one macOS text field and two Android text fields.
- Benchmark output with model load, finalization P50/P95, RTF, and peak memory.
- UI states reviewed: idle, recording, transcribing, permission/error, and
  model-missing.

## Known Limitations

- macOS builds are unsigned and unnotarized until Phase 5 release artifacts.
- Android release signing and GitHub-hosted artifacts are Phase 5 scope.
- Real Parakeet accuracy and latency depend on the locally supplied model pack
  and ONNX Runtime binary.
- The macOS V1 insertion path uses clipboard paste and requires Accessibility
  trust.
- Android model installation is a developer-style `adb` flow until packaged
  artifacts are added.
- The benchmark uses finalized chunks only; V1 does not stream tokens.
