//! Local model-pack manifest parsing and validation.

use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// Default manifest filename inside a model pack.
pub const MANIFEST_FILE_NAME: &str = "manifest.json";

/// Parsed local model pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPack {
    path: PathBuf,
    manifest: ModelManifest,
}

impl ModelPack {
    /// Load, parse, and validate a model pack from a directory.
    pub fn load_from_dir(path: impl Into<PathBuf>) -> Result<Self, ModelPackError> {
        let path = path.into();
        let manifest_path = path.join(MANIFEST_FILE_NAME);
        let manifest_json = fs::read_to_string(&manifest_path).map_err(|source| {
            ModelPackError::Io(format!(
                "failed to read {}: {source}",
                manifest_path.display()
            ))
        })?;
        let manifest = ModelManifest::from_json(&manifest_json)?;
        manifest.validate_files(&path)?;

        Ok(Self { path, manifest })
    }

    /// Directory containing the model pack.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Parsed and validated manifest.
    #[must_use]
    pub fn manifest(&self) -> &ModelManifest {
        &self.manifest
    }
}

/// Model-pack manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelManifest {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Stable model identifier.
    pub model_id: String,
    /// Human-readable model name.
    pub display_name: String,
    /// ASR runtime identifier, such as `fake` or `onnx`.
    pub runtime: String,
    /// Quantization label from the pack metadata.
    pub quantization: String,
    /// Audio format expected by the pack.
    pub audio: AudioSpec,
    /// BCP-47 language tags supported by the pack.
    pub languages: Vec<String>,
    /// Pack capability metadata.
    pub capabilities: ModelCapabilities,
    /// Files declared by the pack.
    pub files: Vec<ModelFile>,
}

impl ModelManifest {
    /// Parse a V1 manifest from JSON text.
    pub fn from_json(input: &str) -> Result<Self, ModelPackError> {
        let audio_object = extract_object(input, "audio")?;
        let capabilities_object = extract_object(input, "capabilities")?;

        let manifest = Self {
            schema_version: extract_u32(input, "schemaVersion")?,
            model_id: extract_string(input, "modelId")?,
            display_name: extract_string(input, "displayName")?,
            runtime: extract_string(input, "runtime")?,
            quantization: extract_string(input, "quantization")?,
            audio: AudioSpec {
                sample_rate_hz: extract_u32(&audio_object, "sampleRateHz")?,
                channels: extract_u16(&audio_object, "channels")?,
                sample_format: extract_string(&audio_object, "sampleFormat")?,
            },
            languages: extract_string_array(input, "languages")?,
            capabilities: ModelCapabilities {
                chunked: extract_bool(&capabilities_object, "chunked")?,
                streaming: extract_bool(&capabilities_object, "streaming")?,
                timestamps: extract_bool(&capabilities_object, "timestamps")?,
                punctuation: extract_bool(&capabilities_object, "punctuation")?,
                language_detection: extract_bool(&capabilities_object, "languageDetection")?,
            },
            files: extract_files(input)?,
        };

        manifest.validate_metadata()?;
        Ok(manifest)
    }

    /// Validate manifest metadata that does not require filesystem access.
    pub fn validate_metadata(&self) -> Result<(), ModelPackError> {
        if self.schema_version != 1 {
            return Err(ModelPackError::Validation(format!(
                "unsupported schemaVersion {}",
                self.schema_version
            )));
        }

        require_non_empty("modelId", &self.model_id)?;
        require_non_empty("displayName", &self.display_name)?;
        require_non_empty("runtime", &self.runtime)?;
        require_non_empty("quantization", &self.quantization)?;

        if !matches!(self.runtime.as_str(), "fake" | "onnx") {
            return Err(ModelPackError::Validation(format!(
                "unsupported runtime {}",
                self.runtime
            )));
        }

        if self.audio.sample_rate_hz == 0 {
            return Err(ModelPackError::Validation(
                "audio.sampleRateHz must be greater than zero".to_owned(),
            ));
        }

        if self.audio.channels == 0 {
            return Err(ModelPackError::Validation(
                "audio.channels must be greater than zero".to_owned(),
            ));
        }

        if self.audio.sample_format != "f32" {
            return Err(ModelPackError::Validation(format!(
                "unsupported audio.sampleFormat {}",
                self.audio.sample_format
            )));
        }

        if self.languages.is_empty() {
            return Err(ModelPackError::Validation(
                "languages must contain at least one language".to_owned(),
            ));
        }

        for language in &self.languages {
            require_non_empty("languages[]", language)?;
        }

        for file in &self.files {
            require_non_empty("files[].role", &file.role)?;
            require_non_empty("files[].path", &file.path)?;
            validate_relative_file_path(&file.path)?;
        }

        Ok(())
    }

