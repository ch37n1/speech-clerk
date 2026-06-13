//! Audio pipeline primitives.

use core::fmt;

/// V1 internal sample rate.
pub const INTERNAL_SAMPLE_RATE_HZ: u32 = 16_000;

/// Audio pipeline validation and conversion errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioPipelineError {
    /// The incoming sample rate was zero.
    InvalidSampleRate,
    /// The incoming channel count was zero.
    InvalidChannelCount,
    /// The interleaved sample count was not divisible by the channel count.
    IncompleteInterleavedFrame {
        /// Number of samples passed by the platform layer.
        samples: usize,
        /// Number of channels declared by the platform layer.
        channels: u16,
    },
    /// A sample was NaN or infinite.
    NonFiniteSample,
    /// Resampling would create more samples than this process can address.
    TooManySamples,
}

impl fmt::Display for AudioPipelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRate => write!(formatter, "sample rate must be greater than zero"),
            Self::InvalidChannelCount => {
                write!(formatter, "channel count must be greater than zero")
            }
            Self::IncompleteInterleavedFrame { samples, channels } => write!(
                formatter,
                "sample count {samples} is not divisible by channel count {channels}"
            ),
            Self::NonFiniteSample => write!(formatter, "audio samples must be finite f32 values"),
            Self::TooManySamples => write!(formatter, "audio conversion produced too many samples"),
        }
    }
}

impl std::error::Error for AudioPipelineError {}

/// Interleaved audio frame received from a platform capture API.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrame {
    /// Interleaved `f32` samples.
    pub samples: Vec<f32>,
    /// Source sample rate in hertz.
    pub sample_rate_hz: u32,
    /// Number of channels in the interleaved input.
    pub channels: u16,
}

impl AudioFrame {
    /// Create and validate an incoming platform audio frame.
    pub fn new(
        samples: Vec<f32>,
        sample_rate_hz: u32,
        channels: u16,
    ) -> Result<Self, AudioPipelineError> {
        if sample_rate_hz == 0 {
            return Err(AudioPipelineError::InvalidSampleRate);
        }

        if channels == 0 {
            return Err(AudioPipelineError::InvalidChannelCount);
        }

        let channel_count = usize::from(channels);
        if samples.len() % channel_count != 0 {
            return Err(AudioPipelineError::IncompleteInterleavedFrame {
                samples: samples.len(),
                channels,
            });
        }

        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(AudioPipelineError::NonFiniteSample);
        }

        Ok(Self {
            samples,
            sample_rate_hz,
            channels,
        })
    }
}

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

/// Bounded-owner buffer for one dictation recording.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct AudioBuffer {
    samples: Vec<f32>,
}

impl AudioBuffer {
    /// Create an empty audio buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a platform frame after converting it to the V1 mono 16 kHz format.
    pub fn push_frame(&mut self, frame: AudioFrame) -> Result<(), AudioPipelineError> {
        let mono = downmix_to_mono(&frame);
        let normalized = resample_linear(&mono, frame.sample_rate_hz)?;
        self.samples.extend(normalized);
        Ok(())
    }

    /// Remove all buffered samples and return a finalized chunk.
    #[must_use]
    pub fn drain_chunk(&mut self) -> AudioChunk {
        let samples = core::mem::take(&mut self.samples);
        AudioChunk::new(samples)
    }

    /// Discard buffered samples.
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// Return `true` when no samples are buffered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Number of normalized mono samples currently buffered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }
}

fn downmix_to_mono(frame: &AudioFrame) -> Vec<f32> {
    let channels = usize::from(frame.channels);
    let mut mono = Vec::with_capacity(frame.samples.len() / channels);

    for interleaved in frame.samples.chunks_exact(channels) {
        let sum = interleaved.iter().copied().sum::<f32>();
        mono.push(sum / f32::from(frame.channels));
    }

    mono
}

fn resample_linear(samples: &[f32], source_rate_hz: u32) -> Result<Vec<f32>, AudioPipelineError> {
    if source_rate_hz == INTERNAL_SAMPLE_RATE_HZ || samples.is_empty() {
        return Ok(samples.to_vec());
    }

    let target_len_u128 = (samples.len() as u128 * u128::from(INTERNAL_SAMPLE_RATE_HZ))
        .div_ceil(u128::from(source_rate_hz));
    let target_len =
        usize::try_from(target_len_u128).map_err(|_| AudioPipelineError::TooManySamples)?;

    if target_len == 0 {
        return Ok(Vec::new());
    }

    if samples.len() == 1 {
        return Ok(vec![samples[0]; target_len]);
    }

    let source_rate = f64::from(source_rate_hz);
    let target_rate = f64::from(INTERNAL_SAMPLE_RATE_HZ);
    let mut output = Vec::with_capacity(target_len);

    for target_index in 0..target_len {
        let source_position = target_index as f64 * source_rate / target_rate;
        let left_index = source_position.floor() as usize;
        let right_index = left_index.saturating_add(1).min(samples.len() - 1);
        let fraction = (source_position - left_index as f64) as f32;
        let left = samples[left_index.min(samples.len() - 1)];
        let right = samples[right_index];
        output.push(left + (right - left) * fraction);
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::{AudioBuffer, AudioChunk, AudioFrame, AudioPipelineError, INTERNAL_SAMPLE_RATE_HZ};

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

    #[test]
    fn rejects_incomplete_interleaved_frames() {
        let result = AudioFrame::new(vec![0.0, 0.1, 0.2], 48_000, 2);

        assert_eq!(
            result,
            Err(AudioPipelineError::IncompleteInterleavedFrame {
                samples: 3,
                channels: 2
            })
        );
    }

    #[test]
    fn downmixes_stereo_and_resamples_to_internal_rate() -> Result<(), AudioPipelineError> {
        let frame = AudioFrame::new(vec![0.0, 0.5, 0.5, 1.0, 1.0, 1.0], 48_000, 2)?;
        let mut buffer = AudioBuffer::new();

        buffer.push_frame(frame)?;
        let chunk = buffer.drain_chunk();

        assert_eq!(chunk.sample_rate_hz, INTERNAL_SAMPLE_RATE_HZ);
        assert_eq!(chunk.samples.len(), 1);
        assert_eq!(chunk.samples[0], 0.25);
        Ok(())
    }
}
