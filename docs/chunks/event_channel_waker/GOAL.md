---
status: ACTIVE
ticket: null
parent_chunk: pty_wakeup_reentrant
code_paths:
- crates/editor/src/event_channel.rs
code_references:
  - ref: crates/editor/src/event_channel.rs#EventSender::send_key
    implements: "Key event waker call fix"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_mouse
    implements: "Mouse event waker call fix"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_scroll
    implements: "Scroll event waker call fix"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_cursor_blink
    implements: "Cursor blink event waker call fix"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_resize
    implements: "Resize event waker call fix"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- pty_wakeup_reentrant
- terminal_shell_env
---

# Chunk Goal

## Minor Goal

Fix run loop waker calls missing from non-PTY event senders in `EventSender`.

The `pty_wakeup_reentrant` chunk introduced a channel-based event architecture
where all event sources enqueue into an mpsc channel and a CFRunLoopSource
callback drains and processes them. However, only `send_pty_wakeup()` calls
the `run_loop_waker` after enqueueing. The other senders (`send_key`,
`send_mouse`, `send_scroll`, `send_cursor_blink`, `send_resize`) enqueue
events but never signal the CFRunLoopSource, so those events are never
drained. This makes the editor unresponsive to all keyboard, mouse, and
scroll input.

## Success Criteria

- All `EventSender` send methods (`send_key`, `send_mouse`, `send_scroll`,
  `send_cursor_blink`, `send_resize`) call `run_loop_waker` after enqueueing
- Editor responds to hotkeys (Cmd+P, Cmd+S, etc.) when running
- Mouse clicks and scroll events are processed
- Cursor blink and window resize events are processed
- Existing tests pass; new tests verify the waker is called for each event type

## Relationship to Parent

The parent chunk `pty_wakeup_reentrant` introduced the event channel
architecture (`event_channel.rs`) and correctly wired the PTY wakeup path
with the run loop waker. The overall channel + drain loop design is sound.
The only deficiency is that the non-PTY send methods were left without waker
calls, making them dead-letter events that are enqueued but never processed.

