#!/bin/sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
STATE_DIR="$ROOT_DIR/.build/e2e/macos"
LOG_FILE="$STATE_DIR/SpeechClerkMac.log"
PID_FILE="$STATE_DIR/SpeechClerkMac.pid"
SCREENSHOT_FILE="${SCREENSHOT_FILE:-$STATE_DIR/SpeechClerkMac.png}"

CARGO="${CARGO:-cargo}"
SWIFT="${SWIFT:-swift}"
SWIFT_PACKAGE_PATH="${SWIFT_PACKAGE_PATH:-apps/macos}"
SWIFT_SCRATCH_PATH="${SWIFT_SCRATCH_PATH:-$ROOT_DIR/.build/swiftpm/macos}"
SWIFT_MODULE_CACHE_PATH="${SWIFT_MODULE_CACHE_PATH:-$ROOT_DIR/.build/swiftpm/clang-module-cache}"

usage() {
    cat <<'EOF'
Usage: tools/macos-e2e.sh <command>

Commands:
  build       Build Rust FFI and the macOS Swift package
  launch      Build and launch SpeechClerkMac in the background
  wait-window Wait until the app exposes a visible window
  inspect     Print basic UI elements through System Events
  screenshot  Capture a screenshot into .build/e2e/macos
  smoke       Launch, wait for a window, inspect, and screenshot
  stop        Stop the launched app process
EOF
}

ensure_state_dir() {
    mkdir -p "$STATE_DIR"
}

build_app() {
    cd "$ROOT_DIR"
    "$CARGO" build -p ffi
    CLANG_MODULE_CACHE_PATH="$SWIFT_MODULE_CACHE_PATH" \
        "$SWIFT" build \
        --package-path "$SWIFT_PACKAGE_PATH" \
        --scratch-path "$SWIFT_SCRATCH_PATH"
}

is_running() {
    test -f "$PID_FILE" && kill -0 "$(cat "$PID_FILE")" 2>/dev/null
}

launch_app() {
    ensure_state_dir
    if is_running; then
        echo "SpeechClerkMac already launched with pid $(cat "$PID_FILE")"
        return
    fi

    build_app
    cd "$ROOT_DIR"
    : >"$LOG_FILE"
    (
        CLANG_MODULE_CACHE_PATH="$SWIFT_MODULE_CACHE_PATH" \
            "$SWIFT" run \
            --package-path "$SWIFT_PACKAGE_PATH" \
            --scratch-path "$SWIFT_SCRATCH_PATH" \
            SpeechClerkMac
    ) >>"$LOG_FILE" 2>&1 &
    echo "$!" >"$PID_FILE"
    echo "Launched SpeechClerkMac with pid $(cat "$PID_FILE")"
    echo "Log: $LOG_FILE"
}

wait_for_window() {
    osascript <<'APPLESCRIPT'
tell application "System Events"
    repeat 40 times
        if exists process "SpeechClerkMac" then
            tell process "SpeechClerkMac"
                if (count of windows) > 0 then
                    return "SpeechClerkMac window ready"
                end if
            end tell
        end if
        delay 0.25
    end repeat
end tell
error "SpeechClerkMac window did not appear"
APPLESCRIPT
}

inspect_window() {
    osascript <<'APPLESCRIPT'
tell application "System Events"
    if not (exists process "SpeechClerkMac") then
        error "SpeechClerkMac process is not running"
    end if

    tell process "SpeechClerkMac"
        set frontmost to true
        if (count of windows) is 0 then
            error "SpeechClerkMac has no visible windows"
        end if

        tell window 1
            set buttonNames to {}
            repeat with candidate in buttons
                set end of buttonNames to name of candidate
            end repeat

            set staticTextValues to {}
            repeat with candidate in static texts
                set end of staticTextValues to value of candidate
            end repeat

            return "buttons=" & buttonNames & linefeed & "static_text=" & staticTextValues
        end tell
    end tell
end tell
APPLESCRIPT
}

capture_screenshot() {
    ensure_state_dir
    if screencapture -x "$SCREENSHOT_FILE" && test -s "$SCREENSHOT_FILE"; then
        echo "Screenshot: $SCREENSHOT_FILE"
    else
        rm -f "$SCREENSHOT_FILE"
        echo "Screenshot capture failed: $SCREENSHOT_FILE" >&2
        return 1
    fi
}

smoke_app() {
    launch_app

    if wait_for_window; then
        :
    else
        status=$?
        echo "Window inspection failed. macOS may require Accessibility permission for the host terminal or test runner." >&2
        capture_screenshot || echo "Screenshot capture also failed. macOS may require Screen Recording permission." >&2
        exit "$status"
    fi

    if inspect_window; then
        :
    else
        status=$?
        echo "UI inspection failed. macOS may require Accessibility permission for the host terminal or test runner." >&2
        capture_screenshot || echo "Screenshot capture also failed. macOS may require Screen Recording permission." >&2
        exit "$status"
    fi

    capture_screenshot
}

stop_app() {
    if is_running; then
        kill "$(cat "$PID_FILE")" 2>/dev/null || true
        rm -f "$PID_FILE"
        echo "Stopped SpeechClerkMac launcher"
    else
        rm -f "$PID_FILE"
        osascript -e 'tell application "SpeechClerkMac" to quit' >/dev/null 2>&1 || true
        echo "No SpeechClerkMac launcher pid found"
    fi
}

case "${1:-}" in
    build)
        build_app
        ;;
    launch)
        launch_app
        ;;
    wait-window)
        wait_for_window
        ;;
    inspect)
        inspect_window
        ;;
    screenshot)
        capture_screenshot
        ;;
    smoke)
        smoke_app
        ;;
    stop)
        stop_app
        ;;
    -h|--help|help|"")
        usage
        ;;
    *)
        usage
        exit 2
        ;;
esac
