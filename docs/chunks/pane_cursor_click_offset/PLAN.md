<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk fixes cursor positioning in non-primary panes by introducing a shared
`resolve_pane_hit()` function that uses renderer-consistent coordinate bounds.
The root cause is a coordinate space mismatch:

- **Renderer** uses bounds `(RAIL_WIDTH, 0, W, H)` — screen space
- **`handle_mouse_buffer`** uses bounds `(0, 0, W-RAIL_WIDTH, H-TAB_BAR_HEIGHT)` — content-local space

When finding which pane was clicked, `handle_mouse_buffer` correctly converts
screen coordinates to content-local coordinates, but when passing coordinates
to the buffer for hit-testing, it doesn't account for the pane's position
within the layout. For the top-left pane (origin 0,0) this works; for any
other pane the cursor lands at the wrong position.

**Strategy:**

1. Add a `resolve_pane_hit()` function to `pane_layout.rs` that:
   - Takes screen coordinates and renderer-consistent bounds
   - Returns the clicked pane, hit zone (TabBar or Content), and pane-local coordinates

2. Refactor `handle_mouse` (tab bar routing) to use `resolve_pane_hit()` instead of
   inline `calculate_pane_rects` + iteration.

3. Refactor `handle_mouse_buffer` (focus switching and cursor positioning) to use
   `resolve_pane_hit()` and correctly pass pane-local coordinates.

This follows the pattern established by `pane_tabs_interaction`, which already
fixed tab bar click routing to use renderer-consistent bounds. We're completing
the consolidation by also fixing buffer mouse handling.

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk USES renderer coordinate
  conventions. The renderer uses screen-space bounds `(RAIL_WIDTH, 0, W, H)` for
  pane layout calculation. The new `resolve_pane_hit()` function will follow this
  convention to ensure consistency.

## Sequence

### Step 1: Define HitZone enum and PaneHit struct in pane_layout.rs

**Location:** `crates/editor/src/pane_layout.rs`

Add types to represent the result of pane hit-testing:

```rust
/// Zone within a pane that was hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitZone {
    /// The tab bar region at the top of the pane
    TabBar,
    /// The content region below the tab bar
    Content,
}

/// Result of hit-testing a point against the pane layout.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneHit {
    /// ID of the pane that was hit
    pub pane_id: PaneId,
    /// Which zone within the pane was hit
    pub zone: HitZone,
    /// X coordinate relative to the pane's content origin (not screen)
    pub local_x: f32,
    /// Y coordinate relative to the pane's content origin (not screen)
    pub local_y: f32,
    /// The pane rect that was hit (for access to pane dimensions)
    pub pane_rect: PaneRect,
}
```

### Step 2: Implement resolve_pane_hit() in pane_layout.rs

**Location:** `crates/editor/src/pane_layout.rs`

Add a function that performs hit-testing against pane layout:

```rust
/// Resolves a screen-space point to a pane hit.
///
/// # Arguments
///
/// * `x` - Screen X coordinate (pixels)
/// * `y` - Screen Y coordinate (pixels, y=0 at top)
/// * `bounds` - Renderer-consistent bounds (x, y, width, height) for the pane area
/// * `pane_root` - Root of the pane layout tree
/// * `tab_bar_height` - Height of tab bar region in each pane
///
/// # Returns
///
/// `Some(PaneHit)` if the point is within a pane, `None` otherwise.
///
/// The returned `local_x` and `local_y` are relative to the pane's **content** origin,
/// which is (pane.x, pane.y + tab_bar_height) in screen space. This allows downstream
/// code to use the coordinates directly for buffer hit-testing.
pub fn resolve_pane_hit(
    x: f32,
    y: f32,
    bounds: (f32, f32, f32, f32),
    pane_root: &PaneLayoutNode,
    tab_bar_height: f32,
) -> Option<PaneHit>
```

The implementation:
1. Calls `calculate_pane_rects(bounds, pane_root)` to get pane rectangles
2. Iterates to find which pane contains the point
3. Determines if the point is in TabBar or Content zone
4. Computes pane-local coordinates:
   - For Content zone: `local_x = x - pane.x`, `local_y = y - pane.y - tab_bar_height`
   - For TabBar zone: similar but no tab_bar_height subtraction

### Step 3: Add unit tests for resolve_pane_hit()

**Location:** `crates/editor/src/pane_layout.rs` (in `mod tests`)

Add tests that verify:
- Single pane: click in tab bar returns HitZone::TabBar
- Single pane: click in content returns HitZone::Content with correct local coords
- Horizontal split: click in right pane returns correct pane_id and local coords
- Vertical split: click in bottom pane returns correct pane_id and local coords
- Click outside all panes returns None
- Local coordinates are correctly computed (x relative to pane, y relative to content)

### Step 4: Refactor handle_mouse tab bar routing to use resolve_pane_hit()

**Location:** `crates/editor/src/editor_state.rs` — `handle_mouse` method

Replace the inline pane rect iteration for tab bar detection (lines ~1784-1817)
with a call to `resolve_pane_hit()`:

```rust
// Chunk: docs/chunks/pane_cursor_click_offset - Unified pane hit resolution
{
    use crate::pane_layout::{resolve_pane_hit, HitZone};

    let is_tab_bar_click = if let Some(workspace) = self.editor.active_workspace() {
        // Renderer-consistent bounds
        let bounds = (
            RAIL_WIDTH,
            0.0,
            self.view_width - RAIL_WIDTH,
            self.view_height,
        );

        if let Some(hit) = resolve_pane_hit(
            screen_x as f32,
            screen_y as f32,
            bounds,
            &workspace.pane_root,
            TAB_BAR_HEIGHT,
        ) {
            hit.zone == HitZone::TabBar
        } else {
            false
        }
    } else {
        false
    };

    // ... rest unchanged
}
```

