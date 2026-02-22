# Implementation Plan

## Approach

This chunk fixes the ~3-row vertical offset bug when clicking inside a terminal tab running a program with mouse input (e.g., vim, htop). The bug is in `EditorState::handle_mouse_buffer`, where the terminal mouse coordinate calculation uses an inline Y-flip formula that doesn't correctly account for the coordinate system transformation.

**Root Cause Analysis:**

The current inline calculation:
```rust
let adjusted_y = self.view_height as f64 - TAB_BAR_HEIGHT as f64 - y;
let row = (adjusted_y / cell_height as f64) as usize;
```

This attempts to:
1. Flip the NSView y-coordinate (y=0 at bottom) to content-relative (y=0 at top of content)
2. Divide by cell_height to get the row

However, the formula `view_height - TAB_BAR_HEIGHT - y` produces:
- `adjusted_y = content_height` when clicking at NSView y=0 (bottom of window)
- `adjusted_y = 0` when clicking at NSView y = content_height (top of content area)

This appears mathematically correct for mapping to terminal rows where row 0 is at the top. However, the GOAL.md notes that the offset is ~3 rows while `TAB_BAR_HEIGHT / cell_height = 32 / 16 = 2`. This extra ~1 row discrepancy suggests an additional offset in the rendering that isn't accounted for.

**Key Insight from File Buffer Path:**

For file buffers, the code does NOT pre-flip the y coordinate. Instead:
1. It adjusts x by subtracting RAIL_WIDTH
2. It passes the raw y coordinate unchanged
3. It uses `content_height = view_height - TAB_BAR_HEIGHT` when creating the EditorContext
4. The flip happens inside `pixel_to_buffer_position`: `flipped_y = content_height - y`

The terminal inline calculation does the flip differently, and may not be consistent with how the terminal content is actually rendered.

**Fix Strategy:**

Align the terminal mouse calculation with the rendering coordinate system. The `TerminalFocusTarget::pixel_to_cell` function already exists and expects:
- `pixel_pos` - position in pixels from **top-left of view** (Metal-style coordinates)
- `view_origin` - origin of the terminal view in the overall window (e.g., `(RAIL_WIDTH, TAB_BAR_HEIGHT)`)

The fix will:
1. Convert NSView coordinates to Metal-style coordinates (flip y)
2. Use the correct view_origin that matches where terminal content is rendered
3. Apply the same `pixel_to_cell` logic

**Testing Philosophy Alignment:**

Following the Humble View Architecture from TESTING_PHILOSOPHY.md:
- Write a failing test that clicks at a known terminal row position
- Verify the encoded mouse event contains the expected row
- The coordinate transformation is pure math, fully testable without GPU

## Sequence

### Step 1: Write a failing test for terminal mouse row accuracy

Create a test that verifies terminal mouse click row calculation. The test should:
- Set up an EditorState with a terminal tab
- Configure known dimensions: view_height=320, TAB_BAR_HEIGHT=32, cell_height=16
- Click at the position where row N should be
- Capture the mouse event bytes sent to the PTY
- Assert the encoded row matches the expected row

The test will initially fail, demonstrating the bug.

Location: `crates/editor/src/editor_state.rs` (test module)

```rust
#[test]
fn test_terminal_mouse_click_row_accuracy() {
    use crate::tab_bar::TAB_BAR_HEIGHT;
    use crate::left_rail::RAIL_WIDTH;

    // Create editor state with terminal tab
    let mut state = create_terminal_test_state();
    state.update_viewport_dimensions(800.0, 320.0);

    // Target row 5 (0-indexed)
    // In NSView coords (y=0 at bottom), the content area is:
    //   - Top of content: y = view_height - TAB_BAR_HEIGHT = 288
    //   - Bottom of content: y = 0
    // Row 5 center is at: y = 288 - (5 + 0.5) * 16 = 288 - 88 = 200
    let target_row = 5;
    let cell_height = 16.0;
    let content_top_nsview = 320.0 - TAB_BAR_HEIGHT as f64;
    let click_y = content_top_nsview - (target_row as f64 + 0.5) * cell_height;
    let click_x = RAIL_WIDTH as f64 + 50.0; // Some column inside content

    let event = MouseEvent {
        kind: MouseEventKind::Down,
        position: (click_x, click_y),
        modifiers: Modifiers::default(),
        click_count: 1,
    };

    // Enable mouse reporting in terminal
    // ...

    // Handle mouse event (this sends encoded bytes to PTY)
    state.handle_mouse(event);

    // Capture and decode the mouse event bytes
    // Assert row == target_row
}
```