    fn validate_files(&self, pack_dir: &Path) -> Result<(), ModelPackError> {
        self.validate_metadata()?;
        self.validate_runtime_file_contract()?;

        for file in &self.files {
            let path = pack_dir.join(&file.path);
            if !path.is_file() {
                return Err(ModelPackError::Validation(format!(
                    "declared model file is missing: {}",
                    path.display()
                )));
            }

            if let Some(expected) = normalized_sha256(file)? {
                let actual = sha256_file_hex(&path)?;
                if actual != expected {
                    return Err(ModelPackError::Validation(format!(
                        "checksum mismatch for {}: expected {expected}, got {actual}",
                        file.path
                    )));
                }
            }
        }

        Ok(())
    }

    fn validate_runtime_file_contract(&self) -> Result<(), ModelPackError> {
        if self.runtime != "onnx" {
            return Ok(());
        }

        let has_single_model = self.files.iter().any(|file| file.role == "model");
        let has_single_tokenizer = self.files.iter().any(|file| file.role == "tokenizer");
        let has_parakeet_split = self.files.iter().any(|file| file.role == "encoder")
            && self.files.iter().any(|file| file.role == "decoder_joint")
            && self.files.iter().any(|file| file.role == "preprocessor")
            && self.files.iter().any(|file| file.role == "vocab")
            && self.files.iter().any(|file| file.role == "config");

        if !(has_parakeet_split || has_single_model && has_single_tokenizer) {
            return Err(ModelPackError::Validation(
                "onnx model packs must declare either model/tokenizer files or encoder/decoder_joint/preprocessor/vocab/config files".to_owned(),
            ));
        }

        for file in &self.files {
            let Some(checksum) = &file.sha256 else {
                return Err(ModelPackError::Validation(format!(
                    "onnx file {} must declare sha256",
                    file.path
                )));
            };

            if checksum.trim().is_empty() {
                return Err(ModelPackError::Validation(format!(
                    "onnx file {} must declare sha256",
                    file.path
                )));
            }
        }

        Ok(())
    }
}

/// Audio format metadata in a model-pack manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSpec {
    /// Sample rate in hertz.
    pub sample_rate_hz: u32,
    /// Number of audio channels.
    pub channels: u16,
    /// Sample format identifier.
    pub sample_format: String,
}

/// Capability flags declared by a model pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilities {
    /// Whether the model supports chunked transcription.
    pub chunked: bool,
    /// Whether the model supports token-level streaming.
    pub streaming: bool,
    /// Whether the model can produce timestamps.
    pub timestamps: bool,
    /// Whether the model can produce punctuation.
    pub punctuation: bool,
    /// Whether the model can detect language automatically.
    pub language_detection: bool,
}

/// File entry declared by a model-pack manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFile {
    /// File role, such as `model`, `tokenizer`, or `fake-config`.
    pub role: String,
    /// Relative path inside the model-pack directory.
    pub path: String,
    /// Optional SHA-256 checksum string.
    pub sha256: Option<String>,
}

/// Minimal model-pack identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPackRef {
    /// Stable model-pack identifier.
    pub id: String,
}

impl ModelPackRef {
    /// Create a model-pack reference.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

/// Model-pack loading, parsing, and validation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelPackError {
    /// Filesystem access failed.
    Io(String),
    /// Manifest parsing failed.
    Parse(String),
    /// Manifest validation failed.
    Validation(String),
}

impl fmt::Display for ModelPackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "model-pack I/O error: {message}"),
            Self::Parse(message) => write!(formatter, "model-pack parse error: {message}"),
            Self::Validation(message) => {
                write!(formatter, "model-pack validation error: {message}")
            }
        }
    }
}

