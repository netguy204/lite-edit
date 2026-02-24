#!/bin/bash
# Chunk: docs/chunks/macos_app_bundle - macOS app bundle packaging
#
# Assemble the LiteEdit.app bundle structure
# This script creates the standard macOS .app bundle layout

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Paths
BINARY="${PROJECT_ROOT}/target/release/lite-edit"
ICNS="${PROJECT_ROOT}/target/LiteEdit.icns"
INFO_PLIST="${PROJECT_ROOT}/resources/Info.plist"

BUNDLE_DIR="${PROJECT_ROOT}/target/bundle"
APP_DIR="${BUNDLE_DIR}/LiteEdit.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

# Verify prerequisites exist
if [[ ! -f "$BINARY" ]]; then
    echo "Error: Release binary not found at $BINARY"
    echo "Run 'cargo build --release' first."
    exit 1
fi

if [[ ! -f "$ICNS" ]]; then
    echo "Error: Icon not found at $ICNS"
    echo "Run 'scripts/make-icns.sh' first."
    exit 1
fi

if [[ ! -f "$INFO_PLIST" ]]; then
    echo "Error: Info.plist not found at $INFO_PLIST"
    exit 1
fi

echo "Assembling LiteEdit.app..."

# Clean and create bundle directory structure
rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR"
mkdir -p "$RESOURCES_DIR"

# Copy the binary
echo "  Copying binary..."
cp "$BINARY" "$MACOS_DIR/lite-edit"
chmod +x "$MACOS_DIR/lite-edit"

# Copy the icon
echo "  Copying icon..."
cp "$ICNS" "$RESOURCES_DIR/LiteEdit.icns"

# Copy the bundled font and its license
echo "  Copying font..."
cp "${PROJECT_ROOT}/resources/IntelOneMono-Regular.ttf" "$RESOURCES_DIR/IntelOneMono-Regular.ttf"
cp "${PROJECT_ROOT}/resources/OFL.txt" "$RESOURCES_DIR/OFL.txt"

# Copy Info.plist
echo "  Copying Info.plist..."
cp "$INFO_PLIST" "$CONTENTS_DIR/Info.plist"

# Create PkgInfo (standard macOS bundle file)
echo "  Creating PkgInfo..."
echo -n "APPL????" > "$CONTENTS_DIR/PkgInfo"

# Set proper permissions
chmod -R 755 "$APP_DIR"

echo ""
echo "Bundle created: $APP_DIR"
echo ""
echo "Bundle contents:"
find "$APP_DIR" -type f | sed 's|'"$APP_DIR"'|  LiteEdit.app|'
