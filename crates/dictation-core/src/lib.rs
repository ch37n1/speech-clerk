//! Product-level dictation orchestration.

use std::fmt;
use std::path::PathBuf;

use asr_api::{
    AsrCapabilities, AsrEngine, AsrError, ModelAsset, ModelConfig, PcmAudio, Transcript,
};
use audio_pipeline::{AudioFrame, AudioPipelineError, AudioProcessor, AudioProcessorConfig};
use model_pack::{ModelPack, ModelPackError};
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

/// Platform language signals used by the Rust-owned V1 priority order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LanguageContext {
    /// Active keyboard or input-method language, when the platform exposes it.
    pub active_keyboard_language: Option<String>,
    /// Language inferred from the active input field or editor context.
    pub platform_input_language: Option<String>,
    /// Explicit user override kept as the final fallback.
    pub manual_override: Option<String>,
}

impl LanguageContext {
    /// Create a normalized language context from platform language tags.
    #[must_use]
    pub fn new(
        active_keyboard_language: Option<String>,
        platform_input_language: Option<String>,
        manual_override: Option<String>,
    ) -> Self {
        Self {
            active_keyboard_language: normalize_language_tag(active_keyboard_language),
            platform_input_language: normalize_language_tag(platform_input_language),
            manual_override: normalize_language_tag(manual_override),
        }
    }

    fn language_hint(
        &self,
        backend_supports_language_detection: bool,
        last_used_language: Option<&str>,
    ) -> Option<String> {
        if let Some(language) = &self.active_keyboard_language {
            return Some(language.clone());
        }

        if let Some(language) = &self.platform_input_language {
            return Some(language.clone());
        }

        if backend_supports_language_detection {
            return None;
        }

        normalize_language_tag(last_used_language.map(str::to_owned))
            .or_else(|| self.manual_override.clone())
    }
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
    audio_processor_config: AudioProcessorConfig,
    audio_processor: Option<AudioProcessor>,
    post_processor: PostProcessor,
    language_context: LanguageContext,
    last_used_language: Option<String>,
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
            audio_processor_config: AudioProcessorConfig::default(),
            audio_processor: None,
            post_processor: PostProcessor::with_replacements(config.replacement_rules),
            language_context: LanguageContext::default(),
            last_used_language: None,
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

        let config = model_config_from_pack(&pack);
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

        self.audio_processor = Some(AudioProcessor::start(self.audio_processor_config.clone())?);
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
        let processor = self.audio_processor.as_ref().ok_or_else(|| {
            DictationError::InvalidState("audio processor has not started".to_owned())
        })?;
        processor.try_push_frame(frame)?;
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
        let Some(processor) = self.audio_processor.take() else {
            return Err(DictationError::InvalidState(
                "audio processor has not started".to_owned(),
            ));
        };
        let chunks = processor.finish()?;

        if chunks.is_empty() {
            return Ok(None);
        }

        let options = self.transcribe_options_for_current_state();
        let mut transcript_text = String::new();
        let mut transcript_language = None;

        for chunk in chunks {
            let audio = PcmAudio {
                samples: chunk.samples,
                sample_rate_hz: chunk.sample_rate_hz,
            };
            let transcript = self.engine.transcribe(&audio, &options)?;
            if transcript_language.is_none() {
                transcript_language = transcript.language.clone();
            }
            if !transcript.text.trim().is_empty() {
                if !transcript_text.is_empty() {
                    transcript_text.push(' ');
                }
                transcript_text.push_str(transcript.text.trim());
            }
        }

        let transcript = Transcript {
            text: self.post_processor.process(&transcript_text),
            language: transcript_language,
        };

        if let Some(language) = &transcript.language {
            self.last_used_language = Some(language.clone());
        }

