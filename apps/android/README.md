# Speech Clerk Android

Native Kotlin input method for local dictation.

Manual verification is documented in `../../docs/ANDROID_IME_TESTING.md`.
Release-candidate install, benchmark, and known-limitations checks are
documented in `../../docs/RELEASE_CANDIDATE.md`.

The Android shell owns only IME UI, microphone permission, `AudioRecord`
capture, platform language signals, and text insertion through
`InputConnection.commitText()`. Dictation state, model-pack validation, audio
normalization/chunking, ASR dispatch, post-processing, and language priority stay
in Rust behind UniFFI.

## Generated UniFFI Bindings

Regenerate Kotlin bindings from the repository root after changing
`crates/ffi/src/speech_clerk.udl` or exported FFI Rust types:

```sh
make android-uniffi
```

The generated Kotlin package is configured by `crates/ffi/uniffi.toml`.

## Native Libraries

The APK expects native libraries under:

```text
apps/android/app/src/main/jniLibs/<abi>/
```

For an ARM64 device:

```sh
rustup target add aarch64-linux-android
cargo ndk -t arm64-v8a -o apps/android/app/src/main/jniLibs build -p ffi --release
cp /path/to/libonnxruntime.so apps/android/app/src/main/jniLibs/arm64-v8a/
```

The IME sets `ORT_DYLIB_PATH` to the packaged `libonnxruntime.so` before loading
real ONNX model packs.

The Android app declares no network permission and disables cleartext traffic;
real dictation must work from app-private model files and packaged native
libraries.

## Model Packs

Install model packs into the app-private directory:

```text
/data/data/app.speechclerk.android/files/ModelPacks/<model-id>/manifest.json
```

One repeatable development path:

```sh
adb push ./models/parakeet-tdt-0.6b-v3-int8 /data/local/tmp/speech-clerk-model
adb shell run-as app.speechclerk.android mkdir -p files/ModelPacks
adb shell run-as app.speechclerk.android cp -R /data/local/tmp/speech-clerk-model files/ModelPacks/parakeet-tdt-0.6b-v3-int8
```

## Build

From the repository root:

```sh
JAVA_HOME=/opt/homebrew/opt/openjdk/libexec/openjdk.jdk/Contents/Home \
ANDROID_SDK_ROOT=/opt/homebrew/share/android-commandlinetools \
sdkmanager --licenses
gradle -p apps/android assembleDebug
```

## Kotlin Quality

Android Kotlin formatting and static analysis are part of the repository quality
gate:

```sh
make kotlin-fmt
make kotlin-fmt-check
make kotlin-lint
make android-check
```

`ktfmt` formats first-party Android Kotlin sources. Detekt checks the same
first-party Android package with configuration in `config/detekt/detekt.yml`.
Generated UniFFI bindings under `app/speechclerk/ffi` are excluded from both
tools and should be regenerated with `make android-uniffi`.

The Makefile prefers Homebrew `openjdk@17` or `openjdk@21` for Android Gradle
tasks when either is installed. Detekt 1.23.x does not run correctly on newer
JDKs such as OpenJDK 25.

## Manual Check

1. Install the debug APK on an Android device.
2. Allow microphone access when the IME prompts.
3. Enable Speech Clerk in system keyboard settings and select it as the active
   keyboard.
4. Open two unrelated apps with text fields.
5. Tap `Mic`, dictate a short phrase, tap `Stop`, and confirm the local
   transcript is committed into each field.
6. Switch the active input subtype where Android exposes it and repeat the flow.
