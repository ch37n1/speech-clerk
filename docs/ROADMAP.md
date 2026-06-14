# Speech Clerk V1 Roadmap

This roadmap turns the V1 technical architecture into large product phases. Every phase must end with a state where a human can launch an application, use a visible workflow, and verify the result manually.

V1 constraints:

- Local-only execution.
- macOS is the primary platform.
- Android IME is the required second platform.
- No iOS in V1.
- Chunked transcription only; no token-level streaming.
- Rust owns dictation logic; platform apps stay thin.
- ONNX Runtime is the only V1 ASR backend.
- Parakeet TDT 0.6B V3 INT8 model pack is the default V1 model target.

## Phase 1 - macOS Dictation Shell With Fake Transcription

Build the first visible macOS app and connect it to the shared Rust API using a fake ASR backend. This phase proves the product workflow before real model inference is ready.

Scope:

- Create the Rust workspace structure:
  - `crates/dictation-core`
  - `crates/audio-pipeline`
  - `crates/asr-api`
  - `crates/model-pack`
  - `crates/postprocess`
  - `crates/ffi`
- Create the native macOS app in `apps/macos`.
- Expose the product-level `DictationController` through UniFFI.
- Implement a fake `AsrEngine` behind the real ASR trait.
- Implement model-pack manifest parsing and validation with a fake local model pack.
- Implement deterministic post-processing:
  - whitespace cleanup
  - punctuation cleanup
  - simple custom replacements
- Build the SwiftUI/AppKit shell with:
  - model selection
  - microphone permission prompt
  - local settings
  - visible recording state
  - hold-to-record or toggle recording
- Capture microphone audio with `AVAudioEngine` and send frames to Rust.
- Return a fake transcript through the same path the real backend will use.
- Insert text into the active macOS field with the V1 clipboard paste flow.

Out of scope:

- Real ONNX inference.
- Benchmark targets.
- Android.
- Accessibility-native text insertion.

Manual verification deliverable:

Launch the macOS app, select the fake local model pack, open a text editor or browser text field, start dictation from the app or hotkey, speak into the microphone, stop dictation, and confirm that a fake transcript is inserted into the active field. Change a custom replacement rule in settings, repeat the flow, and confirm that the inserted text changes. Use `docs/MACOS_E2E_TESTING.md` for repeatable smoke evidence before or alongside this manual workflow.

## Phase 2 - macOS App With Real Local ONNX Transcription

Replace fake transcription with the V1 ONNX backend while keeping the same macOS app workflow.

Scope:

- Implement `crates/asr-onnx`.
- Implement `OrtAsrEngine` behind the existing `AsrEngine` trait.
- Load models only through the model-pack manifest.
- Package the Parakeet TDT 0.6B V3 INT8 ONNX model as the default local model pack.
- Validate model files with manifest checksums.
- Implement tokenizer/model asset loading required by the Parakeet-class model.
- Complete the audio pipeline around the V1 internal format:
  - `f32`
  - mono
  - 16 kHz
  - bounded queue between capture and processing
  - worker-thread processing
- Implement chunking rules:
  - 700 ms minimum speech chunk
  - 5-15 s target chunk length
  - 15 s maximum chunk length
  - 250 ms pre-roll
  - 500 ms post-roll
- Add `tools/dictation-bench`.
- Add a macOS benchmark screen that runs the same local model pack.
- Measure:
  - model load time
  - audio preprocessing time
  - ASR inference time
  - post-processing time
  - end-to-end latency
  - RTF
  - P50 latency
  - P95 latency
  - peak memory

Out of scope:

- Sherpa backend.
- whisper.cpp backend.
- CoreML backend.
- Cloud fallback.
- LLM rewriting.
- Android.

Manual verification deliverable:

Launch the macOS app, select the real Parakeet model pack, open a text editor or browser text field, dictate a 5-15 second phrase, stop dictation, and confirm that the real transcript is inserted into the active field. Then open the benchmark screen in the app, run a benchmark against the installed model pack, and confirm that the app shows latency and memory results. On an M3-class Mac, verify the 3 second post-finalization target or record the measured gap as a follow-up issue.

