<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk fixes the find-in-file (Cmd+F) rendering bug where the focused pane
expands to fill the entire window in multi-pane layouts. The root cause is that
`render_with_find_strip()` is a separate rendering path that has no awareness of
the pane tree — it renders as if there's only one pane.

**Strategy**: Eliminate `render_with_find_strip()` entirely and fold find strip
rendering into `render_with_editor()`. The find strip state will be passed as an
optional parameter, mirroring the existing `selector: Option<&SelectorWidget>`
pattern.

Key insights from code exploration:

1. `render_with_editor()` already handles both single-pane and multi-pane layouts:
   - Single-pane: calls `draw_tab_bar()` and `render_text()` directly
   - Multi-pane: iterates over pane rects and calls `render_pane()` for each

2. `render_with_find_strip()` bypasses all this — it renders the left rail, editor
   content, and find strip as if the window had only one pane. This explains why
   the focused pane "expands to fill the screen."

3. The fix requires:
   - Adding find strip state as an optional parameter to `render_with_editor()`
   - In single-pane mode: drawing the find strip at the bottom of the content area
   - In multi-pane mode: drawing the find strip at the bottom of the focused pane

4. The find strip geometry calculation (`calculate_find_strip_geometry()`) must be
   updated to accept pane bounds rather than assuming full viewport dimensions.

Tests will verify:
- Find strip renders within pane bounds (not full viewport) in multi-pane mode
- All other panes remain visible and correctly positioned during find
- Single-pane behavior is unchanged
- Find strip works correctly in panes of different sizes (horizontal/vertical splits)

Following the project's Humble View Architecture, testing focuses on the geometry
calculations (testable pure Rust) rather than Metal rendering (humble object).

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk IMPLEMENTS a fix within the
  renderer subsystem, consolidating the find strip into the main render_with_editor
  path. The existing invariants are preserved:
  - Single Frame Contract: Still one complete frame per render call
  - Layering Contract: Find strip still renders on top of editor content
  - Scissor Rects for Containment: Will apply scissor rect to clip find strip to pane

## Sequence

### Step 1: Create `FindStripState` struct for passing find strip info

**Location**: `crates/editor/src/selector_overlay.rs`

Create a small struct that bundles the find strip state needed for rendering:

```rust
/// State needed to render the find strip overlay.
///
/// This is passed to render_with_editor to enable find strip rendering
/// within pane bounds (rather than full viewport).
pub struct FindStripState<'a> {
    /// The current query text
    pub query: &'a str,
    /// Cursor column position in the query
    pub cursor_col: usize,
    /// Whether the cursor is currently visible (for blinking)
    pub cursor_visible: bool,
}
```

This mirrors how `selector: Option<&SelectorWidget>` is passed to enable selector
overlay rendering.

### Step 2: Add pane-aware find strip geometry calculation

**Location**: `crates/editor/src/selector_overlay.rs`

Create a new function `calculate_find_strip_geometry_in_pane()` that calculates
find strip geometry within a pane's bounds:

```rust
pub fn calculate_find_strip_geometry_in_pane(
    pane_x: f32,
    pane_y: f32,
    pane_width: f32,
    pane_height: f32,
    line_height: f32,
    glyph_width: f32,
    cursor_col: usize,
) -> FindStripGeometry
```

The existing `calculate_find_strip_geometry()` assumes full viewport, so this
new function will:
- Position `strip_y` relative to the pane bottom (not viewport bottom)
- Position `strip_x` at the pane left edge (not 0)
- Constrain `strip_width` to the pane width (not viewport width)

The existing function can be refactored to call this new one with full viewport
bounds for backwards compatibility in single-pane mode.

### Step 3: Update `render_with_editor()` signature to accept find strip state

**Location**: `crates/editor/src/renderer.rs`

Change the signature from:
```rust
pub fn render_with_editor(
    &mut self,
    view: &MetalView,
    editor: &Editor,
    selector: Option<&SelectorWidget>,
    selector_cursor_visible: bool,
)
```

