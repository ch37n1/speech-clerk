//! Package local model exports into Speech Clerk model packs.

use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use model_pack::sha256_file_hex;

const DEFAULT_MODEL_ID: &str = "parakeet-tdt-0.6b-v3-int8";
const DEFAULT_DISPLAY_NAME: &str = "Parakeet TDT 0.6B V3 INT8";

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run() -> Result<(), PackageError> {
    match PackageCommand::parse(env::args().skip(1))? {
        PackageCommand::ParakeetSplit(config) => package_parakeet_split(config),
    }
}

fn package_parakeet_split(config: ParakeetSplitConfig) -> Result<(), PackageError> {
    let pack_dir = config.output_dir.join(&config.model_id);
    fs::create_dir_all(&pack_dir).map_err(|source| {
        PackageError::Io(format!("failed to create {}: {source}", pack_dir.display()))
    })?;

    let assets = [
        PackAsset::new("encoder", "encoder-model.int8.onnx"),
        PackAsset::new("decoder_joint", "decoder_joint-model.int8.onnx"),
        PackAsset::new("preprocessor", "nemo128.onnx"),
        PackAsset::new("vocab", "vocab.txt"),
        PackAsset::new("config", "config.json"),
    ];

    for asset in &assets {
        copy_asset(&config.source_dir, &pack_dir, asset.path)?;
    }

    let manifest = render_manifest(&config, &pack_dir, &assets)?;
    fs::write(pack_dir.join("manifest.json"), manifest).map_err(|source| {
        PackageError::Io(format!(
            "failed to write {}: {source}",
            pack_dir.join("manifest.json").display()
        ))
    })?;

    println!("{}", pack_dir.display());
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PackageCommand {
    ParakeetSplit(ParakeetSplitConfig),
}

impl PackageCommand {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, PackageError> {
        let command = args.next().ok_or_else(usage_error)?;

        match command.as_str() {
            "parakeet-split" => Ok(Self::ParakeetSplit(ParakeetSplitConfig::parse(args)?)),
            _ => Err(usage_error()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParakeetSplitConfig {
    source_dir: PathBuf,
    output_dir: PathBuf,
    model_id: String,
    display_name: String,
}

impl ParakeetSplitConfig {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, PackageError> {
        let mut source_dir = None;
        let mut output_dir = None;
        let mut model_id = DEFAULT_MODEL_ID.to_owned();
        let mut display_name = DEFAULT_DISPLAY_NAME.to_owned();

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--source" => source_dir = Some(PathBuf::from(next_value(&mut args, "--source")?)),
                "--output" => output_dir = Some(PathBuf::from(next_value(&mut args, "--output")?)),
                "--model-id" => model_id = next_value(&mut args, "--model-id")?,
                "--display-name" => display_name = next_value(&mut args, "--display-name")?,
                _ => return Err(usage_error()),
            }
        }

        Ok(Self {
            source_dir: source_dir.ok_or_else(usage_error)?,
            output_dir: output_dir.ok_or_else(usage_error)?,
            model_id,
            display_name,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PackAsset {
    role: &'static str,
    path: &'static str,
}

impl PackAsset {
    fn new(role: &'static str, path: &'static str) -> Self {
        Self { role, path }
    }
}

fn copy_asset(source_dir: &Path, pack_dir: &Path, file_name: &str) -> Result<(), PackageError> {
    let source = source_dir.join(file_name);
    let destination = pack_dir.join(file_name);

    if !source.is_file() {
        return Err(PackageError::Message(format!(
            "required source asset is missing: {}",
            source.display()
        )));
    }

    fs::copy(&source, &destination).map_err(|copy_error| {
        PackageError::Io(format!(
            "failed to copy {} to {}: {copy_error}",
            source.display(),
            destination.display()
        ))
    })?;
    Ok(())
}

fn render_manifest(
    config: &ParakeetSplitConfig,
    pack_dir: &Path,
    assets: &[PackAsset],
) -> Result<String, PackageError> {
    let mut files = String::new();

    for (index, asset) in assets.iter().enumerate() {
        if index > 0 {
            files.push_str(",\n");
        }

        let sha256 = sha256_file_hex(&pack_dir.join(asset.path))
            .map_err(|error| PackageError::Message(error.to_string()))?;
        files.push_str(&format!(
            "    {{\"role\": \"{}\", \"path\": \"{}\", \"sha256\": \"{}\"}}",
            asset.role, asset.path, sha256
        ));
    }

    Ok(format!(
        concat!(
            "{{\n",
            "  \"schemaVersion\": 1,\n",
            "  \"modelId\": \"{}\",\n",
            "  \"displayName\": \"{}\",\n",
            "  \"runtime\": \"onnx\",\n",
            "  \"quantization\": \"int8\",\n",
            "  \"audio\": {{\n",
            "    \"sampleRateHz\": 16000,\n",
            "    \"channels\": 1,\n",
            "    \"sampleFormat\": \"f32\"\n",
            "  }},\n",
            "  \"languages\": [\"en\", \"ru\", \"uk\", \"de\", \"fr\", \"es\"],\n",
            "  \"capabilities\": {{\n",
            "    \"chunked\": true,\n",
            "    \"streaming\": false,\n",
            "    \"timestamps\": true,\n",
            "    \"punctuation\": true,\n",
            "    \"languageDetection\": true\n",
            "  }},\n",
            "  \"files\": [\n",
            "{}\n",
            "  ]\n",
            "}}\n"
        ),
        escape_json(&config.model_id),
        escape_json(&config.display_name),
        files
    ))
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, PackageError> {
    args.next()
        .ok_or_else(|| PackageError::Message(format!("missing value for {flag}")))
}

fn usage_error() -> PackageError {
    PackageError::Message(
        "usage: model-packager parakeet-split --source <export-dir> --output <model-pack-root> [--model-id parakeet-tdt-0.6b-v3-int8] [--display-name \"Parakeet TDT 0.6B V3 INT8\"]"
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
enum PackageError {
    Message(String),
    Io(String),
}

impl fmt::Display for PackageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) | Self::Io(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for PackageError {}
