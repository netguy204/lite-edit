---
status: ACTIVE
ticket: null
parent_chunk: terminal_pty_wakeup
code_paths:
  - crates/editor/src/editor_event.rs
  - crates/editor/src/event_channel.rs
  - crates/editor/src/runloop_source.rs
  - crates/editor/src/drain_loop.rs
  - crates/editor/src/main.rs
  - crates/editor/src/metal_view.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/Cargo.toml
  - crates/terminal/src/pty_wakeup.rs
  - crates/terminal/src/lib.rs
  - crates/terminal/tests/wakeup_integration.rs
  - crates/input/src/lib.rs
code_references:
  - ref: crates/editor/src/editor_event.rs#EditorEvent
    implements: "Unified event type enum with variants for all event sources (Key, Mouse, Scroll, PtyWakeup, CursorBlink, Resize)"
  - ref: crates/editor/src/event_channel.rs#EventSender
    implements: "Thread-safe event sender with typed convenience methods and WakeupSignal implementation"
  - ref: crates/editor/src/event_channel.rs#EventReceiver
    implements: "Main-thread-only event receiver with drain() method for batch processing"
  - ref: crates/editor/src/event_channel.rs#create_event_channel
    implements: "Factory function creating sender/receiver pair with run loop waker callback"
  - ref: crates/editor/src/runloop_source.rs#RunLoopSource
    implements: "CFRunLoopSource wrapper for waking main run loop from background threads"
  - ref: crates/editor/src/runloop_source.rs#create_waker
    implements: "Creates a waker function that signals the CFRunLoopSource"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop
    implements: "Single-ownership event processor that owns EditorState/Renderer directly (no Rc/RefCell)"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::process_pending_events
    implements: "Drains all queued events and processes them sequentially with exclusive controller access"
  - ref: crates/editor/src/main.rs#DRAIN_LOOP
    implements: "Global pointer to drain loop for CFRunLoopSource callback access"
  - ref: crates/editor/src/main.rs#AppDelegate::setup_window
    implements: "Event queue initialization: creates channel, RunLoopSource, EventDrainLoop, and wires them together"
  - ref: crates/editor/src/metal_view.rs#MetalView::set_event_sender
    implements: "Connects NSView to event channel (EventSender replaces closure-based handlers)"
  - ref: crates/editor/src/editor_state.rs#EditorState::set_event_sender
    implements: "Stores EventSender for creating PtyWakeup handles"
  - ref: crates/editor/src/editor_state.rs#EditorState::create_pty_wakeup
    implements: "Creates PtyWakeup with WakeupSignal trait (replaces global callback pattern)"
  - ref: crates/terminal/src/pty_wakeup.rs#PtyWakeup::with_signal
    implements: "New constructor accepting Box<dyn WakeupSignal> for trait-based signaling"
  - ref: crates/input/src/lib.rs#WakeupSignal
    implements: "Cross-crate trait enabling terminal to signal editor's event loop"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- unsaved_tab_tint
- cursor_blink_pane_focus
- pane_hover_scroll
---

# Chunk Goal

## Minor Goal

Eliminate the `Rc<RefCell<EditorController>>` shared-ownership pattern that causes reentrant borrow panics. The immediate trigger is a crash when a PTY wakeup fires via `dispatch_async` while the controller is already mutably borrowed (e.g., during workspace open), but the underlying problem is structural: 5 event sources (key, mouse, scroll, PTY wakeup, blink timer) each hold an `Rc<RefCell<EditorController>>` clone and race to `borrow_mut()` within the same main-thread run loop cycle.

### Crash trace

```
Thread 0 Crashed:: main Dispatch queue: com.apple.main-thread
11  lite-edit  core::cell::panic_already_borrowed::do_panic::runtime
12  lite-edit  core::cell::panic_already_borrowed
13  lite-edit  lite_edit::handle_pty_wakeup_global + 360
14  lite-edit  dispatch2::utils::function_wrapper
```

### Design: unified event queue with CFRunLoopSource

Replace the current callback-borrows-controller pattern with a single `mpsc` channel drained by a `CFRunLoopSource` on the main run loop:

```
NSView key/mouse/scroll callbacks ──┐
PTY reader thread ──────────────────┤──→ mpsc::Sender<EditorEvent> ──→ CFRunLoopSource ──→ drain & process
Blink timer ────────────────────────┘
```

1. **Introduce `EditorEvent` enum** — variants for `Key(KeyEvent)`, `Mouse(MouseEvent)`, `Scroll(ScrollDelta)`, `PtyWakeup`, `CursorBlink`, `Resize`, etc.
2. **All event sources send to `mpsc::Sender<EditorEvent>`** — NS callbacks convert and send; PTY reader thread sends directly (replacing `dispatch_async` + `PtyWakeup::signal`); blink timer sends `CursorBlink`.
3. **`CFRunLoopSource` drain callback owns `EditorController`** — no `Rc`, no `RefCell`. The drain callback holds `&mut EditorController` exclusively, processes all queued events, then calls `render_if_dirty()`.
4. **Wake mechanism** — `CFRunLoopSourceSignal` + `CFRunLoopWakeUp` when events arrive from background threads (PTY reader). For events already on the main thread (NS callbacks), the run loop is already awake and the source fires naturally in the same cycle.

This eliminates `RefCell` on the controller entirely. Reentrant dispatch becomes impossible because the controller is never borrowed through callbacks — it's owned by the drain loop.

## Success Criteria

- `EditorController` is owned directly (plain struct), not wrapped in `Rc<RefCell<>>`
- No `borrow_mut()` or `try_borrow_mut()` calls on the controller anywhere
- All event sources (key, mouse, scroll, PTY wakeup, blink timer, resize) flow through `mpsc::Sender<EditorEvent>`
- Opening a new workspace while a terminal tab has active PTY output does not crash
- PTY wakeup latency (<5ms from `write_input()` to render) is preserved
- No events are silently dropped
- Blink timer and window delegate events no longer need `try_borrow_mut` guards

## Relationship to Parent

Parent chunk `terminal_pty_wakeup` introduced the global PTY wakeup mechanism with `handle_pty_wakeup_global` and the `thread_local! { PTY_WAKEUP_CONTROLLER: RefCell<Weak<RefCell<EditorController>>> }` pattern. This chunk replaces that entire pattern — and the broader `Rc<RefCell<EditorController>>` sharing strategy — with a channel-based event queue. The PTY wakeup concept survives (PTY reader thread signals the main thread when data arrives) but the mechanism changes from `dispatch_async` + `RefCell` borrow to `mpsc::send` + `CFRunLoopSource` drain.