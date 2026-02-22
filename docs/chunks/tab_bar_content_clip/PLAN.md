# Implementation Plan

## Approach

Clip the buffer content rendering pass using Metal's scissor rect API. When the
buffer is scrolled near the top, glyphs are rendered at y-coordinates that
overlap the tab bar region. The scissor rect constrains fragment output to the
area below `TAB_BAR_HEIGHT`, preventing buffer content from bleeding into the
tab bar.

**Strategy:**

This follows the exact pattern established by `selector_list_clipping`:

1. In `render_with_editor`, after drawing the tab bar but **before** drawing the
   buffer content, set a scissor rect that excludes the tab bar region: from
   `TAB_BAR_HEIGHT` to the bottom of the viewport.

2. Draw the buffer content (glyphs, selection, cursor, etc.) with the scissor
   rect active.

3. Reset the scissor rect to the full viewport after the buffer content draw
   so subsequent rendering (selector overlay) is unaffected.

**Metal API used:**

- `MTLRenderCommandEncoder::setScissorRect(MTLScissorRect)` — sets the clipping
  rectangle in pixel coordinates (origin at top-left, Y increases downward).

- Reuse the existing `full_viewport_scissor_rect` helper for resetting.

**No changes to:**

- `GlyphBuffer` or `glyph_buffer.rs` — vertex generation is unchanged.
- `TabBarGlyphBuffer` or tab bar rendering — it renders before the scissor.
- `Viewport` or scroll calculations — the scissor is a purely GPU-side clip.
- Buffer model or cursor positioning — no model changes.

**Testing:**

Per the project's Humble View Architecture (TESTING_PHILOSOPHY.md), scissor rect
application is a renderer-side concern in the humble view layer. It cannot be
meaningfully unit-tested without a GPU. Visual verification will confirm:
- Buffer content never appears above `TAB_BAR_HEIGHT`.
- Tab bar labels and close buttons remain fully visible.
- No regression at normal scroll positions (away from top).

Existing geometry tests in `tab_bar.rs` and `glyph_buffer.rs` remain valid.

## Sequence

### Step 1: Add helper function for buffer content scissor rect

Create a helper function in `renderer.rs` that computes the scissor rect for
the buffer content area. The rect excludes the tab bar by starting at
`TAB_BAR_HEIGHT` and extending to the bottom of the viewport.

```rust
// Chunk: docs/chunks/tab_bar_content_clip - Clip buffer content below tab bar
/// Creates a scissor rect for clipping buffer content to the area below the tab bar.
///
/// The rect starts at `TAB_BAR_HEIGHT` and extends to the bottom of the viewport,
/// preventing buffer content from bleeding into the tab bar region.
fn buffer_content_scissor_rect(
    tab_bar_height: f32,
    view_width: f32,
    view_height: f32,
) -> MTLScissorRect {
    // Y coordinate: tab_bar_height (top of buffer region)
    let y = (tab_bar_height as usize).min(view_height as usize);

    // Height: from tab_bar_height to bottom of viewport
    let height = (view_height as usize).saturating_sub(y);

    MTLScissorRect {
        x: 0,
        y,
        width: view_width as usize,
        height,
    }
}
```

Location: `crates/editor/src/renderer.rs` (in the "Scissor Rect Helpers" section,
after `full_viewport_scissor_rect`)

### Step 2: Modify render_with_editor to apply scissor rect

Update `render_with_editor` to bracket the buffer content draw call with
scissor rect changes:

1. After `draw_tab_bar(&encoder, view, editor)` completes, apply the buffer
   content scissor rect.

2. Draw the buffer content (existing `render_text` call).

3. Reset the scissor rect to full viewport before drawing the selector overlay.

The modified code structure in `render_with_editor`:

```rust
// Draw tab bar at top of content area
self.draw_tab_bar(&encoder, view, editor);

// Chunk: docs/chunks/tab_bar_content_clip - Clip buffer content to area below tab bar
// Apply scissor rect to prevent buffer text from bleeding into tab bar region.
let content_scissor = buffer_content_scissor_rect(TAB_BAR_HEIGHT, view_width, view_height);
encoder.setScissorRect(content_scissor);

// Render editor text content (offset by RAIL_WIDTH and TAB_BAR_HEIGHT)
if self.glyph_buffer.index_count() > 0 {
    self.render_text(&encoder, view);
}

// Chunk: docs/chunks/tab_bar_content_clip - Reset scissor for selector overlay
// Restore full viewport scissor so selector overlay renders correctly.
let full_scissor = full_viewport_scissor_rect(view_width, view_height);
encoder.setScissorRect(full_scissor);

// Render selector overlay on top if active
if let Some(widget) = selector {
    self.draw_selector_overlay(&encoder, view, widget, selector_cursor_visible);
}
```

Location: `crates/editor/src/renderer.rs#render_with_editor`

### Step 3: Extract view dimensions earlier in render_with_editor

The scissor rect helper needs `view_width` and `view_height`. Currently these
values are computed locally within `draw_tab_bar`. Extract them to the top of
`render_with_editor` so they're available for the scissor rect calculation.

```rust
// Get view dimensions for scissor rect calculation
let frame = view.frame();
let scale = view.scale_factor();
let view_width = (frame.size.width * scale) as f32;
let view_height = (frame.size.height * scale) as f32;
```

Location: `crates/editor/src/renderer.rs#render_with_editor` (early in the method)

### Step 4: Update code_paths in GOAL.md

Add `crates/editor/src/renderer.rs` to the `code_paths` field in the chunk's
GOAL.md frontmatter.

Location: `docs/chunks/tab_bar_content_clip/GOAL.md`

### Step 5: Visual verification

Build and run the editor. With the tab bar visible:

1. Open a file with content.
2. Scroll the buffer to the very top (line 1 visible at top of content area).
3. Continue scrolling until the buffer's first line would be positioned at y=0.
4. Verify that no buffer text, cursor, or gutter pixels appear above the tab bar.
5. Verify that tab bar labels and close buttons remain fully visible and legible.
6. Scroll to a normal position and verify buffer renders correctly.
7. Open the selector (Cmd+P) and verify it renders correctly (not clipped).

This is a manual verification step per the project's Humble View Architecture.

### Step 6: Run existing tests

Run all existing tests to ensure no regressions:

```bash
cargo test -p editor
```

All tests should pass — this change is renderer-only and does not affect
geometry calculations or buffer behavior tested in existing unit tests.

## Dependencies

- **content_tab_bar** (ACTIVE, parent chunk): Provides `TAB_BAR_HEIGHT` constant
  and the tab bar rendering infrastructure. This chunk fixes a visual artifact
  that `content_tab_bar` did not address.

- **selector_list_clipping** (ACTIVE): Provides the pattern for scissor rect
  clipping. The helper function `full_viewport_scissor_rect` is reused.

Both dependencies are satisfied.

## Risks and Open Questions

1. **Scissor rect coordinate system:** Metal uses top-left origin with Y
   increasing downward, matching our screen coordinate system. This is the
   same system used by `selector_list_clipping`, so the pattern is validated.

2. **Left rail clipping:** The buffer content scissor rect starts at x=0, which
   includes the left rail region. This is intentional — the left rail renders
   before the scissor is applied, and the buffer content already starts at
   `RAIL_WIDTH` due to the x_offset. No issue expected.

3. **Selector overlay interaction:** The scissor rect is reset before drawing
   the selector overlay, so the file picker and command palette will render
   correctly over the full viewport.

4. **Performance:** `setScissorRect` is a trivial GPU state change with
   negligible cost. No performance impact expected.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->