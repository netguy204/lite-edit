# lite-edit

A lightweight, GPU-accelerated text editor for macOS.

## Overview

lite-edit is a native macOS text editor built with Rust that uses Metal for GPU-accelerated rendering. The design prioritizes:

- **Low latency**: Keystroke-to-glyph target of <8ms P99
- **Minimal footprint**: Small, fast core with plugins on separate threads
- **Direct Metal rendering**: No Electron, no webview, no terminal emulator

## Building

### Prerequisites

- macOS (Apple Silicon or Intel)
- Rust toolchain (install via [rustup](https://rustup.rs/))
- Xcode Command Line Tools (`xcode-select --install`)

### Build Commands

```bash
# Debug build
cargo build

# Release build (recommended)
cargo build --release
```

### Running

```bash
# Debug mode
cargo run

# Release mode
cargo run --release
```

## Current Status

This is the initial implementation proving the Rust → Cocoa → Metal pipeline works:

- ✅ Native macOS window with title bar
- ✅ Metal-backed rendering via CAMetalLayer
- ✅ Solid background color (#1e1e2e - Catppuccin Mocha base)
- ✅ Window resize handling with retina support
- ✅ Clean shutdown on window close or Cmd-Q

## Expected Behavior

When you run the application:

1. A native macOS window titled "lite-edit" opens
2. The window content is a solid dark purple/blue color (#1e1e2e)
3. The window is resizable - resizing causes the Metal layer to resize correctly
4. Closing the window (red button) or pressing Cmd-Q terminates the app cleanly

## Architecture

```
src/
├── main.rs         # Application bootstrap, NSApplication/NSWindow setup
├── metal_view.rs   # CAMetalLayer-backed NSView subclass
└── renderer.rs     # Metal rendering pipeline (clear to background color)
```

The implementation uses the `objc2` family of crates for type-safe Objective-C bindings, providing direct access to Cocoa and Metal frameworks without additional abstraction layers.

## Packaging for Distribution

LiteEdit can be packaged as a standard macOS application bundle (`.app`) with code signing and notarization for distribution outside the Mac App Store.

### Quick Start (Local Testing)

```bash
# Build and create an unsigned app bundle
make bundle

# Run the app
open target/bundle/LiteEdit.app
```

### Full Distribution Build

For distribution, you need an Apple Developer ID certificate and notarization credentials.

#### Prerequisites

1. **Apple Developer Program membership** ($99/year)
2. **Developer ID Application certificate** installed in your Keychain
3. **App-specific password** for notarization (generate at [appleid.apple.com](https://appleid.apple.com/account/manage))

#### Environment Variables

Set these before running the distribution build:

```bash
# Your code signing identity (find with: security find-identity -v -p codesigning)
export APPLE_DEVELOPER_IDENTITY="Developer ID Application: Your Name (TEAMID)"

# Notarization credentials
export APPLE_ID="your@email.com"
export APPLE_TEAM_ID="ABCD123456"  # 10-character Team ID
export APPLE_APP_PASSWORD="xxxx-xxxx-xxxx-xxxx"
```

#### Build Commands

```bash
# Full pipeline: build → sign → notarize → create DMG
make dmg

# The signed, notarized DMG will be at: target/bundle/LiteEdit.dmg
```

### Build Targets

| Target | Description |
|--------|-------------|
| `make build` | Build release binary |
| `make bundle` | Create unsigned app bundle (for local testing) |
| `make sign` | Code sign the app bundle |
| `make notarize` | Notarize with Apple |
| `make dmg` | Full pipeline including DMG creation |
| `make clean` | Remove build artifacts |

### Customizing the Icon

Replace `icon.png` in the project root with your own 1024×1024 PNG icon. Run `make icns` to regenerate the macOS icon file.

### Notes

- The app is **not sandboxed** because the terminal emulator requires PTY access, which is incompatible with the App Sandbox. This means the app cannot be distributed via the Mac App Store.
- Code signing uses **hardened runtime**, which is required for notarization.
- Notarization typically takes 5-15 minutes. The build will wait for completion.

## License

MIT
