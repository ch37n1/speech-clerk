# Android IME Testing

This guide defines the repeatable Phase 3 Android verification loop. It
complements `docs/ROADMAP.md` and `apps/android/README.md`.

## Prerequisites

- Android SDK, platform tools, and a Gradle installation available on `PATH`.
- `cargo-ndk` installed.
- Rust target for the test ABI, usually `aarch64-linux-android`.
- A device or emulator with microphone input.
- `libonnxruntime.so` for the device ABI.
- A real local model pack with a valid `manifest.json`.

Accept Android SDK licenses manually before the first APK build:

```sh
JAVA_HOME=/opt/homebrew/opt/openjdk/libexec/openjdk.jdk/Contents/Home \
ANDROID_SDK_ROOT=/opt/homebrew/share/android-commandlinetools \
sdkmanager --licenses
```

## Build

From the repository root:

```sh
make android-uniffi
make android-rust ANDROID_ABI=arm64-v8a
gradle -p apps/android assembleDebug
```

The Rust build writes native libraries under:

```text
apps/android/app/src/main/jniLibs/<abi>/
```

Copy the ONNX Runtime shared library next to the Rust FFI library:

```sh
cp /path/to/libonnxruntime.so apps/android/app/src/main/jniLibs/arm64-v8a/
```

Re-run the Gradle build after copying native libraries.

## Install

```sh
adb install -r apps/android/app/build/outputs/apk/debug/app-debug.apk
adb shell pm grant app.speechclerk.android android.permission.RECORD_AUDIO
adb shell ime enable app.speechclerk.android/.SpeechClerkImeService
adb shell ime set app.speechclerk.android/.SpeechClerkImeService
```

If Android blocks `pm grant`, open the app permission screen and grant
microphone access manually after the IME prompts.

## Install Model Pack

Use `/data/local/tmp` as the handoff location, then copy into the app-private
model-pack root:

```sh
adb push ./models/parakeet-tdt-0.6b-v3-int8 /data/local/tmp/speech-clerk-model
adb shell run-as app.speechclerk.android mkdir -p files/ModelPacks
adb shell run-as app.speechclerk.android cp -R /data/local/tmp/speech-clerk-model files/ModelPacks/parakeet-tdt-0.6b-v3-int8
adb shell run-as app.speechclerk.android ls files/ModelPacks/parakeet-tdt-0.6b-v3-int8/manifest.json
```

## Manual Smoke

1. Select Speech Clerk as the active keyboard.
2. Open an app with a text field, such as a notes app.
3. Tap `Mic`, dictate a short phrase, then tap `Stop`.
4. Confirm the transcript is committed into the active field.
5. Repeat the same flow in a second unrelated app with a text field.
6. Switch the active input subtype/language where supported.
7. Repeat dictation and confirm the Rust language priority still works.

## Evidence To Record

- Device or emulator model and Android version.
- ABI used for native libraries.
- Model pack id from `manifest.json`.
- The two apps used for text-field insertion.
- Whether active input language changes were exposed by Android.
- Any app logcat errors from `app.speechclerk.android`.

Useful log command:

```sh
adb logcat --pid="$(adb shell pidof app.speechclerk.android | tr -d '\r')"
```
