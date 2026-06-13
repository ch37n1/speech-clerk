# macOS Agent Testing

This document defines the repeatable manual-testing loop for AI agents working
on the macOS app. The goal is the native-app equivalent of a Playwright smoke
test: launch the real app, inspect the visible UI, capture evidence, and keep
the limits of OS permissions explicit.

## Testing Levels

Use the cheapest level that proves the change.

| Level | Command | What it proves |
| --- | --- | --- |
| L0 deterministic gate | `make c` | Rust, Swift formatting, linting where installed, Rust tests, Swift build, and Swift test harness pass. |
| L1 macOS build | `make macos-agent-build` | Rust FFI and the macOS Swift package compile for the local machine. |
| L2 visible app smoke | `make macos-agent-smoke` | The real macOS app launches, exposes a window to the OS accessibility tree, and a screenshot is captured. |
| L3 manual workflow | Follow the phase checklist in `apps/macos/README.md` | Microphone permission, paste control, active-app insertion, and replacement-rule behavior work in a real text field. |

L0 is required after code changes. L2 is required after changes to SwiftUI,
permissions, app launch, model selection, or insertion behavior unless the
environment blocks GUI automation. L3 is required when a change affects
microphone capture, Accessibility trust, clipboard paste, focus restoration, or
the end-to-end visible workflow.

## Agent Commands

The Makefile wraps `tools/macos-agent.sh`.

```sh
make macos-agent-build
make macos-agent-launch
make macos-agent-smoke
make macos-agent-screenshot
make macos-agent-stop
```

Generated evidence goes under:

```text
.build/agent/macos/
```

Important files:

- `SpeechClerkMac.log`: stdout/stderr from the launched app.
- `SpeechClerkMac.pid`: process id for the launched SwiftPM run.
- `SpeechClerkMac.png`: screenshot captured during smoke testing.

These files are build artifacts and stay ignored by git through `.build/`.

## Permissions

macOS GUI automation is permission-gated. Agents must not silently bypass or
grant these permissions.

Required for L2 inspection:

- The terminal or host app running the agent may need Accessibility permission
  so `System Events` can inspect the app window.
- The terminal or host app may need Screen Recording permission for reliable
  screenshots.

Required for L3 workflow:

- Speech Clerk needs microphone permission.
- Speech Clerk needs Accessibility trust for the V1 clipboard paste flow.
- A real target text field must be opened in another app before stopping
  dictation.

If permission is missing, record the blocked permission and provide the evidence
that was still collected. Do not mark a GUI workflow verified from build output
alone.

## Stable UI Hooks

SwiftUI controls that agents or future UI tests need to find should have stable
accessibility identifiers. Use lower-kebab-case names.

Current identifiers:

- `app-status`
- `model-picker`
- `load-model-button`
- `microphone-permission-button`
- `microphone-permission-status`
- `paste-permission-button`
- `paste-permission-status`
- `replacement-pattern-field`
- `replacement-value-field`
- `apply-replacement-button`
- `record-toggle-button`
- `cancel-recording-button`
- `last-transcript`

Changing visible copy should not require changing these identifiers.

## L2 Smoke Contract

`make macos-agent-smoke` should:

1. Build Rust FFI and the macOS Swift package.
2. Launch `SpeechClerkMac` through SwiftPM.
3. Wait for a visible app window.
4. Print discovered button names from the accessibility tree.
5. Capture a screenshot to `.build/agent/macos/SpeechClerkMac.png`.

This is not a substitute for L3. It proves that the agent can see and inspect
the native app, not that microphone capture or paste insertion succeeded.

## L3 Manual Workflow Evidence

For phase-one macOS changes, report:

- `make c` result.
- `make macos-agent-smoke` result, screenshot path, and any permission limits.
- Whether microphone permission was allowed or blocked.
- Whether paste control was allowed or blocked.
- The target app used for insertion.
- The inserted transcript text or the observed failure state.
- Whether changing the replacement fields changed the inserted fake transcript.

Use exact status labels from the app when reporting failures, for example
`Mic blocked`, `Paste blocked`, `Inserted`, or `No audio`.

## Future Improvements

The current app is a SwiftPM executable, so the first native testing layer uses
Makefile targets, AppleScript/System Events, screenshots, and manual evidence.
When the project grows a packaged `.app` or Xcode project, add XCUITest around
the same accessibility identifiers.

Before real ASR work expands UI testing, add an agent-only deterministic audio
injection mode so agents can exercise start/stop/transcript behavior without
using the physical microphone.
