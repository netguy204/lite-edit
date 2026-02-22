---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/terminal/src/cold_scrollback.rs
  - crates/terminal/src/terminal_buffer.rs
  - crates/terminal/src/lib.rs
  - crates/terminal/Cargo.toml
  - crates/terminal/tests/integration.rs
code_references:
  - ref: crates/terminal/src/cold_scrollback.rs#serialize_styled_line
    implements: "Compact binary serialization of StyledLine for disk storage"
  - ref: crates/terminal/src/cold_scrollback.rs#deserialize_styled_line
    implements: "Deserialization of StyledLine from compact binary format"
  - ref: crates/terminal/src/cold_scrollback.rs#ColdScrollback
    implements: "On-disk cold scrollback storage with line offset index"
  - ref: crates/terminal/src/cold_scrollback.rs#PageCache
    implements: "LRU page cache for cold scrollback reads"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer
    implements: "Extended with cold scrollback fields and tiered storage"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::check_scrollback_overflow
    implements: "Scrollback overflow detection and capture trigger"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::capture_cold_lines
    implements: "Captures oldest hot lines to cold storage"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::styled_line_hot
    implements: "Returns styled lines from hot scrollback region"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::get_cold_line
    implements: "Retrieves lines from cold storage via page cache"
  - ref: crates/terminal/src/lib.rs
    implements: "Module exports and crate documentation for scrollback"
narrative: null
investigation: hierarchical_terminal_tabs
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- terminal_emulator
created_after:
- file_save
- viewport_fractional_scroll
- word_boundary_primitives
- word_forward_delete
- word_jump_navigation
---

# Chunk Goal

## Minor Goal

Replace alacritty_terminal's in-memory scrollback with a tiered storage system that provides unlimited scrollback history with bounded memory. This is critical for the multi-agent workspace scenario — 10 workspaces × 10K in-memory scrollback = 276 MB, which is unacceptable. With file-backed scrollback, each terminal uses ~6.6 MB regardless of history length.

**Architecture:**

```
┌─────────────────────────┐
│   Viewport (40 lines)   │  alacritty_terminal grid (always in memory)
├─────────────────────────┤
│ Recent cache (~2K lines) │  alacritty_terminal scrollback (in memory)
├─────────────────────────┤
│  Cold scrollback (file)  │  Our code — StyledLines on disk
└─────────────────────────┘
```

As lines scroll off alacritty's in-memory grid (which has a small scrollback configured, e.g., 2K lines), intercept them and convert to `StyledLine` format (which we're already doing for rendering). Append to a compact on-disk log file. When the user scrolls into cold scrollback, page from disk.

**Compact format**: Don't store alacritty's 24-byte Cell structs. Store styled text runs: UTF-8 text + style change markers. A typical 120-char line compresses from 2,880 bytes (cell grid) to ~150-200 bytes — a ~15x reduction.

`BufferView::styled_line(n)` is the sole consumer and doesn't change its signature — it transparently returns from memory or disk depending on `n`.

## Success Criteria

- `TerminalBuffer` memory usage stays under ~7 MB per terminal regardless of scrollback depth
- Scrollback of 100K+ lines works without increased memory usage
- `styled_line(n)` returns correct content for lines in cold storage (file-backed region)
- Scrolling through cold scrollback feels responsive (target: <1ms per page of 40 lines)
- On-disk format achieves at least 10x size reduction vs. raw cell grid storage
- Cold scrollback survives `TerminalBuffer` lifetime (persisted to temp file, cleaned up on drop)
- No data loss at the hot/cold boundary — lines transition smoothly from in-memory to file-backed
- Concurrent access is safe: background PTY reader can append while main thread reads for rendering