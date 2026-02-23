#!/bin/bash
# Chunk: docs/chunks/macos_app_bundle - macOS app bundle packaging
#
# Generate macOS .icns file from source icon.png
# Uses built-in macOS tools: sips and iconutil

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

SOURCE_ICON="${PROJECT_ROOT}/icon.png"
ICONSET_DIR="${PROJECT_ROOT}/target/LiteEdit.iconset"
OUTPUT_ICNS="${PROJECT_ROOT}/target/LiteEdit.icns"

# Required icon sizes for macOS (1x and 2x variants)
# See: https://developer.apple.com/design/human-interface-guidelines/app-icons
SIZES=(16 32 64 128 256 512 1024)

if [[ ! -f "$SOURCE_ICON" ]]; then
    echo "Error: Source icon not found at $SOURCE_ICON"
    echo "Please provide a 1024x1024 PNG icon at the project root."
    exit 1
fi

# Verify source icon is at least 1024x1024
SOURCE_WIDTH=$(sips -g pixelWidth "$SOURCE_ICON" | tail -n1 | awk '{print $2}')
SOURCE_HEIGHT=$(sips -g pixelHeight "$SOURCE_ICON" | tail -n1 | awk '{print $2}')

if [[ "$SOURCE_WIDTH" -lt 1024 || "$SOURCE_HEIGHT" -lt 1024 ]]; then
    echo "Warning: Source icon is ${SOURCE_WIDTH}x${SOURCE_HEIGHT}."
    echo "For best results, use a 1024x1024 or larger image."
fi

# Clean and create iconset directory
rm -rf "$ICONSET_DIR"
mkdir -p "$ICONSET_DIR"
mkdir -p "$(dirname "$OUTPUT_ICNS")"

echo "Generating icon sizes from $SOURCE_ICON..."

# Generate each size variant
for size in "${SIZES[@]}"; do
    # 1x variant
    output_file="${ICONSET_DIR}/icon_${size}x${size}.png"
    sips -z "$size" "$size" "$SOURCE_ICON" --out "$output_file" > /dev/null
    echo "  Generated: icon_${size}x${size}.png"

    # 2x variant (for Retina displays)
    # The @2x version is named as the display size, but contains 2x pixels
    # e.g., icon_16x16@2x.png is 32x32 pixels shown at 16x16 on Retina
    if [[ "$size" -lt 1024 ]]; then
        double_size=$((size * 2))
        output_file="${ICONSET_DIR}/icon_${size}x${size}@2x.png"
        sips -z "$double_size" "$double_size" "$SOURCE_ICON" --out "$output_file" > /dev/null
        echo "  Generated: icon_${size}x${size}@2x.png"
    fi
done

# Special case: 512x512@2x is the same as 1024x1024
cp "${ICONSET_DIR}/icon_1024x1024.png" "${ICONSET_DIR}/icon_512x512@2x.png"
echo "  Generated: icon_512x512@2x.png (copy of 1024x1024)"

# Compile iconset into .icns
echo "Compiling iconset into .icns..."
iconutil -c icns "$ICONSET_DIR" -o "$OUTPUT_ICNS"

# Clean up intermediate iconset
rm -rf "$ICONSET_DIR"

echo "Done: $OUTPUT_ICNS"
