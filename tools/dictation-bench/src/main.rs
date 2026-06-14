//! Command-line benchmark runner for local dictation model packs.

use std::env;
use std::fmt;
use std::fs;
use std::mem::MaybeUninit;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{Duration, Instant};

use asr_api::{AsrEngine, ModelAsset, ModelConfig, PcmAudio, TranscribeOptions};
use asr_onnx::OrtAsrEngine;
use audio_pipeline::{AudioFrame, AudioProcessor, AudioProcessorConfig};
use model_pack::{ModelPack, ModelPackError};
use postprocess::PostProcessor;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run() -> Result<(), BenchError> {
    let command = BenchCommand::parse(env::args().skip(1))?;

    match command {
        BenchCommand::Transcribe(config) => run_transcribe(config),
    }
}

fn run_transcribe(config: TranscribeCommand) -> Result<(), BenchError> {
    let pack = ModelPack::load_from_dir(&config.model_dir)?;
    let model_config = model_config_from_pack(&pack);
    let mut peak_memory_bytes = current_resident_memory_bytes().unwrap_or(0);

    let mut engine = OrtAsrEngine::default();
    let load_start = Instant::now();
    engine.load(&model_config)?;
    let model_load = load_start.elapsed();
    peak_memory_bytes = peak_memory_bytes.max(current_resident_memory_bytes().unwrap_or(0));

    let preprocess_start = Instant::now();
    let frame = read_wav_audio(&config.audio_path)?;
    let audio_seconds = audio_duration_seconds(&frame);
    let processor = AudioProcessor::start(AudioProcessorConfig::default())?;
    processor.try_push_frame(frame)?;
    let chunks = processor.finish()?;
    let preprocessing = preprocess_start.elapsed();
    peak_memory_bytes = peak_memory_bytes.max(current_resident_memory_bytes().unwrap_or(0));

    if chunks.is_empty() {
        return Err(BenchError::Message(
            "audio did not produce a speech chunk".to_owned(),
        ));
    }

    let mut run_results = Vec::with_capacity(config.runs);
    let options = TranscribeOptions {
        language_hint: None,
    };
    let post_processor = PostProcessor::new();

    for _ in 0..config.runs {
        let run_start = Instant::now();
        let inference_start = Instant::now();
        let mut raw_text = String::new();

        for chunk in &chunks {
            let transcript = engine.transcribe(
                &PcmAudio {
                    samples: chunk.samples.clone(),
                    sample_rate_hz: chunk.sample_rate_hz,
                },
                &options,
            )?;
            if !transcript.text.trim().is_empty() {
                if !raw_text.is_empty() {
                    raw_text.push(' ');
                }
                raw_text.push_str(transcript.text.trim());
            }
        }

        let inference = inference_start.elapsed();
        let postprocess_start = Instant::now();
        let transcript = post_processor.process(&raw_text);
        let postprocess = postprocess_start.elapsed();
        let end_to_end = run_start.elapsed();
        peak_memory_bytes = peak_memory_bytes.max(current_resident_memory_bytes().unwrap_or(0));

        run_results.push(RunResult {
            inference,
            postprocess,
            end_to_end,
            transcript,
        });
    }

    let report = BenchReport {
        model_id: pack.manifest().model_id.clone(),
        runs: config.runs,
        audio_seconds,
        model_load,
        preprocessing,
        peak_memory_bytes,
        run_results,
    };
    let json = report.to_json();

    if let Some(output_path) = config.output_path {
        fs::write(&output_path, json).map_err(|source| {
            BenchError::Io(format!(
                "failed to write {}: {source}",
                output_path.display()
            ))
        })?;
    } else {
        println!("{json}");
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BenchCommand {
    Transcribe(TranscribeCommand),
}

impl BenchCommand {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, BenchError> {
        let command = args.next().ok_or_else(usage_error)?;

        match command.as_str() {
            "transcribe" => Ok(Self::Transcribe(TranscribeCommand::parse(args)?)),
            _ => Err(usage_error()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TranscribeCommand {
    model_dir: PathBuf,
    audio_path: PathBuf,
    runs: usize,
    output_path: Option<PathBuf>,
}

impl TranscribeCommand {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, BenchError> {
        let mut model_dir = None;
        let mut audio_path = None;
        let mut runs = 30_usize;
        let mut output_path = None;

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--model" => model_dir = Some(PathBuf::from(next_value(&mut args, "--model")?)),
                "--audio" => audio_path = Some(PathBuf::from(next_value(&mut args, "--audio")?)),
                "--runs" => {
                    let value = next_value(&mut args, "--runs")?;
                    runs = value.parse::<usize>().map_err(|source| {
                        BenchError::Message(format!("invalid --runs value {value}: {source}"))
                    })?;
                    if runs == 0 {
                        return Err(BenchError::Message(
                            "--runs must be greater than zero".to_owned(),
                        ));
                    }
                }
                "--output" => output_path = Some(PathBuf::from(next_value(&mut args, "--output")?)),
                _ => return Err(usage_error()),
            }
        }

        Ok(Self {
            model_dir: model_dir.ok_or_else(usage_error)?,
            audio_path: audio_path.ok_or_else(usage_error)?,
            runs,
            output_path,
        })
    }
}

#[derive(Debug, Clone)]
struct BenchReport {
    model_id: String,
    runs: usize,
    audio_seconds: f64,
    model_load: Duration,
    preprocessing: Duration,
    peak_memory_bytes: u64,
    run_results: Vec<RunResult>,
}

impl BenchReport {
    fn to_json(&self) -> String {
        let latencies = self
            .run_results
            .iter()
            .map(|result| duration_ms(result.end_to_end))
            .collect::<Vec<_>>();
        let inference_total = self
            .run_results
            .iter()
            .map(|result| result.inference)
            .fold(Duration::ZERO, |left, right| left + right);
        let postprocess_total = self
            .run_results
            .iter()
            .map(|result| result.postprocess)
            .fold(Duration::ZERO, |left, right| left + right);
        let e2e_total = self
            .run_results
            .iter()
            .map(|result| result.end_to_end)
            .fold(Duration::ZERO, |left, right| left + right);
        let first_transcript = self
            .run_results
            .first()
            .map(|result| result.transcript.as_str())
            .unwrap_or("");
        let rtf = if self.audio_seconds > 0.0 && self.runs > 0 {
            inference_total.as_secs_f64() / self.runs as f64 / self.audio_seconds
        } else {
            0.0
        };

        format!(
            concat!(
                "{{\n",
                "  \"modelId\": \"{}\",\n",
                "  \"runs\": {},\n",
                "  \"audioSeconds\": {:.6},\n",
                "  \"modelLoadMs\": {:.3},\n",
                "  \"audioPreprocessingMs\": {:.3},\n",
                "  \"asrInferenceMsMean\": {:.3},\n",
                "  \"postProcessingMsMean\": {:.3},\n",
                "  \"endToEndLatencyMsMean\": {:.3},\n",
                "  \"rtf\": {:.6},\n",
                "  \"p50LatencyMs\": {:.3},\n",
                "  \"p95LatencyMs\": {:.3},\n",
                "  \"peakMemoryBytes\": {},\n",
                "  \"transcript\": \"{}\",\n",
                "  \"runLatenciesMs\": [{}]\n",
                "}}\n"
            ),
            escape_json(&self.model_id),
            self.runs,
            self.audio_seconds,
            duration_ms(self.model_load),
            duration_ms(self.preprocessing),
            duration_ms(inference_total) / self.runs as f64,
            duration_ms(postprocess_total) / self.runs as f64,
            duration_ms(e2e_total) / self.runs as f64,
            rtf,
            percentile(&latencies, 0.50),
            percentile(&latencies, 0.95),
            self.peak_memory_bytes,
            escape_json(first_transcript),
            latencies
                .iter()
                .map(|latency| format!("{latency:.3}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

#[derive(Debug, Clone)]
struct RunResult {
    inference: Duration,
    postprocess: Duration,
    end_to_end: Duration,
    transcript: String,
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

fn read_wav_audio(path: &Path) -> Result<AudioFrame, BenchError> {
    let bytes = fs::read(path)
        .map_err(|source| BenchError::Io(format!("failed to read {}: {source}", path.display())))?;

    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(BenchError::Message(format!(
            "{} is not a RIFF/WAVE file",
            path.display()
        )));
    }

    let mut cursor = 12_usize;
    let mut format = None;
    let mut data = None;

    while cursor + 8 <= bytes.len() {
        let chunk_id = &bytes[cursor..cursor + 4];
        let chunk_size = read_u32_le(&bytes, cursor + 4)? as usize;
        let data_start = cursor + 8;
        let data_end = data_start.checked_add(chunk_size).ok_or_else(|| {
            BenchError::Message("WAV chunk size overflowed address space".to_owned())
        })?;
        if data_end > bytes.len() {
            return Err(BenchError::Message(
                "WAV chunk extends beyond file length".to_owned(),
            ));
        }

        if chunk_id == b"fmt " {
            format = Some(WavFormat {
                audio_format: read_u16_le(&bytes, data_start)?,
                channels: read_u16_le(&bytes, data_start + 2)?,
                sample_rate_hz: read_u32_le(&bytes, data_start + 4)?,
                bits_per_sample: read_u16_le(&bytes, data_start + 14)?,
            });
        } else if chunk_id == b"data" {
            data = Some(bytes[data_start..data_end].to_vec());
        }

        cursor = data_end + (chunk_size % 2);
    }

    let format =
        format.ok_or_else(|| BenchError::Message("WAV file is missing fmt chunk".to_owned()))?;
    let data =
        data.ok_or_else(|| BenchError::Message("WAV file is missing data chunk".to_owned()))?;
    let samples = decode_wav_samples(&format, &data)?;

    Ok(AudioFrame::new(
        samples,
        format.sample_rate_hz,
        format.channels,
    )?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WavFormat {
    audio_format: u16,
    channels: u16,
    sample_rate_hz: u32,
    bits_per_sample: u16,
}

fn decode_wav_samples(format: &WavFormat, data: &[u8]) -> Result<Vec<f32>, BenchError> {
    match (format.audio_format, format.bits_per_sample) {
        (1, 16) => {
            if data.len() % 2 != 0 {
                return Err(BenchError::Message(
                    "16-bit PCM WAV data has an odd byte length".to_owned(),
                ));
            }

            let mut samples = Vec::with_capacity(data.len() / 2);
            for chunk in data.chunks_exact(2) {
                let value = i16::from_le_bytes([chunk[0], chunk[1]]);
                samples.push(f32::from(value) / 32_768.0);
            }
            Ok(samples)
        }
        (3, 32) => {
            if data.len() % 4 != 0 {
                return Err(BenchError::Message(
                    "32-bit float WAV data is not aligned to f32 samples".to_owned(),
                ));
            }

            let mut samples = Vec::with_capacity(data.len() / 4);
            for chunk in data.chunks_exact(4) {
                samples.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
            Ok(samples)
        }
        _ => Err(BenchError::Message(format!(
            "unsupported WAV format {} with {} bits per sample",
            format.audio_format, format.bits_per_sample
        ))),
    }
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16, BenchError> {
    if offset + 2 > bytes.len() {
        return Err(BenchError::Message(
            "unexpected end of WAV file while reading u16".to_owned(),
        ));
    }

    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, BenchError> {
    if offset + 4 > bytes.len() {
        return Err(BenchError::Message(
            "unexpected end of WAV file while reading u32".to_owned(),
        ));
    }

    Ok(u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

fn audio_duration_seconds(frame: &AudioFrame) -> f64 {
    frame.samples.len() as f64 / f64::from(frame.channels) / f64::from(frame.sample_rate_hz)
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = ((sorted.len() as f64 * percentile).ceil() as usize).saturating_sub(1);
    sorted[index.min(sorted.len() - 1)]
}

fn current_resident_memory_bytes() -> Option<u64> {
    let mut usage = MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: `getrusage` writes a fully initialized `rusage` value when it
    // returns 0. The pointer is valid for writes for the duration of the call.
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if result != 0 {
        return None;
    }

    // SAFETY: The successful `getrusage` call above initialized `usage`.
    let usage = unsafe { usage.assume_init() };
    let max_rss = u64::try_from(usage.ru_maxrss).ok()?;

    #[cfg(target_os = "macos")]
    {
        Some(max_rss)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Some(max_rss.saturating_mul(1_024))
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, BenchError> {
    args.next()
        .ok_or_else(|| BenchError::Message(format!("missing value for {flag}")))
}

fn usage_error() -> BenchError {
    BenchError::Message(
        "usage: dictation-bench transcribe --model <model-pack-dir> --audio <audio.wav> [--runs 30] [--output bench.json]"
            .to_owned(),
    )
}

fn escape_json(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            other => output.push(other),
        }
    }
    output
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BenchError {
    Message(String),
    Io(String),
    ModelPack(String),
    Audio(String),
    Asr(String),
}

impl fmt::Display for BenchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message)
            | Self::Io(message)
            | Self::ModelPack(message)
            | Self::Audio(message)
            | Self::Asr(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for BenchError {}

impl From<ModelPackError> for BenchError {
    fn from(error: ModelPackError) -> Self {
        Self::ModelPack(error.to_string())
    }
}

impl From<audio_pipeline::AudioPipelineError> for BenchError {
    fn from(error: audio_pipeline::AudioPipelineError) -> Self {
        Self::Audio(error.to_string())
    }
}

impl From<asr_api::AsrError> for BenchError {
    fn from(error: asr_api::AsrError) -> Self {
        Self::Asr(error.to_string())
    }
}
