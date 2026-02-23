# Chunk: docs/chunks/macos_app_bundle - macOS app bundle packaging
#
# Makefile for building and packaging LiteEdit
#
# Targets:
#   make build    - Build release binary
#   make icns     - Generate .icns icon from icon.png
#   make bundle   - Build + create app bundle (for local testing)
#   make sign     - Code sign the app bundle
#   make notarize - Notarize with Apple
#   make dmg      - Create DMG disk image
#   make clean    - Remove build artifacts
#
# For distribution, run: make dmg
# This runs the full pipeline: build → icns → bundle → sign → notarize → dmg
#
# Environment variables for signing/notarization:
#   APPLE_DEVELOPER_IDENTITY - Developer ID Application certificate identity
#   APPLE_ID                 - Apple ID email for notarization
#   APPLE_TEAM_ID            - Apple Developer Team ID
#   APPLE_APP_PASSWORD       - App-specific password for notarization

.PHONY: build icns bundle sign notarize dmg clean help

# Default target
help:
	@echo "LiteEdit Build System"
	@echo ""
	@echo "Development:"
	@echo "  make build   - Build release binary"
	@echo "  make bundle  - Create app bundle (unsigned, for local testing)"
	@echo ""
	@echo "Distribution:"
	@echo "  make sign      - Sign the app bundle"
	@echo "  make notarize  - Notarize with Apple (requires signing)"
	@echo "  make dmg       - Create DMG disk image (full pipeline)"
	@echo ""
	@echo "Other:"
	@echo "  make icns    - Generate .icns from icon.png"
	@echo "  make clean   - Remove build artifacts"
	@echo ""
	@echo "Environment variables for distribution:"
	@echo "  APPLE_DEVELOPER_IDENTITY - Code signing identity"
	@echo "  APPLE_ID                 - Apple ID for notarization"
	@echo "  APPLE_TEAM_ID            - Team ID for notarization"
	@echo "  APPLE_APP_PASSWORD       - App-specific password"

# Build the release binary
build:
	cargo build --release

# Generate .icns icon file
icns: icon.png
	./scripts/make-icns.sh

# Create the app bundle (unsigned, for local development/testing)
bundle: build icns
	./scripts/bundle-app.sh
	@echo ""
	@echo "App bundle created at: target/bundle/LiteEdit.app"
	@echo "Double-click in Finder to test, or run from terminal:"
	@echo "  open target/bundle/LiteEdit.app"

# Code sign the app bundle
sign: bundle
	./scripts/codesign-app.sh

# Notarize with Apple
notarize: sign
	./scripts/notarize-app.sh

# Create DMG disk image (runs full pipeline including notarization)
dmg: notarize
	./scripts/make-dmg.sh
	@echo ""
	@echo "Distribution DMG created at: target/bundle/LiteEdit.dmg"

# Create unsigned DMG (for testing DMG creation without credentials)
dmg-unsigned: bundle
	./scripts/make-dmg.sh
	@echo ""
	@echo "Unsigned DMG created at: target/bundle/LiteEdit.dmg"
	@echo "Note: This DMG is not notarized and may trigger Gatekeeper warnings."

# Clean build artifacts
clean:
	cargo clean
	rm -rf target/bundle
	rm -f target/LiteEdit.icns

# Run the app from bundle (for testing)
run-bundle: bundle
	open target/bundle/LiteEdit.app
