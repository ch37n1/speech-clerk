# Speech Clerk

Speech Clerk is a local-first speech-to-text app. V1 targets macOS first and Android IME second, with shared dictation logic implemented in Rust and exposed to platform apps through UniFFI.

## Start Here

- `docs/TECHNICAL_ARCHITECTURE.md` defines the V1 implementation architecture.
- `docs/ROADMAP.md` defines phase-by-phase manual verification deliverables.
- `docs/DDD_GUIDE.md` defines crate boundaries and domain ownership rules.
- `CONTRIBUTING.md` defines branch, commit, PR, testing, and quality practices.
- `AGENTS.md` is the short operating guide for coding agents.

## Commands

Use `make` for routine work:

```sh
make help
make c
```

Install optional local quality tools:

```sh
make install-tools
```

The full quality gate runs Rust formatting, TOML formatting when `taplo` is installed, `cargo check`, strict `clippy`, tests, dependency/security checks when `cargo-deny` is installed, dead dependency checks when `cargo-machete` is installed, and Biome only when a web app is introduced.
