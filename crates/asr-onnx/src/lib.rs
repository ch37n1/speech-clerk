//! ONNX Runtime ASR backend boundary.
//!
//! This crate owns concrete ONNX model-pack asset loading and keeps runtime
//! details behind the backend-neutral `asr-api` trait.

use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

use asr_api::{
    AsrCapabilities, AsrEngine, AsrError, ModelAsset, ModelConfig, PcmAudio, TranscribeOptions,
    Transcript,
};
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::{Tensor, ValueType};
use serde_json::Value as JsonValue;
use tracing::{debug, info};

const FIXTURE_TOKENIZER_HEADER: &str = "speech-clerk-tokenizer-fixture-v1";
const ORT_DYLIB_ENV: &str = "ORT_DYLIB_PATH";
const ORT_INTRA_THREADS_ENV: &str = "SPEECH_CLERK_ORT_INTRA_THREADS";
const ORT_INTER_THREADS_ENV: &str = "SPEECH_CLERK_ORT_INTER_THREADS";
const ORT_PARALLEL_ENV: &str = "SPEECH_CLERK_ORT_PARALLEL";
const ORT_MEMORY_PATTERN_ENV: &str = "SPEECH_CLERK_ORT_MEMORY_PATTERN";
const SAMPLE_RATE_HZ: u32 = 16_000;
const DEFAULT_MAX_SYMBOLS_PER_STEP: usize = 10;
const DEFAULT_INTER_THREADS: usize = 1;
const MAX_DEFAULT_INTRA_THREADS: usize = 4;

/// ASR engine for V1 ONNX model packs.
#[derive(Debug, Default)]
pub struct OrtAsrEngine {
    loaded_model: Option<LoadedOnnxModel>,
}

impl OrtAsrEngine {
    /// Return the loaded model id, if any.
    #[must_use]
    pub fn loaded_model_id(&self) -> Option<&str> {
        self.loaded_model
            .as_ref()
            .map(|model| model.config.model_id.as_str())
    }
}

impl AsrEngine for OrtAsrEngine {
    fn load(&mut self, config: &ModelConfig) -> Result<(), AsrError> {
        if config.runtime != "onnx" {
            return Err(AsrError::LoadFailed(format!(
                "OrtAsrEngine cannot load runtime {}",
                config.runtime
            )));
        }

        let model_asset = required_asset_with_roles(config, &["model", "encoder"])?;
        let tokenizer_asset = required_asset_with_roles(config, &["tokenizer", "vocab"])?;
        let tokenizer_bytes =
            read_required_asset(&tokenizer_asset.absolute_path, &tokenizer_asset.role)?;
        let tokenizer = TokenizerAsset::load(tokenizer_bytes)?;
        let parakeet_config = load_parakeet_config(config)?;
        read_required_asset(&model_asset.absolute_path, "model")?;
        let sessions = if tokenizer.is_fixture() {
            debug!(
                model_id = %config.model_id,
                "loading fixture ONNX tokenizer path without runtime sessions"
            );
            Vec::new()
        } else {
            load_onnx_sessions(config, OrtSessionTuning::from_env()?)?
        };

        info!(
            model_id = %config.model_id,
            display_name = %config.display_name,
            session_count = sessions.len(),
            "loaded ONNX ASR model"
        );
        self.loaded_model = Some(LoadedOnnxModel {
            config: config.clone(),
            tokenizer,
            parakeet_config,
            sessions,
        });
        Ok(())
    }

    fn transcribe(
        &mut self,
        audio: &PcmAudio,
        options: &TranscribeOptions,
    ) -> Result<Transcript, AsrError> {
        if audio.sample_rate_hz != SAMPLE_RATE_HZ {
            return Err(AsrError::TranscriptionFailed(format!(
                "onnx backend expects 16000 Hz audio, got {}",
                audio.sample_rate_hz
            )));
        }

        if audio.samples.is_empty() {
            return Err(AsrError::TranscriptionFailed(
                "audio chunk is empty".to_owned(),
            ));
        }

        let model = self.loaded_model.as_ref().ok_or_else(|| {
            AsrError::TranscriptionFailed("onnx model has not been loaded".to_owned())
        })?;

        debug!(
            model_id = %model.config.model_id,
            sample_count = audio.samples.len(),
            language_hint = options.language_hint.as_deref().unwrap_or("auto"),
            "transcribing ONNX audio chunk"
        );

        if let Some(transcript) = &model.tokenizer.fixture_transcript {
            return Ok(Transcript {
                text: transcript.clone(),
                language: options
                    .language_hint
                    .clone()
                    .or_else(|| model.tokenizer.fixture_language.clone())
                    .or_else(|| model.config.languages.first().cloned()),
            });
        }

        let token_ids = transcribe_parakeet_tdt(model, &audio.samples)?;
        let text = model.tokenizer.decode_tokens(&token_ids);

        Ok(Transcript {
            text,
            language: options
                .language_hint
                .clone()
                .or_else(|| model.config.languages.first().cloned()),
        })
    }

    fn capabilities(&self) -> AsrCapabilities {
        AsrCapabilities {
            supports_chunked: true,
            supports_streaming: false,
            supports_punctuation: true,
            supports_language_detection: true,
        }
    }

