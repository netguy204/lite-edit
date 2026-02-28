---
decision: APPROVE
summary: Implementation eliminates GCD indirection and double debounce, consolidating wakeup path to a single atomic flag in EventSender for reliable PTY wakeup delivery.
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: Opening vim (or another static fullscreen app like less, man) in a terminal tab paints complete initial screen content within one render frame of the PTY output arriving — no blank screen

- **Status**: satisfied
- **Evidence**: The GCD hop that caused non-deterministic timing gaps has been removed. `PtyWakeup::signal()` now directly calls `self.inner.signal.signal()` (pty_wakeup.rs:113), which invokes `EventSender::send_pty_wakeup()` synchronously from the PTY reader thread. This eliminates the timing window where wakeups could be lost.

### Criterion 2: Typing in vim updates the display immediately (no frozen frames requiring split-pane to fix)

- **Status**: satisfied
- **Evidence**: Same fix as criterion 1 — the reliable wakeup path ensures PTY output triggers drain loop processing. Additionally, `poll_after_input()` in drain_loop.rs:467-476 immediately polls PTY events after user input for responsive echo.

### Criterion 3: The existing `terminal_fullscreen_paint` alt-screen detection continues to function correctly (it marks dirty on mode transition; this chunk ensures the wakeup that triggers it arrives reliably)

- **Status**: satisfied
- **Evidence**: This chunk only changes the wakeup delivery mechanism, not the event processing flow. The `poll_agents()` call in `handle_pty_wakeup()` (drain_loop.rs:371) is unchanged, so alt-screen mode transitions continue to mark dirty regions correctly.

### Criterion 4: Animated apps like htop continue to render correctly (no regression)

- **Status**: satisfied
- **Evidence**: The wakeup mechanism still fires on every PTY data arrival (now more reliably). The debouncing in `EventSender::send_pty_wakeup()` (event_channel.rs:154-166) coalesces rapid signals into one per drain cycle, which is the existing behavior. No change to rendering logic.

### Criterion 5: The `terminal_flood_starvation` byte-budget and follow-up wakeup mechanism continues to work (the follow-up path must also use the direct CFRunLoopSource signal)

- **Status**: satisfied
- **Evidence**: `send_pty_wakeup_followup()` (event_channel.rs:180-187) bypasses debouncing and calls the run_loop_waker directly, ensuring byte-budget continuations are delivered. The drain loop calls this in `handle_pty_wakeup()` (drain_loop.rs:380-382) when `needs_rewakeup` is true.

### Criterion 6: Noisy PTY output does not cause input lag — debounce coalescing is preserved (at-most-one wakeup per drain cycle), byte budget bounds processing cost, and priority partitioning ensures input events are processed first

- **Status**: satisfied
- **Evidence**:
  1. Debouncing: `send_pty_wakeup()` uses `wakeup_pending.swap(true, Ordering::SeqCst)` (event_channel.rs:156) to skip if already pending
  2. Priority partitioning: `process_pending_events()` partitions events via `is_priority_event()` and processes priority events first (drain_loop.rs:158-170)
  3. Clear after processing: `clear_wakeup_pending()` called after PTY wakeup processing (drain_loop.rs:173-175)

### Criterion 7: App Nap behavior is unchanged — activity assertion is held during terminal activity and released during quiescence, regardless of the wakeup delivery mechanism

- **Status**: satisfied
- **Evidence**: The wakeup delivery change (GCD → direct CFRunLoopSource) doesn't affect App Nap interaction. Both `CFRunLoopWakeUp()` and GCD's `exec_async()` are equally subject to App Nap throttling. The activity assertion logic in `poll_agents()` runs on the main thread regardless of wakeup delivery mechanism.

### Criterion 8: The cursor blink timer backup polling (`handle_cursor_blink` calling `poll_agents`) remains as a defense-in-depth safety net but is no longer the primary recovery mechanism

- **Status**: satisfied
- **Evidence**: `handle_cursor_blink()` (drain_loop.rs:387-403) still calls `poll_agents()` and schedules follow-up wakeups if needed. This backup mechanism is unchanged and remains operational.

### Criterion 9: No new race conditions introduced — the wakeup is at-least-once (redundant signals are harmless; lost signals are not)

- **Status**: satisfied
- **Evidence**:
  1. `mpsc::Sender::send()` is thread-safe and lock-free
  2. `CFRunLoopSourceSignal()` and `CFRunLoopWakeUp()` are documented by Apple as thread-safe
  3. The atomic debounce flag uses `SeqCst` ordering for proper visibility
  4. Test `test_pty_wakeup_concurrent_signals` verifies concurrent signaling works correctly
  5. Redundant signals are coalesced (harmless), but no signals are lost