        Ok(Some(transcript))
    }

    /// Cancel the active recording and discard buffered audio.
    pub fn cancel_recording(&mut self) -> Result<(), DictationError> {
        if self.state != RecordingState::Recording {
            return Err(DictationError::InvalidState(
                "recording has not started".to_owned(),
            ));
        }

        if let Some(processor) = self.audio_processor.take() {
            processor.cancel()?;
        }
        self.state = RecordingState::Idle;
        Ok(())
    }

    /// Set the language mode used for subsequent transcriptions.
    pub fn set_language_mode(&mut self, mode: LanguageMode) {
        self.language_context.manual_override = match mode {
            LanguageMode::Auto => None,
            LanguageMode::Manual(language) => normalize_language_tag(Some(language)),
        };
    }

    /// Replace the platform language context used for subsequent transcriptions.
    pub fn set_language_context(&mut self, context: LanguageContext) {
        self.language_context = LanguageContext::new(
            context.active_keyboard_language,
            context.platform_input_language,
            context.manual_override,
        );
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
        let language_hint = self.language_context.language_hint(
            self.engine.capabilities().supports_language_detection,
            self.last_used_language.as_deref(),
        );

        asr_api::TranscribeOptions { language_hint }
    }
}

fn normalize_language_tag(language: Option<String>) -> Option<String> {
    language
        .map(|value| value.trim().replace('_', "-"))
        .filter(|value| !value.is_empty())
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

fn model_config_from_pack(pack: &ModelPack) -> ModelConfig {
    let manifest = pack.manifest();
    let files = manifest
        .files
        .iter()
        .map(|file| {
            ModelAsset::new(
                file.role.clone(),
                file.path.clone(),
                pack.path().join(&file.path),
                file.sha256.clone(),
            )
        })
        .collect();

    ModelConfig::new(
        manifest.model_id.clone(),
        manifest.display_name.clone(),
        manifest.runtime.clone(),
        manifest.languages.clone(),
    )
    .with_assets(pack.path().to_path_buf(), files)
}

#[cfg(test)]
mod tests {
    use super::{
        DictationConfig, DictationController, DictationError, LanguageContext, LanguageMode,
    };
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
        controller.push_audio(vec![0.1; 16_000], 16_000, 1)?;
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

    #[test]
    fn active_keyboard_language_beats_manual_language() -> Result<(), Box<dyn std::error::Error>> {
        let root = create_fake_pack()?;
        let mut controller = DictationController::new(DictationConfig::new(&root));

        controller.load_model("fake-local")?;
        controller.set_language_context(LanguageContext::new(
            Some("ru_RU".to_owned()),
            None,
            Some("en".to_owned()),
        ));
        controller.start_recording()?;
        controller.push_audio(vec![0.1; 16_000], 16_000, 1)?;
        let transcript = controller.stop_recording()?;

        assert!(matches!(
            transcript
                .as_ref()
                .and_then(|value| value.language.as_deref()),
            Some("ru-RU")
        ));
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn platform_language_is_second_priority() -> Result<(), Box<dyn std::error::Error>> {
        let root = create_fake_pack()?;
        let mut controller = DictationController::new(DictationConfig::new(&root));

        controller.load_model("fake-local")?;
        controller.set_language_context(LanguageContext::new(
            None,
            Some("de".to_owned()),
            Some("en".to_owned()),
        ));
        controller.start_recording()?;
        controller.push_audio(vec![0.1; 16_000], 16_000, 1)?;
        let transcript = controller.stop_recording()?;

        assert!(matches!(
            transcript
                .as_ref()
                .and_then(|value| value.language.as_deref()),
            Some("de")
        ));
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn last_used_language_beats_manual_fallback() -> Result<(), Box<dyn std::error::Error>> {
        let root = create_fake_pack()?;
        let mut controller = DictationController::new(DictationConfig::new(&root));

        controller.load_model("fake-local")?;
        controller.set_language_context(LanguageContext::new(Some("ru".to_owned()), None, None));
        controller.start_recording()?;
        controller.push_audio(vec![0.1; 16_000], 16_000, 1)?;
        let _ = controller.stop_recording()?;

        controller.set_language_context(LanguageContext::new(None, None, Some("en".to_owned())));
        controller.start_recording()?;
        controller.push_audio(vec![0.1; 16_000], 16_000, 1)?;
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

    #[test]
    fn language_hint_defers_to_model_auto_detection_before_manual() {
        let context = LanguageContext::new(None, None, Some("en".to_owned()));

        assert_eq!(context.language_hint(true, Some("ru")), None);
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
