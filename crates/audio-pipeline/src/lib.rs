//! Audio pipeline primitives.

use core::fmt;
use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};

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
    /// The bounded processing queue is full.
    QueueFull,
    /// The audio worker has already stopped.
    WorkerStopped,
    /// The audio worker thread stopped unexpectedly.
    WorkerPanicked,
    /// Audio chunking settings are invalid.
    InvalidChunkingConfig(String),
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
            Self::QueueFull => write!(formatter, "audio processing queue is full"),
            Self::WorkerStopped => write!(formatter, "audio processing worker has stopped"),
            Self::WorkerPanicked => write!(formatter, "audio processing worker panicked"),
            Self::InvalidChunkingConfig(message) => {
                write!(formatter, "invalid chunking config: {message}")
            }
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
        let normalized = normalize_frame(frame)?;
        self.samples.extend(normalized.samples);
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

/// Convert a platform frame to the V1 mono 16 kHz internal format.
pub fn normalize_frame(frame: AudioFrame) -> Result<AudioChunk, AudioPipelineError> {
    let mono = downmix_to_mono(&frame);
    let normalized = resample_linear(&mono, frame.sample_rate_hz)?;
    Ok(AudioChunk::new(normalized))
}

/// V1 chunking configuration in milliseconds and normalized samples.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkingConfig {
    /// Minimum speech duration required before a chunk can be finalized.
    pub minimum_speech_ms: u32,
    /// Lower bound of the target finalized chunk length.
    pub target_min_ms: u32,
    /// Upper bound of the target finalized chunk length.
    pub target_max_ms: u32,
    /// Hard maximum finalized chunk length.
    pub max_chunk_ms: u32,
    /// Audio retained before detected speech.
    pub pre_roll_ms: u32,
    /// Audio retained after speech before finalizing.
    pub post_roll_ms: u32,
    /// Absolute-amplitude threshold used by the lightweight V1 chunker.
    pub speech_threshold: f32,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            minimum_speech_ms: 700,
            target_min_ms: 5_000,
            target_max_ms: 15_000,
            max_chunk_ms: 15_000,
            pre_roll_ms: 250,
            post_roll_ms: 500,
            speech_threshold: 0.001,
        }
    }
}

impl ChunkingConfig {
    fn validate(&self) -> Result<(), AudioPipelineError> {
        if self.minimum_speech_ms == 0 {
            return Err(AudioPipelineError::InvalidChunkingConfig(
                "minimum_speech_ms must be greater than zero".to_owned(),
            ));
        }

        if self.target_min_ms == 0 || self.target_min_ms > self.target_max_ms {
            return Err(AudioPipelineError::InvalidChunkingConfig(
                "target_min_ms must be greater than zero and no larger than target_max_ms"
                    .to_owned(),
            ));
        }

        if self.target_max_ms > self.max_chunk_ms {
            return Err(AudioPipelineError::InvalidChunkingConfig(
                "target_max_ms must be no larger than max_chunk_ms".to_owned(),
            ));
        }

        if !self.speech_threshold.is_finite() || self.speech_threshold < 0.0 {
            return Err(AudioPipelineError::InvalidChunkingConfig(
                "speech_threshold must be a finite non-negative value".to_owned(),
            ));
        }

        Ok(())
    }

    fn samples_for_ms(&self, milliseconds: u32) -> usize {
        ((u64::from(milliseconds) * u64::from(INTERNAL_SAMPLE_RATE_HZ)) / 1_000) as usize
    }
}

/// Lightweight chunker for V1 finalized speech chunks.
#[derive(Debug, Clone)]
pub struct AudioChunker {
    config: ChunkingConfig,
    pre_roll: VecDeque<f32>,
    active: Vec<f32>,
    active_speech_samples: usize,
    trailing_silence_samples: usize,
    max_pre_roll_samples: usize,
    minimum_speech_samples: usize,
    target_min_samples: usize,
    max_chunk_samples: usize,
    post_roll_samples: usize,
}

