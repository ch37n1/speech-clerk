# V1 Technical Implementation Plan
## Local Dictation App for macOS and Android

This document specifies the technical implementation path for V1 of the local cross-platform dictation product. It extends the existing V1 product specification and preserves its main constraints: local-only execution, macOS as the primary platform, Android as the required second platform, no iOS in V1, chunked dictation, and a backend-agnostic ASR architecture. :contentReference[oaicite:0]{index=0}

---

# 1. Final Architecture

V1 will be implemented as:

```text
Native macOS App
Native Android IME
        ↓
UniFFI Bindings
        ↓
Rust Dictation Core
        ↓
ONNX Runtime ASR Backend
        ↓
Local Model Pack
```

The reusable product logic lives in Rust.

The platform apps are thin native shells.

---

# 2. Final Stack

## Shared Core

```text
Rust
Cargo Workspace
UniFFI
ONNX Runtime
```

## macOS

```text
SwiftUI
AppKit
AVAudioEngine
NSPasteboard
Accessibility APIs where needed
```

## Android

```text
Kotlin
InputMethodService
AudioRecord
InputConnection.commitText()
```

## Core Rust Libraries

```text
serde
serde_json
toml
thiserror
anyhow
tokio
tracing
parking_lot
crossbeam-channel
directories
sha2
blake3
rubato
hound
```

## VAD

```text
WebRTC VAD
```

## Primary ASR Runtime

```text
ONNX Runtime
```

## V1 Default Model Target

```text
Parakeet TDT 0.6B V3
ONNX
INT8 quantized model pack
16 kHz mono audio
```

Canary-class support remains an architectural requirement, but V1 implementation should start with the Parakeet-class model because it is the better practical target for local macOS and Android latency.

---

# 3. Repository Structure

```text
/crates
  /dictation-core
  /audio-pipeline
  /asr-api
  /asr-onnx
  /model-pack
  /postprocess
  /ffi

/apps
  /macos
  /android

/tools
  /dictation-bench
  /model-packager
```

---

# 4. Core Runtime Flow

```text
Start Recording
    ↓
Capture PCM Audio
    ↓
Convert to f32 Mono
    ↓
Resample to 16 kHz
    ↓
Run VAD
    ↓
Finalize Audio Chunk
    ↓
Run ONNX ASR
    ↓
Post-process Text
    ↓
Insert Text into Active Field
```

V1 uses chunked transcription.

V1 does not implement token-level streaming.

---

# 5. Rust Core Responsibility

The Rust core owns:

```text
Recording session state
Audio buffering
Resampling
VAD
Chunking
Model loading
ASR execution
Post-processing
Language selection logic
Benchmarking hooks
```

Platform code must not contain dictation logic.

---

# 6. ASR Interface

All ASR execution goes through one internal Rust trait:

```rust
pub trait AsrEngine: Send {
    fn load(&mut self, config: &ModelConfig) -> Result<(), AsrError>;

    fn transcribe(
        &mut self,
        audio: &PcmAudio,
        options: &TranscribeOptions,
    ) -> Result<Transcript, AsrError>;

    fn capabilities(&self) -> AsrCapabilities;

    fn unload(&mut self) -> Result<(), AsrError>;
}
```

V1 implements only one concrete backend:

```text
OrtAsrEngine
```

No Sherpa backend.

No whisper.cpp backend.

No CoreML backend.

No second backend until the ONNX path is working and benchmarked.

---

# 7. FFI API

Platform apps talk to Rust through UniFFI.

The exported API should be product-level, not ML-level.

```rust
pub struct DictationController;

impl DictationController {
    pub fn new(config: DictationConfig) -> Result<Self, DictationError>;

    pub fn load_model(&self, model_id: String) -> Result<(), DictationError>;

    pub fn start_recording(&self) -> Result<(), DictationError>;

    pub fn push_audio(
        &self,
        samples: Vec<f32>,
        sample_rate_hz: u32,
        channels: u16,
    ) -> Result<(), DictationError>;

    pub fn stop_recording(&self) -> Result<Option<Transcript>, DictationError>;

    pub fn cancel_recording(&self) -> Result<(), DictationError>;

    pub fn set_language_mode(&self, mode: LanguageMode) -> Result<(), DictationError>;
}
```

Platform apps should never touch ONNX sessions directly.

---

# 8. Audio Pipeline

## Internal Format

```text
f32
mono
16 kHz
```

## Default Chunking Parameters

```text
Minimum speech chunk: 700 ms
Target chunk length: 5–15 s
Maximum chunk length: 15 s
Pre-roll: 250 ms
Post-roll: 500 ms
```

## Audio Rules

```text
Do not run inference in the audio callback.
Do not block the capture thread.
Use a bounded queue between capture and processing.
Run ASR on a worker thread.
```

---

# 9. Model Pack Format

Every model is installed as a local model pack.