impl std::error::Error for ModelPackError {}

fn require_non_empty(field: &str, value: &str) -> Result<(), ModelPackError> {
    if value.trim().is_empty() {
        return Err(ModelPackError::Validation(format!(
            "{field} must not be empty"
        )));
    }

    Ok(())
}

fn validate_relative_file_path(path: &str) -> Result<(), ModelPackError> {
    let path = Path::new(path);

    if path.is_absolute() {
        return Err(ModelPackError::Validation(
            "file paths must be relative to the pack directory".to_owned(),
        ));
    }

    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(ModelPackError::Validation(
            "file paths must stay inside the pack directory".to_owned(),
        ));
    }

    Ok(())
}

fn normalized_sha256(file: &ModelFile) -> Result<Option<String>, ModelPackError> {
    let Some(checksum) = &file.sha256 else {
        return Ok(None);
    };
    let checksum = checksum.trim().to_ascii_lowercase();

    if checksum.is_empty() {
        return Ok(None);
    }

    if checksum.len() != 64
        || !checksum
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(ModelPackError::Validation(format!(
            "sha256 for {} must be 64 lowercase or uppercase hex characters",
            file.path
        )));
    }

    Ok(Some(checksum))
}

/// Return the SHA-256 digest for a local file as lowercase hex.
pub fn sha256_file_hex(path: &Path) -> Result<String, ModelPackError> {
    let mut file = fs::File::open(path).map_err(|source| {
        ModelPackError::Io(format!("failed to open {}: {source}", path.display()))
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];

    loop {
        let read = file.read(&mut buffer).map_err(|source| {
            ModelPackError::Io(format!("failed to read {}: {source}", path.display()))
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hex_lower(&hasher.finalize()))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }

    output
}

#[derive(Debug, Clone)]
struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    bit_len: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            buffer_len: 0,
            bit_len: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        self.bit_len = self.bit_len.wrapping_add((input.len() as u64) * 8);

        if self.buffer_len > 0 {
            let remaining = 64 - self.buffer_len;
            let to_copy = remaining.min(input.len());
            self.buffer[self.buffer_len..self.buffer_len + to_copy]
                .copy_from_slice(&input[..to_copy]);
            self.buffer_len += to_copy;
            input = &input[to_copy..];

            if self.buffer_len == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffer_len = 0;
            }
        }

        while input.len() >= 64 {
            let mut block = [0_u8; 64];
            block.copy_from_slice(&input[..64]);
            self.compress(&block);
            input = &input[64..];
        }

        if !input.is_empty() {
            self.buffer[..input.len()].copy_from_slice(input);
            self.buffer_len = input.len();
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;

        if self.buffer_len > 56 {
            for byte in &mut self.buffer[self.buffer_len..] {
                *byte = 0;
            }
            let block = self.buffer;
            self.compress(&block);
            self.buffer_len = 0;
        }

        for byte in &mut self.buffer[self.buffer_len..56] {
            *byte = 0;
        }
        self.buffer[56..64].copy_from_slice(&self.bit_len.to_be_bytes());

        let block = self.buffer;
        self.compress(&block);

        let mut output = [0_u8; 32];
        for (index, value) in self.state.iter().enumerate() {
            output[index * 4..index * 4 + 4].copy_from_slice(&value.to_be_bytes());
        }
        output
    }

    fn compress(&mut self, block: &[u8; 64]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut schedule = [0_u32; 64];

        for (index, word) in schedule.iter_mut().enumerate().take(16) {
            let start = index * 4;
            *word = u32::from_be_bytes([
                block[start],
                block[start + 1],
                block[start + 2],
                block[start + 3],
            ]);
        }

        for index in 16..64 {
            let small_sigma0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let small_sigma1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(small_sigma0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(small_sigma1);
        }

        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];

        for index in 0..64 {
            let big_sigma1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(big_sigma1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(schedule[index]);
            let big_sigma0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = big_sigma0.wrapping_add(majority);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

fn extract_string(input: &str, key: &str) -> Result<String, ModelPackError> {
    let value_start = find_value_start(input, key)?;
    let (value, _) = parse_json_string_at(input, value_start)?;
    Ok(value)
}

fn extract_optional_string(input: &str, key: &str) -> Result<Option<String>, ModelPackError> {
    match find_value_start(input, key) {
        Ok(value_start) => {
            let (value, _) = parse_json_string_at(input, value_start)?;
            Ok(Some(value))
        }
        Err(ModelPackError::Parse(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

fn extract_u16(input: &str, key: &str) -> Result<u16, ModelPackError> {
    let value = extract_u32(input, key)?;
    u16::try_from(value).map_err(|_| ModelPackError::Parse(format!("{key} does not fit in u16")))
}

fn extract_u32(input: &str, key: &str) -> Result<u32, ModelPackError> {
    let value_start = find_value_start(input, key)?;
    let digits = input[value_start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();

    if digits.is_empty() {
        return Err(ModelPackError::Parse(format!("{key} must be a number")));
    }

    digits
        .parse::<u32>()
        .map_err(|source| ModelPackError::Parse(format!("{key} is invalid: {source}")))
}

fn extract_bool(input: &str, key: &str) -> Result<bool, ModelPackError> {
    let value_start = find_value_start(input, key)?;
    let value = &input[value_start..];

    if value.starts_with("true") {
        Ok(true)
    } else if value.starts_with("false") {
        Ok(false)
    } else {
        Err(ModelPackError::Parse(format!("{key} must be a boolean")))
    }
}

fn extract_object(input: &str, key: &str) -> Result<String, ModelPackError> {
    let value_start = find_value_start(input, key)?;
    let end = find_enclosed_end(input, value_start, b'{', b'}')?;
    Ok(input[value_start..end].to_owned())
}

fn extract_string_array(input: &str, key: &str) -> Result<Vec<String>, ModelPackError> {
    let value_start = find_value_start(input, key)?;
    let end = find_enclosed_end(input, value_start, b'[', b']')?;
    let mut cursor = value_start + 1;
    let mut values = Vec::new();

    while cursor < end - 1 {
        cursor = skip_ascii_whitespace_and_commas(input, cursor);
        if cursor >= end - 1 {
            break;
        }

        let (value, next_cursor) = parse_json_string_at(input, cursor)?;
        values.push(value);
        cursor = next_cursor;
    }

    Ok(values)
}

fn extract_files(input: &str) -> Result<Vec<ModelFile>, ModelPackError> {
    let value_start = find_value_start(input, "files")?;
    let end = find_enclosed_end(input, value_start, b'[', b']')?;
    let mut cursor = value_start + 1;
    let mut files = Vec::new();

    while cursor < end - 1 {
        cursor = skip_ascii_whitespace_and_commas(input, cursor);
        if cursor >= end - 1 {
            break;
        }

        let object_end = find_enclosed_end(input, cursor, b'{', b'}')?;
        let object = &input[cursor..object_end];
        files.push(ModelFile {
            role: extract_string(object, "role")?,
            path: extract_string(object, "path")?,
            sha256: extract_optional_string(object, "sha256")?,
        });
        cursor = object_end;
    }

    Ok(files)
}

fn find_value_start(input: &str, key: &str) -> Result<usize, ModelPackError> {
    let needle = format!("\"{key}\"");
    let key_start = input
        .find(&needle)
        .ok_or_else(|| ModelPackError::Parse(format!("missing key {key}")))?;
    let after_key = key_start + needle.len();
    let colon_offset = input[after_key..]
        .find(':')
        .ok_or_else(|| ModelPackError::Parse(format!("missing colon after {key}")))?;
    let value_start = after_key + colon_offset + 1;
    Ok(skip_ascii_whitespace(input, value_start))
}

fn skip_ascii_whitespace(input: &str, mut index: usize) -> usize {
    while matches!(input.as_bytes().get(index), Some(byte) if byte.is_ascii_whitespace()) {
        index += 1;
    }

    index
}

fn skip_ascii_whitespace_and_commas(input: &str, mut index: usize) -> usize {
    while matches!(input.as_bytes().get(index), Some(byte) if byte.is_ascii_whitespace() || *byte == b',')
    {
        index += 1;
    }

    index
}

fn parse_json_string_at(input: &str, start: usize) -> Result<(String, usize), ModelPackError> {
    if input.as_bytes().get(start) != Some(&b'"') {
        return Err(ModelPackError::Parse("expected JSON string".to_owned()));
    }

    let mut output = String::new();
    let mut escaped = false;

    for (offset, character) in input[start + 1..].char_indices() {
        let cursor = start + 1 + offset + character.len_utf8();

        if escaped {
            match character {
                '"' | '\\' | '/' => output.push(character),
                'b' => output.push('\u{0008}'),
                'f' => output.push('\u{000c}'),
                'n' => output.push('\n'),
                'r' => output.push('\r'),
                't' => output.push('\t'),
                other => {
                    return Err(ModelPackError::Parse(format!(
                        "unsupported JSON escape \\{other}"
                    )));
                }
            }
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Ok((output, cursor));
        } else {
            output.push(character);
        }
    }

    Err(ModelPackError::Parse("unterminated JSON string".to_owned()))
}

fn find_enclosed_end(
    input: &str,
    start: usize,
    open: u8,
    close: u8,
) -> Result<usize, ModelPackError> {
    if input.as_bytes().get(start) != Some(&open) {
        return Err(ModelPackError::Parse("expected enclosed value".to_owned()));
    }

    let mut depth = 0_u32;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, byte) in input.as_bytes()[start..].iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }

        if *byte == b'"' {
            in_string = true;
        } else if *byte == open {
            depth += 1;
        } else if *byte == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Ok(start + offset + 1);
            }
        }
    }

    Err(ModelPackError::Parse(
        "unterminated enclosed JSON value".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{ModelManifest, ModelPack, ModelPackError, sha256_file_hex};
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
  "languages": ["en", "ru"],
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

    const ONNX_MANIFEST_TEMPLATE: &str = r#"{
  "schemaVersion": 1,
  "modelId": "parakeet-tdt-0.6b-v3-int8",
  "displayName": "Parakeet TDT 0.6B V3 INT8",
  "runtime": "onnx",
  "quantization": "int8",
  "audio": {
    "sampleRateHz": 16000,
    "channels": 1,
    "sampleFormat": "f32"
  },
  "languages": ["en"],
  "capabilities": {
    "chunked": true,
    "streaming": false,
    "timestamps": true,
    "punctuation": true,
    "languageDetection": true
  },
  "files": [
    {
      "role": "model",
      "path": "model.onnx",
      "sha256": "__MODEL_SHA__"
    },
    {
      "role": "tokenizer",
      "path": "tokenizer.model",
      "sha256": "__TOKENIZER_SHA__"
    }
  ]
}"#;

    #[test]
    fn parses_manifest_metadata() -> Result<(), ModelPackError> {
        let manifest = ModelManifest::from_json(MANIFEST)?;

        assert_eq!(manifest.model_id, "fake-local");
        assert_eq!(manifest.audio.sample_rate_hz, 16_000);
        assert_eq!(manifest.languages, vec!["en".to_owned(), "ru".to_owned()]);
        assert_eq!(manifest.files.len(), 1);
        Ok(())
    }

    #[test]
    fn rejects_unsupported_schema() {
        let result = ModelManifest::from_json(
            &MANIFEST.replace("\"schemaVersion\": 1", "\"schemaVersion\": 2"),
        );

        assert!(matches!(result, Err(ModelPackError::Validation(_))));
    }

    #[test]
    fn validates_declared_files() -> Result<(), Box<dyn std::error::Error>> {
        let dir = unique_temp_dir("speech-clerk-model-pack-valid")?;
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("manifest.json"), MANIFEST)?;
        fs::write(dir.join("fake_asr.txt"), "fake")?;

        let pack = ModelPack::load_from_dir(&dir)?;

        assert_eq!(pack.manifest().model_id, "fake-local");
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn rejects_missing_declared_files() -> Result<(), Box<dyn std::error::Error>> {
        let dir = unique_temp_dir("speech-clerk-model-pack-missing")?;
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("manifest.json"), MANIFEST)?;

        let result = ModelPack::load_from_dir(&dir);

        assert!(matches!(result, Err(ModelPackError::Validation(_))));
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn computes_sha256_file_hex() -> Result<(), Box<dyn std::error::Error>> {
        let dir = unique_temp_dir("speech-clerk-model-pack-sha")?;
        fs::create_dir_all(&dir)?;
        let path = dir.join("digest.txt");
        fs::write(&path, "abc")?;

        let digest = sha256_file_hex(&path)?;

        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn validates_onnx_file_checksums() -> Result<(), Box<dyn std::error::Error>> {
        let dir = create_onnx_pack()?;

        let pack = ModelPack::load_from_dir(&dir)?;

        assert_eq!(pack.manifest().runtime, "onnx");
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn validates_split_parakeet_onnx_assets() -> Result<(), Box<dyn std::error::Error>> {
        let dir = unique_temp_dir("speech-clerk-model-pack-parakeet-split")?;
        fs::create_dir_all(&dir)?;
        let encoder_path = dir.join("encoder-model.int8.onnx");
        let decoder_path = dir.join("decoder_joint-model.int8.onnx");
        let preprocessor_path = dir.join("nemo128.onnx");
        let vocab_path = dir.join("vocab.txt");
        let config_path = dir.join("config.json");
        fs::write(&encoder_path, "encoder fixture")?;
        fs::write(&decoder_path, "decoder fixture")?;
        fs::write(&preprocessor_path, "preprocessor fixture")?;
        fs::write(&vocab_path, "<unk> 0\n")?;
        fs::write(&config_path, "{\"model_type\":\"nemo-conformer-tdt\"}")?;
        let manifest = format!(
            r#"{{
  "schemaVersion": 1,
  "modelId": "parakeet-tdt-0.6b-v3-int8",
  "displayName": "Parakeet TDT 0.6B V3 INT8",
  "runtime": "onnx",
  "quantization": "int8",
  "audio": {{
    "sampleRateHz": 16000,
    "channels": 1,
    "sampleFormat": "f32"
  }},
  "languages": ["en"],
  "capabilities": {{
    "chunked": true,
    "streaming": false,
    "timestamps": true,
    "punctuation": true,
    "languageDetection": true
  }},
	  "files": [
	    {{"role": "encoder", "path": "encoder-model.int8.onnx", "sha256": "{}"}},
	    {{"role": "decoder_joint", "path": "decoder_joint-model.int8.onnx", "sha256": "{}"}},
	    {{"role": "preprocessor", "path": "nemo128.onnx", "sha256": "{}"}},
	    {{"role": "vocab", "path": "vocab.txt", "sha256": "{}"}},
	    {{"role": "config", "path": "config.json", "sha256": "{}"}}
	  ]
	}}"#,
            sha256_file_hex(&encoder_path)?,
            sha256_file_hex(&decoder_path)?,
            sha256_file_hex(&preprocessor_path)?,
            sha256_file_hex(&vocab_path)?,
            sha256_file_hex(&config_path)?
        );
        fs::write(dir.join("manifest.json"), manifest)?;

        let pack = ModelPack::load_from_dir(&dir)?;

        assert_eq!(pack.manifest().files.len(), 5);
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn rejects_onnx_checksum_mismatch() -> Result<(), Box<dyn std::error::Error>> {
        let dir = create_onnx_pack()?;
        fs::write(dir.join("model.onnx"), "changed")?;

        let result = ModelPack::load_from_dir(&dir);

        assert!(matches!(result, Err(ModelPackError::Validation(_))));
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    fn create_onnx_pack() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let dir = unique_temp_dir("speech-clerk-model-pack-onnx")?;
        fs::create_dir_all(&dir)?;
        let model_path = dir.join("model.onnx");
        let tokenizer_path = dir.join("tokenizer.model");
        fs::write(&model_path, "onnx fixture")?;
        fs::write(&tokenizer_path, "tokenizer fixture")?;
        let manifest = ONNX_MANIFEST_TEMPLATE
            .replace("__MODEL_SHA__", &sha256_file_hex(&model_path)?)
            .replace("__TOKENIZER_SHA__", &sha256_file_hex(&tokenizer_path)?);
        fs::write(dir.join("manifest.json"), manifest)?;
        Ok(dir)
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
