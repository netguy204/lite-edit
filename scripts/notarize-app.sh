#!/bin/bash
# Chunk: docs/chunks/macos_app_bundle - macOS app bundle packaging
#
# Submit LiteEdit.app for Apple notarization and staple the ticket.
# This allows the app to be distributed outside the Mac App Store
# without Gatekeeper warnings.
#
# Prerequisites:
#   - App must be code-signed with a Developer ID Application certificate
#   - Apple Developer account credentials in environment variables
#
# Required environment variables:
#   APPLE_ID          - Your Apple ID email address
#   APPLE_TEAM_ID     - Your Apple Developer Team ID (10-character string)
#   APPLE_APP_PASSWORD - App-specific password from appleid.apple.com
#
# To create an app-specific password:
#   1. Go to https://appleid.apple.com/account/manage
#   2. Sign in with your Apple ID
#   3. In "App-Specific Passwords", click "Generate Password"
#   4. Name it "LiteEdit Notarization" or similar
#
# Example:
#   export APPLE_ID="your@email.com"
#   export APPLE_TEAM_ID="ABCD123456"
#   export APPLE_APP_PASSWORD="xxxx-xxxx-xxxx-xxxx"
#   ./scripts/notarize-app.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

APP_DIR="${PROJECT_ROOT}/target/bundle/LiteEdit.app"
ZIP_FILE="${PROJECT_ROOT}/target/bundle/LiteEdit.zip"

# Check required environment variables
MISSING_VARS=()
[[ -z "${APPLE_ID:-}" ]] && MISSING_VARS+=("APPLE_ID")
[[ -z "${APPLE_TEAM_ID:-}" ]] && MISSING_VARS+=("APPLE_TEAM_ID")
[[ -z "${APPLE_APP_PASSWORD:-}" ]] && MISSING_VARS+=("APPLE_APP_PASSWORD")

if [[ ${#MISSING_VARS[@]} -gt 0 ]]; then
    echo "Error: Missing required environment variables:"
    for var in "${MISSING_VARS[@]}"; do
        echo "  - $var"
    done
    echo ""
    echo "Required variables:"
    echo "  APPLE_ID          - Your Apple ID email"
    echo "  APPLE_TEAM_ID     - Your 10-character Team ID"
    echo "  APPLE_APP_PASSWORD - App-specific password from appleid.apple.com"
    exit 1
fi

# Verify the app bundle exists
if [[ ! -d "$APP_DIR" ]]; then
    echo "Error: App bundle not found at $APP_DIR"
    echo "Run 'make sign' first."
    exit 1
fi

# Verify app is signed
if ! codesign --verify "$APP_DIR" 2>/dev/null; then
    echo "Error: App is not properly signed."
    echo "Run 'scripts/codesign-app.sh' first."
    exit 1
fi

echo "Preparing app for notarization..."

# Create a ZIP for submission
# Using ditto to preserve resource forks and metadata
echo "  Creating ZIP archive..."
rm -f "$ZIP_FILE"
ditto -c -k --keepParent "$APP_DIR" "$ZIP_FILE"

echo "  ZIP created: $ZIP_FILE"
echo ""
echo "Submitting to Apple for notarization..."
echo "  This may take 5-15 minutes..."
echo ""

# Submit for notarization using notarytool (macOS 12+)
# --wait: Block until notarization completes
xcrun notarytool submit "$ZIP_FILE" \
    --apple-id "$APPLE_ID" \
    --team-id "$APPLE_TEAM_ID" \
    --password "$APPLE_APP_PASSWORD" \
    --wait

echo ""
echo "Stapling notarization ticket to app..."

# Staple the ticket to the app
# This embeds the notarization ticket so the app works offline
xcrun stapler staple "$APP_DIR"

echo ""
echo "Verifying notarization..."

# Verify with spctl (Gatekeeper)
spctl --assess --type exec --verbose "$APP_DIR"

echo ""
echo "Notarization complete!"
echo ""
echo "The app is now ready for distribution: $APP_DIR"

# Clean up ZIP (optional, keep for reference)
# rm -f "$ZIP_FILE"
