//! Backend-neutral ASR contracts.

/// Backend-neutral model loading configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelConfig {
    /// Stable model-pack identifier.
    pub model_id: String,
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
}

/// Capabilities reported by an ASR backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrCapabilities {
    /// Whether backend supports token-level streaming.
    pub supports_streaming: bool,
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