    fn unload(&mut self) -> Result<(), AsrError> {
        self.loaded_model = None;
        Ok(())
    }
}

#[derive(Debug)]
struct LoadedOnnxModel {
    config: ModelConfig,
    tokenizer: TokenizerAsset,
    parakeet_config: ParakeetConfig,
    sessions: Vec<OrtModelSession>,
}

#[derive(Debug)]
struct OrtModelSession {
    role: String,
    inputs: Vec<ModelIo>,
    session: Session,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OrtSessionTuning {
    intra_threads: usize,
    inter_threads: usize,
    parallel_execution: bool,
    memory_pattern: bool,
}

impl Default for OrtSessionTuning {
    fn default() -> Self {
        Self {
            intra_threads: default_intra_threads(),
            inter_threads: DEFAULT_INTER_THREADS,
            parallel_execution: false,
            memory_pattern: false,
        }
    }
}

impl OrtSessionTuning {
    fn from_env() -> Result<Self, AsrError> {
        let defaults = Self::default();
        Ok(Self {
            intra_threads: parse_positive_usize_env(ORT_INTRA_THREADS_ENV)?
                .unwrap_or(defaults.intra_threads),
            inter_threads: parse_positive_usize_env(ORT_INTER_THREADS_ENV)?
                .unwrap_or(defaults.inter_threads),
            parallel_execution: parse_bool_env(ORT_PARALLEL_ENV)?
                .unwrap_or(defaults.parallel_execution),
            memory_pattern: parse_bool_env(ORT_MEMORY_PATTERN_ENV)?
                .unwrap_or(defaults.memory_pattern),
        })
    }
}

#[derive(Debug, Clone)]
struct ModelIo {
    name: String,
    dimensions: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq)]
struct OrtTensor<T> {
    shape: Vec<i64>,
    values: Vec<T>,
}

#[derive(Debug, Clone, PartialEq)]
struct DecoderState {
    state_1: OrtTensor<f32>,
    state_2: OrtTensor<f32>,
}

#[derive(Debug, Clone)]
struct TokenizerAsset {
    fixture_transcript: Option<String>,
    fixture_language: Option<String>,
    vocabulary: Vec<String>,
    blank_token_id: Option<usize>,
}

impl TokenizerAsset {
    fn load(bytes: Vec<u8>) -> Result<Self, AsrError> {
        if bytes.is_empty() {
            return Err(AsrError::LoadFailed(
                "tokenizer asset must not be empty".to_owned(),
            ));
        }

        let text = String::from_utf8(bytes).ok();
        let fixture_transcript = text
            .as_deref()
            .filter(|value| value.lines().next() == Some(FIXTURE_TOKENIZER_HEADER))
            .and_then(|value| field_value(value, "transcript"));
        let fixture_language = text
            .as_deref()
            .filter(|value| value.lines().next() == Some(FIXTURE_TOKENIZER_HEADER))
            .and_then(|value| field_value(value, "language"));
        let indexed_vocabulary = text.as_deref().map(parse_vocabulary).unwrap_or_default();
        let blank_token_id = indexed_vocabulary
            .iter()
            .find(|(_, token)| token == "<blk>" || token == "<blank>")
            .map(|(index, _)| *index);
        let vocabulary = indexed_vocabulary
            .into_iter()
            .map(|(_, token)| token)
            .collect();

        Ok(Self {
            fixture_transcript,
            fixture_language,
            vocabulary,
            blank_token_id,
        })
    }

    fn is_fixture(&self) -> bool {
        self.fixture_transcript.is_some()
    }