To:
```rust
pub fn render_with_editor(
    &mut self,
    view: &MetalView,
    editor: &Editor,
    selector: Option<&SelectorWidget>,
    selector_cursor_visible: bool,
    find_strip: Option<FindStripState<'_>>,
)
```

Initially, pass `None` at all call sites to maintain current behavior.

### Step 4: Implement find strip rendering in single-pane mode

**Location**: `crates/editor/src/renderer.rs`

In the single-pane branch of `render_with_editor()` (where `pane_rects.len() <= 1`),
after drawing the welcome screen or buffer content, check if `find_strip` is
`Some` and call `draw_find_strip()`:

```rust
// After buffer content rendering:
if let Some(ref find_state) = find_strip {
    self.draw_find_strip(
        &encoder,
        view,
        &find_state.query,
        find_state.cursor_col,
        find_state.cursor_visible,
    );
}
```

This replaces what `render_with_find_strip()` did for single-pane mode.

### Step 5: Implement find strip rendering in multi-pane mode

**Location**: `crates/editor/src/renderer.rs`

In the multi-pane branch (where `pane_rects.len() > 1`), after rendering all panes,
find the focused pane's rect and draw the find strip within its bounds:

```rust
if let Some(ref find_state) = find_strip {
    // Find the focused pane's rect
    if let Some(focused_rect) = pane_rects.iter().find(|r| r.pane_id == focused_pane_id) {
        self.draw_find_strip_in_pane(
            &encoder,
            view,
            &find_state.query,
            find_state.cursor_col,
            find_state.cursor_visible,
            focused_rect,
            view_width,
            view_height,
        );
    }
}
```

This ensures the find strip appears within the focused pane's bounds.

### Step 6: Create `draw_find_strip_in_pane()` method

**Location**: `crates/editor/src/renderer.rs`

Create a new method that draws the find strip within pane bounds:

```rust
fn draw_find_strip_in_pane(
    &mut self,
    encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    view: &MetalView,
    query: &str,
    cursor_col: usize,
    cursor_visible: bool,
    pane_rect: &PaneRect,
    view_width: f32,
    view_height: f32,
)
```

This method will:
1. Set scissor rect to the pane's bounds
2. Calculate find strip geometry within the pane using `calculate_find_strip_geometry_in_pane()`
3. Update the find strip buffer and issue draw calls
4. Restore the previous scissor rect

### Step 7: Update `render_pane()` to reserve space for find strip

**Location**: `crates/editor/src/renderer.rs`

When the find strip is active, the content area within the focused pane should be
reduced by one line height to prevent content from being hidden behind the strip.
This means the `content_scissor` should be adjusted when find is active.

Note: This step may be optional if we decide the find strip can overlap content
(simpler implementation). Document the decision during implementation.

### Step 8: Update call sites in `drain_loop.rs`

**Location**: `crates/editor/src/drain_loop.rs`

In `render_if_dirty()`, replace the `EditorFocus::FindInFile` branch:

**Before** (calls `render_with_find_strip`):
```rust
EditorFocus::FindInFile => {
    self.renderer.set_cursor_visible(self.state.cursor_visible);
    if let Some(ref mini_buffer) = self.state.find_mini_buffer {
        self.renderer.render_with_find_strip(
            &self.metal_view,
            &self.state.editor,
            &mini_buffer.content(),
            mini_buffer.cursor_col(),
            self.state.overlay_cursor_visible,
        );
    }
}
```

**After** (calls `render_with_editor` with find_strip parameter):
```rust
EditorFocus::FindInFile => {
    self.renderer.set_cursor_visible(self.state.cursor_visible);
    let find_strip = self.state.find_mini_buffer.as_ref().map(|mb| {
        FindStripState {
            query: &mb.content(),
            cursor_col: mb.cursor_col(),
            cursor_visible: self.state.overlay_cursor_visible,
        }
    });
    self.renderer.render_with_editor(
        &self.metal_view,
        &self.state.editor,
        None,
        self.state.cursor_visible,
        find_strip,
    );
}
```

### Step 9: Update other call sites to pass `None` for find_strip

**Location**: `crates/editor/src/drain_loop.rs`

