//! Audio pipeline primitives.

/// V1 internal sample rate.
pub const INTERNAL_SAMPLE_RATE_HZ: u32 = 16_000;

/// Normalized audio chunk used inside the Rust core.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioChunk {
    /// Mono `f32` samples.
    pub samples: Vec<f32>,
    /// Sample rate in hertz.
    pub sample_rate_hz: u32,
}

impl AudioChunk {
    /// Create a normalized V1 chunk.
    #[must_use]
    pub fn new(samples: Vec<f32>) -> Self {
        Self {
            samples,
            sample_rate_hz: INTERNAL_SAMPLE_RATE_HZ,
        }
    }

    /// Return `true` when the chunk contains no audio samples.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioChunk, INTERNAL_SAMPLE_RATE_HZ};

    #[test]
    fn new_chunk_uses_internal_sample_rate() {
        let chunk = AudioChunk::new(vec![0.0, 0.5]);

        assert_eq!(chunk.sample_rate_hz, INTERNAL_SAMPLE_RATE_HZ);
    }

    #[test]
    fn new_chunk_is_not_empty_when_samples_exist() {
        let chunk = AudioChunk::new(vec![0.0, 0.5]);

        assert!(!chunk.is_empty());
    }
}
