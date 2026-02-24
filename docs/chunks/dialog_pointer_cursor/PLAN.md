<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Extend the existing `update_cursor_regions` function in `drain_loop.rs` to handle
the `EditorFocus::ConfirmDialog` case. When a confirm dialog is active, compute
its geometry using the existing `calculate_confirm_dialog_geometry` function and
register pointer-cursor `CursorRect` regions for each button.

The implementation follows the established pattern from `docs/chunks/cursor_pointer_ui_hints`:
1. The `update_cursor_regions` function already handles multiple UI states (rail, tab bar, selector overlay, buffer content)
2. Each interactive region calls `regions.add_pointer()` with a `CursorRect`
3. Dialog geometry is already computed by `calculate_confirm_dialog_geometry` in `confirm_dialog.rs`

This is a small, focused change to a single function that adds cursor region
computation for a new UI element type.

Per `docs/trunk/TESTING_PHILOSOPHY.md`, cursor region registration is "humble"
platform code that projects state onto the OS cursor system. The geometry
calculation functions (`calculate_confirm_dialog_geometry`, `is_cancel_button`,
`is_confirm_button`) are already unit-tested in `confirm_dialog.rs`. This chunk
adds integration of those tested functions into the cursor region system.

## Sequence

### Step 1: Add ConfirmDialog case to update_cursor_regions

In `crates/editor/src/drain_loop.rs`, inside the `update_cursor_regions` method,
add a new block that handles `EditorFocus::ConfirmDialog`. When this focus mode
is active:

1. Check if `self.state.confirm_dialog` is `Some`
2. Compute the dialog geometry using `calculate_confirm_dialog_geometry`
3. Register a pointer cursor region for the Cancel button
4. Register a pointer cursor region for the Confirm/Abandon button

**Location**: `crates/editor/src/drain_loop.rs#update_cursor_regions`

**Implementation details**:
- Import `calculate_confirm_dialog_geometry` from `confirm_dialog` module
- Use the same coordinate transform pattern as the selector overlay (`px_to_pt`)
- Button regions are defined by `geometry.cancel_button_x`, `geometry.abandon_button_x`,
  `geometry.buttons_y`, `geometry.button_width`, and `geometry.button_height`

**Pattern to follow**: The selector overlay case (lines 381-414) shows how to:
- Match on `EditorFocus` variant
- Access overlay state with `if let Some(ref ...) = self.state.xxx`
- Compute geometry using helper function
- Convert pixel coordinates to point coordinates with `px_to_pt`
- Call `regions.add_pointer()` for clickable areas

### Step 2: Manual verification

Verify the implementation by:
1. Opening a dirty file (make an edit, don't save)
2. Closing the tab (Cmd+W) to trigger the confirm dialog
3. Hovering over each button - cursor should change to pointer
4. Hovering over the dialog background (between buttons, on prompt) - cursor should NOT be pointer
5. Moving mouse off the dialog to buffer area - cursor should revert to I-beam
6. Verify existing cursor behavior for rail, tab bar, and selector overlay is unchanged

## Dependencies

The following are already implemented and available:
- `calculate_confirm_dialog_geometry` function in `confirm_dialog.rs`
- `CursorRect`, `CursorRegions`, `add_pointer` API in `metal_view.rs`
- `EditorFocus::ConfirmDialog` variant in `editor_state.rs`
- `confirm_dialog` field on `EditorState`

## Risks and Open Questions

- **Coordinate system alignment**: The geometry is computed in pixels (as used by
  the renderer) but cursor regions are specified in points (macOS coordinate system).
  The existing `px_to_pt` helper in `update_cursor_regions` handles this conversion.
  Need to ensure the button coordinates transform correctly through this conversion.

- **Focus state consistency**: The code must check both `EditorFocus::ConfirmDialog`
  AND `self.state.confirm_dialog.is_some()` to be safe. The goal states that
  dialog background should NOT show pointer cursor, which is handled implicitly -
  only button regions are registered as pointer regions.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->