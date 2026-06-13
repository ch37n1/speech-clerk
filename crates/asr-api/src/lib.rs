//! Backend-neutral ASR contracts.

use core::fmt;

/// Backend-neutral model loading configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelConfig {
    /// Stable model-pack identifier.
    pub model_id: String,
    /// Human-readable model name.
    pub display_name: String,
    /// Runtime identifier from the model-pack manifest.
    pub runtime: String,
    /// Languages advertised by the model pack.
    pub languages: Vec<String>,
}

impl ModelConfig {
    /// Create a model loading configuration.
    #[must_use]
    pub fn new(
        model_id: impl Into<String>,
        display_name: impl Into<String>,
        runtime: impl Into<String>,
        languages: Vec<String>,
    ) -> Self {
        Self {
            model_id: model_id.into(),
            display_name: display_name.into(),
            runtime: runtime.into(),
            languages,
        }
    }
}

/// Audio expected by ASR backends.
#[derive(Debug, Clone, PartialEq)]
pub struct PcmAudio {
    /// Mono `f32` samples.
    pub samples: Vec<f32>,
    /// Sample rate in hertz.
    pub sample_rate_hz: u32,
}

/// Transcription options owned by the dictation product layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscribeOptions {
    /// Optional BCP-47 language hint.
    pub language_hint: Option<String>,
}

/// A completed transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    /// Transcript text after backend decoding and before product post-processing.
    pub text: String,
    /// Optional BCP-47 language tag chosen by the backend.
    pub language: Option<String>,
}

impl Transcript {
    /// Create a transcript with no backend language decision.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            language: None,
        }
    }
}

/// Capabilities reported by an ASR backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrCapabilities {
    /// Whether backend supports finalized audio chunks.
    pub supports_chunked: bool,
    /// Whether backend supports token-level streaming.
    pub supports_streaming: bool,
    /// Whether backend can emit punctuation.
    pub supports_punctuation: bool,
    /// Whether backend can detect language automatically.
    pub supports_language_detection: bool,
}

/// Backend-neutral ASR errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsrError {
    /// Model loading failed.
    LoadFailed(String),
    /// Transcription failed.
    TranscriptionFailed(String),
    /// Model unloading failed.
    UnloadFailed(String),
}

impl fmt::Display for AsrError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoadFailed(message) => write!(formatter, "model load failed: {message}"),
            Self::TranscriptionFailed(message) => {
                write!(formatter, "transcription failed: {message}")
            }
            Self::UnloadFailed(message) => write!(formatter, "model unload failed: {message}"),
        }
    }
}

impl std::error::Error for AsrError {}

/// Interface implemented by concrete ASR engines.
pub trait AsrEngine: Send {
    /// Load a model into the engine.
    fn load(&mut self, config: &ModelConfig) -> Result<(), AsrError>;

    /// Transcribe a finalized audio chunk.
    fn transcribe(
        &mut self,
        audio: &PcmAudio,
        options: &TranscribeOptions,
    ) -> Result<Transcript, AsrError>;

    /// Report backend capabilities.
    fn capabilities(&self) -> AsrCapabilities;

    /// Release backend resources.
    fn unload(&mut self) -> Result<(), AsrError>;
}
