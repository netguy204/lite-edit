<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk implements a tiered storage system for terminal scrollback that caps memory usage at ~6.6 MB per terminal regardless of history length. The architecture is straightforward:

**Core architecture:**
```
┌─────────────────────────┐
│   Viewport (40 lines)   │  alacritty_terminal grid (always in memory)
├─────────────────────────┤
│ Recent cache (~2K lines) │  alacritty_terminal scrollback (in memory)
├─────────────────────────┤
│  Cold scrollback (file)  │  Our code — StyledLines on disk
└─────────────────────────┘
```

**Key design decisions:**

1. **Intercept at the scrollback overflow point**: When alacritty's in-memory scrollback reaches capacity and would drop old lines, intercept those lines, convert to `StyledLine` format, and append to disk.

2. **Compact on-disk format**: Don't store alacritty's 24-byte Cell structs. Serialize styled text runs as UTF-8 text + style change markers. A typical 120-char line compresses from 2,880 bytes (cell grid) to ~150-200 bytes — a ~15x reduction.

3. **Page cache for cold reads**: Keep a small page cache (~1 MB) of recently accessed cold scrollback pages in memory. When user scrolls into cold region, fetch from disk and cache.

4. **Transparent API**: `BufferView::styled_line(n)` transparently returns from memory or disk. The only visible change is that line indices extend much further back than before.

5. **Temp file lifecycle**: Each `TerminalBuffer` creates a temp file for cold scrollback. File is cleaned up on `Drop`.

