---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- Makefile
- resources/Info.plist
- resources/LiteEdit.entitlements
- scripts/make-icns.sh
- scripts/bundle-app.sh
- scripts/codesign-app.sh
- scripts/notarize-app.sh
- scripts/make-dmg.sh
code_references:
  - ref: Makefile
    implements: "Build workflow orchestration with targets for build, icns, bundle, sign, notarize, and dmg"
  - ref: resources/Info.plist
    implements: "Bundle metadata including identifier, version, icon reference, document types, and minimum system requirements"
  - ref: resources/LiteEdit.entitlements
    implements: "Code signing entitlements with hardened runtime settings (non-sandboxed for PTY access)"
  - ref: scripts/make-icns.sh
    implements: "Icon generation from icon.png to .icns with all required macOS sizes using sips and iconutil"
  - ref: scripts/bundle-app.sh
    implements: "App bundle assembly creating Contents/MacOS, Contents/Resources, Info.plist, and PkgInfo"
  - ref: scripts/codesign-app.sh
    implements: "Code signing with Developer ID certificate, hardened runtime, and signature verification"
  - ref: scripts/notarize-app.sh
    implements: "Apple notarization submission via notarytool and ticket stapling"
  - ref: scripts/make-dmg.sh
    implements: "DMG disk image creation with Applications symlink for drag-to-install"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- tiling_tab_movement
- tiling_tree_model
---

# Chunk Goal

## Minor Goal

Package lite-edit as a proper native macOS application bundle (`.app`), code-signed with the developer's Apple signing keys. This advances the project goal of shipping a native macOS editor with minimal footprint and fast startup by making it distributable as a standard macOS application.

The chunk should:

- Create a macOS `.app` bundle with proper `Info.plist`, bundle identifier, and the app icon from `icon.png` (generating the required `.icns` with all icon sizes)
- Set up code signing using the developer's Apple Developer ID certificate
- Add notarization support so the app can be distributed outside the Mac App Store without Gatekeeper warnings
- Integrate the packaging into the Cargo/build workflow (e.g., a `make bundle` or build script)
- Produce a `.dmg` disk image for distribution

## Success Criteria

- `make bundle` (or equivalent) produces a valid `LiteEdit.app` bundle in a `target/` or `dist/` directory
- The `.app` bundle contains the correct `Info.plist` with bundle identifier, version, and icon reference
- The app icon from `icon.png` is converted to a proper `.icns` file with standard macOS icon sizes (16x16 through 1024x1024)
- The binary inside the bundle is the release-mode Rust binary
- The app is code-signed with a Developer ID Application certificate (`codesign --verify` passes)
- The app is notarized with Apple (`spctl --assess --type exec` passes)
- A `.dmg` disk image is produced containing the `.app`
- The app launches correctly from the `.app` bundle (not just the raw binary)
- Double-clicking the `.app` in Finder opens lite-edit