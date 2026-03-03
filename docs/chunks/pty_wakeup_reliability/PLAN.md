<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The current PTY wakeup pipeline has a fragile two-hop dispatch with double debouncing:

```
PTY reader thread
  → PtyWakeup::signal()          [debounce #1: PtyWakeup.pending AtomicBool]
    → DispatchQueue::main().exec_async()    [GCD hop — non-deterministic timing]
      → EventSender::send_pty_wakeup()      [debounce #2: wakeup_pending AtomicBool]
        → mpsc channel + CFRunLoopSourceSignal
```

The fix eliminates the GCD indirection by signaling the CFRunLoopSource directly from the PTY reader thread, and collapses the double debounce into a single atomic flag at the EventSender level.

**Key insight**: Both `CFRunLoopSourceSignal()` and `CFRunLoopWakeUp()` are thread-safe per Apple documentation. The PTY reader thread can call these directly without GCD intermediation.

**Architecture after fix**:

```
PTY reader thread
  → PtyWakeup::signal()
    → inner.signal.signal()       [calls WakeupSignal::signal() directly]
      → EventSender::send_pty_wakeup()   [single debounce: wakeup_pending AtomicBool]
        → mpsc channel send
        → (run_loop_waker)()      [CFRunLoopSourceSignal + CFRunLoopWakeUp]
```

The debouncing is now consolidated in `EventSender::send_pty_wakeup()`, and the GCD hop is eliminated. The atomic flag in `PtyWakeup` becomes redundant and is removed.

**Testing Strategy**: Per TESTING_PHILOSOPHY.md, we focus on testing the testable logic (debounce behavior, event delivery order) in unit tests. The actual PTY wakeup timing is verified through manual testing with vim/less/htop since it involves GPU rendering and platform state.

## Sequence

### Step 1: Remove GCD dispatch from PtyWakeup::signal()

Modify `crates/terminal/src/pty_wakeup.rs`:

1. Remove `DispatchQueue::main().exec_async()` wrapper in `PtyWakeup::signal()`
2. Remove the `pending: AtomicBool` field from `PtyWakeupInner` - the debounce is now handled entirely by `EventSender::wakeup_pending`
3. Call `signal.signal()` directly on the PTY reader thread (no GCD hop)
4. Remove the legacy global callback path entirely (it uses GCD and has the same problem)

The new `signal()` implementation becomes simply:

```rust
pub fn signal(&self) {
    if let Some(ref signal) = self.inner.signal {
        signal.signal();
    }
}
```

Location: `crates/terminal/src/pty_wakeup.rs`

### Step 2: Update PtyWakeup constructors

1. Remove `PtyWakeupInner::pending` field
2. Simplify `PtyWakeup::new()` - it now only stores the signal (or None for legacy)
3. Remove the legacy global callback code path and the `WAKEUP_CALLBACK` static
4. Remove `set_global_wakeup_callback()` function (no longer used)
5. Update the doc comments to reflect the new architecture

Location: `crates/terminal/src/pty_wakeup.rs`

### Step 3: Remove dispatch2 dependency from terminal crate

Since GCD is no longer used in the terminal crate:

1. Remove `use dispatch2::DispatchQueue` from `pty_wakeup.rs`
2. Check if `dispatch2` is used elsewhere in the terminal crate
3. If not, remove `dispatch2` from `crates/terminal/Cargo.toml`

Location: `crates/terminal/Cargo.toml`, `crates/terminal/src/pty_wakeup.rs`

### Step 4: Verify EventSender::send_pty_wakeup() debounce is sufficient

Review `crates/editor/src/event_channel.rs` to confirm:

1. `send_pty_wakeup()` already has the `wakeup_pending` AtomicBool debounce
2. `send_pty_wakeup_followup()` bypasses debouncing (used by terminal_flood_starvation for byte-budget continuation)
3. `clear_wakeup_pending()` is called after processing PtyWakeup events in the drain loop
4. The debounce semantics match the goal: at-most-one-wakeup-per-drain-cycle coalescing

No code changes expected here - this is a verification step.

Location: `crates/editor/src/event_channel.rs`, `crates/editor/src/drain_loop.rs`

### Step 5: Update drain_loop.rs wakeup_pending clearing

Verify that `clear_wakeup_pending()` is called at the right point in the drain loop to enable the next wakeup:

1. Currently called at the end of `process_pending_events()` after all PTY events are processed
2. This is correct - it allows new PTY data arriving after the drain cycle starts to trigger a new wakeup

No code changes expected here - this is a verification step.

Location: `crates/editor/src/drain_loop.rs`

### Step 6: Update existing tests

Review and update tests in:

1. `crates/terminal/tests/wakeup_integration.rs` - Update tests that may assume GCD dispatch behavior
2. `crates/editor/src/event_channel.rs` tests - Verify debounce tests still pass
3. Remove any tests that rely on the legacy global callback mechanism

Location: `crates/terminal/tests/wakeup_integration.rs`, `crates/editor/src/event_channel.rs`

### Step 7: Add thread-safety documentation

Add documentation comments explaining the thread-safety guarantees:

1. Document that `WakeupSignal::signal()` is called directly from the PTY reader thread
2. Document that `CFRunLoopSourceSignal()` and `CFRunLoopWakeUp()` are thread-safe
3. Document the single debounce point at `EventSender::wakeup_pending`

Location: `crates/terminal/src/pty_wakeup.rs`, `crates/editor/src/event_channel.rs`

### Step 8: Manual testing

Manual verification of the fix:

1. Open vim in a terminal tab - should paint immediately (no blank screen)
2. Type in vim - should echo immediately (no frozen frames)
3. Open less/man on a file - should paint immediately
4. Open htop - should continue to animate correctly
5. Run `yes` in 4 terminal panes - input should remain responsive (terminal_flood_starvation preserved)
6. Background the app, return - terminals should resume correctly (App Nap interaction)
7. Move terminal tab to split pane - should continue to work (resize triggers SIGWINCH)

### Step 9: Build and run tests

```bash
cargo build --release
cargo test -p lite-edit-terminal
cargo test -p lite-edit-editor
```

## Risks and Open Questions

1. **Thread safety of WakeupSignal::signal()**: The current `EventSender::send_pty_wakeup()` calls `self.inner.sender.send()` (mpsc sender is Send) and `(self.inner.run_loop_waker)()` (calls CFRunLoopSourceSignal/CFRunLoopWakeUp). Both are documented as thread-safe, but this is a change from the previous GCD-serialized path.

2. **Ordering between mpsc send and run loop wake**: There's a theoretical race where the run loop wakes before the event is in the channel. However, the drain loop calls `receiver.drain()` which uses `try_recv()` in a loop, so it will pick up the event on the same or next iteration. The existing code already has this pattern.

3. **Follow-up wakeup timing**: The `send_pty_wakeup_followup()` path (used by terminal_flood_starvation) must also work correctly without GCD. Since it also calls the run_loop_waker directly, no change is needed.

4. **Legacy code removal**: Removing `set_global_wakeup_callback()` and the legacy path. Need to verify no code still uses this. The chunk references indicate all PTY spawning goes through `create_pty_wakeup()` which uses `with_signal()`.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?
-->
