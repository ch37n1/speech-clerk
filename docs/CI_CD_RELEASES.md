# CI/CD Release Approach

This document defines the intended V1 release pipeline for downloadable local
installers. The goal is a simple GitHub-native flow that produces artifacts a
person can install without using a developer checkout.

## Goals

- Build on every push to `main`.
- Publish a mutable GitHub Release named `nightly`.
- Attach installable macOS and Android artifacts to the release.
- Support immutable versioned releases from `v*` tags later.
- Keep distribution outside the App Store and Play Store for V1.
- Avoid external CI services and self-hosted runners.

## Current Baseline

The repository already has `.github/workflows/ci.yml`. It runs the local quality
gate on GitHub's macOS runner for pull requests and pushes to `main`:

```sh
make c
```

Current local build outputs are not yet a complete release pipeline:

- Android can build a debug APK through `make android-build`.
- macOS currently builds a SwiftPM executable through `make swift-build`.
- macOS still needs a packaged `.app` bundle and ZIP artifact.
- Android release distribution should use a stable signing key instead of debug
  signing so later APKs can update earlier installs.

## Recommended Flow

Keep the existing CI workflow for validation. Add one release-artifacts workflow
that runs on pushes to `main` and tags:

```text
push to main
    -> GitHub Actions on macOS runner
    -> run quality gate
    -> build Android release APK
    -> build macOS app bundle
    -> zip macOS .app
    -> replace assets in nightly release
    -> download from GitHub Releases
```

For versioned releases:

```text
push tag v0.4.0
    -> GitHub Actions on macOS runner
    -> build the same artifacts
    -> create or update the v0.4.0 release
    -> attach the same artifact types
```

## Release Channels

### Nightly

The `nightly` release is automatically refreshed on every push to `main`.

Expected assets:

```text
SpeechClerk-macos.zip
SpeechClerk-android.apk
```

The workflow should clobber existing nightly assets so the release always points
to the latest successful `main` build.

### Versioned

Versioned releases are created by pushing tags such as:

```text
v0.4.0
```

Versioned release assets should be immutable after publication unless there is a
clear release mistake. This can be postponed until the app is ready to distribute
to people outside development.

## Signing Policy

### macOS

V1 release artifacts are unsigned and unnotarized.

Installation expectation:

1. Download `SpeechClerk-macos.zip`.
2. Extract it.
3. Right click the app and choose `Open`.
4. Accept the expected Gatekeeper warning.

No Apple Developer ID, notarization, or App Store workflow is required for this
phase.

### Android

Use a stable release keystore even for unofficial builds. This avoids forcing
users to uninstall the app before installing a newer APK.

Required GitHub Secrets:

```text
ANDROID_RELEASE_KEYSTORE_BASE64
ANDROID_RELEASE_KEYSTORE_PASSWORD
ANDROID_RELEASE_KEY_ALIAS
ANDROID_RELEASE_KEY_PASSWORD
```

The release workflow should decode the keystore during the job, build a signed
release APK, upload the APK, and remove the decoded keystore before the job
finishes. The keystore file and passwords must never be committed.

## Repository-Specific Build Targets

Phase 5 should add or standardize these Make targets:

```sh
make macos-package
make android-release
make release-artifacts
```

Expected outputs:

```text
.build/dist/SpeechClerk-macos.zip
.build/dist/SpeechClerk-android.apk
```

`make macos-package` should build the Rust FFI library, build the macOS app, put
it in a real `.app` bundle, include the required native libraries/resources, and
zip the bundle.

`make android-release` should build the Android Rust FFI library for the release
ABI, package required native libraries, and produce a signed release APK.

`make release-artifacts` should produce both files in `.build/dist` and be the
single command used by GitHub Actions before uploading release assets.

## GitHub Actions Shape

The workflow can be a single file:

```text
.github/workflows/release-artifacts.yml
```

Use `macos-latest` first because the repository already needs macOS for Swift
and Rust FFI checks. Splitting Android and macOS into separate jobs can wait
until build time becomes a problem.

The workflow should:

- Check out the repository.
- Install the Rust toolchain and required targets.
- Install or configure Android SDK, NDK, Java, Gradle, UniFFI, and `cargo-ndk`.
- Run the quality gate or an agreed release-safe subset.
- Run `make release-artifacts`.
- For `main`, move or recreate the `nightly` tag and update the `nightly`
  prerelease with clobbered assets.
- For `v*` tags, create or update the matching GitHub Release with the same
  assets.

## Model Packs

The app artifacts are the primary CI/CD deliverables. If the default model pack
is legally and practically distributable through GitHub Releases, add a separate
model-pack ZIP asset later. Otherwise, release notes must link to the documented
local model-pack setup.

## Out of Scope

- App Store distribution.
- Play Store distribution.
- macOS notarization.
- Apple Developer ID signing.
- Play App Signing.
- External CI services.
- Self-hosted macOS runners.
- Automated semantic version management.

## Manual Verification

After the release-artifacts workflow lands:

1. Push a branch through PR and confirm the existing CI workflow passes.
2. Merge or push to `main`.
3. Open the `nightly` GitHub Release.
4. Download `SpeechClerk-macos.zip` and install the macOS app.
5. Download `SpeechClerk-android.apk` and install it on an Android device.
6. Confirm both artifacts can launch or enable the expected app surface.
7. Push a test `v*` tag and confirm the versioned release gets equivalent
   assets.
