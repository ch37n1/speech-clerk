//! Product-level dictation orchestration.

use std::fmt;
use std::path::PathBuf;

use asr_api::{AsrCapabilities, AsrEngine, AsrError, ModelConfig, PcmAudio, Transcript};
use audio_pipeline::{AudioBuffer, AudioFrame, AudioPipelineError};
use model_pack::{ModelManifest, ModelPack, ModelPackError};
use postprocess::{PostProcessor, ReplacementRule};

/// Product-level dictation configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictationConfig {
    /// Directory containing installed model-pack directories.
    pub model_packs_dir: PathBuf,
    /// Initial deterministic replacement rules.
    pub replacement_rules: Vec<ReplacementRule>,
}

impl DictationConfig {
    /// Create configuration with a model-pack root directory.
    #[must_use]
    pub fn new(model_packs_dir: impl Into<PathBuf>) -> Self {
        Self {
            model_packs_dir: model_packs_dir.into(),
            replacement_rules: Vec::new(),
        }
    }

    /// Add initial deterministic replacement rules.
    #[must_use]
    pub fn with_replacement_rules(mut self, replacement_rules: Vec<ReplacementRule>) -> Self {
        self.replacement_rules = replacement_rules;
        self
    }
}

/// Language selection mode for dictation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LanguageMode {
    /// Let the product and backend choose the language.
    #[default]
    Auto,
    /// Use an explicit BCP-47 language hint.
    Manual(String),
}

/// Visible recording state owned by the Rust core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingState {
    /// No active recording.
    Idle,
    /// The controller is accepting audio frames.
    Recording,
}

/// Product-level dictation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictationError {
    /// A recording method was called in the wrong state.
    InvalidState(String),
    /// A model-dependent operation was called before loading a model.
    ModelNotLoaded,
    /// Model-pack loading or validation failed.
    ModelPack(String),
    /// Audio validation or conversion failed.
    Audio(String),
    /// ASR execution failed.
    Asr(String),
}

impl fmt::Display for DictationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidState(message) => write!(formatter, "invalid dictation state: {message}"),
            Self::ModelNotLoaded => write!(formatter, "no model has been loaded"),
            Self::ModelPack(message) => write!(formatter, "model-pack error: {message}"),
            Self::Audio(message) => write!(formatter, "audio error: {message}"),
            Self::Asr(message) => write!(formatter, "asr error: {message}"),
        }
    }
}

impl std::error::Error for DictationError {}

impl From<ModelPackError> for DictationError {
    fn from(error: ModelPackError) -> Self {
        Self::ModelPack(error.to_string())
    }
}

impl From<AudioPipelineError> for DictationError {
    fn from(error: AudioPipelineError) -> Self {
        Self::Audio(error.to_string())
    }
}

impl From<AsrError> for DictationError {
    fn from(error: AsrError) -> Self {
        Self::Asr(error.to_string())
    }
}

/// Product-level dictation controller.
pub struct DictationController {
    engine: Box<dyn AsrEngine>,
    model_packs_dir: PathBuf,
    loaded_model: Option<ModelPack>,
    audio_buffer: AudioBuffer,
    post_processor: PostProcessor,
    language_mode: LanguageMode,
    state: RecordingState,
}

impl DictationController {
    /// Create a controller backed by the Phase 1 fake ASR engine.
    #[must_use]
    pub fn new(config: DictationConfig) -> Self {
        Self::with_engine(config, Box::new(FakeAsrEngine::default()))
    }

    /// Create a controller around an explicit ASR engine.
    #[must_use]
    pub fn with_engine(config: DictationConfig, engine: Box<dyn AsrEngine>) -> Self {
        Self {
            engine,
            model_packs_dir: config.model_packs_dir,
            loaded_model: None,
            audio_buffer: AudioBuffer::new(),
            post_processor: PostProcessor::with_replacements(config.replacement_rules),
            language_mode: LanguageMode::Auto,
            state: RecordingState::Idle,
        }
    }

