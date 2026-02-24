<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk generalizes the confirmation dialog from `dirty_tab_close_confirm` to support multiple use cases through a context/action enum pattern. The key insight is that the current `pending_close: Option<(PaneId, usize)>` field tightly couples the dialog to tab-close, making it impossible to reuse for quit confirmation, reload prompts, or other binary decisions.

**Strategy:**

1. **Replace `pending_close` with a `ConfirmDialogContext` enum** that captures what triggered the dialog and what action should occur on confirmation. Each variant contains the data needed for its specific outcome handler.

2. **Route outcomes through a match on context** instead of hardcoding the close-tab behavior. When `ConfirmOutcome::Confirmed` occurs, we match on the context variant and dispatch to the appropriate handler.

3. **Add mouse click support** by implementing hit testing on button rects in `handle_mouse` when `EditorFocus::ConfirmDialog` is active. The existing `ConfirmDialogGeometry` already has button positions.

4. **Parameterize button labels** to support different prompts (e.g., "Yes/No" vs "Cancel/Abandon"). This is a small extension to `ConfirmDialog` and `ConfirmDialogGeometry`.

**Building on existing code:**

- `ConfirmDialog` widget from `confirm_dialog.rs` — extend with parameterized button labels
- `ConfirmDialogGeometry` — extend to return button hit rects
- `EditorState::handle_mouse` routing pattern from `handle_tab_bar_click` and selector mouse handling
- Hit testing pattern from `TabRect::contains` and `CloseButtonRect::contains`

Following the Humble View Architecture: the `ConfirmDialogContext` enum and hit testing are pure state/logic that can be unit tested without Metal or macOS. Only the rendering remains in the humble view.

## Subsystem Considerations

No subsystems are currently documented that relate to this chunk. The confirm dialog widget is self-contained and follows the project's Humble View Architecture as documented in TESTING_PHILOSOPHY.md.

## Sequence

### Step 1: Define `ConfirmDialogContext` enum

Create an enum in `confirm_dialog.rs` that captures the context/intent of the dialog:

```rust
/// Context for what triggered the confirm dialog and what action to take on confirmation.
#[derive(Debug, Clone)]
pub enum ConfirmDialogContext {
    /// Closing a tab with unsaved changes
    CloseDirtyTab {
        pane_id: PaneId,
        tab_idx: usize,
    },
    /// Quitting the application with dirty tabs
    QuitWithDirtyTabs {
        /// Number of dirty tabs (for display in prompt)
        dirty_count: usize,
    },
}
```

This replaces the semantic coupling of `pending_close: Option<(PaneId, usize)>` with a typed discriminant that can be extended for future use cases.

Location: `crates/editor/src/confirm_dialog.rs`

Tests (TDD - write first, then implement):
- `test_context_close_dirty_tab_stores_pane_and_index()`
- `test_context_quit_with_dirty_tabs_stores_count()`

### Step 2: Parameterize button labels in `ConfirmDialog`

Extend `ConfirmDialog` to accept optional button labels (defaulting to "Cancel"/"Abandon"):

```rust
pub struct ConfirmDialog {
    pub prompt: String,
    pub selected: ConfirmButton,
    pub cancel_label: String,   // NEW
    pub confirm_label: String,  // NEW
}

impl ConfirmDialog {
    pub fn new(prompt: impl Into<String>) -> Self { ... }
    pub fn with_labels(prompt: impl Into<String>, cancel: &str, confirm: &str) -> Self { ... }
}
```

Update `ConfirmButton` to use generic variants (`Cancel`/`Confirm` instead of `Cancel`/`Abandon`) or keep existing names for compatibility.

Location: `crates/editor/src/confirm_dialog.rs`

Tests (TDD):
- `test_new_uses_default_labels()`
- `test_with_labels_uses_custom_labels()`

### Step 3: Update `ConfirmDialogGeometry` for button hit rects

Add button rectangle accessors to `ConfirmDialogGeometry` for mouse hit testing:

```rust
impl ConfirmDialogGeometry {
    /// Returns true if (x, y) is inside the cancel button.
    pub fn is_cancel_button(&self, x: f32, y: f32) -> bool { ... }

    /// Returns true if (x, y) is inside the confirm button.
    pub fn is_confirm_button(&self, x: f32, y: f32) -> bool { ... }
}
```

Location: `crates/editor/src/confirm_dialog.rs`

Tests (TDD):
- `test_is_cancel_button_inside()`
- `test_is_cancel_button_outside()`
- `test_is_confirm_button_inside()`
- `test_is_confirm_button_outside()`
- `test_button_hit_areas_do_not_overlap()`

### Step 4: Replace `pending_close` with `confirm_context` in `EditorState`

In `editor_state.rs`, replace:

```rust
pub pending_close: Option<(PaneId, usize)>,
```

with:

```rust
pub confirm_context: Option<ConfirmDialogContext>,
```

Update all usages:
- `show_confirm_dialog()` → takes a `ConfirmDialogContext` instead of pane_id/tab_idx
- `handle_key_confirm_dialog()` → dispatch on context variant instead of assuming close-tab
- `close_confirm_dialog()` → clears `confirm_context`
- Initialization in `new()` and `empty()` → set to `None`

Location: `crates/editor/src/editor_state.rs`

Tests (update existing, no new tests needed):
- Existing tests use `pending_close` assertions — update to `confirm_context`
- `test_close_dirty_tab_sets_pending_close` → `test_close_dirty_tab_sets_confirm_context`

### Step 5: Implement context-based outcome routing

