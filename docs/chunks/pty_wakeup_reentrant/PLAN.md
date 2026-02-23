<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The current architecture uses `Rc<RefCell<EditorController>>` shared across 5 event sources (key, mouse, scroll, PTY wakeup, blink timer), each holding a clone and calling `borrow_mut()` independently. This creates reentrant borrow panics when `dispatch_async` delivers PTY wakeup callbacks while the controller is already borrowed (e.g., during workspace open which triggers modal dialogs).

The solution is to replace the callback-borrows-controller pattern with a **unified event queue** and a **CFRunLoopSource** drain mechanism:

1. **Introduce `EditorEvent` enum** — all event types (`Key`, `Mouse`, `Scroll`, `PtyWakeup`, `CursorBlink`, `Resize`) flow through a single channel
2. **Single ownership of `EditorController`** — the drain callback owns the controller directly (plain struct, no `Rc`, no `RefCell`), processing events one at a time
3. **CFRunLoopSource for event delivery** — when events arrive (from any source), signal the source to wake the run loop and drain the channel

This eliminates `RefCell` entirely by ensuring the controller is only accessed from one code path: the drain loop. The `mpsc` channel is the synchronization boundary — background threads (PTY reader) send events, and the main thread drains them in the single run loop callback.

### Key Design Decisions

- **Use `std::sync::mpsc` channel**: The PTY reader thread is the only background producer. `mpsc::Sender` is `Send` but `mpsc::Receiver` is not — this is exactly what we want. The receiver stays on the main thread.
- **CFRunLoopSource instead of dispatch_async**: The current PTY wakeup uses `dispatch_async(main_queue, callback)`, which delivers callbacks via a separate code path that races with event handlers. `CFRunLoopSource` integrates with `NSRunLoop` more cleanly — we signal the source and the run loop wakes and calls our drain callback, which has exclusive access to the controller.
- **Debouncing preserved**: Multiple rapid PTY outputs coalesce naturally because we drain the entire channel before rendering once. No need for explicit debounce logic.

### Pattern Alignment

This follows the drain-all-then-render pattern documented in the `editor_core_architecture` investigation:

```
loop {
    // Drain all events from the channel
    while let Ok(event) = channel.try_recv() {
        process_event(event, &mut controller);
    }
    // Render once if dirty
    if controller.state.is_dirty() {
        controller.render_if_dirty();
    }
    // NSRunLoop sleeps until next event or timer
}
```

The difference from the current architecture is that **all** event sources now use this pattern, not just keyboard events. The blink timer, PTY wakeup, and resize events all send to the channel rather than calling `borrow_mut()` directly.

## Subsystem Considerations

No subsystems are directly relevant to this refactoring. The `viewport_scroll` subsystem (`docs/subsystems/viewport_scroll`) is touched by this chunk in that scroll events flow through the new channel, but the scroll handling logic itself doesn't change — only the delivery mechanism does.

## Sequence

### Step 1: Define the EditorEvent enum

Create a new file `crates/editor/src/editor_event.rs` with:

```rust
pub enum EditorEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Scroll(ScrollDelta),
    PtyWakeup,
    CursorBlink,
    Resize,
}
```

This is the unified event type that all sources will send.

**Location**: `crates/editor/src/editor_event.rs` (new file)

### Step 2: Create the event channel module

Create `crates/editor/src/event_channel.rs` with:

- `EventSender` — a wrapper around `mpsc::Sender<EditorEvent>` that is `Clone + Send`
- `EventReceiver` — a wrapper around `mpsc::Receiver<EditorEvent>` that is `!Send` (main thread only)
- `create_event_channel()` function that returns `(EventSender, EventReceiver)`

The sender wrapper should provide typed convenience methods: `send_key(KeyEvent)`, `send_mouse(MouseEvent)`, `send_pty_wakeup()`, etc.

**Location**: `crates/editor/src/event_channel.rs` (new file)