impl AudioChunker {
    /// Create a chunker with V1 chunking rules.
    pub fn new(config: ChunkingConfig) -> Result<Self, AudioPipelineError> {
        config.validate()?;
        let max_pre_roll_samples = config.samples_for_ms(config.pre_roll_ms);
        let minimum_speech_samples = config.samples_for_ms(config.minimum_speech_ms);
        let target_min_samples = config.samples_for_ms(config.target_min_ms);
        let max_chunk_samples = config.samples_for_ms(config.max_chunk_ms);
        let post_roll_samples = config.samples_for_ms(config.post_roll_ms);

        Ok(Self {
            config,
            pre_roll: VecDeque::with_capacity(max_pre_roll_samples),
            active: Vec::new(),
            active_speech_samples: 0,
            trailing_silence_samples: 0,
            max_pre_roll_samples,
            minimum_speech_samples,
            target_min_samples,
            max_chunk_samples,
            post_roll_samples,
        })
    }

    /// Push normalized mono 16 kHz samples and return any finalized chunks.
    pub fn push_samples(&mut self, samples: &[f32]) -> Vec<AudioChunk> {
        let mut chunks = Vec::new();

        for sample in samples {
            let is_speech = sample.abs() >= self.config.speech_threshold;

            if self.active.is_empty() && !is_speech {
                self.push_pre_roll(*sample);
                continue;
            }

            if self.active.is_empty() && is_speech {
                self.active.extend(self.pre_roll.drain(..));
            }

            self.active.push(*sample);
            if is_speech {
                self.active_speech_samples += 1;
                self.trailing_silence_samples = 0;
            } else {
                self.trailing_silence_samples += 1;
            }

            if self.active.len() >= self.max_chunk_samples || self.ready_after_post_roll() {
                chunks.push(self.drain_active_chunk());
            }
        }

        chunks
    }

    /// Finish chunking and return a final chunk when enough speech was captured.
    pub fn finish(&mut self) -> Option<AudioChunk> {
        if self.active_speech_samples >= self.minimum_speech_samples {
            Some(self.drain_active_chunk())
        } else {
            self.clear();
            None
        }
    }

    fn push_pre_roll(&mut self, sample: f32) {
        if self.max_pre_roll_samples == 0 {
            return;
        }

        if self.pre_roll.len() == self.max_pre_roll_samples {
            let _ = self.pre_roll.pop_front();
        }
        self.pre_roll.push_back(sample);
    }

    fn ready_after_post_roll(&self) -> bool {
        self.active_speech_samples >= self.minimum_speech_samples
            && self.active.len() >= self.target_min_samples
            && self.trailing_silence_samples >= self.post_roll_samples
    }

    fn drain_active_chunk(&mut self) -> AudioChunk {
        let samples = core::mem::take(&mut self.active);
        self.active_speech_samples = 0;
        self.trailing_silence_samples = 0;
        self.pre_roll.clear();
        AudioChunk::new(samples)
    }

    fn clear(&mut self) {
        self.active.clear();
        self.active_speech_samples = 0;
        self.trailing_silence_samples = 0;
        self.pre_roll.clear();
    }
}

/// Configuration for the bounded audio worker.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioProcessorConfig {
    /// Maximum number of platform frames allowed in the queue.
    pub queue_capacity: usize,
    /// Chunking configuration used by the worker.
    pub chunking: ChunkingConfig,
}

impl Default for AudioProcessorConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 32,
            chunking: ChunkingConfig::default(),
        }
    }
}

/// Bounded worker that normalizes captured frames and finalizes V1 chunks.
#[derive(Debug)]
pub struct AudioProcessor {
    sender: SyncSender<AudioWorkerMessage>,
    worker: Option<JoinHandle<Result<Vec<AudioChunk>, AudioPipelineError>>>,
}

impl AudioProcessor {
    /// Start a bounded audio processing worker.
    pub fn start(config: AudioProcessorConfig) -> Result<Self, AudioPipelineError> {
        if config.queue_capacity == 0 {
            return Err(AudioPipelineError::InvalidChunkingConfig(
                "queue_capacity must be greater than zero".to_owned(),
            ));
        }
        config.chunking.validate()?;

        let (sender, receiver) = sync_channel(config.queue_capacity);
        let worker = thread::spawn(move || run_audio_worker(receiver, config.chunking));
        Ok(Self {
            sender,
            worker: Some(worker),
        })
    }

    /// Try to enqueue a frame without blocking the caller.
    pub fn try_push_frame(&self, frame: AudioFrame) -> Result<(), AudioPipelineError> {
        self.sender
            .try_send(AudioWorkerMessage::Frame(frame))
            .map_err(|error| match error {
                TrySendError::Full(_) => AudioPipelineError::QueueFull,
                TrySendError::Disconnected(_) => AudioPipelineError::WorkerStopped,
            })
    }

