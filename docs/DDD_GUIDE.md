# DDD Guide

This project uses pragmatic domain-driven design. The goal is to keep dictation product rules in Rust domain crates, keep native platform code at the edges, and make service boundaries explicit enough to test.

## Core Principles

- Model product concepts in product terms: `DictationSession`, `AudioChunk`, `ModelPack`, `Transcript`, `LanguageMode`, and `AsrEngine`.
- Organize Rust code by domain responsibility, not by global technical layers.
- Keep platform capture and insertion in `apps/`.
- Keep concrete model runtime details behind ASR interfaces.
- Prefer explicit domain objects over loose maps or generic JSON payloads.

## Workspace Shape

The V1 Rust workspace starts with these crates:

- `asr-api`: ASR traits, capabilities, transcript contracts, and backend-neutral errors.
- `audio-pipeline`: PCM normalization, resampling, VAD, buffering, and chunking.
- `model-pack`: model-pack manifests, paths, checksums, and validation.
- `postprocess`: deterministic transcript cleanup and replacement rules.
- `dictation-core`: product-level orchestration and recording session state.
- `ffi`: UniFFI-exported product API for Swift and Kotlin.

Native apps stay under `apps/` and should call only the product-level FFI API.
For macOS-specific Swift conventions, follow `docs/SWIFT_GUIDE.md`.

## Boundary Rules

- Platform apps must not load model sessions or call ONNX Runtime directly.
- `dictation-core` coordinates crates but should not contain low-level model runtime code.
- `asr-api` must not depend on concrete ASR backend crates.
- Concrete ASR backends, such as the future `asr-onnx`, implement `asr-api` contracts.
- Services should accept and return domain-owned types, not platform DTOs.
- Shared code must have an owning crate or module; do not add generic `utils`, `common`, or `types` buckets.

## Adding a Domain Concept

1. Name the product concept first.
2. Put the type in the crate that owns the behavior.
3. Add behavior next to the type where practical.
4. Expose only the smaller interface another crate needs.
5. Add tests at the owning boundary.
6. Update architecture docs when the boundary changes.

## Testing Expectations

- Unit-test pure domain behavior in the owning crate.
- Integration-test crate boundaries when orchestration matters.
- Keep platform tests focused on platform behavior, permissions, and FFI calls.
- Manual verification remains required for app workflows described in `docs/ROADMAP.md`.
