<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk adds a confirmation dialog for closing dirty tabs using the same architectural patterns as the existing `SelectorWidget` and find strip:

1. **Humble View Architecture**: Pure state struct (`ConfirmDialog`) with no platform dependencies. All layout calculations are pure functions that can be unit tested without Metal.

2. **Focus state routing**: New `EditorFocus::ConfirmDialog` variant that routes keyboard input to the dialog.

3. **Overlay rendering**: Reuse the existing selector overlay rendering infrastructure (`SelectorGlyphBuffer` patterns) for the dialog panel.

4. **Pending action tracking**: `EditorState` gains a `pending_close: Option<(PaneId, usize)>` field to remember which tab triggered the dialog while it's displayed.

The implementation follows the established patterns:
- Focus transitions (like `handle_cmd_f` and `open_selector`)
- Overlay geometry calculation (like `calculate_overlay_geometry`)
- Keyboard handling through the focus system (like `SelectorWidget::handle_key`)

Testing follows the TDD approach in `TESTING_PHILOSOPHY.md`: write failing tests first for the pure logic (dialog state, geometry, key handling), then implement.

## Sequence

### Step 1: Define ConfirmDialog widget (TDD)

Create `crates/editor/src/confirm_dialog.rs` with a pure state struct and key handling.

**Tests first** (in `#[cfg(test)]` module):
- `test_new_dialog_has_cancel_selected_by_default`
- `test_tab_toggles_selection_to_abandon`
- `test_tab_toggles_selection_back_to_cancel`
- `test_left_selects_cancel`
- `test_right_selects_abandon`
- `test_enter_on_cancel_returns_cancelled`
- `test_enter_on_abandon_returns_confirmed`
- `test_escape_always_returns_cancelled`

**Implementation**:
```rust
// Chunk: docs/chunks/dirty_tab_close_confirm - Confirm dialog widget

/// Which button is currently selected in the confirm dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfirmButton {
    #[default]
    Cancel,
    Abandon,
}

/// Outcome of handling a key event in the confirm dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmOutcome {
    /// User pressed Enter with Cancel selected, or Escape
    Cancelled,
    /// User pressed Enter with Abandon selected
    Confirmed,
    /// Dialog is still open
    Pending,
}

/// A confirmation dialog widget for binary yes/no decisions.
#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    /// The prompt message (e.g., "Abandon unsaved changes?")
    pub prompt: String,
    /// Currently selected button (default: Cancel)
    pub selected: ConfirmButton,
}

impl ConfirmDialog {
    pub fn new(prompt: impl Into<String>) -> Self { ... }
    pub fn handle_key(&mut self, event: &KeyEvent) -> ConfirmOutcome { ... }
}
```

Location: `crates/editor/src/confirm_dialog.rs`

### Step 2: Add ConfirmDialog geometry calculation (TDD)

Add pure geometry calculation for the dialog overlay to `confirm_dialog.rs`.

**Tests first**:
- `test_dialog_geometry_centered_horizontally`
- `test_dialog_geometry_centered_vertically`
- `test_dialog_geometry_has_correct_button_positions`
- `test_dialog_geometry_with_small_viewport`

**Implementation**:
```rust
/// Computed geometry for the confirm dialog overlay
#[derive(Debug, Clone, Copy)]
pub struct ConfirmDialogGeometry {
    pub panel_x: f32,
    pub panel_y: f32,
    pub panel_width: f32,
    pub panel_height: f32,
    pub prompt_x: f32,
    pub prompt_y: f32,
    pub cancel_button_x: f32,
    pub abandon_button_x: f32,
    pub buttons_y: f32,
    pub button_width: f32,
    pub button_height: f32,
}

pub fn calculate_confirm_dialog_geometry(
    view_width: f32,
    view_height: f32,
    line_height: f32,
    glyph_width: f32,
) -> ConfirmDialogGeometry { ... }
```

The dialog should be:
- Horizontally centered
- Vertically centered (or ~40% from top)
- Wide enough for the prompt and two buttons side by side
- Two lines tall: prompt row + buttons row (plus padding)

### Step 3: Add EditorFocus::ConfirmDialog variant

Update `crates/editor/src/editor_state.rs`:

1. Add the variant to the `EditorFocus` enum:
```rust
pub enum EditorFocus {
    Buffer,
    Selector,
    FindInFile,
    ConfirmDialog,  // New
}
```

2. Add fields to `EditorState`:
```rust
/// The active confirm dialog (when focus == ConfirmDialog)
pub confirm_dialog: Option<ConfirmDialog>,
/// The tab (pane_id, tab_index) that triggered the confirm dialog
pub pending_close: Option<(PaneId, usize)>,
```

Location: `crates/editor/src/editor_state.rs`

### Step 4: Modify close_tab to show dialog for dirty tabs (TDD)

**Tests first** (add to `editor_state.rs` tests):
- `test_close_dirty_tab_opens_confirm_dialog`
- `test_close_dirty_tab_sets_pending_close`
- `test_close_dirty_tab_sets_focus_to_confirm_dialog`
- `test_close_clean_tab_still_closes_immediately`

**Implementation**:

Modify `close_tab()` to check the dirty flag and either:
- Close immediately (clean tab) - existing behavior
- Open a confirm dialog (dirty tab) - new behavior

