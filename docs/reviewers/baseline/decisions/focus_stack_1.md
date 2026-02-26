---
decision: FEEDBACK
summary: "FocusStack infrastructure is complete but not integrated - EditorFocus enum still drives dispatch"
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: `EditorFocus` enum replaced with `FocusStack` (ordered `Vec<Box<dyn FocusTarget>>`)

- **Status**: gap
- **Evidence**: `FocusStack` exists in `focus.rs` with full implementation and tests. However, `EditorFocus` enum still exists at `editor_state.rs:71` and is the actual dispatch mechanism used in `EditorState::handle_key` (line 922-937). The `focus: EditorFocus` field is still the primary focus state in `EditorState`.

### Criterion 2: `FocusTarget::handle_key` returns `Handled` enum (`Yes` / `No`) to enable propagation

- **Status**: satisfied
- **Evidence**: The `FocusTarget` trait in `focus.rs:67-93` correctly defines `handle_key(&mut self, event: KeyEvent, ctx: &mut EditorContext) -> Handled`. All implementations return `Handled::Yes` or `Handled::No`. The `FocusStack::dispatch_key` method at `focus.rs:210-218` correctly propagates events top-down.

### Criterion 3: Global shortcuts (Cmd+Q, Cmd+W, Cmd+S, Cmd+N, etc.) handled by a single `GlobalShortcutTarget` at the stack bottom — not duplicated per target

- **Status**: gap
- **Evidence**: `GlobalShortcutTarget` exists in `global_shortcuts.rs` with all shortcuts defined in `GlobalAction` enum and `resolve_action()` method. Tests pass. HOWEVER, global shortcuts are STILL duplicated in `EditorState::handle_key()` at lines 740-920, which is the actual code path used. The `GlobalShortcutTarget` is not being invoked.

### Criterion 4: Overlays push onto stack on open, pop on dismiss (find bar, selector, confirm dialog)

- **Status**: gap
- **Evidence**: Focus targets exist (`SelectorFocusTarget`, `FindFocusTarget`, `ConfirmDialogFocusTarget`) with tests. However, they are not used. The actual opening/closing logic in `EditorState` sets `self.focus = EditorFocus::Selector` (line 1022), `self.focus = EditorFocus::FindInFile` (line 1081), etc. No `focus_stack.push()/pop()` calls exist in `EditorState`.

### Criterion 5: Buffer focus target is always present as the base editing layer

- **Status**: gap
- **Evidence**: `BufferFocusTarget` exists and implements `FocusTarget` correctly. However, `EditorState` does not use a `FocusStack`; it still has `focus_target: BufferFocusTarget` as a separate field (line 106) and dispatches directly to it via `handle_key_buffer()`.

### Criterion 6: Event propagation is top-down (most recently pushed target handles first)

- **Status**: gap (for actual dispatch)
- **Evidence**: `FocusStack::dispatch_key` at line 210-218 correctly iterates `self.targets.iter_mut().rev()` for top-down propagation. Unit tests verify this. However, this code is never called from `EditorState`. The actual dispatch in `handle_key()` uses `match self.focus`.

### Criterion 7: Existing keyboard behavior unchanged for all current interactions

- **Status**: satisfied
- **Evidence**: All 1106+ tests pass. The existing behavior works because the OLD dispatch mechanism is still in place. The new infrastructure was added alongside but not integrated.

### Criterion 8: New focus targets can be added without modifying any enum

- **Status**: gap
- **Evidence**: With the trait-based `FocusTarget` design, new targets could be added without modifying `FocusLayer`. However, since `EditorFocus` is still the actual dispatch mechanism, any new focus mode would STILL require modifying `EditorFocus` enum, `EditorState::handle_key`, and related match arms.

## Feedback Items

### Issue 1: Integration Step Not Completed

- **Location**: `crates/editor/src/editor_state.rs`
- **Concern**: Steps 7-10 from PLAN.md were not executed. The `FocusStack` infrastructure is complete but `EditorState` still uses `EditorFocus` enum with manual dispatch.
- **Suggestion**: Complete Step 7 (Integrate `FocusStack` into `EditorState`) by:
  1. Add `focus_stack: FocusStack` field to `EditorState`
  2. Initialize with `GlobalShortcutTarget` at bottom and `BufferFocusTarget` above
  3. Replace `handle_key` dispatch with `self.focus_stack.dispatch_key(event, &mut ctx)`
  4. Replace overlay open methods with `focus_stack.push()`
  5. Replace overlay close methods with `focus_stack.pop()`
- **Severity**: functional
- **Confidence**: high

### Issue 2: EditorFocus Enum Not Removed

- **Location**: `crates/editor/src/editor_state.rs:71-83`
- **Concern**: Step 10 from PLAN.md (Remove `EditorFocus` enum) was not executed. The enum and all match arms using it remain.
- **Suggestion**: After completing Step 7, remove the `EditorFocus` enum and the `focus: EditorFocus` field from `EditorState`. The `FocusLayer` enum returned by `focus_stack.top_layer()` serves the same purpose for rendering decisions.
- **Severity**: functional
- **Confidence**: high

### Issue 3: GlobalShortcutTarget Not Invoked

- **Location**: `crates/editor/src/editor_state.rs:740-920`
- **Concern**: The `GlobalShortcutTarget` implementation exists but is never used. Global shortcuts are still handled by hardcoded conditionals at the top of `handle_key()`.
- **Suggestion**: When integrating `FocusStack`, remove the hardcoded shortcut handling from `handle_key()` and let `GlobalShortcutTarget` handle them via stack dispatch. After dispatch, check `global_target.take_action()` and execute the action on `EditorState`.
- **Severity**: functional
- **Confidence**: high
