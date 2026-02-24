# Dirty Tab Close Confirmation — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** When closing a dirty tab, show an in-engine confirmation dialog. On confirm, close without saving. On cancel, dismiss.

**Architecture:** New `ConfirmDialog` widget (pure state) + `ConfirmDialogOverlay` (Metal vertex buffers) following the existing SelectorWidget/SelectorOverlay pattern. Integrates via `EditorFocus::ConfirmDialog` variant and `pending_close` field on `EditorState`.

**Tech Stack:** Rust, Metal (via objc2-metal), existing glyph rendering pipeline

---

### Task 1: ConfirmDialog Widget — Core Types and Key Handling

**Files:**
- Create: `crates/editor/src/confirm_dialog.rs`
- Modify: `crates/editor/src/main.rs:74` (add `mod confirm_dialog;`)

**Step 1: Write the failing tests**

Create `crates/editor/src/confirm_dialog.rs` with tests first, implementation stubs that don't compile:

```rust
//! Confirm dialog widget for yes/no modal interactions.
//!
//! A minimal two-button dialog following the Humble View Architecture.
//! Pure state, no platform dependencies. Downstream code (renderers,
//! focus targets) consume this state and translate it to pixels.

use crate::input::{Key, KeyEvent};

/// Which button is currently selected in the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmDialogChoice {
    /// The cancel/safe option (left button)
    Cancel,
    /// The confirm/destructive option (right button)
    Confirm,
}

/// The outcome of handling an input event in the confirm dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmDialogOutcome {
    /// The dialog is still open; no decision made.
    Pending,
    /// The user confirmed the action.
    Confirmed,
    /// The user cancelled.
    Cancelled,
}

/// A two-button confirmation dialog.
///
/// Displays a prompt and two buttons (cancel / confirm).
/// Default selection is `Cancel` (safe default).
pub struct ConfirmDialog {
    prompt: String,
    confirm_label: String,
    cancel_label: String,
    selected: ConfirmDialogChoice,
}

impl ConfirmDialog {
    /// Creates a new confirm dialog with the given prompt and button labels.
    ///
    /// Default selection is `Cancel`.
    pub fn new(prompt: impl Into<String>, confirm_label: impl Into<String>, cancel_label: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            confirm_label: confirm_label.into(),
            cancel_label: cancel_label.into(),
            selected: ConfirmDialogChoice::Cancel,
        }
    }

    /// Returns the prompt text.
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// Returns the confirm button label.
    pub fn confirm_label(&self) -> &str {
        &self.confirm_label
    }

    /// Returns the cancel button label.
    pub fn cancel_label(&self) -> &str {
        &self.cancel_label
    }

    /// Returns which button is currently selected.
    pub fn selected(&self) -> ConfirmDialogChoice {
        self.selected
    }

    /// Handles a key event and returns the outcome.
    pub fn handle_key(&mut self, event: &KeyEvent) -> ConfirmDialogOutcome {
        match event.key {
            Key::Escape => ConfirmDialogOutcome::Cancelled,
            Key::Return => match self.selected {
                ConfirmDialogChoice::Confirm => ConfirmDialogOutcome::Confirmed,
                ConfirmDialogChoice::Cancel => ConfirmDialogOutcome::Cancelled,
            },
            Key::Tab | Key::Left | Key::Right => {
                self.selected = match self.selected {
                    ConfirmDialogChoice::Cancel => ConfirmDialogChoice::Confirm,
                    ConfirmDialogChoice::Confirm => ConfirmDialogChoice::Cancel,
                };
                ConfirmDialogOutcome::Pending
            }
            _ => ConfirmDialogOutcome::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Modifiers;

    fn dialog() -> ConfirmDialog {
        ConfirmDialog::new("Abandon unsaved changes?", "Abandon", "Cancel")
    }

    #[test]
    fn default_selection_is_cancel() {
        let d = dialog();
        assert_eq!(d.selected(), ConfirmDialogChoice::Cancel);
    }

    #[test]
    fn escape_always_cancels() {
        let mut d = dialog();
        let outcome = d.handle_key(&KeyEvent::new(Key::Escape, Modifiers::default()));
        assert_eq!(outcome, ConfirmDialogOutcome::Cancelled);
    }

    #[test]
    fn enter_on_cancel_returns_cancelled() {
        let mut d = dialog();
        assert_eq!(d.selected(), ConfirmDialogChoice::Cancel);
        let outcome = d.handle_key(&KeyEvent::new(Key::Return, Modifiers::default()));
        assert_eq!(outcome, ConfirmDialogOutcome::Cancelled);
    }

    #[test]
    fn tab_toggles_selection() {
        let mut d = dialog();
        assert_eq!(d.selected(), ConfirmDialogChoice::Cancel);

        let outcome = d.handle_key(&KeyEvent::new(Key::Tab, Modifiers::default()));
        assert_eq!(outcome, ConfirmDialogOutcome::Pending);
        assert_eq!(d.selected(), ConfirmDialogChoice::Confirm);

        let outcome = d.handle_key(&KeyEvent::new(Key::Tab, Modifiers::default()));
        assert_eq!(outcome, ConfirmDialogOutcome::Pending);
        assert_eq!(d.selected(), ConfirmDialogChoice::Cancel);
    }

    #[test]
    fn left_right_toggle_selection() {
        let mut d = dialog();

        d.handle_key(&KeyEvent::new(Key::Right, Modifiers::default()));
        assert_eq!(d.selected(), ConfirmDialogChoice::Confirm);

        d.handle_key(&KeyEvent::new(Key::Left, Modifiers::default()));
        assert_eq!(d.selected(), ConfirmDialogChoice::Cancel);
    }

    #[test]
    fn enter_on_confirm_returns_confirmed() {
        let mut d = dialog();
        d.handle_key(&KeyEvent::new(Key::Tab, Modifiers::default())); // move to Confirm
        let outcome = d.handle_key(&KeyEvent::new(Key::Return, Modifiers::default()));
        assert_eq!(outcome, ConfirmDialogOutcome::Confirmed);
    }

    #[test]
    fn escape_cancels_even_when_confirm_selected() {
        let mut d = dialog();
        d.handle_key(&KeyEvent::new(Key::Tab, Modifiers::default())); // move to Confirm
        assert_eq!(d.selected(), ConfirmDialogChoice::Confirm);
        let outcome = d.handle_key(&KeyEvent::new(Key::Escape, Modifiers::default()));
        assert_eq!(outcome, ConfirmDialogOutcome::Cancelled);
    }

    #[test]
    fn unrecognized_keys_return_pending() {
        let mut d = dialog();
        let outcome = d.handle_key(&KeyEvent::char('x'));
        assert_eq!(outcome, ConfirmDialogOutcome::Pending);
    }

    #[test]
    fn accessors_return_constructor_values() {
        let d = dialog();
        assert_eq!(d.prompt(), "Abandon unsaved changes?");
        assert_eq!(d.confirm_label(), "Abandon");
        assert_eq!(d.cancel_label(), "Cancel");
    }
}
```