### Step 2: Investigate the exact rendering offset

Before implementing the fix, verify where terminal content is actually rendered relative to the window. Check:

1. The terminal glyph buffer's y_offset setting
2. Any additional padding or margins in the rendering
3. Whether the terminal uses the same content_y_offset as the text buffer

This will identify if there's a rendering offset beyond TAB_BAR_HEIGHT that explains the ~3 row vs ~2 row discrepancy.

Location: `crates/editor/src/renderer.rs`, `crates/editor/src/glyph_buffer.rs`

### Step 3: Fix the terminal mouse coordinate calculation

Apply the correct coordinate transformation. Two options:

**Option A: Align with file buffer approach**

Use the same pattern as file buffers - don't pre-flip, use content_height for the flip:

```rust
let (x, y) = event.position;
let adjusted_x = (x - RAIL_WIDTH as f64).max(0.0);

// Use content_height for the flip, matching how file buffers do it
let content_height = self.view_height as f64 - TAB_BAR_HEIGHT as f64;
let content_y = content_height - y;  // Flip: y=0 at bottom → y=0 at top of content

// Clamp to prevent negative values (click above content area)
let content_y = content_y.max(0.0);

let col = (adjusted_x / cell_width as f64) as usize;
let row = (content_y / cell_height as f64) as usize;
```

**Option B: Use TerminalFocusTarget::pixel_to_cell logic**

Convert to Metal-style coordinates and apply view_origin:

```rust
let (x, y) = event.position;

// Convert NSView coords to Metal-style (y=0 at top)
let metal_y = self.view_height as f64 - y;

// Subtract view_origin to get content-relative position
let content_x = (x - RAIL_WIDTH as f64).max(0.0);
let content_y = (metal_y - TAB_BAR_HEIGHT as f64).max(0.0);

let col = (content_x / cell_width as f64) as usize;
let row = (content_y / cell_height as f64) as usize;
```

Both options are mathematically equivalent. Choose Option A to match the file buffer pattern.

Location: `crates/editor/src/editor_state.rs`, lines 1326-1353

Add backreference:
```rust
// Chunk: docs/chunks/terminal_mouse_offset - Fixed terminal mouse Y coordinate calculation
```

### Step 4: Verify test passes

Run the test from Step 1:
```bash
cargo test -p lite-edit-editor test_terminal_mouse_click_row_accuracy
```

If the test still fails after the code change, there may be additional offsets to account for. Investigate:
- Font baseline offset
- Any padding in terminal content rendering
- Scale factor handling

### Step 5: Run existing tests

Ensure no regressions:
```bash
cargo test -p lite-edit-editor
cargo test -p lite-edit-terminal
cargo test -p lite-edit-input
```

Pay special attention to:
- Existing terminal mouse encoding tests
- Click-to-position tests for file buffers
- Coordinate transformation tests

### Step 6: Manual verification

Test manually with programs that use mouse input:

1. **vim test**: Open a terminal tab, run `vim`, click at various positions
   - Click at line 1 → cursor should be at line 1
   - Click at line 10 → cursor should be at line 10
   - Click at last visible line → cursor should be at that line

2. **htop test**: Run `htop`, click on process rows
   - Selection should follow click position exactly

3. **Edge cases**:
   - Click at very top of content area (row 0)
   - Click at bottom of content area (last visible row)
   - Click while terminal is scrolled (if applicable)
   - Test at different window sizes

### Step 7: Update code_paths in GOAL.md

Update the GOAL.md frontmatter to reflect the actual files modified:

```yaml
code_paths:
  - crates/editor/src/editor_state.rs
```

## Risks and Open Questions

1. **Exact source of the ~3 row offset**: The GOAL.md indicates ~3 rows but TAB_BAR_HEIGHT/cell_height = 2. If the fix in Step 3 doesn't fully resolve the issue, there may be:
   - Font baseline offset affecting visual position
   - Additional content padding not accounted for
   - Integer truncation vs floor differences

2. **Scroll offset interaction**: For terminals with scrollback, the row calculation may need to account for the current scroll position. Verify this works correctly or note as out of scope if scrollback mouse interaction isn't implemented.

3. **Scale factor handling**: The mouse coordinates are in scaled pixels. Verify the fix works correctly at both 1x and 2x (Retina) scale factors.

4. **Consistency check**: After fixing, verify that clicking the same visual position in a file tab vs terminal tab produces the same logical row (accounting for different content).