## Phase 3 - Android IME With Local Dictation

Build the required second V1 platform as a real Android input method that reuses the Rust dictation core.

Scope:

- Create the native Android project in `apps/android`.
- Implement a Kotlin `InputMethodService`.
- Add a visible keyboard mic button for dictation.
- Capture microphone audio with `AudioRecord`.
- Integrate UniFFI bindings to the Rust dictation core.
- Load local model packs through the shared manifest format.
- Send audio frames to `DictationController`.
- Insert transcripts with `InputConnection.commitText()`.
- Reuse deterministic post-processing from Rust.
- Reuse language handling from Rust using the V1 priority order:
  - active keyboard/input language
  - platform input context
  - model language auto-detection
  - last used language
  - manual override
- Keep dictation logic out of Kotlin except for platform capture and insertion.

Out of scope:

- Normal Android note-taking app mode.
- Cloud fallback.
- iOS.
- Multiple ASR backends.

Manual verification deliverable:

Install the Android app, enable Speech Clerk as the active keyboard, open at least two unrelated apps with text fields, tap the keyboard mic button, dictate a short phrase, and confirm that the real local transcript is committed into each active text field. Switch the active input language where supported and verify that dictation still follows the shared language-selection behavior.

## Phase 4 - V1 Release Candidate

Harden the working macOS and Android apps into a release candidate that can be installed, configured, benchmarked, and used by another person.

Scope:

- Tune chunk size, VAD thresholds, queue bounds, and worker behavior.
- Tune ONNX Runtime threading and memory settings.
- Improve cold start and warm inference latency.
- Validate model quantization and package size.
- Harden model-pack installation and checksum errors.
- Harden microphone permission and failure states.
- Harden clipboard restore behavior on macOS.
- Harden Android IME lifecycle handling.
- Polish the visible macOS and Android UI:
  - clearer visual hierarchy and spacing
  - consistent typography, controls, and icons
  - understandable idle, recording, transcribing, error, and model-missing states
  - accessible labels, focus behavior, and contrast for the primary workflows
- Add practical logging with `tracing`.
- Verify local-only behavior.
- Document installation, model-pack setup, benchmark usage, and known limitations.
- Prepare clean macOS and Android builds.

Out of scope:

- Training or fine-tuning.
- Sync.
- Account system.
- Cloud inference.
- Semantic correction.
- Additional ASR backends.

Manual verification deliverable:

Install the macOS app and Android IME from clean release-candidate builds, install the default model pack locally, dictate into real text fields on both platforms, run the benchmark workflow, and confirm that documented setup steps are sufficient for another person to repeat the workflow without developer assistance. During the same pass, review the primary macOS and Android surfaces in idle, recording, transcribing, permission/error, and model-missing states, and confirm that the UI looks coherent enough for a V1 release candidate.

## Phase 5 - CI/CD Release Artifacts

Add the simple GitHub Actions release flow described in `docs/CI_CD_RELEASES.md` so successful builds produce downloadable installable artifacts.

Scope:

- Keep the existing CI quality gate for pull requests and `main`.
- Add release packaging targets for macOS and Android.
- Package the macOS app as an unsigned, unnotarized `.app` ZIP.
- Build an Android release APK signed with a stable GitHub Secrets-backed keystore.
- Publish `SpeechClerk-macos.zip` and `SpeechClerk-android.apk` to a mutable `nightly` GitHub Release on every push to `main`.
- Support equivalent artifacts for `v*` tag releases.
- Document install expectations, release secrets, and known unsigned-build warnings.

Out of scope:

- App Store distribution.
- Play Store distribution.
- macOS notarization.
- External CI services.
- Self-hosted runners.
- Automated semantic version management.

Manual verification deliverable:

Push to `main`, open the `nightly` GitHub Release, download the macOS ZIP and Android APK, install or launch both artifacts on real devices, and confirm they come from the latest successful commit. Push a test `v*` tag and confirm that the versioned release receives the same artifact types.