    /// Finish processing and return finalized chunks.
    pub fn finish(mut self) -> Result<Vec<AudioChunk>, AudioPipelineError> {
        self.sender
            .send(AudioWorkerMessage::Finish)
            .map_err(|_| AudioPipelineError::WorkerStopped)?;
        self.join_worker()
    }

    /// Cancel processing and discard captured audio.
    pub fn cancel(mut self) -> Result<(), AudioPipelineError> {
        self.sender
            .send(AudioWorkerMessage::Cancel)
            .map_err(|_| AudioPipelineError::WorkerStopped)?;
        self.join_worker().map(|_| ())
    }

    fn join_worker(&mut self) -> Result<Vec<AudioChunk>, AudioPipelineError> {
        let Some(worker) = self.worker.take() else {
            return Err(AudioPipelineError::WorkerStopped);
        };

        worker
            .join()
            .map_err(|_| AudioPipelineError::WorkerPanicked)?
    }
}

#[derive(Debug)]
enum AudioWorkerMessage {
    Frame(AudioFrame),
    Finish,
    Cancel,
}

fn run_audio_worker(
    receiver: Receiver<AudioWorkerMessage>,
    chunking: ChunkingConfig,
) -> Result<Vec<AudioChunk>, AudioPipelineError> {
    let mut chunker = AudioChunker::new(chunking)?;
    let mut chunks = Vec::new();

    while let Ok(message) = receiver.recv() {
        match message {
            AudioWorkerMessage::Frame(frame) => {
                let normalized = normalize_frame(frame)?;
                chunks.extend(chunker.push_samples(&normalized.samples));
            }
            AudioWorkerMessage::Finish => {
                if let Some(chunk) = chunker.finish() {
                    chunks.push(chunk);
                }
                return Ok(chunks);
            }
            AudioWorkerMessage::Cancel => return Ok(Vec::new()),
        }
    }

    Err(AudioPipelineError::WorkerStopped)
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
    use super::{
        AudioBuffer, AudioChunk, AudioChunker, AudioFrame, AudioPipelineError, AudioProcessor,
        AudioProcessorConfig, ChunkingConfig, INTERNAL_SAMPLE_RATE_HZ,
    };

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

    #[test]
    fn chunker_keeps_pre_and_post_roll_around_speech() -> Result<(), AudioPipelineError> {
        let mut chunker = AudioChunker::new(ChunkingConfig {
            minimum_speech_ms: 1,
            target_min_ms: 2,
            target_max_ms: 10,
            max_chunk_ms: 10,
            pre_roll_ms: 1,
            post_roll_ms: 1,
            speech_threshold: 0.5,
        })?;
        let pre_roll = vec![0.0; 16];
        let speech = vec![1.0; 16];
        let post_roll = vec![0.0; 32];

        assert!(chunker.push_samples(&pre_roll).is_empty());
        assert!(chunker.push_samples(&speech).is_empty());
        let chunks = chunker.push_samples(&post_roll);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].samples.len(), 48);
        Ok(())
    }

    #[test]
    fn chunker_splits_at_max_chunk_length() -> Result<(), AudioPipelineError> {
        let mut chunker = AudioChunker::new(ChunkingConfig {
            minimum_speech_ms: 1,
            target_min_ms: 1,
            target_max_ms: 2,
            max_chunk_ms: 2,
            pre_roll_ms: 0,
            post_roll_ms: 1,
            speech_threshold: 0.5,
        })?;

        let chunks = chunker.push_samples(&[1.0; 40]);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].samples.len(), 32);
        Ok(())
    }

    #[test]
    fn audio_processor_uses_bounded_nonblocking_queue() -> Result<(), AudioPipelineError> {
        let processor = AudioProcessor::start(AudioProcessorConfig {
            queue_capacity: 1,
            chunking: ChunkingConfig {
                minimum_speech_ms: 1,
                target_min_ms: 2,
                target_max_ms: 10,
                max_chunk_ms: 10,
                pre_roll_ms: 0,
                post_roll_ms: 1,
                speech_threshold: 0.5,
            },
        })?;

        processor.try_push_frame(AudioFrame::new(vec![1.0; 32], 16_000, 1)?)?;
        let chunks = processor.finish()?;

        assert_eq!(chunks.len(), 1);
        Ok(())
    }
}
