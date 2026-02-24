<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The welcome screen currently has no scroll state — `calculate_welcome_geometry` centers
content vertically and clamps `content_y` to `≥ 0`. When the viewport is shorter than the
content, the bottom of the hotkey table is clipped and unreachable.

We add a per-tab `welcome_scroll_offset_px: f32` field that accumulates scroll-wheel pixel
deltas. During geometry calculation, this offset is subtracted from `content_y` (shifting
content upward), with clamping to `[0, (content_height_px - viewport_height_px).max(0)]`
applied inside `calculate_welcome_geometry`. The scroll handler stores the raw accumulated
value (lower-bound clamped at 0); the geometry function is the authoritative upper-bound
clamp so resizing the window always produces correct visual clamping without needing to
recompute the upper bound in the event path.

The welcome screen content is static (fixed text in `welcome_screen.rs`) — its height in
pixels is deterministic from font metrics alone. No new data structures are needed. The
`Tab` struct gets one new `f32` field, and two rendering functions get one new `f32`
parameter.

**Pattern alignment**: The implementation follows the same philosophy as the
`viewport_scroll` subsystem (tracking `scroll_offset_px` as a float, clamping to valid
bounds) but uses a plain `f32` rather than `RowScroller` because the welcome screen has
fixed pixel content, not a line-based buffer with a row count.

## Subsystem Considerations

**docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the subsystem's
patterns. The welcome scroll offset is tracked as a pixel float and clamped to
`[0, max_scroll]` — the same invariant as `RowScroller`. The implementation intentionally
does _not_ use `RowScroller` or `Viewport` because the welcome screen content is fixed-
height pixel art, not a line buffer with dynamic row counts. No deviation to document;
the subsystem's scope explicitly excludes non-buffer scrollable UI elements.

## Sequence

### Step 1: Make `calculate_content_dimensions` pub in `welcome_screen.rs`

The scroll handler in `editor_state.rs` does not need the content dimensions directly
(upper-bound clamping is delegated to the geometry function). However, exposing this
function as `pub(crate)` makes it testable and usable from `editor_state.rs` if ever
needed. Change the visibility from private to `pub(crate)`.

Location: `crates/editor/src/welcome_screen.rs`

### Step 2: Modify `calculate_welcome_geometry` to accept `scroll_offset_px: f32`

Add a `scroll_offset_px: f32` parameter after `line_height: f32`. Apply clamping and
offset inside the function:

```rust
let content_height_px = content_height_lines as f32 * line_height;
let max_scroll = (content_height_px - viewport_height).max(0.0);
let effective_scroll = scroll_offset_px.clamp(0.0, max_scroll);
let content_y = ((viewport_height - content_height_px) / 2.0).max(0.0) - effective_scroll;
```

When `viewport_height >= content_height_px`: `max_scroll = 0`, `effective_scroll = 0`,
`content_y` is the centered value — identical to current behavior.

When `viewport_height < content_height_px`: `content_y` starts at 0.0 (top of content
visible) and decreases to `-(content_height_px - viewport_height)` at max scroll
(bottom of content visible).

Update the docstring to document the new parameter and scroll behavior.

Location: `crates/editor/src/welcome_screen.rs`

### Step 3: Update tests for `calculate_welcome_geometry`

**Write tests first** (TDD — these fail before Step 2 is implemented if done in order,
but in practice Steps 2 and 3 are done together since the signature change must compile):

- `test_geometry_scroll_offsets_content_y`: With a small viewport (< content), assert
  that `content_y` equals `0.0 - scroll_offset_px` for a valid scroll value.
- `test_geometry_scroll_clamps_at_top`: Negative scroll_offset_px clamps to 0 (content_y
  unchanged from no-scroll case).
- `test_geometry_scroll_clamps_at_bottom`: scroll_offset_px > max_scroll clamps to
  max_scroll; `content_y` equals `-(content_height_px - viewport_height_px)`.
- `test_geometry_large_viewport_ignores_scroll`: When viewport > content, any scroll
  offset has no effect (max_scroll = 0) and centering is preserved.
- `test_geometry_scroll_zero_unchanged`: Passing `scroll_offset_px = 0.0` produces the
  same result as the old no-scroll behavior (regression guard for existing tests).

Update existing tests that call `calculate_welcome_geometry` to pass `0.0` as the new
last argument.

Location: `crates/editor/src/welcome_screen.rs` (in `#[cfg(test)] mod tests`)

### Step 4: Add `welcome_scroll_offset_px: f32` to `Tab` in `workspace.rs`

Add the field and initialize it to `0.0` in all Tab constructors:
- `new_file(…)` — `welcome_scroll_offset_px: 0.0`
- `new_agent(…)` — `welcome_scroll_offset_px: 0.0`
- `new_terminal(…)` — `welcome_scroll_offset_px: 0.0`

Add two methods:

```rust
pub fn welcome_scroll_offset_px(&self) -> f32 {
    self.welcome_scroll_offset_px
}

pub fn set_welcome_scroll_offset_px(&mut self, offset: f32) {
    self.welcome_scroll_offset_px = offset.max(0.0);
}
```

The setter enforces the lower bound (≥ 0) defensively; the upper bound is enforced at
render time by `calculate_welcome_geometry`.

Location: `crates/editor/src/workspace.rs`

### Step 5: Add `welcome_scroll_offset_px()` to `Editor` in `workspace.rs`

Convenience accessor for the renderer, which has an `&Editor` reference at call sites:

```rust
pub fn welcome_scroll_offset_px(&self) -> f32 {
    self.active_workspace()
        .and_then(|ws| ws.active_tab())
        .map(|t| t.welcome_scroll_offset_px())
        .unwrap_or(0.0)
}
```

