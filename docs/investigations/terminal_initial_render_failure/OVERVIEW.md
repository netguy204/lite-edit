---
status: SOLVED
trigger: "terminal_tab_initial_render chunk did not fix the blank terminal on tab creation — new terminals still require a window resize to display content"
proposed_chunks:
  - prompt: "Fix terminal tab initial render: the terminal tab viewport is created with visible_rows=0 because sync_active_tab_viewport skips non-file tabs. When poll_standalone_terminals calls scroll_to_bottom with visible_rows=0, it computes max_offset = line_count * line_height (scrolling past all content). The renderer then tries to show lines starting from an offset beyond the terminal's content, producing a blank screen. The fix should ensure terminal tab viewports get their visible_rows set when created. Either sync_active_tab_viewport should handle terminal tabs (using BufferView::line_count instead of requiring a TextBuffer), or new_terminal_tab should explicitly call viewport.update_size with the correct content_height and line_count after creating the tab. Also remove the spin-poll mechanism from terminal_tab_initial_render as it's treating a symptom, not the cause."
    chunk_directory: null
    depends_on: []
created_after: ["terminal_tab_initial_render"]
---

## Trigger

The `terminal_tab_initial_render` chunk attempted to fix blank terminal tabs by adding a spin-poll mechanism that waits up to 100ms for shell output after tab creation. Despite this, new terminal tabs still show blank content until the window is resized. The spin-poll approach was treating a symptom rather than the root cause.

## Success Criteria

- Identify the root cause of why terminal tabs render blank until resized
- Propose a fix that addresses the root cause rather than working around timing

## Testable Hypotheses

### H1: The terminal tab's viewport has visible_rows=0, causing scroll_to_bottom to scroll past all content

- **Rationale**: `Tab::new_terminal` creates a `Viewport::new(line_height)` which initializes `visible_rows=0`. `sync_active_tab_viewport` explicitly skips non-file tabs. When `poll_standalone_terminals` calls `scroll_to_bottom` with `visible_rows=0`, the max_offset calculation becomes `(line_count - 0) * line_height`, scrolling the viewport past all terminal content.
- **Test**: Trace the viewport state through tab creation → poll → render
- **Status**: VERIFIED

## Exploration Log

### 2026-02-22: Root cause analysis

Traced the full rendering pipeline for a newly created terminal tab:

1. **Tab creation** (`new_terminal_tab`):
   - Creates `Viewport::new(line_height)` → `RowScroller::new` → `visible_rows: 0`
   - Calls `sync_active_tab_viewport()` which returns early for non-file tabs
   - Sets `dirty_region = FullViewport` and `pending_terminal_created = true`

2. **Spin-poll** (`spin_poll_terminal_startup`):
   - Calls `poll_agents()` which calls `poll_standalone_terminals()`
   - Inside, `is_at_bottom(line_count)` returns `true` (because with `visible_rows=0`, it falls through to `self.scroll_offset_px() <= 0.0` which is true)
   - After shell output arrives, `scroll_to_bottom(line_count)` is called
   - With `visible_rows=0` and `line_count=40`: `max_offset = (40 - 0) * 16.0 = 640.0px`
   - Viewport scroll offset is set to 640px — **past all terminal content**

3. **Render** (`render_if_dirty`):
   - Syncs scroll offset: `state.viewport().scroll_offset_px()` = 640.0
   - Sets renderer viewport to 640px scroll offset
   - `update_glyph_buffer` tries to render from `first_visible_screen_row = 640/16 = 40`
   - Terminal only has 40 lines (0-39) → nothing to render → blank screen

4. **Resize fixes it** (`handle_resize`):
   - Calls `update_viewport_dimensions` which calls `self.viewport_mut().update_size(content_height, line_count)`
   - This sets `visible_rows` to the correct value (e.g., 40)
   - `update_size` also re-clamps scroll offset via `set_scroll_offset_px`
   - With `visible_rows=40` and `line_count=40`: max_offset = 0, scroll clamped to 0
   - Render now shows content from row 0 → terminal is visible

### Key finding: The PTY wakeup mechanism works correctly

The PTY wakeup (`dispatch_async` → `handle_pty_wakeup`) fires and triggers a render. But the render still shows blank content because the viewport scroll offset is already wrong (scrolled past all content). The wakeup re-polls, gets no new data (already drained), and renders at the same wrong offset.

## Findings

### Verified Findings

- **Root cause**: Terminal tab viewports are created with `visible_rows=0` and never get updated until a window resize occurs.
- **`sync_active_tab_viewport` explicitly skips terminal tabs** (line 386: `None => return, // Non-file tab, skip viewport sync`). This was likely intentional to avoid issues with TextBuffer-specific logic, but it means terminal tabs never get their viewport dimensions set.
- **`scroll_to_bottom` with `visible_rows=0` scrolls past all content**: The formula `(line_count - visible_lines) * line_height` produces `line_count * line_height` when `visible_lines=0`, which is the full content height — one full viewport past the end.
- **The spin-poll mechanism is ineffective**: Even when it successfully captures shell output, it triggers `scroll_to_bottom` with the broken viewport, making things worse (scrolling to an invalid position).

### Hypotheses/Opinions

- The simplest fix is to have `new_terminal_tab` explicitly call `viewport.update_size(content_height, line_count)` on the newly created tab's viewport after adding it to the workspace. This matches what `update_viewport_dimensions` does during resize.
- The spin-poll mechanism (`spin_poll_terminal_startup` and `pending_terminal_created`) can likely be removed entirely once the viewport is initialized correctly, since the PTY wakeup mechanism handles async shell output rendering.

## Proposed Chunks

1. **Fix terminal viewport initialization**: Set `visible_rows` on terminal tab viewports at creation time so `scroll_to_bottom` computes correct offsets. Remove the spin-poll workaround.
   - Priority: High
   - Dependencies: None
   - Notes: The fix is in `new_terminal_tab` or `sync_active_tab_viewport`. Either make `sync_active_tab_viewport` handle terminal tabs by using `BufferView::line_count()` instead of requiring `TextBuffer`, or explicitly initialize the viewport in `new_terminal_tab` after the tab is added.

## Resolution Rationale

Root cause identified: terminal tab viewports have `visible_rows=0` because `sync_active_tab_viewport` skips non-file tabs and `Viewport::new()` initializes with 0 visible rows. This causes `scroll_to_bottom` (called from `poll_standalone_terminals`) to scroll past all content. The existing spin-poll workaround in `terminal_tab_initial_render` is ineffective because it triggers the same broken scroll logic. A straightforward fix is to initialize the viewport dimensions when creating the terminal tab.
