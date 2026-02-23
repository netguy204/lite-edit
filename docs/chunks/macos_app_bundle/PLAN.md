# Implementation Plan

## Approach

Create a macOS application bundle (`.app`) with proper structure, code signing, notarization, and DMG packaging. The approach uses standard Apple tooling via shell scripts integrated into the build workflow:

1. **Bundle structure**: Standard macOS `.app` structure with `Contents/MacOS/` (binary), `Contents/Resources/` (icons), and `Contents/Info.plist` (metadata).

2. **Icon generation**: Use `sips` and `iconutil` (built-in macOS tools) to convert the source `icon.png` into a proper `.icns` file with all required sizes.

3. **Code signing**: Use `codesign` with a Developer ID Application certificate. The identity is read from environment variables to avoid hardcoding credentials.

4. **Notarization**: Use `notarytool` (replaces altool, available since macOS 12) to submit the app for notarization, then `stapler` to staple the ticket.

5. **DMG creation**: Use `hdiutil` to create a compressed disk image containing the app.

6. **Build integration**: A `Makefile` orchestrates the workflow, with a `make bundle` target that runs cargo build in release mode, then packages everything.

Per TESTING_PHILOSOPHY.md, the packaging scripts are "platform shell" code that isn't unit-tested. Verification is done by running the build and checking the outputs with `codesign --verify` and `spctl --assess`.

## Sequence

### Step 1: Create the source icon

Add a placeholder `icon.png` at the project root. This should be at least 1024x1024 pixels (the largest size in the macOS icon set). The actual icon design is out of scope for this chunk—we provide a working placeholder that can be replaced later.

Location: `icon.png` (project root)

Note: If the user provides their own icon.png, skip this step.

### Step 2: Create the icon generation script

Create a script that:
1. Takes `icon.png` as input
2. Creates an `iconset` directory with all required sizes (16, 32, 64, 128, 256, 512, 1024 at 1x and 2x)
3. Uses `sips` to resize the source image to each size
4. Uses `iconutil` to compile the iconset into `LiteEdit.icns`

Location: `scripts/make-icns.sh`

### Step 3: Create the Info.plist template

Create the Info.plist with:
- `CFBundleIdentifier`: `com.liteedit.app` (or similar)
- `CFBundleName`: `LiteEdit`
- `CFBundleDisplayName`: `LiteEdit`
- `CFBundleExecutable`: `lite-edit`
- `CFBundleIconFile`: `LiteEdit` (references `LiteEdit.icns` in Resources)
- `CFBundleVersion` and `CFBundleShortVersionString`: Read from `Cargo.toml` version (0.1.0)
- `LSMinimumSystemVersion`: `10.15` (Catalina—minimum for Metal 3 features)
- `NSHighResolutionCapable`: `true`
- `NSSupportsAutomaticTermination`: `false`
- `CFBundlePackageType`: `APPL`
- `NSPrincipalClass`: `NSApplication`

Location: `resources/Info.plist`

### Step 4: Create the entitlements file

Create an entitlements file for code signing with:
- `com.apple.security.app-sandbox`: `false` (not sandboxed—terminal emulator needs PTY access)
- `com.apple.security.cs.allow-unsigned-executable-memory`: `true` (if needed for JIT; may not be required)
- `com.apple.security.automation.apple-events`: `true` (for AppleScript automation if desired)
- `com.apple.security.device.audio-input`: `false`

Note: The PTY functionality requires hardened runtime but cannot be sandboxed. Disable sandbox for now. Document this constraint.

Location: `resources/LiteEdit.entitlements`

### Step 5: Create the bundle assembly script

Create a script that:
1. Creates the `.app` bundle directory structure under `target/bundle/LiteEdit.app/`
2. Copies `target/release/lite-edit` to `Contents/MacOS/lite-edit`
3. Copies `LiteEdit.icns` to `Contents/Resources/`
4. Copies `Info.plist` to `Contents/`
5. Creates `Contents/PkgInfo` with `APPL????`

Location: `scripts/bundle-app.sh`

### Step 6: Create the code signing script

Create a script that:
1. Reads `APPLE_DEVELOPER_IDENTITY` from environment (e.g., "Developer ID Application: Name (TEAMID)")
2. Uses `codesign --deep --force --options runtime --entitlements resources/LiteEdit.entitlements --sign "$APPLE_DEVELOPER_IDENTITY" target/bundle/LiteEdit.app`
3. Verifies with `codesign --verify --deep --strict target/bundle/LiteEdit.app`

