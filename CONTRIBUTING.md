# Contributing Guide

## Branch Strategy

Use GitFlow-lite: `main` is the protected branch, and all work happens in short-lived branches that merge by PR.

Branch names use `<type>/<short-description>`:

| Prefix | Use case | Example |
| --- | --- | --- |
| `feat/` | New features | `feat/macos-fake-dictation` |
| `fix/` | Bug fixes | `fix/model-manifest-checksum` |
| `chore/` | Tooling and maintenance | `chore/add-cargo-deny` |
| `docs/` | Documentation-only work | `docs/phase-one-scope` |
| `refactor/` | Structure changes without behavior changes | `refactor/asr-trait-errors` |
| `test/` | Test additions or changes | `test/postprocess-rules` |
| `experiment/` | Spikes that should not merge to `main` | `experiment/parakeet-onnx-bench` |

Use lowercase kebab-case. Include a ticket ID when one exists.

## Commit Messages

Use Conventional Commits:

```text
<type>(<scope>): <short summary>
```

Examples:

```text
feat(core): add dictation session state
fix(model-pack): reject missing tokenizer asset
docs(architecture): clarify android ime boundary
test(postprocess): cover whitespace cleanup
```

Types: `feat`, `fix`, `docs`, `test`, `refactor`, `chore`, `perf`, `ci`.

## Pull Requests

- Title follows Conventional Commits and becomes the squash commit message.
- Description explains what changed and why.
- Link related issues or design docs when available.
- `make c` must pass before review.
- Public API or FFI changes must include tests and documentation updates.

## Quality Gate

Run the full local gate:

```sh
make c
```

The gate is intentionally strict:

- Rust formatting with `rustfmt`
- TOML formatting with `taplo` when available
- Linting with `clippy -D warnings`
- Tests with `cargo test`
- Dependency/security checks with `cargo-deny` when available
- Dead-code dependency checks with `cargo-machete` when available

Install optional tools with:

```sh
make install-tools
```

## Architecture Rules

Follow `docs/DDD_GUIDE.md`.

- Organize by product/domain responsibility, not by generic technical buckets.
- Keep platform apps thin. Swift and Kotlin handle platform capture, permissions, and text insertion only.
- Keep dictation state, chunking, model-pack validation, ASR orchestration, post-processing, and language selection in Rust.
- Keep concrete ONNX Runtime code behind `asr-api`.
- Do not introduce a second ASR backend before the ONNX path is working and benchmarked.
- Do not create broad `utils`, `common`, or `types` modules for domain concepts.

## Testing

- Add focused unit tests for new Rust behavior.
- Add integration tests when behavior crosses crate boundaries.
- Add manual verification notes to the relevant roadmap phase when work changes a visible app workflow.
- Keep platform-specific code covered at the thinnest practical boundary.

## Dependencies

- Prefer small, well-maintained dependencies.
- Keep dependency additions scoped to the owning crate.
- Run `make deny` after dependency changes when `cargo-deny` is installed.
- Run `make machete` after removing or reshaping code.