Update the `EditorFocus::Buffer`, `EditorFocus::Selector`, and
`EditorFocus::ConfirmDialog` branches to pass `None` for the find_strip parameter:

```rust
EditorFocus::Buffer => {
    self.renderer.render_with_editor(
        &self.metal_view,
        &self.state.editor,
        None,
        self.state.cursor_visible,
        None,  // No find strip
    );
}
```

### Step 10: Remove `render_with_find_strip()` method

**Location**: `crates/editor/src/renderer.rs`

Delete the entire `render_with_find_strip()` method (approximately lines 1993-2108).
This method is now obsolete since all find strip rendering goes through
`render_with_editor()`.

### Step 11: Unit tests for pane-aware find strip geometry

**Location**: `crates/editor/src/selector_overlay.rs` (test module)

Add tests for `calculate_find_strip_geometry_in_pane()`:

```rust
#[test]
fn test_find_strip_geometry_in_pane_positions_at_pane_bottom() {
    let geometry = calculate_find_strip_geometry_in_pane(
        100.0,  // pane_x
        50.0,   // pane_y
        400.0,  // pane_width
        300.0,  // pane_height
        16.0,   // line_height
        8.0,    // glyph_width
        0,      // cursor_col
    );

    // strip_y should be at bottom of pane, not viewport
    // pane bottom = pane_y + pane_height = 50 + 300 = 350
    // strip_y = pane_bottom - strip_height
    let expected_strip_y = 50.0 + 300.0 - geometry.strip_height;
    assert_eq!(geometry.strip_y, expected_strip_y);

    // strip_x should start at pane_x, not 0
    assert_eq!(geometry.strip_x, 100.0);

    // strip_width should be pane_width, not viewport width
    assert_eq!(geometry.strip_width, 400.0);
}

#[test]
fn test_find_strip_geometry_in_small_pane() {
    // Test that geometry is correct even in a small pane (e.g., 1/4 of screen)
    let geometry = calculate_find_strip_geometry_in_pane(
        800.0,  // right side pane
        0.0,
        200.0,  // narrow pane
        600.0,
        16.0,
        8.0,
        5,      // cursor at position 5
    );

    // Verify cursor position is within pane bounds
    assert!(geometry.cursor_x >= 800.0);
    assert!(geometry.cursor_x < 1000.0);
}
```

### Step 12: Integration test for multi-pane find strip

**Location**: `crates/editor/tests/` (new test file or existing)

This test verifies the integration at the `render_with_editor` level. Since we
can't directly test Metal rendering, we verify that:
- The find strip state is correctly passed through
- The focused pane is correctly identified

Note: The actual rendering is a humble object and not unit tested per the
project's testing philosophy.

### Step 13: Manual verification

Manually verify the fix:

1. Open lite-edit
2. Split the window into multiple panes (Cmd+Shift+D or similar)
3. Focus one of the panes
4. Press Cmd+F to open find
5. **Verify**: All panes remain visible; find strip appears only in focused pane
6. Type a search query
7. **Verify**: Search highlighting works within the focused pane
8. Press Escape to dismiss
9. **Verify**: All panes return to normal state with no visual glitch
10. Repeat with different split configurations (horizontal, vertical, nested)

## Risks and Open Questions

1. **Content overlap vs. viewport reduction**: The find strip could either:
   - Overlap the bottom line of content (simpler)
   - Reduce the visible content area by one line (more correct)

   The original `render_with_find_strip` overlapped content. Decide during
   implementation whether to preserve this behavior or improve it.

2. **Scissor rect interactions**: When drawing the find strip in a pane, we need
   to ensure the scissor rect is correctly set to clip to pane bounds. If the
   find strip buffer generates quads outside the pane, they must be clipped.

3. **FindStripState lifetime**: The `FindStripState` struct holds references to
   the query string. Verify that the lifetime works correctly when constructing
   it from `MiniBuffer::content()` (which returns a `String`). May need to store
   the content in a local variable before constructing the state.

4. **Tab bar interaction**: In multi-pane mode, each pane has its own tab bar.
   The find strip should appear below the content area, not overlapping the tab
   bar. Verify this works correctly.

## Deviations

_To be populated during implementation._