**Step 2: Add module declaration**

In `crates/editor/src/main.rs`, add after the `mod clipboard;` line (line 37):

```rust
mod confirm_dialog;
```

**Step 3: Run tests to verify they pass**

Run: `cargo test -p lite-edit --lib confirm_dialog`
Expected: All 8 tests PASS (implementation is inline with tests in the same file)

**Step 4: Commit**

```bash
git add crates/editor/src/confirm_dialog.rs crates/editor/src/main.rs
git commit -m "feat: add ConfirmDialog widget with key handling and tests"
```

---

### Task 2: EditorFocus::ConfirmDialog Variant and State Fields

**Files:**
- Modify: `crates/editor/src/editor_state.rs:52-62` (EditorFocus enum)
- Modify: `crates/editor/src/editor_state.rs:79-125` (EditorState struct)
- Modify: `crates/editor/src/editor_state.rs:270-350` (constructors — two `fn empty` and `fn new` or similar)

**Step 1: Add the focus variant**

In `crates/editor/src/editor_state.rs`, add to the `EditorFocus` enum (after `FindInFile` at line 61):

```rust
    /// Confirm dialog is active (e.g., abandon unsaved changes)
    ConfirmDialog,
```

**Step 2: Add state fields**

In the `EditorState` struct, add after `language_registry` (line 124):

```rust
    /// The active confirm dialog (when focus == ConfirmDialog)
    pub active_confirm_dialog: Option<crate::confirm_dialog::ConfirmDialog>,
    /// The pane ID and tab index of the tab pending close confirmation
    pending_close: Option<(crate::pane_layout::PaneId, usize)>,
```

**Step 3: Initialize fields in constructors**

