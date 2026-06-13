#!/bin/sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
STATE_DIR="$ROOT_DIR/.build/ui/macos"
SCREENSHOT_FILE="${SCREENSHOT_FILE:-$STATE_DIR/SpeechClerkMac.png}"

SWIFT="${SWIFT:-swift}"
SWIFT_PACKAGE_PATH="${SWIFT_PACKAGE_PATH:-apps/macos}"
SWIFT_SCRATCH_PATH="${SWIFT_SCRATCH_PATH:-$ROOT_DIR/.build/swiftpm/macos}"
SWIFT_MODULE_CACHE_PATH="${SWIFT_MODULE_CACHE_PATH:-$ROOT_DIR/.build/swiftpm/clang-module-cache}"

usage() {
    cat <<'EOF'
Usage: tools/macos-ui.sh <command> [args...]

Commands:
  build                         Build the macOS UI access tool
  launch                        Launch SpeechClerkMac through the e2e launcher
  permissions                   Print Accessibility trust status
  prompt-permissions            Prompt for Accessibility trust when macOS allows it
  tree [--max-depth N]          Print the app accessibility tree
  find <identifier-or-title>    Print one matching accessibility element
  press <identifier-or-title>   Press a matching accessibility element
  set-text <identifier> <text>  Set the AXValue of a matching text element
  value <identifier-or-title>   Print the AXValue of a matching element
  screenshot [path]             Capture a screen image
  stop                          Stop the e2e-launched app
EOF
}

build_tool() {
    cd "$ROOT_DIR"
    CLANG_MODULE_CACHE_PATH="$SWIFT_MODULE_CACHE_PATH" \
        "$SWIFT" build \
        --package-path "$SWIFT_PACKAGE_PATH" \
        --scratch-path "$SWIFT_SCRATCH_PATH" \
        --product SpeechClerkMacUITool
}

run_tool() {
    cd "$ROOT_DIR"
    CLANG_MODULE_CACHE_PATH="$SWIFT_MODULE_CACHE_PATH" \
        "$SWIFT" run \
        --package-path "$SWIFT_PACKAGE_PATH" \
        --scratch-path "$SWIFT_SCRATCH_PATH" \
        SpeechClerkMacUITool "$@"
}

capture_screenshot() {
    mkdir -p "$STATE_DIR"
    target="${1:-$SCREENSHOT_FILE}"
    if screencapture -x "$target" && test -s "$target"; then
        echo "Screenshot: $target"
    else
        rm -f "$target"
        echo "Screenshot capture failed: $target" >&2
        return 1
    fi
}

command="${1:-}"
case "$command" in
    build)
        build_tool
        ;;
    launch)
        sh "$ROOT_DIR/tools/macos-e2e.sh" launch
        ;;
    permissions|prompt-permissions|tree|find|press|set-text|value)
        shift
        run_tool "$command" "$@"
        ;;
    screenshot)
        shift
        capture_screenshot "$@"
        ;;
    stop)
        sh "$ROOT_DIR/tools/macos-e2e.sh" stop
        ;;
    -h|--help|help|"")
        usage
        ;;
    *)
        usage
        exit 2
        ;;
esac
