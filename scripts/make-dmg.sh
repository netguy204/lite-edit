#!/bin/bash
# Chunk: docs/chunks/macos_app_bundle - macOS app bundle packaging
#
# Create a DMG disk image for distribution.
# The DMG contains the app and a symlink to /Applications for drag-to-install.
#
# Prerequisites:
#   - App must be built, signed, and notarized
#   - APPLE_DEVELOPER_IDENTITY environment variable (for signing the DMG)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

APP_DIR="${PROJECT_ROOT}/target/bundle/LiteEdit.app"
DMG_DIR="${PROJECT_ROOT}/target/bundle/dmg-contents"
DMG_FILE="${PROJECT_ROOT}/target/bundle/LiteEdit.dmg"
VOLUME_NAME="LiteEdit"

# Verify the app bundle exists
if [[ ! -d "$APP_DIR" ]]; then
    echo "Error: App bundle not found at $APP_DIR"
    echo "Run 'make bundle' (or 'make notarize' for signed builds) first."
    exit 1
fi

echo "Creating DMG disk image..."

# Clean up any previous DMG staging
rm -rf "$DMG_DIR"
rm -f "$DMG_FILE"

# Create staging directory with app and Applications symlink
mkdir -p "$DMG_DIR"
cp -R "$APP_DIR" "$DMG_DIR/"
ln -s /Applications "$DMG_DIR/Applications"

echo "  Contents: LiteEdit.app, Applications symlink"

# Create the DMG
# -volname: Name shown when mounted
# -srcfolder: Source directory
# -ov: Overwrite existing
# -format UDZO: Compressed, read-only
hdiutil create \
    -volname "$VOLUME_NAME" \
    -srcfolder "$DMG_DIR" \
    -ov \
    -format UDZO \
    "$DMG_FILE"

# Clean up staging directory
rm -rf "$DMG_DIR"

# Sign the DMG if identity is available
if [[ -n "${APPLE_DEVELOPER_IDENTITY:-}" ]]; then
    echo ""
    echo "Signing DMG..."
    codesign --sign "$APPLE_DEVELOPER_IDENTITY" --timestamp "$DMG_FILE"
    echo "  DMG signed with: $APPLE_DEVELOPER_IDENTITY"
else
    echo ""
    echo "Note: DMG not signed (APPLE_DEVELOPER_IDENTITY not set)."
    echo "Set this environment variable to sign the DMG for distribution."
fi

echo ""
echo "DMG created: $DMG_FILE"
echo ""

# Show file size
DMG_SIZE=$(du -h "$DMG_FILE" | cut -f1)
echo "Size: $DMG_SIZE"
