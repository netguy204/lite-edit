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

# Copy Info.plist with version from Cargo.toml
echo "  Copying Info.plist..."

# Extract version from Cargo.toml (prefer workspace.package.version)
VERSION=$(grep -A 5 "workspace.package" "$PROJECT_ROOT/Cargo.toml" | grep "version" | sed 's/.*"\(.*\)".*/\1/')

# Fallback to root package version if workspace.package not found
if [[ -z "$VERSION" ]]; then
    VERSION=$(grep "^version = " "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
fi

# Default to 0.0.0 if still empty
if [[ -z "$VERSION" ]]; then
    VERSION="0.0.0"
    echo "  Warning: Could not extract version, using default: $VERSION"
else
    echo "  Using version from Cargo.toml: $VERSION"
fi

# Update version in Info.plist for CFBundleVersion and CFBundleShortVersionString
while IFS= read -r line; do
    if echo "$line" | grep -q "CFBundleVersion</key>"; then
        echo "$line"
    elif echo "$line" | grep -q "CFBundleShortVersionString</key>"; then
        echo "$line"
    elif echo "$line" | grep -Eq "^[[:space:]]*<string>[0-9]+\.[0-9]+\.[0-9]+</string>$"; then
        echo "    <string>$VERSION</string>"
    else
        echo "$line"
    fi
done < "$INFO_PLIST" > "$CONTENTS_DIR/Info.plist"

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
