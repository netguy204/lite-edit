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

## License

MIT