```json
{
  "schemaVersion": 1,
  "modelId": "parakeet-tdt-0.6b-v3-int8",
  "displayName": "Parakeet TDT 0.6B V3 INT8",
  "runtime": "onnx",
  "quantization": "int8",

  "audio": {
    "sampleRateHz": 16000,
    "channels": 1,
    "sampleFormat": "f32"
  },

  "languages": ["en", "ru", "uk", "de", "fr", "es"],

  "capabilities": {
    "chunked": true,
    "streaming": false,
    "timestamps": true,
    "punctuation": true,
    "languageDetection": true
  },

  "files": [
    {
      "role": "model",
      "path": "model.onnx",
      "sha256": "..."
    },
    {
      "role": "tokenizer",
      "path": "tokenizer.model",
      "sha256": "..."
    }
  ]
}
```

The app loads models only through this manifest.

No model assumptions should be hard-coded in macOS or Android code.

---

# 10. Language Handling

Use this priority order:

```text
1. Active keyboard/input language
2. Platform input context
3. Model language auto-detection
4. Last used language
5. Manual override
```

Manual language selection exists only as a fallback.

---

# 11. macOS Implementation

The macOS app is a native SwiftUI/AppKit application.

It provides:

```text
Global hotkey
Hold-to-record
Toggle recording
Microphone permission handling
Model selection
Local settings
Text insertion
Benchmark screen
```

## macOS Audio

Use:

```text
AVAudioEngine
```

The Swift layer captures audio and sends PCM frames to Rust through UniFFI.

## macOS Text Insertion

Use clipboard paste for V1:

```text
Save current clipboard
Write transcript to clipboard
Send Cmd+V
Restore clipboard when safe
```

Accessibility insertion can be added later, but it is not part of the first implementation path.

---

# 12. Android Implementation

The Android app is a native Kotlin IME.

Use:

```text
InputMethodService
```

The dictation flow is:

```text
Mic button in keyboard
    ↓
AudioRecord capture
    ↓
Rust Dictation Core
    ↓
Transcript
    ↓
InputConnection.commitText()
```

Android should not be implemented as a normal note-taking app.

Android V1 must prove local dictation inside arbitrary text fields.

---

# 13. Post-processing

V1 supports only deterministic post-processing:

```text
Whitespace cleanup
Punctuation cleanup
Simple custom replacements
```

Example:

```json
{
  "канари": "Canary",
  "паркет": "Parakeet",
  "виспер": "Whisper"
}
```

No LLM rewriting.

No semantic correction.

No cloud post-processing.

---

# 14. Benchmarking

Build one benchmark CLI:

```text
dictation-bench
```

Example:

```bash
dictation-bench transcribe \
  --model ./models/parakeet-tdt-0.6b-v3-int8 \
  --audio ./fixtures/ru_10s.wav \
  --runs 30 \
  --output bench.json
```

Measure:

```text
Model load time
Audio preprocessing time
ASR inference time
Post-processing time
End-to-end latency
RTF
P50 latency
P95 latency
Peak memory
```

---

# 15. Performance Target

Primary benchmark device:

```text
Mac with M3-class CPU
```

Input:

```text
5–15 seconds of speech
```

Acceptance target:

```text
≤ 3 seconds after chunk finalization
```

Preferred target:

```text
≤ 1.5 seconds after chunk finalization
```

---

# 16. Implementation Plan

## Phase 1 — Rust Skeleton

Build:

```text
dictation-core
audio-pipeline
asr-api
model-pack
postprocess
ffi
```

Use a fake ASR backend first.

---

## Phase 2 — ONNX Backend

Build:

```text
asr-onnx
OrtAsrEngine
```

Validate:

```text
model loading
inference
memory usage
latency
```

---

## Phase 3 — macOS Prototype

Build:

```text
SwiftUI/AppKit shell
AVAudioEngine capture
global hotkey
clipboard insertion
model picker
settings
```

Goal:

```text
usable daily macOS dictation
```

---

## Phase 4 — Android IME

Build:

```text
Kotlin InputMethodService
AudioRecord capture
UniFFI integration
commitText insertion
```

Goal:

```text
working local dictation inside Android text fields
```

---

## Phase 5 — Optimization

Optimize:

```text
chunk size
VAD thresholds
thread count
model quantization
memory usage
cold start
warm inference latency
```

---

# 17. V1 Exclusions

Do not implement in V1:

```text
Tauri
Sherpa
whisper.cpp
CoreML backend
Cloud inference
iOS
LLM rewriting
Token-level streaming
Multiple ASR backends
Account system
Sync
Training or fine-tuning
```

---

# 18. Final Decision

V1 should be built as:

```text
Rust Dictation Core
+
UniFFI
+
Native SwiftUI/AppKit macOS App
+
Native Kotlin Android IME
+
ONNX Runtime
+
Parakeet TDT 0.6B V3 INT8 Model Pack
```

This is the cleanest implementation path because it keeps the hard product logic portable while preserving native integration where the operating systems demand it: hotkeys and text insertion on macOS, IME behavior on Android, and local ASR execution through a single Rust-owned runtime boundary.