Find all places that construct `EditorState` (the `empty()` and `new_with_file()` methods, around lines 270-350). Add to each:

```rust
            active_confirm_dialog: None,
            pending_close: None,
```

**Step 4: Fix all match exhaustiveness errors**

The compiler will flag every `match self.focus { ... }` that doesn't handle `ConfirmDialog`. Find and fix each one. The pattern for each:

- **`handle_key` routing (line ~652-663):** Add `EditorFocus::ConfirmDialog => { self.handle_key_confirm_dialog(event); }` — we'll implement this method in Task 3.
- **`handle_cmd_p` (line ~669-681):** Add `EditorFocus::ConfirmDialog => {}` (no-op — don't open file picker during dialog).
- **`handle_cmd_f` (line ~781-808):** Add `EditorFocus::ConfirmDialog => {}` (no-op).
- **`handle_mouse` routing (line ~1437-1445):** Add `EditorFocus::ConfirmDialog => {}` (no-op — keyboard only for now).
- **`handle_scroll` (line ~1699-1704):** After the selector check, add: `if self.focus == EditorFocus::ConfirmDialog { return; }`
- **`tick_picker` (line ~1904-1908):** The `!=` check is fine, no change needed.
- **`toggle_cursor_blink` (line ~2025-2050):** Add `EditorFocus::ConfirmDialog => { ... }` — handle like `Selector | FindInFile` (toggle overlay cursor).
- **drain_loop.rs render match (~264-295):** Add `EditorFocus::ConfirmDialog => { ... }` — for now, render same as `Buffer` (we'll add overlay rendering in Task 5).

For the stub `handle_key_confirm_dialog`, add an empty method:

```rust
    fn handle_key_confirm_dialog(&mut self, _event: KeyEvent) {
        // Implemented in Task 3
    }
```

**Step 5: Run full test suite**

Run: `cargo test -p lite-edit`
Expected: All existing tests PASS. No new failures from exhaustiveness or initialization.

**Step 6: Commit**

```bash
git add crates/editor/src/editor_state.rs crates/editor/src/drain_loop.rs
git commit -m "feat: add EditorFocus::ConfirmDialog variant and state fields"
```

---

### Task 3: Close-Tab Flow Change and Key Routing

**Files:**
- Modify: `crates/editor/src/editor_state.rs:2379-2439` (close_tab method)
- Modify: `crates/editor/src/editor_state.rs` (handle_key_confirm_dialog method)

**Step 1: Write failing tests**

Add these tests to the `#[cfg(test)] mod tests` block in `editor_state.rs`:

```rust
    #[test]
    fn test_close_dirty_tab_opens_confirm_dialog() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Type to make tab dirty
        state.handle_key(KeyEvent::char('a'));
        assert!(state.editor.active_workspace().unwrap().active_pane().unwrap().tabs[0].dirty);

        // Try to close — should open dialog instead
        state.close_tab(0);

        assert_eq!(state.focus, EditorFocus::ConfirmDialog);
        assert!(state.active_confirm_dialog.is_some());
        // Tab should still be there
        assert_eq!(state.editor.active_workspace().unwrap().active_pane().unwrap().tabs.len(), 1);
    }

    #[test]
    fn test_confirm_dialog_escape_cancels() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Make dirty and try to close
        state.handle_key(KeyEvent::char('a'));
        state.close_tab(0);
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);

        // Press Escape — should cancel
        state.handle_key(KeyEvent::new(Key::Escape, Modifiers::default()));

        assert_eq!(state.focus, EditorFocus::Buffer);
        assert!(state.active_confirm_dialog.is_none());
        // Tab still there, still dirty
        assert!(state.editor.active_workspace().unwrap().active_pane().unwrap().tabs[0].dirty);
    }

    #[test]
    fn test_confirm_dialog_confirm_closes_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add a second tab so closing doesn't create a new empty tab
        {
            let tab_id = state.editor.gen_tab_id();
            let line_height = state.editor.line_height();
            let tab = crate::workspace::Tab::empty_file(tab_id, line_height);
            state.editor.active_workspace_mut().unwrap().add_tab(tab);
        }
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 2);

        // Make first tab dirty
        state.handle_key(KeyEvent::char('a'));
        assert!(state.editor.active_workspace().unwrap().active_pane().unwrap().tabs[0].dirty);

        // Try to close tab 0
        state.close_tab(0);
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);

        // Tab to move selection to Confirm, then Enter
        state.handle_key(KeyEvent::new(Key::Tab, Modifiers::default()));
        state.handle_key(KeyEvent::new(Key::Return, Modifiers::default()));

        // Dialog dismissed, tab closed
        assert_eq!(state.focus, EditorFocus::Buffer);
        assert!(state.active_confirm_dialog.is_none());
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);
    }

    #[test]
    fn test_confirm_dialog_enter_on_cancel_keeps_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        state.handle_key(KeyEvent::char('a'));
        state.close_tab(0);
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);

        // Press Enter without toggling — Cancel is default
        state.handle_key(KeyEvent::new(Key::Return, Modifiers::default()));

        assert_eq!(state.focus, EditorFocus::Buffer);
        assert!(state.active_confirm_dialog.is_none());
        // Tab still there
        assert_eq!(state.editor.active_workspace().unwrap().active_pane().unwrap().tabs.len(), 1);
    }
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p lite-edit --lib -- test_close_dirty_tab_opens_confirm_dialog test_confirm_dialog_escape_cancels test_confirm_dialog_confirm_closes_tab test_confirm_dialog_enter_on_cancel_keeps_tab`
Expected: FAIL

**Step 3: Implement close_tab dirty guard change**

Replace the dirty guard in `close_tab()` (lines 2387-2394) with:

```rust
            // Guard: dirty tabs require confirmation before closing
            if let Some(pane) = workspace.active_pane() {
                if let Some(tab) = pane.tabs.get(index) {
                    if tab.dirty {
                        // Store which tab to close and open confirmation dialog
                        self.pending_close = Some((workspace.active_pane_id, index));
                        self.active_confirm_dialog = Some(
                            crate::confirm_dialog::ConfirmDialog::new(
                                "Abandon unsaved changes?",
                                "Abandon",
                                "Cancel",
                            ),
                        );
                        self.focus = EditorFocus::ConfirmDialog;
                        self.dirty_region.merge(DirtyRegion::FullViewport);
                        return;
                    }
                }
            }
```

**Step 4: Implement handle_key_confirm_dialog**

Replace the stub method with:

```rust
    fn handle_key_confirm_dialog(&mut self, event: KeyEvent) {
        let outcome = if let Some(ref mut dialog) = self.active_confirm_dialog {
            dialog.handle_key(&event)
        } else {
            return;
        };

        match outcome {
            crate::confirm_dialog::ConfirmDialogOutcome::Pending => {
                self.dirty_region.merge(DirtyRegion::FullViewport);
            }
            crate::confirm_dialog::ConfirmDialogOutcome::Confirmed => {
                // Close the tab, bypassing the dirty guard
                if let Some((pane_id, tab_index)) = self.pending_close.take() {
                    self.force_close_tab(pane_id, tab_index);
                }
                self.active_confirm_dialog = None;
                self.focus = EditorFocus::Buffer;
                self.cursor_visible = true;
                self.last_keystroke = std::time::Instant::now();
                self.dirty_region.merge(DirtyRegion::FullViewport);
            }
            crate::confirm_dialog::ConfirmDialogOutcome::Cancelled => {
                self.active_confirm_dialog = None;
                self.pending_close = None;
                self.focus = EditorFocus::Buffer;
                self.cursor_visible = true;
                self.last_keystroke = std::time::Instant::now();
                self.dirty_region.merge(DirtyRegion::FullViewport);
            }
        }
    }
```

**Step 5: Implement force_close_tab**

Add a new method that performs the close without the dirty guard. Extract the body of `close_tab` after the guard into a shared helper, or add a simpler method:

```rust
    /// Closes a tab unconditionally (no dirty guard).
    /// Used by the confirm dialog after user confirms abandoning changes.
    fn force_close_tab(&mut self, pane_id: PaneId, tab_index: usize) {
        let tab_id = self.editor.gen_tab_id();
        let line_height = self.editor.line_height();

        if let Some(workspace) = self.editor.active_workspace_mut() {
            let pane_count = workspace.pane_root.pane_count();

            // Find the target pane (it might not be the active pane if focus shifted)
            let pane = workspace.pane_root.pane_mut(pane_id);
            if pane.is_none() {
                return;
            }

            if pane_count > 1 {
                let pane_will_be_empty = workspace.pane_root
                    .pane(pane_id)
                    .map(|p| p.tabs.len() == 1)
                    .unwrap_or(false);

                let fallback_focus = if pane_will_be_empty {
                    workspace.find_fallback_focus()
                } else {
                    None
                };

                if let Some(pane) = workspace.pane_root.pane_mut(pane_id) {
                    pane.close_tab(tab_index);
                }

                if pane_will_be_empty {
                    if let Some(fallback_pane_id) = fallback_focus {
                        workspace.active_pane_id = fallback_pane_id;
                    }
                    crate::pane_layout::cleanup_empty_panes(&mut workspace.pane_root);
                }
            } else {
                if let Some(pane) = workspace.pane_root.pane_mut(pane_id) {
                    if pane.tabs.len() > 1 {
                        pane.close_tab(tab_index);
                    } else {
                        let new_tab = crate::workspace::Tab::empty_file(tab_id, line_height);
                        pane.tabs[0] = new_tab;
                        pane.active_tab = 0;
                    }
                }
            }
            self.dirty_region.merge(DirtyRegion::FullViewport);
        }
    }
```

Note: Check whether `pane_root.pane()` and `pane_root.pane_mut()` methods exist. If they only work through `active_pane()` / `active_pane_mut()`, use those instead and verify the active pane ID matches. Adapt the implementation to match the actual API available on `PaneLayoutNode`.

**Step 6: Run tests to verify they pass**

Run: `cargo test -p lite-edit --lib -- test_close_dirty_tab_opens_confirm_dialog test_confirm_dialog_escape_cancels test_confirm_dialog_confirm_closes_tab test_confirm_dialog_enter_on_cancel_keeps_tab`
Expected: All 4 PASS

**Step 7: Run full test suite**

Run: `cargo test -p lite-edit`
Expected: All tests PASS

**Step 8: Commit**

```bash
git add crates/editor/src/editor_state.rs
git commit -m "feat: dirty tab close triggers confirmation dialog"
```

---

### Task 4: ConfirmDialog Overlay Geometry

**Files:**
- Modify: `crates/editor/src/selector_overlay.rs` (add geometry function and buffer type at the end, before tests)

Add the geometry calculation and `ConfirmDialogGlyphBuffer` to `selector_overlay.rs` since that's where all overlay geometry and glyph buffers live.

**Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests` block in `selector_overlay.rs`:

```rust
    // =========================================================================
    // calculate_confirm_dialog_geometry tests
    // =========================================================================

    #[test]
    fn confirm_dialog_centered_horizontally() {
        let geom = calculate_confirm_dialog_geometry(
            1000.0, 800.0, 20.0, 8.0,
            "Abandon unsaved changes?", "Abandon", "Cancel",
        );
        let expected_x = (1000.0 - geom.panel_width) / 2.0;
        assert!((geom.panel_x - expected_x).abs() < 0.01);
    }

    #[test]
    fn confirm_dialog_at_30_percent_from_top() {
        let geom = calculate_confirm_dialog_geometry(
            1000.0, 800.0, 20.0, 8.0,
            "Abandon unsaved changes?", "Abandon", "Cancel",
        );
        assert!((geom.panel_y - 240.0).abs() < 0.01); // 30% of 800
    }

    #[test]
    fn confirm_dialog_width_fits_prompt() {
        let geom = calculate_confirm_dialog_geometry(
            1000.0, 800.0, 20.0, 8.0,
            "Abandon unsaved changes?", "Abandon", "Cancel",
        );
        let prompt_len = "Abandon unsaved changes?".len() as f32;
        let min_width = prompt_len * 8.0 + 2.0 * OVERLAY_PADDING_X;
        assert!(geom.panel_width >= min_width);
    }

    #[test]
    fn confirm_dialog_has_two_rows() {
        let geom = calculate_confirm_dialog_geometry(
            1000.0, 800.0, 20.0, 8.0,
            "Abandon unsaved changes?", "Abandon", "Cancel",
        );
        // Panel should be: padding + prompt row + padding + button row + padding
        let expected_height = OVERLAY_PADDING_Y * 3.0 + 20.0 * 2.0;
        assert!((geom.panel_height - expected_height).abs() < 0.01);
    }
```

**Step 2: Implement the geometry function**

Add before the tests section in `selector_overlay.rs`:

```rust
// =============================================================================
// Confirm Dialog Geometry
// =============================================================================

/// Computed geometry for the confirm dialog overlay
#[derive(Debug, Clone, Copy)]
pub struct ConfirmDialogGeometry {
    /// Left edge of the panel
    pub panel_x: f32,
    /// Top edge of the panel
    pub panel_y: f32,
    /// Width of the panel
    pub panel_width: f32,
    /// Height of the panel
    pub panel_height: f32,
    /// Y coordinate for the prompt text
    pub prompt_y: f32,
    /// Y coordinate for the button row
    pub button_y: f32,
    /// X coordinate where prompt text starts
    pub prompt_x: f32,
    /// X coordinate where the cancel button label starts
    pub cancel_x: f32,
    /// Width of the cancel button highlight
    pub cancel_width: f32,
    /// X coordinate where the confirm button label starts
    pub confirm_x: f32,
    /// Width of the confirm button highlight
    pub confirm_width: f32,
    /// Line height
    pub line_height: f32,
    /// Glyph width
    pub glyph_width: f32,
}

/// Button padding: extra space on each side of the button label for the highlight
const BUTTON_PADDING_X: f32 = 8.0;
/// Space between the two buttons
const BUTTON_GAP: f32 = 16.0;

/// Calculates the geometry for the confirm dialog overlay
pub fn calculate_confirm_dialog_geometry(
    view_width: f32,
    view_height: f32,
    line_height: f32,
    glyph_width: f32,
    prompt: &str,
    confirm_label: &str,
    cancel_label: &str,
) -> ConfirmDialogGeometry {
    let prompt_text_width = prompt.len() as f32 * glyph_width;
    let cancel_text_width = cancel_label.len() as f32 * glyph_width;
    let confirm_text_width = confirm_label.len() as f32 * glyph_width;

    let cancel_btn_width = cancel_text_width + 2.0 * BUTTON_PADDING_X;
    let confirm_btn_width = confirm_text_width + 2.0 * BUTTON_PADDING_X;
    let buttons_total_width = cancel_btn_width + BUTTON_GAP + confirm_btn_width;

    let content_width = prompt_text_width.max(buttons_total_width);
    let panel_width = content_width + 2.0 * OVERLAY_PADDING_X;

    // Height: padding + prompt + padding + buttons + padding
    let panel_height = OVERLAY_PADDING_Y * 3.0 + line_height * 2.0;

    let panel_x = (view_width - panel_width) / 2.0;
    let panel_y = view_height * 0.3;

    let prompt_y = panel_y + OVERLAY_PADDING_Y;
    let button_y = prompt_y + line_height + OVERLAY_PADDING_Y;

    // Center prompt text within panel
    let prompt_x = panel_x + (panel_width - prompt_text_width) / 2.0;

    // Center buttons within panel
    let buttons_start_x = panel_x + (panel_width - buttons_total_width) / 2.0;
    let cancel_x = buttons_start_x;
    let confirm_x = cancel_x + cancel_btn_width + BUTTON_GAP;

    ConfirmDialogGeometry {
        panel_x,
        panel_y,
        panel_width,
        panel_height,
        prompt_y,
        button_y,
        prompt_x,
        cancel_x,
        cancel_width: cancel_btn_width,
        confirm_x,
        confirm_width: confirm_btn_width,
        line_height,
        glyph_width,
    }
}
```

**Step 3: Run tests**

Run: `cargo test -p lite-edit --lib selector_overlay::tests::confirm_dialog`
Expected: All 4 PASS

**Step 4: Commit**

```bash
git add crates/editor/src/selector_overlay.rs
git commit -m "feat: add confirm dialog overlay geometry"
```

---

### Task 5: ConfirmDialog Glyph Buffer and Renderer Integration

**Files:**
- Modify: `crates/editor/src/selector_overlay.rs` (add `ConfirmDialogGlyphBuffer`)
- Modify: `crates/editor/src/renderer.rs` (add `confirm_dialog_buffer` field, `draw_confirm_dialog` method)
- Modify: `crates/editor/src/drain_loop.rs:264-295` (add `ConfirmDialog` render branch)

**Step 1: Add ConfirmDialogGlyphBuffer**

In `selector_overlay.rs`, add after the geometry code (before tests):

```rust
// =============================================================================
// ConfirmDialogGlyphBuffer
// =============================================================================

/// Manages vertex and index buffers for rendering the confirm dialog overlay.
pub struct ConfirmDialogGlyphBuffer {
    vertex_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    index_count: usize,
    layout: GlyphLayout,

    background_range: QuadRange,
    selection_range: QuadRange,
    prompt_text_range: QuadRange,
    cancel_text_range: QuadRange,
    confirm_text_range: QuadRange,
}
```

Implement `new()`, accessor methods (same pattern as `SelectorGlyphBuffer`), and an `update()` method that takes `device`, `atlas`, `dialog: &ConfirmDialog`, `geometry: &ConfirmDialogGeometry` and builds:
1. Background rect (full panel, `OVERLAY_BACKGROUND_COLOR`)
2. Selection highlight rect (over the selected button, `OVERLAY_SELECTION_COLOR`)
3. Prompt text glyphs
4. Cancel label text glyphs
5. Confirm label text glyphs

Follow the exact quad-building pattern from `SelectorGlyphBuffer::update_from_widget` — use `create_rect_quad`, `create_glyph_quad_at`, `push_quad_indices`, and the same GPU buffer creation code. These helper methods need to be duplicated (or you can extract them into shared functions).

**Step 2: Add renderer support**

In `renderer.rs`, add a field:

```rust
    confirm_dialog_buffer: Option<ConfirmDialogGlyphBuffer>,
```

Initialize it to `None` in the constructor.

Add a `draw_confirm_dialog` method following the `draw_selector_overlay` pattern:
- Calculate geometry from view dimensions and dialog state
- Lazy-initialize the glyph buffer
- Update from widget
- Draw with the same pipeline (set vertex/index buffers, draw indexed for each range with its color uniform)

**Step 3: Wire into drain_loop.rs**

In the render match (drain_loop.rs, around line 264), add:

```rust
                EditorFocus::ConfirmDialog => {
                    self.renderer.set_cursor_visible(self.state.cursor_visible);
                    self.renderer.render_with_editor(
                        &self.metal_view,
                        &self.state.editor,
                        None,
                        self.state.cursor_visible,
                    );
                    // Draw confirm dialog overlay on top
                    // Note: render_with_editor already presented the frame,
                    // so we need a different approach — either pass the dialog
                    // into render_with_editor, or add a new render method.
                }
```

**Important:** The existing render methods call `presentDrawable` and `commit` at the end, so you can't draw after them. Instead, either:
- (a) Add a `confirm_dialog: Option<&ConfirmDialog>` parameter to `render_with_editor`, or
- (b) Create a `render_with_confirm_dialog` method similar to `render_with_find_strip`

Option (a) is cleaner — add the parameter, and after the selector overlay draw call, add the confirm dialog draw call if `Some`. Update all call sites (pass `None` for the existing ones, pass `state.active_confirm_dialog.as_ref()` for the ConfirmDialog branch).

**Step 4: Build and manual test**

Run: `cargo build -p lite-edit`
Expected: Compiles successfully

Manual smoke test: Run the editor, edit a file, try Cmd+W — dialog should appear. Escape cancels. Tab+Enter closes.

**Step 5: Commit**

```bash
git add crates/editor/src/selector_overlay.rs crates/editor/src/renderer.rs crates/editor/src/drain_loop.rs
git commit -m "feat: confirm dialog overlay rendering"
```

---

### Task 6: Create Future Chunk for Generic Yes/No Modals

**Files:**
- Create: `docs/chunks/generic_yes_no_modal/GOAL.md` (via `ve chunk create`)

**Step 1: Create the chunk**

Run: `ve chunk create generic_yes_no_modal`

Edit the generated GOAL.md to describe:
- Generalize the ConfirmDialog for arbitrary yes/no modal use cases
- Support customizable button labels and prompt text (already done in the widget, but the EditorState integration is hardcoded to tab closing)
- Add an action/context enum so the dialog outcome routes to the correct handler (instead of `pending_close` field)
- Support additional use cases: quit confirmation with dirty tabs, reload-from-disk, etc.
- Mouse click support for dialog buttons

**Step 2: Commit**

```bash
git add docs/chunks/generic_yes_no_modal/
git commit -m "feat: create future chunk for generic yes/no modal"
```

---

### Task 7: Final Integration Test and Cleanup

**Files:**
- Modify: `crates/editor/src/editor_state.rs` (any cleanup)

**Step 1: Run full test suite**

Run: `cargo test`
Expected: All tests PASS across all crates

**Step 2: Run clippy**

Run: `cargo clippy -p lite-edit -- -D warnings`
Expected: No warnings

**Step 3: Commit any fixes**

If clippy or tests surface issues, fix and commit.