```rust
// In close_tab():
if tab.dirty {
    // Open confirm dialog instead of returning
    self.confirm_dialog = Some(ConfirmDialog::new("Abandon unsaved changes?"));
    self.pending_close = Some((pane_id, index));
    self.focus = EditorFocus::ConfirmDialog;
    self.dirty_region.merge(DirtyRegion::FullViewport);
    return;
}
```

### Step 5: Add confirm dialog key handling to handle_key (TDD)

**Tests first**:
- `test_confirm_dialog_escape_closes_dialog_keeps_tab`
- `test_confirm_dialog_enter_on_cancel_closes_dialog_keeps_tab`
- `test_confirm_dialog_tab_then_enter_closes_tab`

**Implementation**:

Add a new match arm in `EditorState::handle_key()` for `EditorFocus::ConfirmDialog`:

```rust
EditorFocus::ConfirmDialog => {
    if let Some(dialog) = &mut self.confirm_dialog {
        match dialog.handle_key(&event) {
            ConfirmOutcome::Cancelled => {
                self.close_confirm_dialog();
            }
            ConfirmOutcome::Confirmed => {
                // Close the tab and the dialog
                if let Some((pane_id, tab_idx)) = self.pending_close.take() {
                    self.force_close_tab(pane_id, tab_idx);
                }
                self.close_confirm_dialog();
            }
            ConfirmOutcome::Pending => {
                self.dirty_region.merge(DirtyRegion::FullViewport);
            }
        }
    }
}
```

Add helper methods:
- `close_confirm_dialog()` - clears dialog state, resets focus to Buffer, marks dirty
- `force_close_tab(pane_id, tab_idx)` - closes the tab without checking dirty flag

### Step 6: Add ConfirmDialogGlyphBuffer for rendering

Create the rendering buffer in `crates/editor/src/selector_overlay.rs` (or new file if it gets too large).

Follow the same pattern as `SelectorGlyphBuffer`:
- Background rect
- Prompt text
- Two button rects (Cancel, Abandon)
- Button labels
- Selection highlight on the selected button

```rust
pub struct ConfirmDialogGlyphBuffer {
    vertex_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    index_count: usize,
    layout: GlyphLayout,
    // Quad ranges
    background_range: QuadRange,
    prompt_text_range: QuadRange,
    cancel_button_range: QuadRange,
    cancel_label_range: QuadRange,
    abandon_button_range: QuadRange,
    abandon_label_range: QuadRange,
    selection_highlight_range: QuadRange,
}

impl ConfirmDialogGlyphBuffer {
    pub fn new(layout: GlyphLayout) -> Self { ... }
    pub fn update_from_dialog(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        dialog: &ConfirmDialog,
        geometry: &ConfirmDialogGeometry,
    ) { ... }
}
```

Location: `crates/editor/src/selector_overlay.rs`

### Step 7: Add dialog rendering to Renderer

Update `crates/editor/src/renderer.rs`:

1. Add a `confirm_dialog_buffer: Option<ConfirmDialogGlyphBuffer>` field to `Renderer`.

2. Add a `draw_confirm_dialog()` method similar to `draw_selector_overlay()`.

3. In `render_with_editor()`, after rendering the selector overlay (or instead of, since ConfirmDialog should block Selector):
```rust
if let (Some(dialog), EditorFocus::ConfirmDialog) = (&state.confirm_dialog, state.focus) {
    self.draw_confirm_dialog(&encoder, view, dialog);
}
```

### Step 8: Add focus blocking for other overlays

When `EditorFocus::ConfirmDialog` is active, other overlays (file picker, find strip) should be blocked.

Update `handle_cmd_p` and `handle_cmd_f` to early-return if `focus == ConfirmDialog`.

**Test**:
- `test_cmd_p_blocked_during_confirm_dialog`
- `test_cmd_f_blocked_during_confirm_dialog`

### Step 9: Wire up in drain_loop.rs

Update `crates/editor/src/drain_loop.rs` to handle mouse events for the confirm dialog focus mode (no-op for this chunk since mouse click is out of scope).

Add a match arm for `EditorFocus::ConfirmDialog` in the scroll handling (no-op).

### Step 10: Integration test and clippy

Run the full test suite and clippy:
```bash
cargo test -p lite-edit
cargo clippy -p lite-edit -- -D warnings
```

Verify manual testing:
1. Open a file, type some text (dirty flag set)
2. Press Cmd+W to close
3. Dialog appears with "Abandon unsaved changes?" and Cancel/Abandon buttons
4. Press Escape → dialog closes, tab remains open
5. Press Cmd+W again, dialog reappears
6. Press Tab, then Enter → tab closes
7. Verify clean tabs still close immediately

## Risks and Open Questions

- **Button layout**: The GOAL specifies Tab/Left/Right for toggling between buttons. Need to verify this feels natural (Tab for cycling, arrow keys for direct navigation).

- **Dialog blocking**: When the dialog is open, the user cannot interact with the buffer beneath or open other overlays. This is intentional but should be verified as the expected UX.

- **PaneId type**: The goal mentions `pending_close: Option<(PaneId, usize)>`. Need to verify `PaneId` is the correct type from the pane_layout module and is Copy/Clone.

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