    /// Load a model pack by id from the configured model-pack directory.
    pub fn load_model(&mut self, model_id: impl Into<String>) -> Result<(), DictationError> {
        let model_id = model_id.into();
        let pack = ModelPack::load_from_dir(self.model_packs_dir.join(&model_id))?;

        if pack.manifest().model_id != model_id {
            return Err(DictationError::ModelPack(format!(
                "requested model id {model_id} but manifest declares {}",
                pack.manifest().model_id
            )));
        }

        let config = model_config_from_manifest(pack.manifest());
        self.engine.load(&config)?;
        self.loaded_model = Some(pack);
        Ok(())
    }

    /// Start a recording session.
    pub fn start_recording(&mut self) -> Result<(), DictationError> {
        if self.loaded_model.is_none() {
            return Err(DictationError::ModelNotLoaded);
        }

        if self.state == RecordingState::Recording {
            return Err(DictationError::InvalidState(
                "recording has already started".to_owned(),
            ));
        }

        self.audio_buffer.clear();
        self.state = RecordingState::Recording;
        Ok(())
    }

    /// Push interleaved `f32` audio from a platform capture API.
    pub fn push_audio(
        &mut self,
        samples: Vec<f32>,
        sample_rate_hz: u32,
        channels: u16,
    ) -> Result<(), DictationError> {
        if self.state != RecordingState::Recording {
            return Err(DictationError::InvalidState(
                "audio can only be pushed while recording".to_owned(),
            ));
        }

        let frame = AudioFrame::new(samples, sample_rate_hz, channels)?;
        self.audio_buffer.push_frame(frame)?;
        Ok(())
    }

    /// Stop recording and return a transcript when audio was captured.
    pub fn stop_recording(&mut self) -> Result<Option<Transcript>, DictationError> {
        if self.state != RecordingState::Recording {
            return Err(DictationError::InvalidState(
                "recording has not started".to_owned(),
            ));
        }

        self.state = RecordingState::Idle;

        if self.audio_buffer.is_empty() {
            return Ok(None);
        }

        let chunk = self.audio_buffer.drain_chunk();
        let audio = PcmAudio {
            samples: chunk.samples,
            sample_rate_hz: chunk.sample_rate_hz,
        };
        let mut transcript = self
            .engine
            .transcribe(&audio, &self.transcribe_options_for_current_state())?;
        transcript.text = self.post_processor.process(&transcript.text);
        Ok(Some(transcript))
    }

    /// Cancel the active recording and discard buffered audio.
    pub fn cancel_recording(&mut self) -> Result<(), DictationError> {
        if self.state != RecordingState::Recording {
            return Err(DictationError::InvalidState(
                "recording has not started".to_owned(),
            ));
        }

        self.audio_buffer.clear();
        self.state = RecordingState::Idle;
        Ok(())
    }

    /// Set the language mode used for subsequent transcriptions.
    pub fn set_language_mode(&mut self, mode: LanguageMode) {
        self.language_mode = mode;
    }

    /// Replace deterministic post-processing rules.
    pub fn set_replacement_rules(&mut self, rules: Vec<ReplacementRule>) {
        self.post_processor.set_replacements(rules);
    }

    /// Current recording state.
    #[must_use]
    pub fn recording_state(&self) -> RecordingState {
        self.state
    }

    /// Loaded model id, if any.
    #[must_use]
    pub fn loaded_model_id(&self) -> Option<&str> {
        self.loaded_model
            .as_ref()
            .map(|pack| pack.manifest().model_id.as_str())
    }

    fn transcribe_options_for_current_state(&self) -> asr_api::TranscribeOptions {
        let language_hint = match &self.language_mode {
            LanguageMode::Auto => None,
            LanguageMode::Manual(language) => Some(language.clone()),
        };

        asr_api::TranscribeOptions { language_hint }
    }
}

/// Phase 1 fake backend implementing the real ASR engine trait.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FakeAsrEngine {
    loaded_model: Option<ModelConfig>,
}

