---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/metal_view.rs
- crates/editor/src/drain_loop.rs
- crates/editor/src/editor_event.rs
- crates/editor/src/event_channel.rs
- crates/editor/src/editor_state.rs
- crates/editor/src/shell_escape.rs
code_references:
- ref: crates/editor/src/shell_escape.rs#shell_escape_path
  implements: "Single path shell escaping using POSIX single-quote escaping"
- ref: crates/editor/src/shell_escape.rs#shell_escape_paths
  implements: "Multiple path shell escaping with space-separated joining"
- ref: crates/editor/src/editor_event.rs#EditorEvent::FileDrop
  implements: "Event variant for drag-and-drop file paths"
- ref: crates/editor/src/event_channel.rs#EventSender::send_file_drop
  implements: "Event sender method for file drop events"
- ref: crates/editor/src/metal_view.rs#MetalView::__dragging_entered
  implements: "NSDraggingDestination protocol - drag entered handler"
- ref: crates/editor/src/metal_view.rs#MetalView::__perform_drag_operation
  implements: "NSDraggingDestination protocol - extract file URLs from pasteboard"
- ref: crates/editor/src/metal_view.rs#MetalView::new
  implements: "Register view for file URL drag types via registerForDraggedTypes"
- ref: crates/editor/src/drain_loop.rs#EventDrainLoop::handle_file_drop
  implements: "Drain loop routing of FileDrop events to EditorState"
- ref: crates/editor/src/editor_state.rs#EditorState::handle_file_drop
  implements: "File drop insertion logic - shell escapes paths and routes to terminal or buffer"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- welcome_scroll
---

# Chunk Goal

## Minor Goal

Add drag-and-drop file path insertion to lite-edit's terminal. When a user drags one or more files from Finder (or another app) and drops them onto a terminal pane, the file paths are inserted as text at the cursor position — exactly as Alacritty does it.

This works at the macOS Cocoa layer: the `NSView` subclass in `metal_view.rs` must register as an `NSDraggingDestination`, declare accepted drag types (file URLs), and on a successful drop convert each `NSURL` to a string path and write it into the active focus target as if it were pasted text.

This is **not** about inline image rendering (Kitty graphics protocol, iTerm2 OSC 1337, etc.) — that is a separate, much larger effort. The goal here is the same simple path-insertion behavior that Alacritty provides, which is what makes "drag an image file into the terminal and Claude Code can see the path" work.

## Success Criteria

- Dragging a single file from Finder onto a terminal pane inserts its absolute path as text (e.g., `/Users/btaylor/Desktop/screenshot.png`).
- Dragging multiple files inserts all paths, space-separated (consistent with how other terminals behave).
- Dragging onto a non-terminal pane (buffer editor) is a no-op or also inserts paths — use whatever is simplest given the architecture.
- The drag visual (cursor change to copy indicator) works correctly during hover.
- No crash or panic when a non-file drag type (e.g., plain text from another app) is dropped.
- The feature works at normal and retina scale factors.

## Rejected Ideas

### Support inline image rendering (Kitty/iTerm2 protocols)

The user observed that Claude Code can receive images pasted into the terminal. Investigation revealed this is because Alacritty inserts the *file path* as text — Claude Code then reads that path. Inline image rendering via escape-sequence protocols (Kitty Graphics Protocol, iTerm2 OSC 1337) is a separate feature requiring a renderer change and is explicitly out of scope for this chunk.

---