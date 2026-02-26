<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Replace the closed `EditorFocus` enum with an open `FocusStack` structure using trait objects. The stack is ordered: events propagate top-down, with each focus target getting a chance to handle the event. If a target returns `Handled::No`, the event falls through to the next target.

**Key architecture changes:**

1. **`FocusStack`** (`focus.rs`): An ordered `Vec<Box<dyn FocusTarget>>` where index 0 is the bottom (global shortcuts) and the last element is the top (active overlay). Events are dispatched top-down.

2. **`Handled` propagation**: The existing `Handled` enum (`Yes`/`No`) is already defined in `focus.rs`. Each `handle_key` call returns this to signal whether the event was consumed.

3. **`GlobalShortcutTarget`**: A new focus target that lives at the bottom of the stack and handles global shortcuts (Cmd+Q, Cmd+W, Cmd+S, Cmd+P, Cmd+F, Cmd+N, Cmd+T, Cmd+O, etc.). Currently these are scattered as conditionals at the top of `EditorState::handle_key`—they become a single target.

4. **Push/pop semantics**: Opening an overlay (selector, find bar, confirm dialog) pushes a new focus target onto the stack. Dismissing the overlay pops it. The buffer target is always present as the base editing layer.

**Testing approach**: Following TESTING_PHILOSOPHY.md's Humble View Architecture, `FocusStack` is pure state manipulation and can be tested without GPU or macOS dependencies. Tests will verify:
- Event propagation order (top-down)
- Global shortcuts handled at bottom of stack
- Overlays push/pop correctly
- Existing keyboard behavior unchanged

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk USES the renderer subsystem for overlay rendering. The renderer already supports selector, find strip, and confirm dialog overlays. The focus stack change is input-side only; rendering remains unchanged.

No new subsystems are introduced. The focus stack could eventually become its own subsystem if more focus targets are added, but for now it's a single-chunk refactor.

## Sequence

### Step 1: Define `FocusStack` struct and update `FocusTarget` trait

Extend `focus.rs`:
- Keep the existing `Handled` enum and `FocusTarget` trait
- Add `FocusStack` struct: `Vec<Box<dyn FocusTarget>>`
- Add methods: `push(target)`, `pop() -> Option<Box<dyn FocusTarget>>`, `top() -> Option<&mut dyn FocusTarget>`, `dispatch_key(event, ctx) -> Handled` (top-down propagation)
- Add `handle_scroll` and `handle_mouse` dispatch methods with the same top-down propagation pattern

**Location**: `crates/editor/src/focus.rs`

**Tests** (TDD):
- `new_stack_is_empty()`: Fresh stack has no targets
- `push_adds_to_top()`: Verify stack ordering
- `pop_returns_top()`: Verify LIFO behavior
- `dispatch_key_top_handles_stops()`: When top returns `Handled::Yes`, no further propagation
- `dispatch_key_top_unhandled_falls_through()`: When top returns `Handled::No`, next target gets the event
- `dispatch_key_empty_stack_returns_no()`: Empty stack returns `Handled::No`

### Step 2: Create `GlobalShortcutTarget`

