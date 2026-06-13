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
- For macOS UI work, use `docs/MACOS_APP_ACCESS.md` and `make macos-ui CALL_ARGS="..."` to inspect or operate the app during development.
- If a task changes user-visible macOS functionality, run `make macos-e2e-smoke` as the final verification step. If macOS permissions or the environment block it, report the blocker and any evidence collected.

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
2. For macOS UI work, use the app access flow in `docs/MACOS_APP_ACCESS.md` while developing.
3. After user-visible macOS functionality changes, run `make macos-e2e-smoke` before handing work back. If it is blocked by GUI permissions or environment limits, report the exact blocker.
4. Before modifying code, read the owning crate/module and relevant docs.
5. Do not bypass hooks or quality gates unless explicitly instructed.
6. Keep generated model files, build outputs, and local secrets out of git.
7. If implementation diverges from `docs/TECHNICAL_ARCHITECTURE.md`, update the docs in the same change.
