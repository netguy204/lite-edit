# Project Goal

<!--
This document is the stable anchor for the entire project. It should rarely change.
If you find yourself wanting to modify this frequently, you may have started
building before the problem was well understood.

Write in terms of WHAT and WHY, not HOW. The how belongs in SPEC.md and in chunks.
-->

## Problem Statement

Modern code editors are either fast but limited (terminal editors), or feature-rich but heavyweight (Electron-based editors, IDEs). Developers who want a responsive, native editing experience on macOS must compromise on one axis or the other.

lite-edit solves this by providing a lightweight, GPU-accelerated text editor for macOS that combines the responsiveness of a native Metal-rendered application with the features needed for daily coding work — including an integrated terminal emulator that feels like a normal buffer tab rather than a separate, special-cased UI area.

## Required Properties

- **Low latency rendering**: Keystroke-to-glyph P99 latency under 8ms.
- **Native Metal rendering**: All text and UI rendered directly via Metal on macOS. No Electron, no webview.
- **Integrated terminal emulator**: A built-in terminal that behaves like a regular editor buffer tab — same tab bar, same navigation, same visual weight. Not a special panel or drawer.
- **Minimal footprint**: Small binary, fast startup, low memory baseline.
- **Tree-sitter syntax highlighting**: Language-aware highlighting via tree-sitter grammars.
- **File navigation**: Fuzzy file picker and find-in-file search.

## Constraints

- **macOS only**: Targets macOS with Metal. No cross-platform abstraction layers.
- **Rust**: Core implementation in Rust using `objc2` crate family for Cocoa/Metal bindings.
- **No runtime dependencies**: Ships as a standalone binary. No bundled runtimes or interpreters.
- **Small team maintainability**: Architecture must be understandable and modifiable by a small team.

## Out of Scope

- Cross-platform support (Linux, Windows).
- LSP / language server integration (may come later but is not a current goal).
- Plugin ecosystem or extension API.
- Collaborative editing.

## Success Criteria

- The editor can open, edit, and save source files with syntax highlighting.
- Terminal tabs are indistinguishable in navigation and visual treatment from file buffer tabs.
- Rendering latency meets the <8ms P99 target on Apple Silicon Macs.
- Cold startup to interactive window in under 200ms.