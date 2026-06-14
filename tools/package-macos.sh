#!/bin/sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
APP_NAME="${MACOS_APP_NAME:-Speech Clerk}"
PRODUCT_NAME="${MACOS_PRODUCT_NAME:-SpeechClerkMac}"
DIST_DIR="${DIST_DIR:-$ROOT_DIR/.build/dist}"
PACKAGE_DIR="${MACOS_PACKAGE_DIR:-$ROOT_DIR/.build/macos-package}"
APP_BUNDLE="$PACKAGE_DIR/$APP_NAME.app"
SWIFT_PACKAGE_PATH="${SWIFT_PACKAGE_PATH:-apps/macos}"
SWIFT_SCRATCH_PATH="${SWIFT_SCRATCH_PATH:-$ROOT_DIR/.build/swiftpm/macos}"
SWIFT_MODULE_CACHE_PATH="${SWIFT_MODULE_CACHE_PATH:-$ROOT_DIR/.build/swiftpm/clang-module-cache}"
RUST_TARGET_DIR="${RUST_TARGET_DIR:-$ROOT_DIR/target/release}"
CARGO="${CARGO:-cargo}"
SWIFT="${SWIFT:-swift}"

EXECUTABLE_PATH="$SWIFT_SCRATCH_PATH/release/$PRODUCT_NAME"
FFI_DYLIB="$RUST_TARGET_DIR/libspeech_clerk_ffi.dylib"
ARCHIVE_PATH="$DIST_DIR/SpeechClerk-macos.zip"

cd "$ROOT_DIR"

"$CARGO" build -p ffi --release

CLANG_MODULE_CACHE_PATH="$SWIFT_MODULE_CACHE_PATH" \
    SPEECH_CLERK_RUST_TARGET_DIR="$RUST_TARGET_DIR" \
    SPEECH_CLERK_FFI_RPATH="@executable_path/../Frameworks" \
    "$SWIFT" build \
    --configuration release \
    --package-path "$SWIFT_PACKAGE_PATH" \
    --scratch-path "$SWIFT_SCRATCH_PATH" \
    --product "$PRODUCT_NAME"

test -x "$EXECUTABLE_PATH"
test -f "$FFI_DYLIB"

rm -rf "$PACKAGE_DIR"
mkdir -p \
    "$APP_BUNDLE/Contents/MacOS" \
    "$APP_BUNDLE/Contents/Frameworks" \
    "$APP_BUNDLE/Contents/Resources" \
    "$DIST_DIR"

cp apps/macos/Info.plist "$APP_BUNDLE/Contents/Info.plist"
cp "$EXECUTABLE_PATH" "$APP_BUNDLE/Contents/MacOS/$PRODUCT_NAME"
cp "$FFI_DYLIB" "$APP_BUNDLE/Contents/Frameworks/libspeech_clerk_ffi.dylib"

if [ -d apps/macos/Sources/SpeechClerkMacSupport/Resources ]; then
    cp -R apps/macos/Sources/SpeechClerkMacSupport/Resources/. \
        "$APP_BUNDLE/Contents/Resources/"
fi

if command -v install_name_tool >/dev/null 2>&1; then
    install_name_tool \
        -id "@rpath/libspeech_clerk_ffi.dylib" \
        "$APP_BUNDLE/Contents/Frameworks/libspeech_clerk_ffi.dylib"

    for dependency in $(otool -L "$APP_BUNDLE/Contents/MacOS/$PRODUCT_NAME" \
        | sed -n 's/^[[:space:]]*\([^[:space:]]*libspeech_clerk_ffi\.dylib\).*/\1/p'); do
        install_name_tool \
            -change "$dependency" "@rpath/libspeech_clerk_ffi.dylib" \
            "$APP_BUNDLE/Contents/MacOS/$PRODUCT_NAME"
    done
fi

rm -f "$ARCHIVE_PATH"
if command -v ditto >/dev/null 2>&1; then
    (cd "$PACKAGE_DIR" && ditto -c -k --keepParent "$APP_NAME.app" "$ARCHIVE_PATH")
else
    (cd "$PACKAGE_DIR" && zip -qry "$ARCHIVE_PATH" "$APP_NAME.app")
fi

test -s "$ARCHIVE_PATH"
echo "$ARCHIVE_PATH"
