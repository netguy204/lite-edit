#!/bin/bash
# Chunk: docs/chunks/macos_app_bundle - macOS app bundle packaging
#
# Code sign the LiteEdit.app bundle with a Developer ID Application certificate.
# Enables hardened runtime (required for notarization).
#
# Prerequisites:
#   - Apple Developer ID Application certificate installed in Keychain
#   - APPLE_DEVELOPER_IDENTITY environment variable set
#
# Example:
#   export APPLE_DEVELOPER_IDENTITY="Developer ID Application: Your Name (TEAMID)"
#   ./scripts/codesign-app.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

APP_DIR="${PROJECT_ROOT}/target/bundle/LiteEdit.app"
ENTITLEMENTS="${PROJECT_ROOT}/resources/LiteEdit.entitlements"

# Check for required environment variable
if [[ -z "${APPLE_DEVELOPER_IDENTITY:-}" ]]; then
    echo "Error: APPLE_DEVELOPER_IDENTITY environment variable not set."
    echo ""
    echo "Set it to your Developer ID Application certificate identity:"
    echo '  export APPLE_DEVELOPER_IDENTITY="Developer ID Application: Your Name (TEAMID)"'
    echo ""
    echo "To find your identity, run:"
    echo "  security find-identity -v -p codesigning"
    exit 1
fi

# Verify the app bundle exists
if [[ ! -d "$APP_DIR" ]]; then
    echo "Error: App bundle not found at $APP_DIR"
    echo "Run 'make bundle' first."
    exit 1
fi

# Verify entitlements file exists
if [[ ! -f "$ENTITLEMENTS" ]]; then
    echo "Error: Entitlements file not found at $ENTITLEMENTS"
    exit 1
fi

echo "Code signing LiteEdit.app..."
echo "  Identity: $APPLE_DEVELOPER_IDENTITY"

# Sign the app with hardened runtime and entitlements
# --deep: Sign all nested code (frameworks, helpers)
# --force: Replace any existing signature
# --options runtime: Enable hardened runtime (required for notarization)
# --entitlements: Apply our entitlements file
# --timestamp: Include a secure timestamp (required for notarization)
codesign \
    --deep \
    --force \
    --options runtime \
    --entitlements "$ENTITLEMENTS" \
    --timestamp \
    --sign "$APPLE_DEVELOPER_IDENTITY" \
    "$APP_DIR"

echo ""
echo "Verifying signature..."

# Verify the signature
codesign --verify --deep --strict --verbose=2 "$APP_DIR"

echo ""
echo "Checking with spctl (Gatekeeper)..."

# Check with spctl (may fail before notarization, which is expected)
if spctl --assess --type exec --verbose "$APP_DIR" 2>&1; then
    echo "  App is accepted by Gatekeeper."
else
    echo "  Note: App may need notarization before Gatekeeper will accept it."
fi

echo ""
echo "Code signing complete: $APP_DIR"
