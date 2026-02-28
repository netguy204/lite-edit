<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This fix addresses a pane-targeting bug in drag-and-drop file insertion. The strategy is:

1. **Add position data to the `FileDrop` event** — The Cocoa `NSDraggingInfo` protocol provides `draggingLocation` which gives the drop coordinates in window space. Currently, `__perform_drag_operation` in `metal_view.rs` discards this data. We'll capture it and include it in the `FileDrop` event variant.

2. **Use `resolve_pane_hit` to target the correct pane** — The existing `resolve_pane_hit()` function (from `pane_cursor_click_offset` chunk) already solves pane hit-testing for mouse events. We'll reuse it in `handle_file_drop` to determine which pane the drop landed on, then route the file paths to that specific pane.

3. **Override `acceptsFirstMouse:` for click-through behavior** — When the window is not key (user is in another app), macOS normally consumes the first click to activate the window. By returning `true` from `acceptsFirstMouse:`, we ensure the initial drag-drop or click both activates the window AND delivers the event to the view, allowing pane focus to update correctly.

The approach follows existing patterns:
- The `FileDrop` event variant mirrors `FileRenamed { from, to }` struct-style syntax
- Position extraction follows the same `locationInWindow` → `convertPoint:fromView:` pattern used in `convert_mouse_event`
- Pane hit-testing follows the exact pattern in `handle_mouse_buffer`

No subsystems are directly relevant to this chunk. The renderer and viewport_scroll subsystems don't apply to event handling or drag-and-drop routing.

### Step 1: Change FileDrop event variant to include position

Modify the `EditorEvent::FileDrop` variant from a tuple struct to a struct variant that includes drop position:

Location: `crates/editor/src/editor_event.rs`

```rust
// Before:
FileDrop(Vec<String>),

// After:
FileDrop {
    paths: Vec<String>,
    /// Drop position in screen coordinates (y=0 at top)
    position: (f64, f64),
},
```

Update all match arms that destructure `FileDrop` to use the new struct pattern:
- `editor_event.rs` - `is_user_input()`, `require_redraw()`, Display impl
- `drain_loop.rs` - event handling
- `event_channel.rs` - test

### Step 2: Update send_file_drop to accept position

Modify `EventSender::send_file_drop()` in `event_channel.rs` to accept position:

```rust
pub fn send_file_drop(&self, paths: Vec<String>, position: (f64, f64)) -> Result<(), SendError<EditorEvent>> {
    let result = self.inner.sender.send(EditorEvent::FileDrop { paths, position });
    // ...
}
```

Update the unit test in `event_channel.rs` that exercises `FileDrop` to pass a position.

### Step 3: Extract drop position in __perform_drag_operation

In `metal_view.rs`, modify `__perform_drag_operation` to extract the drag location from `NSDraggingInfo` and convert it to view coordinates (same pattern as `convert_mouse_event`):

```rust
fn __perform_drag_operation(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
    // ... existing path extraction code ...

    // Get the drop location from NSDraggingInfo
    // draggingLocation returns NSPoint in window coordinates
    let location_in_window: objc2_foundation::NSPoint = sender.draggingLocation();

    // Convert to view coordinates (same as mouse events)
    let location_in_view: objc2_foundation::NSPoint =
        unsafe { msg_send![self, convertPoint: location_in_window, fromView: std::ptr::null::<NSView>()] };

    // Get scale factor and convert to pixels
    let scale = self.ivars().scale_factor.get();

    // Note: NSView uses bottom-left origin. We need to flip y to match screen coordinates.
    let frame = self.frame();
    let position = (
        location_in_view.x * scale,
        (frame.size.height - location_in_view.y) * scale,  // Flip y
    );

    // Send with position
    if let Some(event_sender) = event_sender_guard.as_ref() {
        let _ = event_sender.send_file_drop(paths, position);
    }
    // ...
}
```

### Step 4: Add acceptsFirstMouse: override

Add an `acceptsFirstMouse:` method override to `MetalView` that returns `true`, enabling click-through behavior when the window is not key:

Location: `crates/editor/src/metal_view.rs`

```rust
// In the declare_class! impl NSView section:

/// Returns true to accept mouse events on first click when window is inactive.
///
/// This enables click-through behavior: when lite-edit is not the key window,
/// the first click/drag both activates the window AND delivers the event to
/// the view. This is important for drag-and-drop from other apps so that the
/// pane under the drop point can receive focus.
// Chunk: docs/chunks/terminal_image_paste - acceptsFirstMouse for click-through
#[unsafe(method(acceptsFirstMouse:))]
fn __accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
    true
}
```

### Step 5: Modify handle_file_drop to accept position and use resolve_pane_hit

Update the `handle_file_drop` method signature in `drain_loop.rs` and `editor_state.rs` to accept position, then use `resolve_pane_hit` to determine which pane to target:

Location: `crates/editor/src/drain_loop.rs`

```rust
fn handle_file_drop(&mut self, paths: Vec<String>, position: (f64, f64)) {
    self.state.handle_file_drop(paths, position);
    self.poll_after_input();
}
```

