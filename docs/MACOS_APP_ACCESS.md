# macOS App Access

This document defines how an AI agent or developer can inspect and operate the
native macOS app during implementation. It is the app-access layer, not the
final verification layer. For final smoke evidence, use
`docs/MACOS_E2E_TESTING.md`.

The web equivalent would be Playwright's page object and locator access. For
this native app, the first layer is macOS Accessibility plus stable SwiftUI
accessibility identifiers.

## Standard Flow

Use this flow for user-visible macOS work:

1. Build or launch the app.
2. Use the UI access tool to inspect the accessibility tree.
3. Interact with stable accessibility identifiers while developing.
4. Run `make c`.
5. Run `make macos-e2e-smoke` as final visible-app evidence.
6. If macOS permissions block UI access or smoke, report the exact blocker and
   any evidence captured under `.build/ui/macos/` or `.build/e2e/macos/`.

The UI access tool is for exploration and targeted interaction. The e2e smoke
target is the final handoff check.

## Commands

Build the UI access tool:

```sh
make macos-ui-build
```

Run arbitrary UI commands with `CALL_ARGS`:

```sh
make macos-ui CALL_ARGS="permissions"
make macos-ui CALL_ARGS="prompt-permissions"
make macos-ui CALL_ARGS="launch"
make macos-ui CALL_ARGS="tree --max-depth 8"
make macos-ui CALL_ARGS="find record-toggle-button"
make macos-ui CALL_ARGS="press load-model-button"
make macos-ui CALL_ARGS="set-text replacement-pattern-field parakeet"
make macos-ui CALL_ARGS="set-text replacement-value-field Canary"
make macos-ui CALL_ARGS="press apply-replacement-button"
make macos-ui CALL_ARGS="value app-status"
make macos-ui CALL_ARGS="screenshot"
make macos-ui CALL_ARGS="stop"
```

The Make target wraps:

```sh
tools/macos-ui.sh <command> [args...]
```

The wrapper builds/runs the SwiftPM executable `SpeechClerkMacUITool`, which
uses macOS Accessibility APIs to inspect and manipulate `SpeechClerkMac`.

## Permissions

macOS Accessibility permission is required for `tree`, `find`, `press`,
`set-text`, and `value`. Screen Recording permission may be required for
screenshots.

Use:

```sh
make macos-ui CALL_ARGS="permissions"
make macos-ui CALL_ARGS="prompt-permissions"
```

Do not treat a permission prompt or permission denial as test success. Report it
as a blocked UI-access step and continue with deterministic checks where
possible.

## Locator Contract

Prefer stable accessibility identifiers over visible text. Visible text can
change; identifiers should only change when the app contract changes.

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

When adding a user-visible control that an agent may need to inspect or operate,
add a lower-kebab-case accessibility identifier in the SwiftUI view and update
this list.

## Access Tool Scope

The tool intentionally covers generic UI access primitives:

- print the accessibility tree
- find an element
- press a button/control
- set text
- read a value
- capture a screenshot

It should not duplicate product logic or fake dictation behavior. Product
behavior stays in Rust. If a workflow cannot be tested without physical
microphone input, add a deterministic app/test mode through the product boundary
rather than hard-coding behavior in the UI tool.

## Relationship To E2E

Use `make macos-ui ...` while developing or debugging a visible app change.
Use `make macos-e2e-smoke` before handoff to prove the app can be launched and
inspected as a real native process.

If `make macos-e2e-smoke` fails because Accessibility or Screen Recording is
blocked, include:

- the command result
- the blocked permission
- the screenshot path if one was created
- any useful `make macos-ui CALL_ARGS="tree --max-depth N"` output if available
