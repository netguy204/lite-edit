---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/terminal/src/pty_wakeup.rs
- crates/terminal/Cargo.toml
- crates/editor/src/event_channel.rs
- crates/editor/src/drain_loop.rs
- crates/terminal/tests/wakeup_integration.rs
code_references:
  - ref: crates/terminal/src/pty_wakeup.rs#PtyWakeup
    implements: "Direct CFRunLoop signaling handle passed to PTY reader thread"
  - ref: crates/terminal/src/pty_wakeup.rs#PtyWakeup::signal
    implements: "Thread-safe signal passthrough to WakeupSignal (no GCD indirection)"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_pty_wakeup
    implements: "Single debounce point with atomic wakeup_pending flag"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_pty_wakeup_followup
    implements: "Bypass debouncing for byte-budget continuation"
  - ref: crates/editor/src/event_channel.rs#EventSender::clear_wakeup_pending
    implements: "Flag clearing after drain cycle completes"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::process_pending_events
    implements: "Drain loop that clears wakeup_pending after processing PtyWakeup events"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- terminal_fullscreen_paint
- terminal_image_paste
- terminal_word_delete
---

# Chunk Goal

## Minor Goal

Full-screen, mostly-static TUI apps like vim do not reliably paint their initial screen content when launched in a terminal tab. The terminal shows only vim's primary-screen startup message (e.g., `"file.md" [readonly][noeol] 52L, 2072B`) while the alt-screen content (file text, `~` lines, status bar) is blank. Typing keys does not trigger a repaint, but moving the tab to a split pane does (because resize forces SIGWINCH + full redraw). Continuously-refreshing apps like htop work fine because they constantly produce output.

The root cause is a fragile two-hop dispatch with double debouncing in the PTY wakeup pipeline:

```
PTY reader thread
  → PtyWakeup::signal()          [debounce #1: PtyWakeup.pending AtomicBool]
    → DispatchQueue::main().exec_async()    [GCD hop — non-deterministic timing]
      → EventSender::send_pty_wakeup()      [debounce #2: wakeup_pending AtomicBool]
        → mpsc channel + CFRunLoopSourceSignal
          → process_pending_events()
            → poll_events() [4KB byte budget]
```

Three factors combine to lose wakeups:

1. **GCD timing gap**: The `exec_async` closure runs at GCD's discretion. If the CFRunLoopSource callback fires (from a prior signal) in the same run loop iteration BEFORE the GCD block runs, the PtyWakeup event is deferred to a later iteration.

2. **Double debounce suppression**: `PtyWakeup.pending` and `EventSender.wakeup_pending` independently gate signals, cleared at different points in the event loop. A narrow race between their clearing can suppress a wakeup.

3. **Byte budget fragmentation**: Vim's initial paint (~5-20KB) exceeds the 4KB per-poll budget, requiring the follow-up wakeup chain (`send_pty_wakeup_followup`) to fire reliably for each continuation. If any link in that chain is lost, the remaining data sits unprocessed until the cursor blink timer fires (up to 500ms later).

The fix should eliminate the GCD indirection by signaling the CFRunLoopSource directly from the PTY reader thread (both `CFRunLoopSourceSignal` and `CFRunLoopWakeUp` are thread-safe), and collapse the double debounce into a single atomic flag.

## Design Constraints

### Debounce must protect against noisy PTYs

A critical function of the current debounce is preventing high-throughput terminal output (e.g., `cat /dev/urandom`, massive build logs) from flooding the main thread with wakeup signals and causing input lag. The fix must preserve at-most-one-wakeup-per-drain-cycle coalescing. The single atomic flag achieves this: many rapid `PtyWakeup::signal()` calls between drain cycles collapse into one CFRunLoopSource wake. The actual input-lag protection comes from the byte budget (`terminal_flood_starvation`) and priority partitioning (input events processed before PtyWakeup), which are unchanged.

### App Nap compatibility

The `app_nap_activity_assertions` chunk manages an `NSActivityUserInitiated` assertion held while terminals are active, released after 2s quiescence. This is managed on the main thread inside `poll_agents()`. The wakeup delivery change (GCD → direct CFRunLoopSource) does not affect App Nap interaction because:

- Both `CFRunLoopWakeUp()` and `DispatchQueue::main().exec_async()` are equally subject to App Nap throttling when the process is napped
- The activity assertion prevents napping when terminals are active, so the wakeup path is not throttled during active use
- The assertion hold/release logic in `poll_agents()` runs on the main thread inside `process_pending_events()` regardless of how the wakeup was delivered
- During idle→active transitions (first PTY data after quiescence), both mechanisms have the same wakeup latency characteristics under App Nap

## Success Criteria

- Opening vim (or another static fullscreen app like less, man) in a terminal tab paints complete initial screen content within one render frame of the PTY output arriving — no blank screen
- Typing in vim updates the display immediately (no frozen frames requiring split-pane to fix)
- The existing `terminal_fullscreen_paint` alt-screen detection continues to function correctly (it marks dirty on mode transition; this chunk ensures the wakeup that triggers it arrives reliably)
- Animated apps like htop continue to render correctly (no regression)
- The `terminal_flood_starvation` byte-budget and follow-up wakeup mechanism continues to work (the follow-up path must also use the direct CFRunLoopSource signal)
- Noisy PTY output does not cause input lag — debounce coalescing is preserved (at-most-one wakeup per drain cycle), byte budget bounds processing cost, and priority partitioning ensures input events are processed first
- App Nap behavior is unchanged — activity assertion is held during terminal activity and released during quiescence, regardless of the wakeup delivery mechanism
- The cursor blink timer backup polling (`handle_cursor_blink` calling `poll_agents`) remains as a defense-in-depth safety net but is no longer the primary recovery mechanism
- No new race conditions introduced — the wakeup is at-least-once (redundant signals are harmless; lost signals are not)