In `handle_key_confirm_dialog()`, replace the hardcoded close-tab behavior with a match on context:

```rust
ConfirmOutcome::Confirmed => {
    if let Some(ctx) = self.confirm_context.take() {
        match ctx {
            ConfirmDialogContext::CloseDirtyTab { pane_id, tab_idx } => {
                self.force_close_tab(pane_id, tab_idx);
            }
            ConfirmDialogContext::QuitWithDirtyTabs { .. } => {
                // Set a flag to quit after closing dialog
                self.should_quit = true;
            }
        }
    }
    self.close_confirm_dialog();
}
```

Note: The `QuitWithDirtyTabs` variant sets up future work but is not fully implemented in this chunk (the quit machinery is out of scope). The context enum is the extensibility point.

Location: `crates/editor/src/editor_state.rs`

Tests:
- Existing close-dirty-tab tests continue to pass
- `test_confirm_quit_context_sets_should_quit_flag()` (if quit flag is added)

### Step 6: Implement mouse click handling for confirm dialog

Add mouse event handling when `EditorFocus::ConfirmDialog` is active:

```rust
// In handle_mouse(), replace the no-op for ConfirmDialog:
EditorFocus::ConfirmDialog => {
    if let MouseEventKind::Down = screen_event.kind {
        self.handle_mouse_confirm_dialog(screen_x as f32, screen_y as f32);
    }
}
```

Implement `handle_mouse_confirm_dialog()`:

```rust
fn handle_mouse_confirm_dialog(&mut self, x: f32, y: f32) {
    // Calculate geometry
    let geometry = calculate_confirm_dialog_geometry(...);

    if geometry.is_cancel_button(x, y) {
        self.close_confirm_dialog();
    } else if geometry.is_confirm_button(x, y) {
        // Same as Enter with confirm selected
        self.handle_confirm_dialog_confirmed();
    }
}
```

Extract the confirmed outcome handling into a helper `handle_confirm_dialog_confirmed()` to share between keyboard Enter and mouse click.

Location: `crates/editor/src/editor_state.rs`

Tests:
- `test_mouse_click_cancel_button_closes_dialog()`
- `test_mouse_click_confirm_button_closes_tab()`
- `test_mouse_click_outside_buttons_does_nothing()`
- `test_mouse_click_updates_selection_before_close()` (clicking button should visually select it)

### Step 7: Update `ConfirmDialogGlyphBuffer` for dynamic labels

Update the GPU buffer construction to use the dialog's `cancel_label` and `confirm_label` instead of hardcoded `CANCEL_LABEL` / `ABANDON_LABEL` constants:

```rust
// In ConfirmDialogGlyphBuffer::update():
let cancel_len = dialog.cancel_label.len();
let abandon_len = dialog.confirm_label.len();

// ...and use dialog.cancel_label.chars() etc. for rendering
```

Also update `calculate_confirm_dialog_geometry()` to accept the labels for button width calculation, or make it take a `&ConfirmDialog` reference.

Location: `crates/editor/src/confirm_dialog.rs`

Tests:
- `test_geometry_with_custom_labels()`
- `test_geometry_button_width_scales_with_label_length()`

### Step 8: Add visual feedback on mouse hover (optional enhancement)

Add hover state to `ConfirmDialog` to highlight the button under the cursor:

```rust
pub struct ConfirmDialog {
    // ... existing fields
    pub hovered: Option<ConfirmButton>,
}
```

Update rendering to show hover highlight when mouse is over a button. This is optional polish but improves the user experience.

Location: `crates/editor/src/confirm_dialog.rs`, `crates/editor/src/editor_state.rs` (mouse move handling)

Tests:
- `test_mouse_move_updates_hovered_state()`
- `test_mouse_leave_clears_hovered_state()`

### Step 9: Final verification and cleanup

- Run `cargo clippy -p lite-edit -- -D warnings` and fix any warnings
- Run `cargo test -p lite-edit` and ensure all tests pass
- Verify the dialog works correctly by:
  - Making a tab dirty, attempting to close it
  - Clicking Cancel button with mouse
  - Clicking Abandon button with mouse
  - Using keyboard navigation (existing behavior preserved)

## Dependencies

- **dirty_tab_close_confirm** (ACTIVE): This chunk's `ConfirmDialog` widget, `ConfirmDialogGeometry`, `EditorFocus::ConfirmDialog`, and existing integration are the foundation we're extending.

## Risks and Open Questions

1. **Quit confirmation scope**: The GOAL mentions "quit-with-dirty-tabs" as a supported context, but the quit machinery itself (application termination, window close handling) is not in scope for this chunk. This plan adds the `ConfirmDialogContext::QuitWithDirtyTabs` variant and context routing, but the actual quit behavior is deferred. Is this acceptable, or should we remove the quit context variant from this chunk?

2. **Button label flexibility vs. complexity**: Adding parameterized labels increases API surface. If all use cases end up using "Cancel"/"Abandon" anyway, this may be premature generalization. However, the GOAL implies future contexts like "reload-from-disk" which might want different labels ("Reload"/"Keep Edits").

3. **Hover state for mouse**: Step 8 (hover feedback) is marked optional. Without it, clicking a button will work but won't show visual feedback before the click registers. This could feel unpolished. Should hover be required?

4. **Geometry recalculation**: `handle_mouse_confirm_dialog()` needs access to the same geometry used for rendering. Currently, geometry is calculated in the renderer. We may need to cache it or recalculate it in the mouse handler. This is solvable but worth noting.

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