Location: `crates/editor/src/workspace.rs`

### Step 6: Handle welcome screen scroll in `scroll_pane` in `editor_state.rs`

In `scroll_pane`, before the existing `buffer_and_viewport_mut()` branch, add a welcome
screen check:

```rust
// Chunk: docs/chunks/welcome_scroll - Welcome screen vertical scrolling
let is_welcome = tab.kind == TabKind::File
    && tab.as_text_buffer().map(|b| b.is_empty()).unwrap_or(false);

if is_welcome {
    let current = tab.welcome_scroll_offset_px();
    let new_offset = (current + delta.dy as f32).max(0.0);
    tab.set_welcome_scroll_offset_px(new_offset);
    if (new_offset - current).abs() > 0.001 {
        self.dirty_region.merge(DirtyRegion::FullViewport);
    }
    return;
}
```

Return early so the empty-buffer path never tries to scroll a 0-line viewport.

The upper-bound clamp is not applied here; `calculate_welcome_geometry` is the
authoritative clamp. This avoids coupling the event handler to welcome screen geometry
constants and correctly handles viewport size changes (resize makes the upper bound
smaller, renderer re-clamps visually on the next frame).

Location: `crates/editor/src/editor_state.rs`, inside `fn scroll_pane`

### Step 7: Write unit tests for welcome screen scroll routing

In `editor_state.rs` tests (`#[cfg(test)]`):

- `test_welcome_screen_scroll_updates_offset`: Create a state with an empty file tab.
  Call `handle_scroll(ScrollDelta::new(0.0, 50.0))`. Assert the active tab's
  `welcome_scroll_offset_px()` is 50.0, and `dirty_region.is_dirty()` is true.
- `test_welcome_screen_scroll_clamps_at_zero`: Scroll up (negative dy) from offset 0.
  Assert offset stays at 0.
- `test_non_welcome_scroll_uses_viewport`: Create a state with a non-empty file tab.
  Call `handle_scroll`. Assert `welcome_scroll_offset_px()` remains 0.0 (normal viewport
  scroll path taken, not welcome path).

Location: `crates/editor/src/editor_state.rs` (in `#[cfg(test)]`)

### Step 8: Update renderer to pass scroll offset to draw functions

**`draw_welcome_screen`**: Add `scroll_offset_px: f32` parameter. Pass it to
`calculate_welcome_geometry`:

```rust
fn draw_welcome_screen(
    &mut self,
    encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    view: &MetalView,
    scroll_offset_px: f32,       // new
) {
    // ...
    let mut geometry = calculate_welcome_geometry(
        content_width, content_height, glyph_width, line_height,
        scroll_offset_px,         // new
    );
    // ...
}
```

Update the call site in `render_with_editor`:

```rust
if editor.should_show_welcome_screen() {
    let scroll = editor.welcome_scroll_offset_px();
    self.draw_welcome_screen(&encoder, view, scroll);
}
```

**`draw_welcome_screen_in_pane`**: Add `scroll_offset_px: f32` parameter analogously.
Pass it to `calculate_welcome_geometry`.

Update the call site in `render_pane`, where `tab` is already in scope:

```rust
if should_show_welcome {
    let scroll = tab.welcome_scroll_offset_px();
    self.draw_welcome_screen_in_pane(encoder, view, pane_rect, scroll);
}
```

Also check `render_with_find_strip` and `render_with_confirm_dialog` for any additional
`draw_welcome_screen` call sites and update them analogously (passing
`editor.welcome_scroll_offset_px()`).

Location: `crates/editor/src/renderer.rs`

### Step 9: Add backreference comment and update GOAL.md `code_paths`

Add a backreference comment at the top of the welcome screen scroll logic in
`editor_state.rs`:

```rust
// Chunk: docs/chunks/welcome_scroll - Welcome screen vertical scrolling
```

Also add the backreference in `welcome_screen.rs` near `calculate_welcome_geometry`.

Update `docs/chunks/welcome_scroll/GOAL.md` frontmatter `code_paths`:

```yaml
code_paths:
  - crates/editor/src/welcome_screen.rs
  - crates/editor/src/workspace.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/renderer.rs
```

### Step 10: Run tests and verify

```
cargo test --package lite-edit-editor 2>&1
```

All new tests must pass. Existing tests for `calculate_welcome_geometry` must pass after
the signature update (they now pass `0.0`). The welcome screen terminal/UI behavior is
verified visually: scroll should move content, centering should be preserved on large
viewports, and the screen should still disappear on first keypress.

## Dependencies

None. All code is self-contained within the `lite-edit-editor` crate.

## Risks and Open Questions

- **Two render paths**: The welcome screen has a single-pane path (`draw_welcome_screen`)
  and a multi-pane path (`draw_welcome_screen_in_pane`). Both need the scroll offset.
  Make sure both call sites are updated (also check `render_with_find_strip` and
  `render_with_confirm_dialog` which may have their own welcome screen calls).

- **Upper-bound clamping in renderer only**: If the user scrolls far down then resizes
  the window to be very tall, the stored `welcome_scroll_offset_px` may be larger than
  the new `max_scroll = 0`. The renderer will visually clamp to centered content, which
  is correct. But if the user then scrolls down again (on what is now a large viewport),
  the stored offset needs to be high enough to "feel" scrolled. Since `max_scroll = 0`
  for a large viewport, any positive scroll is clamped to 0, so the welcome screen
  centers correctly regardless. No action needed.

- **Dirty region on scroll**: We mark `FullViewport` dirty on welcome screen scroll. This
  is correct since the welcome screen occupies the full content area.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
