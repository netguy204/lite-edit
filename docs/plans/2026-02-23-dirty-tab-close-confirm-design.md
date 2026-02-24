# Dirty Tab Close Confirmation

## Problem

Dirty tabs cannot be closed. The close action silently refuses when `tab.dirty == true`. Users have no way to abandon unsaved changes and close a tab — they must save first.

## Solution

When a user attempts to close a dirty tab, show an in-engine confirmation dialog asking whether to abandon changes. If confirmed, close the tab without saving. If cancelled, dismiss the dialog and leave the tab open.

## Architecture

### ConfirmDialog Widget

A minimal yes/no dialog widget following the Humble View Architecture (pure state, no platform dependencies).

**Struct:** `ConfirmDialog`
- `prompt: String` — the question text (e.g., "Abandon unsaved changes?")
- `selected: ConfirmDialogChoice` — which button is highlighted (`Confirm` or `Cancel`)

**Enum:** `ConfirmDialogChoice` — `Confirm | Cancel`

**Enum:** `ConfirmDialogOutcome` — `Pending | Confirmed | Cancelled`

**Key handling:**
- `Enter` → returns `Confirmed` or `Cancelled` based on `selected`
- `Escape` → always returns `Cancelled`
- `Tab`, `Left`/`Right` arrows → toggle between Confirm and Cancel
- All other keys → `Pending` (ignored)

Default selection: `Cancel` (safe default — user must deliberately move to Confirm).

### EditorState Integration

- Add `EditorFocus::ConfirmDialog` variant to the focus enum.
- Add `active_confirm_dialog: Option<ConfirmDialog>` field to `EditorState`.
- Add `pending_close: Option<(PaneId, usize)>` field to track which tab triggered the dialog.

**Close-tab flow change** (`close_tab()` in `editor_state.rs`):
1. If tab is dirty: store `(pane_id, tab_index)` in `pending_close`, create `ConfirmDialog`, set focus to `ConfirmDialog`. Return without closing.
2. If tab is not dirty: close immediately (existing behavior, unchanged).

**Key routing** (`handle_key()` in `editor_state.rs`):
- When `focus == ConfirmDialog`: delegate to `handle_key_confirm_dialog()`.
- On `Confirmed`: retrieve `pending_close`, call existing close logic (bypassing dirty guard), clear dialog state, return focus to `Buffer`.
- On `Cancelled`: clear dialog state, clear `pending_close`, return focus to `Buffer`.

### Rendering

**ConfirmDialogOverlay** — new overlay buffer following `SelectorGlyphBuffer` / `FindStripGlyphBuffer` patterns.

**Layout:** Compact centered panel, no query row, no item list. Structure:
- Background rect (dark grey, same `OVERLAY_BACKGROUND_COLOR`)
- Prompt text centered on first row
- Two buttons on second row: `[Cancel]` and `[Abandon]`, spaced apart
- Selection highlight behind the focused button (same `OVERLAY_SELECTION_COLOR`)

**Geometry function:** `calculate_confirm_dialog_geometry(view_width, view_height, line_height, glyph_width, prompt, confirm_label, cancel_label) -> ConfirmDialogGeometry`

Panel sized to fit content (prompt width + padding), centered both horizontally and vertically (or at 30% from top to match selector overlay feel).

### Renderer Integration

In `renderer.rs`, after drawing the selector overlay, add a block that draws the confirm dialog overlay when `active_confirm_dialog.is_some()`. Same draw-call pattern: background, selection highlight, text glyphs — each with their own color uniform.

### Future Chunk: Generic Yes/No Modal

The `ConfirmDialog` widget created here is already generic (arbitrary prompt, two choices). A future chunk will:
- Allow customizable button labels (not just Abandon/Cancel)
- Support different dialog contexts beyond tab closing (quit confirmation, reload-from-disk, etc.)
- Potentially add a callback/action enum instead of the `pending_close` approach

## Scope

**In scope:**
- `ConfirmDialog` widget with key handling
- `ConfirmDialogOverlay` rendering
- Integration into `close_tab()` flow
- `EditorFocus::ConfirmDialog` variant
- Unit tests for widget logic and geometry
- Future chunk creation for generic modal work

**Out of scope:**
- Mouse click on dialog buttons (keyboard only for now)
- Save-before-close option (just abandon or cancel)
- Quit-app confirmation for dirty tabs
