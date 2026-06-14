//! Product-level FFI facade for native shells.
//!
//! The public surface in this crate mirrors `speech_clerk.udl`, keeping platform
//! code on controller-level operations instead of ASR or model runtime details.

use std::fmt;
use std::sync::Mutex;

uniffi::include_scaffolding!("speech_clerk");

use asr_api::{
    AsrCapabilities, AsrEngine, AsrError, ModelConfig, PcmAudio, Transcript as AsrTranscript,
};
use asr_onnx::OrtAsrEngine;
use dictation_core::{
    DictationConfig as CoreDictationConfig, DictationController as CoreDictationController,
    FakeAsrEngine, LanguageMode as CoreLanguageMode,
};
use postprocess::ReplacementRule as CoreReplacementRule;

/// FFI-friendly dictation configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictationConfig {
    /// Directory containing installed model-pack directories.
    pub model_packs_dir: String,
}

impl DictationConfig {
    /// Create an FFI configuration.
    #[must_use]
    pub fn new(model_packs_dir: impl Into<String>) -> Self {
        Self {
            model_packs_dir: model_packs_dir.into(),
        }
    }
}

/// FFI-friendly replacement rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementRule {
    /// Text to replace.
    pub pattern: String,
    /// Replacement text.
    pub replacement: String,
}

impl ReplacementRule {
    /// Create an FFI replacement rule.
    #[must_use]
    pub fn new(pattern: impl Into<String>, replacement: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            replacement: replacement.into(),
        }
    }
}

/// FFI-friendly language mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LanguageMode {
    /// Automatic language selection.
    Auto,
    /// Manual language hint.
    Manual,
}

/// FFI-friendly transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    /// Post-processed transcript text.
    pub text: String,
    /// Optional BCP-47 language tag.
    pub language: Option<String>,
}

/// FFI boundary errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictationFfiError {
    /// The controller mutex was poisoned.
    ControllerUnavailable,
    /// The dictation core returned an error.
    Core,
}

impl fmt::Display for DictationFfiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ControllerUnavailable => write!(formatter, "dictation controller unavailable"),
            Self::Core => write!(formatter, "dictation core error"),
        }
    }
}

impl std::error::Error for DictationFfiError {}

/// Product-level dictation controller exposed to native apps.
pub struct DictationController {
    inner: Mutex<CoreDictationController>,
}

impl DictationController {
    /// Create a controller with the Phase 1 fake ASR backend.
    #[must_use]
    pub fn new(config: DictationConfig) -> Self {
        let core_config = CoreDictationConfig::new(config.model_packs_dir);
        Self {
            inner: Mutex::new(CoreDictationController::with_engine(
                core_config,
                Box::<RuntimeDispatchAsrEngine>::default(),
            )),
        }
    }

    /// Load a local model pack by id.
    pub fn load_model(&self, model_id: String) -> Result<(), DictationFfiError> {
        self.with_controller(|controller| controller.load_model(model_id))
    }

    /// Start recording.
    pub fn start_recording(&self) -> Result<(), DictationFfiError> {
        self.with_controller(CoreDictationController::start_recording)
    }

    /// Push interleaved `f32` audio frames to Rust.
    pub fn push_audio(
        &self,
        samples: Vec<f32>,
        sample_rate_hz: u32,
        channels: u16,
    ) -> Result<(), DictationFfiError> {
        self.with_controller(|controller| controller.push_audio(samples, sample_rate_hz, channels))
    }

    /// Stop recording and return a post-processed transcript when audio was captured.
    pub fn stop_recording(&self) -> Result<Option<Transcript>, DictationFfiError> {
        self.with_controller(|controller| {
            controller.stop_recording().map(|maybe_transcript| {
                maybe_transcript.map(|transcript| Transcript {
                    text: transcript.text,
                    language: transcript.language,
                })
            })
        })
    }

    /// Cancel the active recording.
    pub fn cancel_recording(&self) -> Result<(), DictationFfiError> {
        self.with_controller(CoreDictationController::cancel_recording)
    }

    /// Set language selection mode.
    pub fn set_language_mode(
        &self,
        mode: LanguageMode,
        manual_language: Option<String>,
    ) -> Result<(), DictationFfiError> {
        self.with_controller(|controller| {
            controller.set_language_mode(match mode {
                LanguageMode::Auto => CoreLanguageMode::Auto,
                LanguageMode::Manual => match manual_language {
                    Some(language) if !language.is_empty() => CoreLanguageMode::Manual(language),
                    _ => CoreLanguageMode::Auto,
                },
            });
            Ok(())
        })
    }

    /// Replace deterministic post-processing rules.
    pub fn set_replacement_rules(
        &self,
        rules: Vec<ReplacementRule>,
    ) -> Result<(), DictationFfiError> {
        self.with_controller(|controller| {
            controller.set_replacement_rules(
                rules
                    .into_iter()
                    .map(|rule| CoreReplacementRule::new(rule.pattern, rule.replacement))
                    .collect(),
            );
            Ok(())
        })
    }