    fn decode_tokens(&self, token_ids: &[usize]) -> String {
        let mut text = String::new();

        for &token_id in token_ids {
            if Some(token_id) == self.blank_token_id {
                continue;
            }

            let Some(token) = self.vocabulary.get(token_id) else {
                continue;
            };

            if token.starts_with('<') && token.ends_with('>') {
                continue;
            }

            if let Some(word_start) = token.strip_prefix('▁') {
                if !text.is_empty() && !text.ends_with(' ') {
                    text.push(' ');
                }
                text.push_str(word_start);
            } else {
                text.push_str(token);
            }
        }

        text.trim().to_owned()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TdtDecision {
    token_id: usize,
    token_score: f32,
    duration_frames: usize,
    duration_score: f32,
}

impl TdtDecision {
    fn greedy(logits: &[f32], token_count: usize) -> Result<Self, AsrError> {
        if token_count == 0 {
            return Err(AsrError::TranscriptionFailed(
                "TDT decoding requires at least one token".to_owned(),
            ));
        }

        if logits.len() <= token_count {
            return Err(AsrError::TranscriptionFailed(format!(
                "TDT logits must contain token and duration scores, got {} score(s) for {} token(s)",
                logits.len(),
                token_count
            )));
        }

        let (token_id, token_score) = argmax(&logits[..token_count]).ok_or_else(|| {
            AsrError::TranscriptionFailed("TDT token logits are empty".to_owned())
        })?;
        let (duration_frames, duration_score) =
            argmax(&logits[token_count..]).ok_or_else(|| {
                AsrError::TranscriptionFailed("TDT duration logits are empty".to_owned())
            })?;

        Ok(Self {
            token_id,
            token_score,
            duration_frames,
            duration_score,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParakeetConfig {
    features_size: usize,
    subsampling_factor: usize,
}

impl Default for ParakeetConfig {
    fn default() -> Self {
        Self {
            features_size: 128,
            subsampling_factor: 8,
        }
    }
}

impl ParakeetConfig {
    fn from_json(input: &str) -> Result<Self, AsrError> {
        let value = serde_json::from_str::<JsonValue>(input).map_err(|error| {
            AsrError::LoadFailed(format!("invalid Parakeet config JSON: {error}"))
        })?;
        let features_size = value
            .get("features_size")
            .and_then(JsonValue::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(128);
        let subsampling_factor = value
            .get("subsampling_factor")
            .and_then(JsonValue::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(8);

        if features_size == 0 {
            return Err(AsrError::LoadFailed(
                "Parakeet config features_size must be greater than zero".to_owned(),
            ));
        }

        if subsampling_factor == 0 {
            return Err(AsrError::LoadFailed(
                "Parakeet config subsampling_factor must be greater than zero".to_owned(),
            ));
        }

        Ok(Self {
            features_size,
            subsampling_factor,
        })
    }
}

fn load_parakeet_config(config: &ModelConfig) -> Result<ParakeetConfig, AsrError> {
    let Some(asset) = config.files.iter().find(|asset| asset.role == "config") else {
        return Ok(ParakeetConfig::default());
    };
    let bytes = read_required_asset(&asset.absolute_path, "config")?;
    let text = String::from_utf8(bytes).map_err(|error| {
        AsrError::LoadFailed(format!(
            "config asset {} is not valid UTF-8: {error}",
            asset.absolute_path.display()
        ))
    })?;
    ParakeetConfig::from_json(&text)
}

fn transcribe_parakeet_tdt(
    model: &LoadedOnnxModel,
    samples: &[f32],
) -> Result<Vec<usize>, AsrError> {
    let preprocessor = required_session(model, "preprocessor")?;
    let encoder = required_session(model, "encoder")?;
    let decoder = required_session(model, "decoder_joint")?;
    let blank_token_id = model.tokenizer.blank_token_id.ok_or_else(|| {
        AsrError::TranscriptionFailed(format!(
            "vocabulary for {} does not declare <blk> or <blank>",
            model.config.model_id
        ))
    })?;

    let (features, feature_len) = run_preprocessor(preprocessor, samples)?;
    validate_feature_tensor(model, &features, feature_len)?;
    let (encoder_outputs, encoded_len) = run_encoder(encoder, &features, feature_len)?;
    let estimated_encoder_len = feature_len.div_ceil(model.parakeet_config.subsampling_factor);

    if encoded_len == 0 {
        return Err(AsrError::TranscriptionFailed(format!(
            "encoder produced zero frames for {} feature frame(s), estimated {} frame(s)",
            feature_len, estimated_encoder_len
        )));
    }

    greedy_decode_tdt(
        decoder,
        &encoder_outputs,
        encoded_len,
        model.tokenizer.vocabulary.len(),
        blank_token_id,
    )
}

fn run_preprocessor(
    session: &OrtModelSession,
    samples: &[f32],
) -> Result<(OrtTensor<f32>, usize), AsrError> {
    let waveforms = Tensor::from_array((
        [1_usize, samples.len()],
        samples.to_vec().into_boxed_slice(),
    ))
    .map_err(map_ort_transcription_error)?;
    let waveforms_lens = Tensor::from_array((
        [1_usize],
        vec![usize_to_i64(samples.len(), "waveform length")?].into_boxed_slice(),
    ))
    .map_err(map_ort_transcription_error)?;
    let inputs = ort::inputs! {
        "waveforms" => waveforms,
        "waveforms_lens" => waveforms_lens,
    }
    .map_err(map_ort_transcription_error)?;
    let outputs = catch_ort_panic_transcription(|| {
        session
            .session
            .run(inputs)
            .map_err(map_ort_transcription_error)
    })??;
    let features = extract_named_tensor::<f32>(&outputs, "features")?;
    let feature_lens = extract_named_tensor::<i64>(&outputs, "features_lens")?;
    let feature_len = first_len(&feature_lens, "features_lens")?;

    Ok((features, feature_len))
}

fn validate_feature_tensor(
    model: &LoadedOnnxModel,
    features: &OrtTensor<f32>,
    feature_len: usize,
) -> Result<(), AsrError> {
    if features.shape.len() != 3 {
        return Err(AsrError::TranscriptionFailed(format!(
            "preprocessor output features must be rank 3, got shape {:?}",
            features.shape
        )));
    }

    let bins = i64_to_usize(features.shape[1], "feature bin count")?;
    if bins != model.parakeet_config.features_size {
        return Err(AsrError::TranscriptionFailed(format!(
            "preprocessor produced {} feature bins, config expects {}",
            bins, model.parakeet_config.features_size
        )));
    }

    let frames = i64_to_usize(features.shape[2], "feature frame count")?;
    if feature_len > frames {
        return Err(AsrError::TranscriptionFailed(format!(
            "preprocessor feature length {} exceeds feature frame count {}",
            feature_len, frames
        )));
    }

    Ok(())
}

fn run_encoder(
    session: &OrtModelSession,
    features: &OrtTensor<f32>,
    feature_len: usize,
) -> Result<(OrtTensor<f32>, usize), AsrError> {
    let audio_signal = Tensor::from_array((
        features.shape.clone(),
        features.values.clone().into_boxed_slice(),
    ))
    .map_err(map_ort_transcription_error)?;
    let length = Tensor::from_array((
        [1_usize],
        vec![usize_to_i64(feature_len, "feature length")?].into_boxed_slice(),
    ))
    .map_err(map_ort_transcription_error)?;
    let inputs = ort::inputs! {
        "audio_signal" => audio_signal,
        "length" => length,
    }
    .map_err(map_ort_transcription_error)?;
    let outputs = catch_ort_panic_transcription(|| {
        session
            .session
            .run(inputs)
            .map_err(map_ort_transcription_error)
    })??;
    let encoder_outputs = extract_named_tensor::<f32>(&outputs, "outputs")?;
    let encoded_lengths = extract_named_tensor::<i64>(&outputs, "encoded_lengths")?;
    let encoded_len = first_len(&encoded_lengths, "encoded_lengths")?;

    Ok((encoder_outputs, encoded_len))
}

fn greedy_decode_tdt(
    session: &OrtModelSession,
    encoder_outputs: &OrtTensor<f32>,
    encoded_len: usize,
    token_count: usize,
    blank_token_id: usize,
) -> Result<Vec<usize>, AsrError> {
    if token_count == 0 {
        return Err(AsrError::TranscriptionFailed(
            "cannot decode TDT output with an empty vocabulary".to_owned(),
        ));
    }

    let mut token_ids = Vec::new();
    let mut state = initial_decoder_state(session)?;
    let mut last_token = blank_token_id;
    let mut time_idx = 0;

    while time_idx < encoded_len {
        let mut symbols_added = 0;
        let mut need_loop = true;

        while need_loop && symbols_added < DEFAULT_MAX_SYMBOLS_PER_STEP {
            let frame = slice_encoder_frame(encoder_outputs, time_idx)?;
            let step = run_decoder_step(session, &frame, last_token, &state)?;
            let decision = TdtDecision::greedy(&step.logits, token_count)?;
            let mut skip = decision.duration_frames;

            if decision.token_id != blank_token_id {
                token_ids.push(decision.token_id);
                last_token = decision.token_id;
                state = step.state;
                symbols_added += 1;
                time_idx = time_idx.saturating_add(skip);
                need_loop = skip == 0;
            } else {
                if skip == 0 {
                    skip = 1;
                }
                time_idx = time_idx.saturating_add(skip);
                need_loop = false;
            }

            if time_idx >= encoded_len {
                break;
            }
        }

        if symbols_added == DEFAULT_MAX_SYMBOLS_PER_STEP {
            time_idx = time_idx.saturating_add(1);
        }
    }

    Ok(token_ids)
}

#[derive(Debug, Clone, PartialEq)]
struct DecoderStep {
    logits: Vec<f32>,
    state: DecoderState,
}

fn run_decoder_step(
    session: &OrtModelSession,
    encoder_frame: &OrtTensor<f32>,
    last_token: usize,
    state: &DecoderState,
) -> Result<DecoderStep, AsrError> {
    let encoder_outputs = Tensor::from_array((
        encoder_frame.shape.clone(),
        encoder_frame.values.clone().into_boxed_slice(),
    ))
    .map_err(map_ort_transcription_error)?;
    let targets = Tensor::from_array((
        [1_usize, 1_usize],
        vec![usize_to_i32(last_token, "last token id")?].into_boxed_slice(),
    ))
    .map_err(map_ort_transcription_error)?;
    let target_length = Tensor::from_array(([1_usize], vec![1_i32].into_boxed_slice()))
        .map_err(map_ort_transcription_error)?;
    let input_states_1 = Tensor::from_array((
        state.state_1.shape.clone(),
        state.state_1.values.clone().into_boxed_slice(),
    ))
    .map_err(map_ort_transcription_error)?;
    let input_states_2 = Tensor::from_array((
        state.state_2.shape.clone(),
        state.state_2.values.clone().into_boxed_slice(),
    ))
    .map_err(map_ort_transcription_error)?;
    let inputs = ort::inputs! {
        "encoder_outputs" => encoder_outputs,
        "targets" => targets,
        "target_length" => target_length,
        "input_states_1" => input_states_1,
        "input_states_2" => input_states_2,
    }
    .map_err(map_ort_transcription_error)?;
    let outputs = catch_ort_panic_transcription(|| {
        session
            .session
            .run(inputs)
            .map_err(map_ort_transcription_error)
    })??;
    let output = extract_named_tensor::<f32>(&outputs, "outputs")?;
    let output_states_1 = extract_named_tensor::<f32>(&outputs, "output_states_1")?;
    let output_states_2 = extract_named_tensor::<f32>(&outputs, "output_states_2")?;
    let logits_width = output
        .shape
        .last()
        .copied()
        .ok_or_else(|| {
            AsrError::TranscriptionFailed("decoder output logits shape is empty".to_owned())
        })
        .and_then(|value| i64_to_usize(value, "decoder logits width"))?;

    if output.values.len() < logits_width {
        return Err(AsrError::TranscriptionFailed(format!(
            "decoder returned {} value(s), fewer than logits width {}",
            output.values.len(),
            logits_width
        )));
    }

    Ok(DecoderStep {
        logits: output.values[..logits_width].to_vec(),
        state: DecoderState {
            state_1: output_states_1,
            state_2: output_states_2,
        },
    })
}

fn initial_decoder_state(session: &OrtModelSession) -> Result<DecoderState, AsrError> {
    let state_1_shape = concrete_batch_shape(session, "input_states_1")?;
    let state_2_shape = concrete_batch_shape(session, "input_states_2")?;

    Ok(DecoderState {
        state_1: zero_tensor(state_1_shape)?,
        state_2: zero_tensor(state_2_shape)?,
    })
}

fn zero_tensor(shape: Vec<i64>) -> Result<OrtTensor<f32>, AsrError> {
    let len = shape.iter().try_fold(1_usize, |acc, &dimension| {
        let dimension = i64_to_usize(dimension, "tensor dimension")?;
        acc.checked_mul(dimension).ok_or_else(|| {
            AsrError::TranscriptionFailed(format!("tensor shape {shape:?} is too large"))
        })
    })?;

    Ok(OrtTensor {
        shape,
        values: vec![0.0; len],
    })
}

fn concrete_batch_shape(session: &OrtModelSession, input_name: &str) -> Result<Vec<i64>, AsrError> {
    let input = session
        .inputs
        .iter()
        .find(|input| input.name == input_name)
        .ok_or_else(|| {
            AsrError::TranscriptionFailed(format!(
                "{} session does not declare input {}",
                session.role, input_name
            ))
        })?;

    input
        .dimensions
        .iter()
        .map(|&dimension| if dimension < 0 { Ok(1) } else { Ok(dimension) })
        .collect()
}

fn slice_encoder_frame(
    encoder_outputs: &OrtTensor<f32>,
    time_idx: usize,
) -> Result<OrtTensor<f32>, AsrError> {
    if encoder_outputs.shape.len() != 3 {
        return Err(AsrError::TranscriptionFailed(format!(
            "encoder output must be rank 3, got shape {:?}",
            encoder_outputs.shape
        )));
    }

    let batch = i64_to_usize(encoder_outputs.shape[0], "encoder batch size")?;
    let channels = i64_to_usize(encoder_outputs.shape[1], "encoder channel count")?;
    let frames = i64_to_usize(encoder_outputs.shape[2], "encoder frame count")?;

    if batch != 1 {
        return Err(AsrError::TranscriptionFailed(format!(
            "TDT decoder currently expects batch size 1, got {}",
            batch
        )));
    }

    if time_idx >= frames {
        return Err(AsrError::TranscriptionFailed(format!(
            "encoder frame index {} exceeds frame count {}",
            time_idx, frames
        )));
    }

    let mut values = Vec::with_capacity(channels);
    for channel in 0..channels {
        values.push(encoder_outputs.values[channel * frames + time_idx]);
    }

    Ok(OrtTensor {
        shape: vec![1, usize_to_i64(channels, "encoder channel count")?, 1],
        values,
    })
}

fn required_session<'a>(
    model: &'a LoadedOnnxModel,
    role: &str,
) -> Result<&'a OrtModelSession, AsrError> {
    model
        .sessions
        .iter()
        .find(|session| session.role == role)
        .ok_or_else(|| {
            AsrError::TranscriptionFailed(format!(
                "model pack {} did not load required ONNX session role {}",
                model.config.model_id, role
            ))
        })
}

fn extract_named_tensor<T>(
    outputs: &ort::session::SessionOutputs<'_, '_>,
    name: &str,
) -> Result<OrtTensor<T>, AsrError>
where
    T: ort::tensor::PrimitiveTensorElementType + Copy,
{
    let value = outputs.get(name).ok_or_else(|| {
        AsrError::TranscriptionFailed(format!("ONNX output {name} was not returned"))
    })?;
    let (shape, values) = value
        .try_extract_raw_tensor::<T>()
        .map_err(map_ort_transcription_error)?;

    Ok(OrtTensor {
        shape: shape.to_vec(),
        values: values.to_vec(),
    })
}

fn first_len(tensor: &OrtTensor<i64>, name: &str) -> Result<usize, AsrError> {
    let value = tensor
        .values
        .first()
        .copied()
        .ok_or_else(|| AsrError::TranscriptionFailed(format!("ONNX output {name} is empty")))?;
    i64_to_usize(value, name)
}

fn tensor_dimensions(value_type: &ValueType) -> Vec<i64> {
    match value_type {
        ValueType::Tensor { dimensions, .. } => dimensions.clone(),
        _ => Vec::new(),
    }
}

fn usize_to_i64(value: usize, name: &str) -> Result<i64, AsrError> {
    i64::try_from(value).map_err(|_| {
        AsrError::TranscriptionFailed(format!("{name} value {value} does not fit into i64"))
    })
}

fn usize_to_i32(value: usize, name: &str) -> Result<i32, AsrError> {
    i32::try_from(value).map_err(|_| {
        AsrError::TranscriptionFailed(format!("{name} value {value} does not fit into i32"))
    })
}

fn i64_to_usize(value: i64, name: &str) -> Result<usize, AsrError> {
    usize::try_from(value).map_err(|_| {
        AsrError::TranscriptionFailed(format!(
            "{name} value {value} must be non-negative and fit into usize"
        ))
    })
}

fn parse_vocabulary(input: &str) -> Vec<(usize, String)> {
    let mut indexed = input
        .lines()
        .filter_map(parse_vocab_line)
        .collect::<Vec<_>>();
    indexed.sort_by_key(|(index, _)| *index);
    indexed
}

fn parse_vocab_line(line: &str) -> Option<(usize, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with(FIXTURE_TOKENIZER_HEADER) {
        return None;
    }

    let (token, index) = line.rsplit_once(char::is_whitespace)?;
    let index = index.trim().parse::<usize>().ok()?;
    let token = token.trim();

    if token.is_empty() {
        return None;
    }

    Some((index, token.to_owned()))
}

fn argmax(values: &[f32]) -> Option<(usize, f32)> {
    values
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, value)| value.is_finite())
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
}

fn load_onnx_sessions(
    config: &ModelConfig,
    tuning: OrtSessionTuning,
) -> Result<Vec<OrtModelSession>, AsrError> {
    let runtime_path = std::env::var(ORT_DYLIB_ENV).map_err(|_| {
        AsrError::LoadFailed(format!(
            "{ORT_DYLIB_ENV} must point to a local ONNX Runtime dylib before loading real ONNX packs"
        ))
    })?;

    if runtime_path.trim().is_empty() {
        return Err(AsrError::LoadFailed(format!(
            "{ORT_DYLIB_ENV} must not be empty"
        )));
    }

    if !Path::new(&runtime_path).is_file() {
        return Err(AsrError::LoadFailed(format!(
            "{ORT_DYLIB_ENV} points to a missing file: {runtime_path}"
        )));
    }

    info!(
        model_id = %config.model_id,
        intra_threads = tuning.intra_threads,
        inter_threads = tuning.inter_threads,
        parallel_execution = tuning.parallel_execution,
        memory_pattern = tuning.memory_pattern,
        runtime_path = %runtime_path,
        "initializing ONNX Runtime sessions"
    );

    catch_ort_panic(|| {
        ort::init_from(&runtime_path)
            .with_name("speech-clerk")
            .with_telemetry(false)
            .commit()
            .map_err(map_ort_load_error)
    })??;

    let onnx_assets = config
        .files
        .iter()
        .filter(|asset| asset.path.ends_with(".onnx"))
        .cloned()
        .collect::<Vec<_>>();

    if onnx_assets.is_empty() {
        return Err(AsrError::LoadFailed(format!(
            "model pack {} does not declare any .onnx assets",
            config.model_id
        )));
    }

    onnx_assets
        .into_iter()
        .map(|asset| load_onnx_session(asset, tuning))
        .collect::<Result<Vec<_>, _>>()
}

fn load_onnx_session(
    asset: ModelAsset,
    tuning: OrtSessionTuning,
) -> Result<OrtModelSession, AsrError> {
    let log_id = format!("speech-clerk-{}", asset.role);
    let session = catch_ort_panic(|| {
        Session::builder()
            .and_then(|builder| builder.with_optimization_level(GraphOptimizationLevel::Level3))
            .and_then(|builder| builder.with_intra_threads(tuning.intra_threads))
            .and_then(|builder| builder.with_inter_threads(tuning.inter_threads))
            .and_then(|builder| builder.with_parallel_execution(tuning.parallel_execution))
            .and_then(|builder| builder.with_memory_pattern(tuning.memory_pattern))
            .and_then(|builder| builder.with_prepacking(true))
            .and_then(|builder| builder.with_quant_qdq(true))
            .and_then(|builder| builder.with_qdq_cleanup())
            .and_then(|builder| builder.with_intra_op_spinning(false))
            .and_then(|builder| builder.with_inter_op_spinning(false))
            .and_then(|builder| builder.with_log_id(log_id))
            .and_then(|builder| builder.commit_from_file(&asset.absolute_path))
            .map_err(map_ort_load_error)
    })??;
    let inputs = session
        .inputs
        .iter()
        .map(|input| ModelIo {
            name: input.name.clone(),
            dimensions: tensor_dimensions(&input.input_type),
        })
        .collect();

    Ok(OrtModelSession {
        role: asset.role,
        inputs,
        session,
    })
}

fn default_intra_threads() -> usize {
    std::thread::available_parallelism()
        .map(|threads| threads.get())
        .map(|threads| threads.clamp(1, MAX_DEFAULT_INTRA_THREADS))
        .unwrap_or(MAX_DEFAULT_INTRA_THREADS)
}

fn parse_positive_usize_env(name: &str) -> Result<Option<usize>, AsrError> {
    match std::env::var(name) {
        Ok(value) => parse_positive_usize(&value, name).map(Some),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(AsrError::LoadFailed(format!(
            "{name} is not valid UTF-8: {error}"
        ))),
    }
}

fn parse_positive_usize(value: &str, name: &str) -> Result<usize, AsrError> {
    let parsed = value.trim().parse::<usize>().map_err(|error| {
        AsrError::LoadFailed(format!("{name} must be a positive integer: {error}"))
    })?;

    if parsed == 0 {
        return Err(AsrError::LoadFailed(format!(
            "{name} must be greater than zero"
        )));
    }

    Ok(parsed)
}

fn parse_bool_env(name: &str) -> Result<Option<bool>, AsrError> {
    match std::env::var(name) {
        Ok(value) => parse_bool(&value, name).map(Some),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(AsrError::LoadFailed(format!(
            "{name} is not valid UTF-8: {error}"
        ))),
    }
}

fn parse_bool(value: &str, name: &str) -> Result<bool, AsrError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(AsrError::LoadFailed(format!(
            "{name} must be one of 1, 0, true, false, yes, no, on, or off"
        ))),
    }
}

fn catch_ort_panic<T>(operation: impl FnOnce() -> T) -> Result<T, AsrError> {
    catch_ort_panic_as(operation, AsrError::LoadFailed)
}

fn catch_ort_panic_transcription<T>(operation: impl FnOnce() -> T) -> Result<T, AsrError> {
    catch_ort_panic_as(operation, AsrError::TranscriptionFailed)
}

fn catch_ort_panic_as<T>(
    operation: impl FnOnce() -> T,
    classify: fn(String) -> AsrError,
) -> Result<T, AsrError> {
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(AssertUnwindSafe(operation));
    panic::set_hook(previous_hook);

    result.map_err(|panic| {
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("unknown ONNX Runtime panic");
        classify(message.to_owned())
    })
}

fn map_ort_load_error(error: ort::Error) -> AsrError {
    AsrError::LoadFailed(error.to_string())
}

fn map_ort_transcription_error(error: ort::Error) -> AsrError {
    AsrError::TranscriptionFailed(error.to_string())
}

fn required_asset_with_roles(config: &ModelConfig, roles: &[&str]) -> Result<ModelAsset, AsrError> {
    config
        .files
        .iter()
        .find(|asset| roles.iter().any(|role| asset.role == *role))
        .cloned()
        .ok_or_else(|| {
            AsrError::LoadFailed(format!(
                "model pack {} does not declare any of these assets: {}",
                config.model_id,
                roles.join(", ")
            ))
        })
}

fn read_required_asset(path: &Path, role: &str) -> Result<Vec<u8>, AsrError> {
    let bytes = fs::read(path).map_err(|source| {
        AsrError::LoadFailed(format!(
            "failed to read {role} asset {}: {source}",
            path.display()
        ))
    })?;

    if bytes.is_empty() {
        return Err(AsrError::LoadFailed(format!(
            "{role} asset {} must not be empty",
            path.display()
        )));
    }

    Ok(bytes)
}

fn field_value(input: &str, field: &str) -> Option<String> {
    let prefix = format!("{field}=");
    input
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{
        OrtAsrEngine, OrtSessionTuning, ParakeetConfig, TdtDecision, TokenizerAsset, parse_bool,
        parse_positive_usize, parse_vocabulary,
    };
    use asr_api::{AsrEngine, ModelAsset, ModelConfig, PcmAudio, TranscribeOptions};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rejects_non_onnx_runtime() {
        let mut engine = OrtAsrEngine::default();
        let config = ModelConfig::new("fake-local", "Fake Local", "fake", vec!["en".to_owned()]);

        let result = engine.load(&config);

        assert!(result.is_err());
    }

    #[test]
    fn loads_manifest_declared_model_and_tokenizer_assets() -> Result<(), Box<dyn std::error::Error>>
    {
        let pack_dir = unique_temp_dir("speech-clerk-asr-onnx-load")?;
        fs::create_dir_all(&pack_dir)?;
        let model_path = pack_dir.join("model.onnx");
        let tokenizer_path = pack_dir.join("tokenizer.model");
        fs::write(&model_path, "onnx fixture")?;
        fs::write(
            &tokenizer_path,
            "speech-clerk-tokenizer-fixture-v1\ntranscript=hello from onnx\nlanguage=en\n",
        )?;
        let mut engine = OrtAsrEngine::default();

        engine.load(&fixture_config(&pack_dir, &model_path, &tokenizer_path))?;

        assert_eq!(engine.loaded_model_id(), Some("parakeet-fixture"));
        let _ = fs::remove_dir_all(&pack_dir);
        Ok(())
    }

    #[test]
    fn fixture_tokenizer_path_can_transcribe_for_tests() -> Result<(), Box<dyn std::error::Error>> {
        let pack_dir = unique_temp_dir("speech-clerk-asr-onnx-transcribe")?;
        fs::create_dir_all(&pack_dir)?;
        let model_path = pack_dir.join("model.onnx");
        let tokenizer_path = pack_dir.join("tokenizer.model");
        fs::write(&model_path, "onnx fixture")?;
        fs::write(
            &tokenizer_path,
            "speech-clerk-tokenizer-fixture-v1\ntranscript=hello from onnx\nlanguage=en\n",
        )?;
        let mut engine = OrtAsrEngine::default();
        engine.load(&fixture_config(&pack_dir, &model_path, &tokenizer_path))?;

        let transcript = engine.transcribe(
            &PcmAudio {
                samples: vec![0.1; 16_000],
                sample_rate_hz: 16_000,
            },
            &TranscribeOptions {
                language_hint: None,
            },
        )?;

        assert_eq!(transcript.text, "hello from onnx");
        let _ = fs::remove_dir_all(&pack_dir);
        Ok(())
    }

    #[test]
    fn real_onnx_load_requires_runtime_dylib_path() -> Result<(), Box<dyn std::error::Error>> {
        let pack_dir = unique_temp_dir("speech-clerk-asr-onnx-runtime")?;
        fs::create_dir_all(&pack_dir)?;
        let model_path = pack_dir.join("model.onnx");
        let tokenizer_path = pack_dir.join("tokenizer.model");
        fs::write(&model_path, "onnx fixture")?;
        fs::write(&tokenizer_path, "not a fixture tokenizer")?;
        let mut engine = OrtAsrEngine::default();

        let result = engine.load(&fixture_config(&pack_dir, &model_path, &tokenizer_path));

        assert!(result.is_err());
        let _ = fs::remove_dir_all(&pack_dir);
        Ok(())
    }

    #[test]
    fn parakeet_config_uses_manifest_json_fields() -> Result<(), Box<dyn std::error::Error>> {
        let config = ParakeetConfig::from_json(r#"{"features_size":80,"subsampling_factor":4}"#)?;

        assert_eq!(
            config,
            ParakeetConfig {
                features_size: 80,
                subsampling_factor: 4,
            }
        );
        Ok(())
    }

    #[test]
    fn parakeet_config_rejects_zero_feature_count() {
        let result = ParakeetConfig::from_json(r#"{"features_size":0}"#);

        assert!(result.is_err());
    }

    #[test]
    fn parse_vocabulary_orders_tokens_by_declared_index() {
        let vocabulary = parse_vocabulary("world 2\n<blank> 0\nhello 1\n");

        assert_eq!(
            vocabulary,
            vec![
                (0, "<blank>".to_owned()),
                (1, "hello".to_owned()),
                (2, "world".to_owned())
            ]
        );
    }

    #[test]
    fn tokenizer_decodes_sentencepiece_tokens_and_skips_blank()
    -> Result<(), Box<dyn std::error::Error>> {
        let tokenizer =
            TokenizerAsset::load("<unk> 0\n▁hello 1\n▁world 2\n<blk> 3\n".as_bytes().to_vec())?;

        let text = tokenizer.decode_tokens(&[1, 2, 3]);

        assert_eq!(text, "hello world");
        Ok(())
    }

    #[test]
    fn tdt_decision_splits_token_and_duration_logits() -> Result<(), Box<dyn std::error::Error>> {
        let decision = TdtDecision::greedy(&[0.1, 0.9, 0.3, -0.1, 0.2, 0.7], 3)?;

        assert_eq!(
            decision,
            TdtDecision {
                token_id: 1,
                token_score: 0.9,
                duration_frames: 2,
                duration_score: 0.7,
            }
        );
        Ok(())
    }

    #[test]
    fn tdt_decision_rejects_logits_without_duration_scores() {
        let result = TdtDecision::greedy(&[0.1, 0.9, 0.3], 3);

        assert!(result.is_err());
    }

    #[test]
    fn default_ort_tuning_keeps_inter_threads_bounded() {
        let tuning = OrtSessionTuning::default();

        assert_eq!(tuning.inter_threads, 1);
    }

    #[test]
    fn parse_positive_usize_rejects_zero_threads() {
        let result = parse_positive_usize("0", "SPEECH_CLERK_ORT_INTRA_THREADS");

        assert!(result.is_err());
    }

    #[test]
    fn parse_bool_accepts_release_candidate_env_values() -> Result<(), Box<dyn std::error::Error>> {
        let enabled = parse_bool("on", "SPEECH_CLERK_ORT_PARALLEL")?;

        assert!(enabled);
        Ok(())
    }

    fn fixture_config(pack_dir: &Path, model_path: &Path, tokenizer_path: &Path) -> ModelConfig {
        ModelConfig::new(
            "parakeet-fixture",
            "Parakeet Fixture",
            "onnx",
            vec!["en".to_owned()],
        )
        .with_assets(
            pack_dir,
            vec![
                ModelAsset::new("model", "model.onnx", model_path, Some("0".repeat(64))),
                ModelAsset::new(
                    "tokenizer",
                    "tokenizer.model",
                    tokenizer_path,
                    Some("1".repeat(64)),
                ),
            ],
        )
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
