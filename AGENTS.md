# AGENTS.md

Quick guide for agents working in this repo.

## Start Here

- Read `README.md`, `docs/ROADMAP.md`, and `docs/TECHNICAL_ARCHITECTURE.md` before changing implementation.
- Use `docs/DDD_GUIDE.md` for code ownership and boundary rules.
- Treat `docs/` as canonical for architecture and process details.

## Commands

- Use `make` for routine actions.
- Run `make help` to see available targets.
- Run `make c` after any code change before handing work back or committing.

## Structure

- `crates/`: shared Rust implementation
- `apps/`: thin native platform shells
- `tools/`: developer and benchmark tools
- `docs/`: product, architecture, and process documentation

## Conventions

- Rust owns dictation product logic. Platform code must stay thin.
- Keep ASR execution behind `asr-api`; do not call concrete model runtimes from platform code.
- Keep deterministic product behavior in Rust crates, not in Swift or Kotlin.
- Prefer explicit domain types over untyped maps or generic JSON payloads.
- Add shared concepts as named crates or modules, not catch-all utility buckets.
- Use `cargo fmt`, `clippy`, tests, dependency audits, and dead-code checks through `make`.

## Critical Rules

1. After any code change, run `make c` and fix failures.
2. Before modifying code, read the owning crate/module and relevant docs.
3. Do not bypass hooks or quality gates unless explicitly instructed.
4. Keep generated model files, build outputs, and local secrets out of git.
5. If implementation diverges from `docs/TECHNICAL_ARCHITECTURE.md`, update the docs in the same change.