impl FakeAsrEngine {
    /// Return the currently loaded model id.
    #[must_use]
    pub fn loaded_model_id(&self) -> Option<&str> {
        self.loaded_model
            .as_ref()
            .map(|config| config.model_id.as_str())
    }
}

impl AsrEngine for FakeAsrEngine {
    fn load(&mut self, config: &ModelConfig) -> Result<(), AsrError> {
        if config.runtime != "fake" {
            return Err(AsrError::LoadFailed(format!(
                "fake engine cannot load runtime {}",
                config.runtime
            )));
        }

        self.loaded_model = Some(config.clone());
        Ok(())
    }

    fn transcribe(
        &mut self,
        audio: &PcmAudio,
        options: &asr_api::TranscribeOptions,
    ) -> Result<Transcript, AsrError> {
        if audio.sample_rate_hz == 0 {
            return Err(AsrError::TranscriptionFailed(
                "audio sample rate must be greater than zero".to_owned(),
            ));
        }

        let model = self.loaded_model.as_ref().ok_or_else(|| {
            AsrError::TranscriptionFailed("fake model has not been loaded".to_owned())
        })?;
        let duration_ms = audio.samples.len() as u64 * 1_000 / u64::from(audio.sample_rate_hz);
        let language = options
            .language_hint
            .clone()
            .or_else(|| model.languages.first().cloned());
        let language_text = language.as_deref().unwrap_or("auto");

        Ok(Transcript {
            text: format!(
                "fake parakeet dictation from {} , captured {} milliseconds in {}",
                model.display_name, duration_ms, language_text
            ),
            language,
        })
    }

    fn capabilities(&self) -> AsrCapabilities {
        AsrCapabilities {
            supports_chunked: true,
            supports_streaming: false,
            supports_punctuation: true,
            supports_language_detection: false,
        }
    }

    fn unload(&mut self) -> Result<(), AsrError> {
        self.loaded_model = None;
        Ok(())
    }
}

fn model_config_from_manifest(manifest: &ModelManifest) -> ModelConfig {
    ModelConfig::new(
        manifest.model_id.clone(),
        manifest.display_name.clone(),
        manifest.runtime.clone(),
        manifest.languages.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::{DictationConfig, DictationController, DictationError, LanguageMode};
    use postprocess::ReplacementRule;
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
    fn completes_fake_recording_flow() -> Result<(), Box<dyn std::error::Error>> {
        let root = create_fake_pack()?;
        let config = DictationConfig::new(&root)
            .with_replacement_rules(vec![ReplacementRule::new("parakeet", "Canary")]);
        let mut controller = DictationController::new(config);

        controller.load_model("fake-local")?;
        controller.start_recording()?;
        controller.push_audio(vec![0.1; 16_000], 16_000, 1)?;
        let transcript = controller.stop_recording()?;

        assert!(matches!(
            transcript.as_ref().map(|value| value.text.as_str()),
            Some("fake Canary dictation from Fake Local Model, captured 1000 milliseconds in en")
        ));
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn rejects_audio_before_recording() {
        let config = DictationConfig::new("unused");
        let mut controller = DictationController::new(config);
        let result = controller.push_audio(vec![0.0], 16_000, 1);

        assert!(matches!(result, Err(DictationError::InvalidState(_))));
    }

    #[test]
    fn uses_manual_language_hint() -> Result<(), Box<dyn std::error::Error>> {
        let root = create_fake_pack()?;
        let mut controller = DictationController::new(DictationConfig::new(&root));

        controller.load_model("fake-local")?;
        controller.set_language_mode(LanguageMode::Manual("ru".to_owned()));
        controller.start_recording()?;
        controller.push_audio(vec![0.1; 8_000], 16_000, 1)?;
        let transcript = controller.stop_recording()?;

        assert!(matches!(
            transcript
                .as_ref()
                .and_then(|value| value.language.as_deref()),
            Some("ru")
        ));
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    fn create_fake_pack() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let root = unique_temp_dir("speech-clerk-dictation-core")?;
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