Location: `crates/editor/src/editor_state.rs`

Replace the current `handle_file_drop` with position-aware pane targeting:

```rust
pub fn handle_file_drop(&mut self, paths: Vec<String>, position: (f64, f64)) {
    use crate::pane_layout::{resolve_pane_hit, HitZone};

    // Only handle drops when in Buffer focus mode
    if self.focus != EditorFocus::Buffer {
        return;
    }

    if paths.is_empty() {
        return;
    }

    let (screen_x, screen_y) = position;

    // Use renderer-consistent bounds for pane hit resolution
    let bounds = (
        RAIL_WIDTH,
        0.0,
        self.view_width - RAIL_WIDTH,
        self.view_height,
    );

    // Resolve which pane the drop landed on
    let hit = if let Some(workspace) = self.editor.active_workspace() {
        resolve_pane_hit(
            screen_x as f32,
            screen_y as f32,
            bounds,
            &workspace.pane_root,
            TAB_BAR_HEIGHT,
        )
    } else {
        return;
    };

    let Some(hit) = hit else {
        return; // Drop outside any pane
    };

    // Shell-escape and join the paths
    let escaped_text = shell_escape_paths(&paths);

    // Get the specific pane that was hit (not active_pane_id)
    let ws = match self.editor.active_workspace_mut() {
        Some(ws) => ws,
        None => return,
    };

    let pane = match ws.pane_root.get_pane_mut(hit.pane_id) {
        Some(pane) => pane,
        None => return,
    };

    let tab = match pane.active_tab_mut() {
        Some(tab) => tab,
        None => return,
    };

    // Route to terminal or buffer based on tab type
    if let Some((terminal, _viewport)) = tab.terminal_and_viewport_mut() {
        let modes = terminal.term_mode();
        let bytes = InputEncoder::encode_paste(&escaped_text, modes);
        if !bytes.is_empty() {
            let _ = terminal.write_input(&bytes);
        }
        return;
    }

    if let Some((buffer, viewport)) = tab.buffer_and_viewport_mut() {
        let dirty_lines = buffer.insert_str(&escaped_text);
        let dirty = viewport.dirty_lines_to_region(&dirty_lines, buffer.line_count());
        self.invalidation.merge(InvalidationKind::Content(dirty));
        self.dirty_lines.merge(dirty_lines);

        let cursor_line = buffer.cursor_position().line;
        if viewport.ensure_visible(cursor_line, buffer.line_count()) {
            self.invalidation.merge(InvalidationKind::Layout);
        }

        tab.dirty = true;
        self.sync_active_tab_highlighter();
    }
}
```

### Step 6: Write tests for position-aware file drop

Add unit tests that verify:

1. **File drop routes to correct pane** — In a horizontal split with a file buffer on the left and terminal on the right, dropping at coordinates within the right pane routes to the terminal, not the file buffer.

2. **File drop works when target pane is not active** — Verify that dropping on a non-active pane still routes to the correct pane.

3. **Tab bar drops are ignored** — Verify drops in the HitZone::TabBar region don't insert text.

Location: `crates/editor/src/editor_state.rs` (in the existing test module)

```rust
#[test]
fn test_file_drop_targets_pane_under_cursor() {
    // Create a horizontal split: file buffer (left) | terminal (right)
    // Drop coordinates within right pane should route to terminal
    // even if left pane is active
}

#[test]
fn test_file_drop_non_active_pane() {
    // Verify dropping on a pane that isn't active routes correctly
}

#[test]
fn test_file_drop_outside_panes_ignored() {
    // Drop outside pane bounds (e.g., in rail area) should be no-op
}
```

### Step 7: Update existing FileDrop tests

The existing tests in `editor_state.rs` for file drop behavior (escaping, terminal vs buffer routing) need to be updated to pass position. Use a position within the single pane's content area:

```rust
// Existing tests call:
state.handle_file_drop(vec!["/path/file.txt".to_string()]);

// Update to:
state.handle_file_drop(vec!["/path/file.txt".to_string()], (100.0, 100.0));
```

## Dependencies

- **pane_cursor_click_offset** (ACTIVE) — This chunk provides the `resolve_pane_hit()` function used to determine which pane a drop landed on. Already implemented.

## Risks and Open Questions

1. **Y-coordinate flip** — macOS NSView uses bottom-left origin, while our screen coordinates use top-left (y=0 at top). The `convert_mouse_event` method handles this by passing the raw `location_in_view.y` and letting the consumer flip it. For drag operations, we need to flip in `__perform_drag_operation` since we're creating the position tuple directly. We'll use `frame.size.height - location_in_view.y` similar to how mouse events are ultimately processed.

2. **Scale factor correctness** — The drop position must be converted from points to pixels using the same scale factor as mouse events. This is stored in `self.ivars().scale_factor`.

3. **acceptsFirstMouse: side effects** — Returning `true` changes window activation behavior for ALL mouse events, not just drag-and-drop. This is the desired behavior (it matches standard macOS click-through behavior in text editors), but we should verify there are no unintended side effects.

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