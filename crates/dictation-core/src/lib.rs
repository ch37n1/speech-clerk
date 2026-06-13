//! Product-level dictation orchestration.

use asr_api::{AsrEngine, ModelConfig, PcmAudio, TranscribeOptions, Transcript};
use postprocess::normalize_whitespace;

/// Product-level dictation controller.
pub struct DictationController<E> {
    engine: E,
}

impl<E> DictationController<E>
where
    E: AsrEngine,
{
    /// Create a controller around an ASR engine.
    #[must_use]
    pub fn new(engine: E) -> Self {
        Self { engine }
    }

    /// Load a model through the configured ASR engine.
    pub fn load_model(&mut self, model_id: impl Into<String>) -> Result<(), asr_api::AsrError> {
        self.engine.load(&ModelConfig {
            model_id: model_id.into(),
        })
    }

    /// Transcribe one finalized chunk and apply deterministic post-processing.
    pub fn transcribe_chunk(
        &mut self,
        audio: &PcmAudio,
        options: &TranscribeOptions,
    ) -> Result<Transcript, asr_api::AsrError> {
        let mut transcript = self.engine.transcribe(audio, options)?;
        transcript.text = normalize_whitespace(&transcript.text);
        Ok(transcript)
    }
}