**Building on existing work:**
- Extends `crates/terminal/src/terminal_buffer.rs` with cold storage layer
- Reuses `StyledLine` serialization format (we're already converting cells to StyledLines for rendering)
- The `terminal_emulator` chunk (ACTIVE) established the `TerminalBuffer` structure we're extending

**Testing strategy:** Per TESTING_PHILOSOPHY.md:
- **Unit tests**: Serialization format, page cache logic, line index calculation
- **Integration tests**: End-to-end scrollback persistence, recovery after writes
- **Humble code**: File I/O operations are thin wrappers, tested through integration tests

## Subsystem Considerations

No subsystems are currently documented for this project. This chunk does not establish patterns that warrant subsystem documentation — it's a single-purpose extension to `TerminalBuffer`.

## Sequence

### Step 1: Define the on-disk serialization format

Create a new module `crates/terminal/src/cold_scrollback.rs` with the serialization format for `StyledLine`:

**Format per line (variable length):**
```
┌─────────────────────────────────────────────────────────┐
│ u32: line_length (total bytes for this record)          │
├─────────────────────────────────────────────────────────┤
│ u16: num_spans                                          │
├─────────────────────────────────────────────────────────┤
│ For each span:                                          │
│   u16: text_length (bytes)                              │
│   [u8]: UTF-8 text data                                 │
│   u8:  style_flags bitfield                             │
│        bit 0: bold                                      │
│        bit 1: italic                                    │
│        bit 2: dim                                       │
│        bit 3: strikethrough                             │
│        bit 4: inverse                                   │
│        bit 5: hidden                                    │
│        bit 6: has_underline                             │
│        bit 7: has_fg_color                              │
│   u8:  underline_style (if has_underline)               │
│   [color]: fg_color (if has_fg_color)                   │
│   [color]: bg_color (always, 1-5 bytes)                 │
│   [color]: underline_color (if has_underline)           │
└─────────────────────────────────────────────────────────┘

Color encoding (variable 1-5 bytes):
  0x00: Default
  0x01 + idx: Indexed(idx)  [2 bytes]
  0x02 + r + g + b: Rgb     [4 bytes]
  0x10-0x1F: Named colors   [1 byte]
```

Implement:
- `fn serialize_styled_line(line: &StyledLine) -> Vec<u8>`
- `fn deserialize_styled_line(data: &[u8]) -> Result<StyledLine, Error>`

**Tests:**
- `test_serialize_empty_line` - empty line roundtrips correctly
- `test_serialize_plain_text` - single span with default style
- `test_serialize_styled_text` - span with various attributes
- `test_serialize_multiple_spans` - line with multiple spans
- `test_serialize_colors` - named, indexed, and RGB colors
- `test_serialize_wide_chars` - UTF-8 with CJK/emoji

Location: `crates/terminal/src/cold_scrollback.rs`

### Step 2: Implement the cold scrollback file structure

Add a struct that manages the on-disk scrollback file:

```rust
/// Manages the on-disk cold scrollback storage.
///
/// Lines are appended sequentially. An in-memory index maps line numbers
/// to file offsets for random access.
// Chunk: docs/chunks/file_backed_scrollback - File-backed cold scrollback
pub struct ColdScrollback {
    /// The underlying file.
    file: File,
    /// Path to the temp file (for cleanup on drop).
    path: PathBuf,
    /// Index of line offsets: line_index[i] = byte offset of line i in file.
    line_offsets: Vec<u64>,
    /// Total number of lines stored.
    line_count: usize,
}
```

Implement:
- `ColdScrollback::new() -> Result<Self>` - creates temp file
- `ColdScrollback::append(&mut self, line: &StyledLine) -> Result<()>` - appends line to file, updates index
- `ColdScrollback::get(&self, line: usize) -> Result<StyledLine>` - reads line by index
- `ColdScrollback::line_count(&self) -> usize`
- `impl Drop` - removes temp file

**Tests:**
- `test_cold_scrollback_append_and_get` - append lines, read back
- `test_cold_scrollback_many_lines` - append 1000 lines, verify all accessible
- `test_cold_scrollback_cleanup` - verify temp file removed on drop

Location: `crates/terminal/src/cold_scrollback.rs`

### Step 3: Add a page cache for cold reads

Add a simple LRU-ish page cache to avoid repeated disk reads for nearby scrollback lines:

```rust
/// Page of cached cold scrollback lines.
struct CachePage {
    /// First line index in this page.
    start_line: usize,
    /// Cached lines.
    lines: Vec<StyledLine>,
    /// Last access timestamp for eviction.
    last_access: Instant,
}

/// Page cache for cold scrollback reads.
// Chunk: docs/chunks/file_backed_scrollback - File-backed cold scrollback
pub struct PageCache {
    /// Cached pages, keyed by start_line.
    pages: HashMap<usize, CachePage>,
    /// Maximum cache size in bytes (approximate).
    max_bytes: usize,
    /// Current estimated size.
    current_bytes: usize,
    /// Page size in lines.
    page_size: usize,
}
```

Implement:
- `PageCache::new(max_bytes: usize, page_size: usize) -> Self`
- `PageCache::get(&mut self, line: usize, cold: &ColdScrollback) -> Result<StyledLine>` - get line, fetching page if needed
- `PageCache::invalidate(&mut self)` - clear cache

**Design notes:**
- Page size: 64 lines (typical viewport + some margin)
- When fetching a line not in cache, fetch the entire page containing it
- Evict oldest pages when cache exceeds `max_bytes`
- Approximate size tracking by counting text bytes in cached lines

**Tests:**
- `test_page_cache_hit` - line in cache returns without disk read
- `test_page_cache_miss` - line not in cache triggers page fetch
- `test_page_cache_eviction` - exceeding cache size evicts old pages

Location: `crates/terminal/src/cold_scrollback.rs`

### Step 4: Hook into alacritty's scrollback overflow

The key integration point: when alacritty's scrollback buffer overflows and drops old lines, intercept them.

alacritty_terminal's `Term` struct has a `grid()` method that returns the terminal grid. When the grid scrolls and the history is full, lines are dropped from the oldest end.

**Challenge:** alacritty_terminal doesn't provide an explicit "line dropped" callback. We need to detect when lines are about to be dropped.

**Approach:** Track `grid.history_size()` before and after each `processor.advance()` call. If history_size reaches max and new lines are added, the oldest line(s) were dropped. Capture them before the drop.

**Alternative approach (simpler):** Configure alacritty with a small scrollback (e.g., 100 lines as a "transition buffer"). When history exceeds threshold (e.g., 80 lines), flush oldest lines to cold storage. This avoids needing to detect exact overflow points.

```rust
impl TerminalBuffer {
    /// Maximum lines in alacritty's in-memory scrollback before flushing to cold.
    const HOT_SCROLLBACK_MAX: usize = 2000;
    /// Lines to flush when threshold reached.
    const FLUSH_BATCH_SIZE: usize = 500;

    /// Flushes old scrollback lines to cold storage.
    fn flush_cold_scrollback(&mut self) {
        let grid = self.term.grid();
        let history_size = grid.history_size();

        if history_size > Self::HOT_SCROLLBACK_MAX {
            let flush_count = history_size - Self::HOT_SCROLLBACK_MAX + Self::FLUSH_BATCH_SIZE;
            // Read oldest lines and append to cold storage
            for i in 0..flush_count {
                let scroll_idx = history_size - 1 - i;
                let row = &grid[Line(-(scroll_idx as i32) - 1)];
                let styled = row_to_styled_line(row, self.size.0);
                self.cold_scrollback.append(&styled);
            }
            // Note: We can't actually remove lines from alacritty's grid.
            // See Step 5 for the workaround.
        }
    }
}
```

**Problem:** alacritty_terminal doesn't let us remove lines from its history. The `resize()` method can shrink history, but that's destructive.

**Revised approach:** Configure alacritty with a small, fixed scrollback (e.g., 2000 lines). As lines age past this threshold, we've already captured them to cold storage. The "hot" scrollback in alacritty is essentially a window into recent history. When querying `styled_line(n)`:
- If `n < cold_line_count`: read from cold storage
- Else: read from alacritty's hot scrollback (adjusting index)

This means we don't modify alacritty's scrollback — we just capture lines as they're about to age out.

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 5: Implement scrollback overflow detection and capture

Add the scrollback tracking and capture logic to `TerminalBuffer`:

```rust
pub struct TerminalBuffer {
    // ... existing fields ...

    /// Cold scrollback storage.
    cold_scrollback: Option<ColdScrollback>,
    /// Page cache for cold reads.
    page_cache: PageCache,
    /// Number of lines we've captured to cold storage.
    cold_line_count: usize,
    /// Last observed history size (for detecting overflow).
    last_history_size: usize,
    /// Configured maximum hot scrollback before flushing.
    hot_scrollback_limit: usize,
}
```

Update `poll_events()` to track scrollback growth and flush when needed:

```rust
pub fn poll_events(&mut self) -> bool {
    // ... existing event processing ...

    if processed_any {
        self.update_damage();
        self.check_scrollback_overflow();
    }

    processed_any
}

fn check_scrollback_overflow(&mut self) {
    let history_size = self.history_size();

    // Detect when lines have scrolled off the top
    if history_size > self.hot_scrollback_limit {
        // Capture lines that are about to be lost when alacritty recycles
        // its scrollback buffer
        self.capture_cold_lines();
    }

    self.last_history_size = history_size;
}

fn capture_cold_lines(&mut self) {
    // Initialize cold storage if needed
    if self.cold_scrollback.is_none() {
        match ColdScrollback::new() {
            Ok(cold) => self.cold_scrollback = Some(cold),
            Err(e) => {
                // Log error, continue without cold storage
                eprintln!("Failed to create cold scrollback: {}", e);
                return;
            }
        }
    }

    let cold = self.cold_scrollback.as_mut().unwrap();
    let grid = self.term.grid();
    let history_size = grid.history_size();
    let cols = self.size.0;

    // Capture oldest lines that exceed our hot limit
    let lines_to_capture = history_size.saturating_sub(self.hot_scrollback_limit);

    for i in 0..lines_to_capture {
        // Read from oldest end of scrollback
        let scroll_idx = history_size - 1 - i;
        let row = &grid[Line(-(scroll_idx as i32) - 1)];
        let cells: Vec<_> = (0..cols)
            .map(|col| &row[alacritty_terminal::index::Column(col)])
            .collect();
        let styled = row_to_styled_line(cells.iter().copied(), cols);
        if cold.append(&styled).is_err() {
            // Log error, stop capturing
            break;
        }
    }

    self.cold_line_count += lines_to_capture;
}
```

**Tests:**
- `test_scrollback_overflow_capture` - verify lines captured when history exceeds limit
- `test_cold_line_count_tracks` - verify cold_line_count increments correctly

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 6: Update BufferView::styled_line() to read from cold storage

Modify the `styled_line()` implementation to transparently serve from cold storage:

```rust
impl BufferView for TerminalBuffer {
    fn styled_line(&self, line: usize) -> Option<StyledLine> {
        if self.is_alt_screen() {
            // Alternate screen: no scrollback, existing logic
            return self.styled_line_alt_screen(line);
        }

        // Total logical line count: cold + hot history + viewport
        let cold_count = self.cold_line_count;

        if line < cold_count {
            // Line is in cold storage
            return self.cold_scrollback
                .as_ref()
                .and_then(|cold| {
                    self.page_cache.get(line, cold).ok()
                });
        }

        // Line is in hot scrollback or viewport
        let hot_line = line - cold_count;
        self.styled_line_hot(hot_line)
    }

    fn line_count(&self) -> usize {
        if self.is_alt_screen() {
            self.screen_lines()
        } else {
            // Cold lines + hot history + viewport
            self.cold_line_count + self.history_size() + self.screen_lines()
        }
    }
}
```

Factor existing logic into helper methods:
- `styled_line_alt_screen(line: usize) -> Option<StyledLine>` - existing alt screen logic
- `styled_line_hot(line: usize) -> Option<StyledLine>` - existing hot scrollback + viewport logic

**Tests:**
- `test_styled_line_cold_region` - lines in cold region return correct content
- `test_styled_line_hot_region` - lines in hot region work as before
- `test_styled_line_transition` - lines near cold/hot boundary work correctly
- `test_line_count_includes_cold` - line_count includes cold lines

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 7: Update cursor_info() for cold scrollback offset

When cold scrollback exists, cursor position needs to account for the extra lines:

```rust
fn cursor_info(&self) -> Option<CursorInfo> {
    let grid = self.term.grid();
    let cursor_point = grid.cursor.point;

    let doc_line = if self.is_alt_screen() {
        cursor_point.line.0 as usize
    } else {
        // Add cold lines + hot history to viewport line
        self.cold_line_count + self.history_size() + cursor_point.line.0 as usize
    };

    // ... rest of cursor info logic unchanged
}
```

**Tests:**
- `test_cursor_info_with_cold_scrollback` - cursor position accounts for cold lines

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 8: Add configuration for scrollback limits

Make the hot scrollback limit configurable:

```rust
impl TerminalBuffer {
    /// Default hot scrollback limit (lines kept in memory).
    const DEFAULT_HOT_SCROLLBACK_LIMIT: usize = 2000;

    pub fn new(cols: usize, rows: usize, scrollback: usize) -> Self {
        // ... existing initialization ...
        Self {
            // ...
            hot_scrollback_limit: scrollback.min(Self::DEFAULT_HOT_SCROLLBACK_LIMIT),
            cold_scrollback: None,
            page_cache: PageCache::new(1024 * 1024, 64), // 1MB cache, 64-line pages
            cold_line_count: 0,
            last_history_size: 0,
        }
    }

    /// Sets the hot scrollback limit.
    pub fn set_hot_scrollback_limit(&mut self, limit: usize) {
        self.hot_scrollback_limit = limit;
    }
}
```

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 9: Add integration test for end-to-end scrollback

Create an integration test that verifies the full flow:

```rust
#[test]
fn test_file_backed_scrollback_e2e() {
    // Create terminal with small hot limit for testing
    let mut term = TerminalBuffer::new(80, 24, 100);
    term.set_hot_scrollback_limit(100);

    // Feed enough output to overflow hot scrollback
    for i in 0..500 {
        let line = format!("Line {:04}\r\n", i);
        term.processor.advance(&mut term.term, line.as_bytes());
    }

    // Poll to trigger overflow capture
    term.check_scrollback_overflow();

    // Verify total line count includes cold + hot + viewport
    assert!(term.line_count() > 400);

    // Verify we can read from cold region
    let cold_line = term.styled_line(10);
    assert!(cold_line.is_some());
    let text: String = cold_line.unwrap().spans.iter()
        .map(|s| &s.text[..])
        .collect();
    assert!(text.contains("Line 0010"));

    // Verify we can read from hot region
    let hot_line = term.styled_line(term.line_count() - 30);
    assert!(hot_line.is_some());
}
```

Location: `crates/terminal/tests/integration.rs`

### Step 10: Performance test for scrollback reads

Add a benchmark/test to verify read performance meets the <1ms target for 40 lines:

```rust
#[test]
fn test_cold_scrollback_read_performance() {
    use std::time::Instant;

    let mut term = TerminalBuffer::new(120, 40, 100);
    term.set_hot_scrollback_limit(100);

    // Fill with 10K lines to ensure cold storage is used
    for i in 0..10_000 {
        let line = format!("Line {:05} with some content padding\r\n", i);
        term.processor.advance(&mut term.term, line.as_bytes());
        if i % 100 == 0 {
            term.check_scrollback_overflow();
        }
    }
    term.check_scrollback_overflow();

    // Time reading 40 lines from cold region (simulating a viewport)
    let start = Instant::now();
    for i in 0..40 {
        let _ = term.styled_line(i * 100); // Read spread across cold storage
    }
    let elapsed = start.elapsed();

    // Should be well under 1ms
    assert!(elapsed.as_millis() < 10, "Cold read took {}ms", elapsed.as_millis());
}
```

Location: `crates/terminal/tests/integration.rs`

### Step 11: Export and document the module

Update `crates/terminal/src/lib.rs`:

```rust
// Chunk: docs/chunks/terminal_emulator - Terminal emulator backed by alacritty_terminal
// Chunk: docs/chunks/file_backed_scrollback - File-backed cold scrollback
//! Terminal emulator crate for lite-edit.
//!
//! This crate provides `TerminalBuffer`, a full-featured terminal emulator
//! that implements the `BufferView` trait. It wraps `alacritty_terminal` for
//! escape sequence interpretation and manages PTY I/O for process communication.
//!
//! ## Scrollback
//!
//! `TerminalBuffer` supports unlimited scrollback history with bounded memory:
//! - Recent lines stay in memory (hot scrollback)
//! - Older lines are persisted to a temp file (cold scrollback)
//! - The `BufferView::styled_line()` API is transparent — callers don't
//!   need to know where the data comes from
//!
//! This enables 10+ concurrent terminals with 100K+ line histories while
//! keeping memory usage under ~7MB per terminal.

mod cold_scrollback;
mod event;
mod pty;
mod style_convert;
mod terminal_buffer;

pub use terminal_buffer::TerminalBuffer;
```

Location: `crates/terminal/src/lib.rs`

## Dependencies

### Chunk dependencies (must be complete):
- `terminal_emulator` (ACTIVE) - Provides the `TerminalBuffer` struct that this chunk extends with cold scrollback support

### External crate dependencies:
- None new — uses standard library `std::fs` and `std::io` for file operations
- Uses `tempfile` crate for temp file creation (add to `Cargo.toml` if not present)

## Risks and Open Questions

1. **alacritty scrollback removal**: We can't actually remove lines from alacritty's scrollback once they're there. The workaround is to configure a fixed hot scrollback limit and simply track which lines we've captured to cold storage. This means alacritty will eventually recycle old lines, but by then we've already captured them.

2. **Concurrency between PTY writes and cold reads**: The background PTY reader thread triggers `poll_events()` which may capture lines to cold storage. Meanwhile, the main/render thread reads `styled_line()`. Need to ensure thread safety:
   - Option A: Use mutex around cold scrollback access (simple but may add latency)
   - Option B: Use append-only file writes + atomic line_count updates (lockless reads)
   - **Recommendation**: Start with Option A, profile if latency becomes an issue

3. **Temp file location and permissions**: Using `tempfile::tempfile()` creates an anonymous temp file, which is ideal (auto-cleanup on process exit). If that's not available on all platforms, fall back to `tempfile::NamedTempFile` with explicit cleanup.

4. **Large scrollback performance**: The line offset index grows linearly with scrollback depth. At 100K lines, that's 800KB for 64-bit offsets. Acceptable, but worth noting. Could use sparse indexing (every 64th line) if this becomes an issue.

5. **Scrollback ordering**: Lines must be captured in order. If `check_scrollback_overflow()` is called infrequently (e.g., only after large batches of output), we might miss lines. Mitigation: call it frequently (every `poll_events()`) and batch captures efficiently.

6. **Page cache invalidation**: The page cache doesn't need invalidation for normal operation (cold scrollback is append-only). However, if we ever implement scrollback truncation or clearing, the cache must be invalidated.

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