### Step 3: Implement CFRunLoopSource wrapper

Create `crates/editor/src/runloop_source.rs` with:

- `RunLoopSource` struct wrapping a `CFRunLoopSourceRef`
- Constructor that takes a callback closure
- `signal()` method that calls `CFRunLoopSourceSignal` + `CFRunLoopWakeUp`
- Proper `Drop` implementation to invalidate and release the source

The callback will be invoked by the run loop when signaled. It should drain the event channel and process events.

Note: Use `objc2-core-foundation` bindings for `CFRunLoopSourceRef`, `CFRunLoopGetCurrent`, `CFRunLoopAddSource`, `CFRunLoopSourceSignal`, `CFRunLoopWakeUp`.

**Location**: `crates/editor/src/runloop_source.rs` (new file)

### Step 4: Create EventDrainLoop structure

Create `crates/editor/src/drain_loop.rs` with:

- `EventDrainLoop` struct that owns:
  - `EditorController` (directly, no Rc/RefCell)
  - `EventReceiver`
  - `RunLoopSource`
- `process_pending_events(&mut self)` method that drains the channel and processes each event
- Methods to handle each event type (delegating to controller methods)

This is the single point of access to the controller. No other code will touch it directly.

**Location**: `crates/editor/src/drain_loop.rs` (new file)

### Step 5: Update MetalView to use EventSender

Modify `crates/editor/src/metal_view.rs`:

- Replace `key_handler: RefCell<Option<Box<dyn Fn(KeyEvent)>>>` with `key_sender: RefCell<Option<EventSender>>`
- Same for `mouse_handler` and `scroll_handler`
- The `set_key_handler` method becomes `set_event_sender`
- NSView callbacks (`keyDown`, `mouseDown`, `scrollWheel`) now call `sender.send_key(event)` etc.

This eliminates the closure-holds-Rc pattern from the view.

**Location**: `crates/editor/src/metal_view.rs`

### Step 6: Update PtyWakeup to use EventSender

Modify `crates/terminal/src/pty_wakeup.rs`:

- Change `PtyWakeup` to hold an `EventSender` (or a trait object for cross-crate use)
- The `signal()` method now calls `sender.send_pty_wakeup()` instead of `dispatch_async`
- Remove the global callback pattern (`WAKEUP_CALLBACK` static, `set_global_wakeup_callback`)

This requires updating the PTY spawning path to pass an `EventSender` instead of registering a global callback.

**Location**: `crates/terminal/src/pty_wakeup.rs`, `crates/terminal/src/lib.rs`

### Step 7: Update blink timer to use EventSender

Modify `crates/editor/src/main.rs`:

- The blink timer callback now calls `sender.send_blink()` instead of `controller.borrow_mut().toggle_cursor_blink()`
- The timer holds an `EventSender`, not an `Rc<RefCell<EditorController>>`

**Location**: `crates/editor/src/main.rs`

### Step 8: Update window delegate to use EventSender

Modify `crates/editor/src/main.rs`:

- `windowDidResize:` and `windowDidChangeBackingProperties:` now call `sender.send_resize()` instead of `controller.try_borrow_mut().handle_resize()`
- The delegate holds an `EventSender`, not a reference to the controller

**Location**: `crates/editor/src/main.rs`

### Step 9: Refactor AppDelegate and main() to use EventDrainLoop

Major refactoring of `crates/editor/src/main.rs`:

1. Create the event channel in `setup_window`
2. Create `EditorController` directly (not wrapped in Rc/RefCell)
3. Create `EventDrainLoop` owning the controller and receiver
4. Create `RunLoopSource` with callback that calls `drain_loop.process_pending_events()`
5. Store `EventSender` in `AppDelegateIvars` for use by window delegate methods
6. Pass `EventSender` clone to MetalView, blink timer, and PTY wakeup factory

