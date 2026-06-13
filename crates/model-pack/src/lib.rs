//! Local model-pack manifest parsing and validation.

use std::fmt;
use std::fs;
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

        for file in &self.files {
            let path = pack_dir.join(&file.path);
            if !path.is_file() {
                return Err(ModelPackError::Validation(format!(
                    "declared model file is missing: {}",
                    path.display()
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
    use super::{ModelManifest, ModelPack, ModelPackError};
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

    fn unique_temp_dir(prefix: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
        Ok(std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            now.as_nanos()
        )))
    }
}