The `--options runtime` enables hardened runtime, required for notarization.

Location: `scripts/codesign-app.sh`

### Step 7: Create the notarization script

Create a script that:
1. Reads Apple ID credentials from environment:
   - `APPLE_ID`: Apple ID email
   - `APPLE_TEAM_ID`: Team ID
   - `APPLE_APP_PASSWORD`: App-specific password (from appleid.apple.com)
2. Zips the app: `ditto -c -k --keepParent target/bundle/LiteEdit.app target/bundle/LiteEdit.zip`
3. Submits for notarization: `xcrun notarytool submit target/bundle/LiteEdit.zip --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" --wait`
4. Staples the ticket: `xcrun stapler staple target/bundle/LiteEdit.app`
5. Verifies: `spctl --assess --type exec --verbose target/bundle/LiteEdit.app`

Location: `scripts/notarize-app.sh`

### Step 8: Create the DMG creation script

Create a script that:
1. Creates a DMG from the `.app` using `hdiutil`
2. Sets up a simple layout (app + Applications symlink for drag-to-install)
3. Compresses the DMG
4. Signs the DMG with the same developer identity

Commands:
```bash
hdiutil create -volname "LiteEdit" -srcfolder target/bundle/LiteEdit.app -ov -format UDZO target/bundle/LiteEdit.dmg
codesign --sign "$APPLE_DEVELOPER_IDENTITY" target/bundle/LiteEdit.dmg
```

Location: `scripts/make-dmg.sh`

### Step 9: Create the Makefile

Create a Makefile with targets:
- `build`: `cargo build --release`
- `icns`: Run icon generation script
- `bundle`: Build + assemble app bundle (depends on `build`, `icns`)
- `sign`: Code sign the app (depends on `bundle`)
- `notarize`: Notarize the app (depends on `sign`)
- `dmg`: Create DMG (depends on `notarize`)
- `clean`: Remove build artifacts

The `make bundle` target should produce a usable `.app` without signing (for local development).
The `make dmg` target runs the full pipeline including notarization.

Location: `Makefile`

### Step 10: Update README with bundling instructions

Add a "Packaging for Distribution" section to README.md documenting:
- Prerequisites (Apple Developer ID, credentials in environment)
- `make bundle` for local testing
- `make dmg` for distribution
- Environment variables needed

Location: `README.md`

### Step 11: Verification

Manually verify:
1. `make bundle` produces `target/bundle/LiteEdit.app`
2. `target/bundle/LiteEdit.app/Contents/Info.plist` exists and is valid
3. `target/bundle/LiteEdit.app/Contents/Resources/LiteEdit.icns` exists
4. `target/bundle/LiteEdit.app/Contents/MacOS/lite-edit` is the release binary
5. Double-clicking `LiteEdit.app` in Finder launches the editor
6. (With signing credentials) `make sign` produces a signed app that passes `codesign --verify`
7. (With notarization credentials) `make dmg` produces a notarized DMG that passes `spctl --assess`

## Dependencies

- **External tools** (all built into macOS):
  - `sips` - Image manipulation
  - `iconutil` - Icon compilation
  - `codesign` - Code signing
  - `xcrun notarytool` - Notarization (macOS 12+)
  - `xcrun stapler` - Ticket stapling
  - `hdiutil` - DMG creation
  - `make` - Build orchestration

- **Apple Developer Program membership** (for signing and notarization):
  - Developer ID Application certificate installed in Keychain
  - App-specific password for notarization

- **No Rust crate dependencies** - This is purely build tooling.

## Risks and Open Questions

- **Hardened runtime and PTY**: The terminal emulator uses PTY which may require specific entitlements. If notarization fails, we may need to adjust entitlements or use additional exceptions.

- **Icon source resolution**: The chunk GOAL.md references `icon.png` at project root, but no such file exists yet. Step 1 creates a placeholder, but the operator should provide a proper icon.

- **Notarization latency**: Apple's notarization service can take 5-15 minutes. The script uses `--wait` to block until complete.

- **Apple Silicon vs Intel**: The release binary will be native to the build machine's architecture. Universal binary support (fat binary with both architectures) is out of scope for this chunk but could be added later.

- **Version synchronization**: Info.plist version is manually specified to match Cargo.toml. A future improvement could extract this automatically.

- **Sandbox limitations**: The app is not sandboxed because the terminal emulator needs PTY access. This is intentional but limits Mac App Store distribution (out of scope per docs/trunk/GOAL.md anyway).

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->