Remove:
- `PTY_WAKEUP_CONTROLLER` thread local
- `handle_pty_wakeup_global` function
- All `Rc<RefCell<EditorController>>` usage
- All `borrow_mut()` and `try_borrow_mut()` calls on the controller

**Location**: `crates/editor/src/main.rs`

### Step 10: Update EditorState PTY wakeup factory

Modify `crates/editor/src/editor_state.rs`:

- Change `pty_wakeup_factory` from `Option<Arc<dyn Fn() -> PtyWakeup + Send + Sync>>` to `Option<EventSender>`
- `create_pty_wakeup()` now creates a `PtyWakeup` from the stored sender
- Update terminal spawning to use the new pattern

**Location**: `crates/editor/src/editor_state.rs`

### Step 11: Write integration tests for the event flow

Create tests that verify:
1. Key events sent via `EventSender` are received and processed
2. PTY wakeup events trigger `poll_agents` and render
3. Multiple rapid events are batched (drain-all-then-render)
4. Concurrent PTY output and keyboard input don't cause panics

These tests will need to mock or stub the MetalView/renderer since they require a window.

**Location**: `crates/editor/tests/event_channel_integration.rs` (new file)

### Step 12: Clean up and remove dead code

- Remove `crates/terminal/src/pty_wakeup.rs` global callback infrastructure
- Remove `set_global_wakeup_callback` from public API
- Update `crates/terminal/tests/wakeup_integration.rs` to use new pattern
- Update any documentation that references the old pattern

**Location**: Multiple files

## Dependencies

### External Libraries

May need to add or verify:
- `objc2-core-foundation` — for CFRunLoopSource APIs

Check `crates/editor/Cargo.toml` for existing CF bindings. The project already uses `objc2-foundation` which may include some CF types.

### Internal Dependencies

- Parent chunk `terminal_pty_wakeup` — this chunk supersedes its implementation but keeps its concept (PTY reader signals main thread)

## Risks and Open Questions

### CFRunLoopSource callback lifetime

The drain callback needs to reference the `EventDrainLoop` which owns the controller. We'll likely need to store the drain loop in a `Box::leak` or use raw pointers since CFRunLoopSource callbacks use void* context. Careful about lifetimes.

**Mitigation**: The drain loop lives for the application lifetime. We can use `Box::leak` to get a `'static` reference, or use a global (the drain loop is fundamentally a singleton anyway).

### Cross-crate EventSender

`PtyWakeup` is in `crates/terminal/` but needs to send events to the channel in `crates/editor/`. We need either:
1. A trait in a shared crate (like `lite-edit-input`) that abstracts over the sender
2. Move `EventSender` to a shared crate
3. Have `PtyWakeup` accept a callback/closure instead of a sender directly

**Preferred**: Option 1 — define a `WakeupSignal` trait in `lite-edit-input` that `EventSender` implements. This maintains clean crate boundaries.

### Thread safety of EventSender

`mpsc::Sender` is `Send` but uses `Arc` internally. Verify this is acceptable for the PTY reader thread use case. Alternatively, use `crossbeam-channel` which is already a dependency of `crates/terminal/`.

**Mitigation**: The existing code uses `crossbeam_channel` for PTY events within the terminal crate. We could use the same channel type for the editor event queue for consistency.

### Timer vs CFRunLoopSource for blink

The current blink timer uses `NSTimer::scheduledTimerWithTimeInterval_repeats_block`. This already integrates with NSRunLoop. We could either:
1. Keep the timer but have it send to the channel
2. Replace with CFRunLoopTimer

**Preferred**: Option 1 — keep NSTimer, have callback send to channel. This is the minimal change.

### Resize event ordering

Resize events currently call `handle_resize()` directly. Sending them through the channel adds latency (they queue behind other events). Verify this doesn't cause visual glitches during window resize.

**Mitigation**: Resize events are relatively rare and the channel is drained immediately. Should be fine.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->