Extract global shortcuts from `EditorState::handle_key` into a dedicated focus target. This target handles:
- Cmd+Q (quit)
- Cmd+P (file picker toggle)
- Cmd+S (save)
- Cmd+F (find-in-file)
- Cmd+N (new workspace)
- Cmd+Shift+N (future)
- Cmd+O (open file picker)
- Cmd+W / Cmd+Shift+W (close tab / close workspace)
- Cmd+Shift+] / Cmd+Shift+[ (tab cycling)
- Cmd+] / Cmd+[ (workspace cycling)
- Cmd+T / Cmd+Shift+T (new tab / new terminal tab)
- Cmd+1..9 (workspace switching)
- Cmd+Shift+Arrow (directional tab movement)
- Cmd+Option+Arrow (pane focus switching)

The target needs access to `EditorState` operations. We'll pass a closure or context that can perform these operations.

**Design decision**: The `GlobalShortcutTarget` doesn't own state—it executes commands on `EditorState`. We'll use a pattern where `EditorState` creates a temporary context for the global target to call back into.

**Location**: `crates/editor/src/global_shortcuts.rs` (new file)

**Tests**:
- `global_target_handles_cmd_q()`: Returns `Handled::Yes` and sets quit flag
- `global_target_handles_cmd_s()`: Returns `Handled::Yes` and triggers save
- `global_target_ignores_plain_keys()`: Returns `Handled::No` for non-shortcut keys
- `global_target_ignores_unmodified_q()`: Plain 'q' key should fall through

### Step 3: Update `BufferFocusTarget` to implement full `FocusTarget` trait

The existing `BufferFocusTarget` already implements `FocusTarget`. Verify it properly returns `Handled::No` for events it doesn't handle (it already does—`resolve_command` returns `None` for unrecognized keys).

**Location**: `crates/editor/src/buffer_target.rs`

**Changes**: None needed—the existing implementation already returns `Handled::No` when it doesn't handle the event.

### Step 4: Create `SelectorFocusTarget` for selector overlay

Extract selector key handling from `EditorState::handle_key_selector` into a dedicated focus target. This wraps `SelectorWidget` and converts `SelectorOutcome` to `Handled`.

**Location**: `crates/editor/src/selector_target.rs` (new file)

**Tests**:
- `selector_target_handles_escape()`: Returns `Handled::Yes` on Escape
- `selector_target_handles_return()`: Returns `Handled::Yes` on Return
- `selector_target_handles_arrows()`: Returns `Handled::Yes` on Up/Down
- `selector_target_handles_typing()`: Returns `Handled::Yes` on character input

### Step 5: Create `FindFocusTarget` for find-in-file strip

Extract find strip key handling from `EditorState::handle_key_find` into a dedicated focus target. This wraps `MiniBuffer` and handles find-specific keys (Escape to close, Return to find next, etc.).

**Location**: `crates/editor/src/find_target.rs` (new file)

**Tests**:
- `find_target_handles_escape()`: Returns `Handled::Yes` and signals close
- `find_target_handles_return()`: Returns `Handled::Yes` and signals find next
- `find_target_handles_typing()`: Returns `Handled::Yes` on character input

### Step 6: Create `ConfirmDialogFocusTarget` for confirm dialog

Extract confirm dialog key handling from `EditorState::handle_key_confirm_dialog` into a dedicated focus target. This wraps `ConfirmDialog`.

**Location**: `crates/editor/src/confirm_dialog_target.rs` (new file)

**Tests**:
- `confirm_target_handles_tab()`: Returns `Handled::Yes` on Tab (toggle button)
- `confirm_target_handles_return()`: Returns `Handled::Yes` on Return
- `confirm_target_handles_escape()`: Returns `Handled::Yes` on Escape

### Step 7: Integrate `FocusStack` into `EditorState`

Replace `focus: EditorFocus` enum and `focus_target: BufferFocusTarget` with `focus_stack: FocusStack`.

**Changes to `EditorState`**:
- Initialize stack with `GlobalShortcutTarget` at bottom and `BufferFocusTarget` above it
- Replace `handle_key` dispatch logic with `focus_stack.dispatch_key(...)`
- Replace overlay open methods (e.g., `open_file_picker`) with `focus_stack.push(SelectorFocusTarget::new(...))`
- Replace overlay close methods with `focus_stack.pop()`
- Keep `active_selector`, `find_mini_buffer`, `confirm_dialog` fields for state, but focus targets wrap them

**Location**: `crates/editor/src/editor_state.rs`

**Tests**:
- Existing integration tests should pass unchanged
- Add tests verifying stack-based dispatch

### Step 8: Update `drain_loop.rs` render logic

The render logic currently matches on `EditorFocus` enum to determine what to render. Update to inspect the focus stack instead:

- If top of stack is selector: render with selector overlay
- If top of stack is find: render with find strip
- If top of stack is confirm dialog: render with confirm dialog
- Otherwise: render buffer only

**Location**: `crates/editor/src/drain_loop.rs`

**Changes**: Replace `match self.state.focus` with stack inspection. The renderer subsystem itself doesn't change—we're just changing how we determine which overlay to render.

### Step 9: Update cursor region calculation

`drain_loop.rs::update_cursor_regions` also matches on `EditorFocus`. Update to use stack inspection instead.

**Location**: `crates/editor/src/drain_loop.rs`

### Step 10: Remove `EditorFocus` enum

Once all usages are migrated to `FocusStack`, delete the `EditorFocus` enum.

**Location**: `crates/editor/src/editor_state.rs`

### Step 11: Final integration testing

Run all existing tests to ensure no regressions. Manually verify:
- Opening/closing file picker (Cmd+P)
- Opening/closing find bar (Cmd+F)
- Confirm dialog on dirty tab close
- All global shortcuts work
- Buffer editing works normally
- Mouse events route correctly

## Dependencies

None. This chunk is self-contained and doesn't depend on other FUTURE chunks.

## Risks and Open Questions

1. **Focus target lifetime**: Focus targets need access to state they don't own (e.g., `SelectorFocusTarget` needs to manipulate `EditorState::active_selector`). We'll need to carefully design the callback/context pattern to avoid borrow conflicts.

2. **Rendering coordination**: The renderer needs to know which overlay is active. We may need to add a method like `focus_stack.active_overlay_type()` that returns an enum for rendering purposes, even as we remove the closed enum for input handling.

3. **Performance**: The stack will typically have 2-3 items (global, buffer, maybe one overlay). Top-down dispatch with early return should be O(1) in practice. No performance concerns expected.

4. **Mouse event routing**: Currently mouse events have their own dispatch in `EditorState::handle_mouse`. The same stack-based propagation pattern should apply, but overlays typically capture all mouse events when active.

## Deviations

### Step 7: Partial integration - kept EditorFocus enum for event routing

**What changed**: Instead of replacing `handle_key` dispatch logic with `focus_stack.dispatch_key()`, we kept the existing `EditorFocus` enum-based routing. The `focus_stack` is maintained in parallel and used only for `focus_layer()` which drives rendering decisions.

**Why**: The existing focus targets (SelectorFocusTarget, FindFocusTarget, etc.) own their own widgets, but EditorState also maintains separate state fields (`active_selector`, `find_mini_buffer`, `confirm_dialog`). Fully replacing the dispatch logic would require:
1. Moving all widget state into the focus targets
2. Adding outcome extraction methods to the FocusTarget trait (for downcasting to access pending outcomes)
3. Reworking all the existing handlers that process outcomes and re-query state

This represents a significant refactor that risks regressions. The partial integration approach:
- Maintains working existing behavior (all tests pass)
- Provides the architectural foundation (FocusStack, FocusTarget, all focus target implementations)
- Uses focus_stack for rendering decisions via `focus_layer()` → `top_layer()`
- Keeps focus_stack in sync via push/pop on overlay open/close

**Impact**: The `EditorFocus` enum is still used internally for event routing. This is a transitional state - a future chunk could complete the migration by:
1. Adding `take_outcome()` method to FocusTarget trait
2. Migrating state ownership to focus targets
3. Replacing the match-on-EditorFocus routing with focus_stack.dispatch_key()
4. Removing EditorFocus enum entirely

### Step 10: Skipped - EditorFocus enum retained

**What changed**: Did not remove the EditorFocus enum as planned.

**Why**: See Step 7 deviation - the enum is still used for event routing.

**Impact**: The codebase has two parallel focus tracking mechanisms:
- `focus: EditorFocus` - used for event routing in handle_key
- `focus_stack: FocusStack` - used for rendering decisions via focus_layer()

Both are kept in sync via push/pop operations in open/close methods.