    fn with_controller<T>(
        &self,
        operation: impl FnOnce(
            &mut CoreDictationController,
        ) -> Result<T, dictation_core::DictationError>,
    ) -> Result<T, DictationFfiError> {
        let mut controller = self
            .inner
            .lock()
            .map_err(|_| DictationFfiError::ControllerUnavailable)?;
        operation(&mut controller).map_err(|_error| DictationFfiError::Core)
    }
}

#[derive(Debug, Default)]
enum ActiveEngine {
    #[default]
    None,
    Fake,
    Onnx,
}

#[derive(Debug, Default)]
struct RuntimeDispatchAsrEngine {
    fake: FakeAsrEngine,
    onnx: OrtAsrEngine,
    active: ActiveEngine,
}

impl AsrEngine for RuntimeDispatchAsrEngine {
    fn load(&mut self, config: &ModelConfig) -> Result<(), AsrError> {
        match config.runtime.as_str() {
            "fake" => {
                self.fake.load(config)?;
                self.active = ActiveEngine::Fake;
                Ok(())
            }
            "onnx" => {
                self.onnx.load(config)?;
                self.active = ActiveEngine::Onnx;
                Ok(())
            }
            runtime => Err(AsrError::LoadFailed(format!(
                "unsupported runtime {runtime}"
            ))),
        }
    }

    fn transcribe(
        &mut self,
        audio: &PcmAudio,
        options: &asr_api::TranscribeOptions,
    ) -> Result<AsrTranscript, AsrError> {
        match self.active {
            ActiveEngine::Fake => self.fake.transcribe(audio, options),
            ActiveEngine::Onnx => self.onnx.transcribe(audio, options),
            ActiveEngine::None => Err(AsrError::TranscriptionFailed(
                "no runtime engine has been loaded".to_owned(),
            )),
        }
    }

    fn capabilities(&self) -> AsrCapabilities {
        match self.active {
            ActiveEngine::Fake => self.fake.capabilities(),
            ActiveEngine::Onnx => self.onnx.capabilities(),
            ActiveEngine::None => AsrCapabilities {
                supports_chunked: true,
                supports_streaming: false,
                supports_punctuation: true,
                supports_language_detection: false,
            },
        }
    }

    fn unload(&mut self) -> Result<(), AsrError> {
        match self.active {
            ActiveEngine::Fake => self.fake.unload()?,
            ActiveEngine::Onnx => self.onnx.unload()?,
            ActiveEngine::None => {}
        }
        self.active = ActiveEngine::None;
        Ok(())
    }
}

/// Return the FFI boundary name.
#[must_use]
pub fn ffi_boundary_name() -> &'static str {
    "speech-clerk-ffi"
}

#[cfg(test)]
mod tests {
    use super::{
        DictationConfig, DictationController, LanguageMode, ReplacementRule, ffi_boundary_name,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    const MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "modelId": "fake-local",
  "displayName": "Fake Local Model",
  "runtime": "fake",
  "quantization": "none",
  "audio": {
    "sampleRateHz": 16000,
    "channels": 1,
    "sampleFormat": "f32"
  },
  "languages": ["en"],
  "capabilities": {
    "chunked": true,
    "streaming": false,
    "timestamps": false,
    "punctuation": true,
    "languageDetection": false
  },
  "files": [
    {
      "role": "fake-config",
      "path": "fake_asr.txt",
      "sha256": ""
    }
  ]
}"#;

    #[test]
    fn exposes_product_level_recording_flow() -> Result<(), Box<dyn std::error::Error>> {
        let root = create_fake_pack()?;
        let controller =
            DictationController::new(DictationConfig::new(root.to_string_lossy().into_owned()));

        controller.load_model("fake-local".to_owned())?;
        controller.set_language_mode(LanguageMode::Manual, Some("ru".to_owned()))?;
        controller.set_replacement_rules(vec![ReplacementRule::new("parakeet", "Canary")])?;
        controller.start_recording()?;
        controller.push_audio(vec![0.1; 16_000], 16_000, 1)?;
        let transcript = controller.stop_recording()?;

        assert!(matches!(
            transcript.as_ref().map(|value| value.text.as_str()),
            Some("fake Canary dictation from Fake Local Model, captured 1000 milliseconds in ru")
        ));
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn reports_boundary_name() {
        assert_eq!(ffi_boundary_name(), "speech-clerk-ffi");
    }

    fn create_fake_pack() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let root = unique_temp_dir("speech-clerk-ffi")?;
        let pack_dir = root.join("fake-local");
        fs::create_dir_all(&pack_dir)?;
        fs::write(pack_dir.join("manifest.json"), MANIFEST)?;
        fs::write(pack_dir.join("fake_asr.txt"), "fake")?;
        Ok(root)
    }

    fn unique_temp_dir(prefix: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
        Ok(std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            now.as_nanos()
        )))
    }
}