### Step 5: Refactor handle_mouse_buffer to use resolve_pane_hit()

**Location:** `crates/editor/src/editor_state.rs` — `handle_mouse_buffer` method

This is the key fix. Currently the code:
1. Computes pane rects with content-local bounds `(0, 0, content_width, content_height)`
2. Converts screen coords to content-local for hit testing
3. Uses the same content-local coordinates for buffer hit-testing

The bug: coordinates passed to the buffer should be relative to the *pane's*
content origin, not the overall content area origin.

**Fix:**
1. Use renderer-consistent bounds `(RAIL_WIDTH, 0, W, H)` for hit-testing
2. Use `resolve_pane_hit()` which returns pane-local coordinates
3. Pass pane-local coordinates to the buffer for cursor positioning

```rust
// Chunk: docs/chunks/pane_cursor_click_offset - Fixed coordinate transformation
fn handle_mouse_buffer(&mut self, event: MouseEvent) {
    use crate::input::MouseEventKind;
    use crate::pane_layout::{resolve_pane_hit, HitZone};

    self.last_keystroke = Instant::now();
    let (screen_x, screen_y) = event.position;

    // Renderer-consistent bounds for pane layout
    let bounds = (
        RAIL_WIDTH,
        0.0,
        self.view_width - RAIL_WIDTH,
        self.view_height,
    );

    // Resolve which pane was hit and get pane-local coordinates
    let hit = if let Some(workspace) = self.editor.active_workspace() {
        resolve_pane_hit(
            screen_x as f32,
            screen_y as f32,
            bounds,
            &workspace.pane_root,
            TAB_BAR_HEIGHT,
        )
    } else {
        None
    };

    // Click-to-focus pane switching (on MouseDown in Content zone)
    if let MouseEventKind::Down = event.kind {
        if let Some(ref hit) = hit {
            if hit.zone == HitZone::Content {
                if let Some(ws) = self.editor.active_workspace_mut() {
                    if hit.pane_id != ws.active_pane_id {
                        ws.active_pane_id = hit.pane_id;
                        self.dirty_region.merge(DirtyRegion::FullViewport);
                    }
                }
            }
        }
    }

    // Get the (potentially updated) active tab
    let ws = self.editor.active_workspace_mut().expect("no active workspace");
    let tab = ws.active_tab_mut().expect("no active tab");

    // Use pane-local coordinates from hit resolution
    // These are already relative to the pane's content origin
    let (content_x, content_y) = if let Some(ref hit) = hit {
        (hit.local_x as f64, hit.local_y as f64)
    } else {
        // Fallback for clicks outside panes (shouldn't happen in normal use)
        let content_x = (screen_x - RAIL_WIDTH as f64).max(0.0);
        let content_y = (screen_y - TAB_BAR_HEIGHT as f64).max(0.0);
        (content_x, content_y)
    };

    // ... rest of the method continues unchanged, using content_x, content_y
```

### Step 6: Update terminal mouse coordinate handling

**Location:** `crates/editor/src/editor_state.rs` — `handle_mouse_buffer` method

The terminal branch also needs to use pane-local coordinates. Verify that the
`content_x` and `content_y` computed from `hit.local_x` and `hit.local_y` are
correctly used for terminal cell position calculation.

The existing terminal code uses:
```rust
let col = (content_x / cell_width as f64) as usize;
let row = (adjusted_y / cell_height as f64) as usize;
```

This should work correctly once `content_x` and `content_y` are pane-local.

### Step 7: Add integration tests for cursor positioning in split layouts

**Location:** `crates/editor/src/editor_state.rs` — `mod tests`

Add tests that verify the full dispatch path:

```rust
#[test]
fn test_cursor_click_right_pane_horizontal_split() {
    // Setup: HSplit(Pane[file1], Pane[file2])
    // Click in right pane at (center of right pane)
    // Assert: cursor position in file2 is correct (not offset)
}

#[test]
fn test_cursor_click_bottom_pane_vertical_split() {
    // Setup: VSplit(Pane[file1], Pane[file2])
    // Click in bottom pane
    // Assert: cursor position in file2 is correct (not offset downward)
}

#[test]
fn test_cursor_click_top_left_pane_split() {
    // Setup: HSplit(Pane[file1], Pane[file2])
    // Click in left pane
    // Assert: no regression, cursor correct
}

#[test]
fn test_click_switches_focus_to_right_pane() {
    // Setup: HSplit with focus on left pane
    // Click in right pane content area
    // Assert: active_pane_id switches to right pane
}
```

### Step 8: Verify existing pane_tabs_interaction tests pass

Run `cargo test` to ensure all existing tests continue to pass, particularly
the tests from `pane_tabs_interaction` that verify tab bar click routing.

### Step 9: Update code_paths in GOAL.md

Update the chunk's GOAL.md frontmatter with the files touched:
```yaml
code_paths:
- crates/editor/src/pane_layout.rs
- crates/editor/src/editor_state.rs
```

## Risks and Open Questions

1. **Multi-pane terminal tabs**: The terminal coordinate handling has its own
   scroll fraction adjustment. Need to verify this still works correctly with
   pane-local coordinates.

2. **Mouse drag selection**: Drag events also go through `handle_mouse_buffer`.
   Need to ensure drag coordinates are also correctly transformed for
   non-primary panes.

3. **EditorContext view_width/view_height**: The `EditorContext` is created with
   content dimensions. We need to ensure it receives the pane's content dimensions,
   not the overall content area dimensions. This may require passing pane dimensions
   from the `PaneHit` result.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->