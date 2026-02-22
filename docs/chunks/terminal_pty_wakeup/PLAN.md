<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The current architecture polls PTY output only on timer ticks (500ms cursor blink
interval) or immediately after user input events. When no user interaction occurs,
shell output can be delayed by up to 500ms — unacceptable for interactive terminal
use.

**Solution: `dispatch_async` to main queue for run-loop wakeup.**

When the PTY reader thread receives output, it will dispatch a callback to the main
queue. This wakes the `NSRunLoop` and triggers `poll_agents()` + render within ~1ms
of data arrival.

This approach is simpler than CFRunLoopSource (no manual source management) and
simpler than dispatch_source on the PTY FD (we keep the existing crossbeam channel
for data transfer, using dispatch only for wakeup notification).

**Key insight:** The existing crossbeam channel remains the data path. We add
`dispatch_async(main_queue, poll_callback)` as a side effect of channel send to
wake the main thread. The callback doesn't need to carry data — it just triggers
the existing `poll_agents()` flow.

**Architectural alignment with TESTING_PHILOSOPHY.md:**
- The wakeup mechanism is platform code (humble object) — not unit-testable
- The polling/rendering logic (`poll_agents()`, `render_if_dirty()`) is already
  tested via existing unit tests
- We'll add integration tests for latency verification using the existing
  `TerminalBuffer` tests pattern

## Subsystem Considerations

No subsystems are directly relevant to this chunk. The `viewport_scroll` subsystem
handles scroll position tracking, but this chunk is purely about wakeup timing.

## Sequence

### Step 1: Add dispatch2 dependency to terminal crate

The `dispatch2` crate provides Rust bindings for GCD (Grand Central Dispatch).
We need it in `crates/terminal/Cargo.toml` to dispatch callbacks from the PTY
reader thread.

Location: `crates/terminal/Cargo.toml`

```toml
dispatch2 = "0.3"
```

### Step 2: Create a `PtyWakeup` struct for run-loop signaling

Create a new type that the main thread constructs and passes to `PtyHandle`.
This encapsulates the dispatch queue reference and the callback to invoke.

Location: `crates/terminal/src/pty_wakeup.rs` (new file)

```rust
// Chunk: docs/chunks/terminal_pty_wakeup - Run-loop wakeup for PTY output
//! Run-loop wakeup signaling for PTY output.
//!
//! When the PTY reader thread receives data, it needs to wake the main thread's
//! NSRunLoop so that poll_agents() runs promptly. This module provides the
//! cross-thread signaling mechanism using GCD's dispatch_async.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A callback that will be invoked on the main thread when PTY data arrives.
pub type WakeupCallback = Box<dyn Fn() + Send + Sync>;

/// Handle for waking the main run-loop from the PTY reader thread.
///
/// This is constructed on the main thread and passed to `PtyHandle`. When the
/// reader thread receives PTY output, it calls `signal()` which dispatches a
/// callback to the main queue.
///
/// Includes debouncing: multiple rapid signals coalesce into one callback.
pub struct PtyWakeup {
    inner: Arc<PtyWakeupInner>,
}

struct PtyWakeupInner {
    callback: WakeupCallback,
    /// True if a dispatch is pending (prevents duplicate dispatches)
    pending: AtomicBool,
}

impl PtyWakeup {
    /// Creates a new wakeup handle with the given callback.
    ///
    /// The callback will be invoked on the main thread when PTY data arrives.
    pub fn new(callback: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(PtyWakeupInner {
                callback: Box::new(callback),
                pending: AtomicBool::new(false),
            }),
        }
    }

    /// Signals the main thread that PTY data is available.
    ///
    /// This dispatches the callback to the main queue asynchronously.
    /// Safe to call from any thread.
    ///
    /// Debouncing: If a signal is already pending, this is a no-op.
    pub fn signal(&self) {
        // Only dispatch if not already pending
        if self.inner.pending.swap(true, Ordering::SeqCst) {
            return; // Already pending, skip
        }

        let inner = Arc::clone(&self.inner);

        unsafe {
            let main_queue = dispatch2::dispatch_get_main_queue();
            dispatch2::dispatch_async_f(
                main_queue,
                Arc::into_raw(inner) as *mut c_void,
                Some(wakeup_trampoline),
            );
        }
    }
}

impl Clone for PtyWakeup {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// Trampoline function for dispatch_async_f callback.
extern "C" fn wakeup_trampoline(context: *mut c_void) {
    // Reconstruct the Arc and invoke the callback
    let inner: Arc<PtyWakeupInner> =
        unsafe { Arc::from_raw(context as *const PtyWakeupInner) };

    // Clear pending flag BEFORE invoking callback
    // This allows new signals during callback execution to trigger another dispatch
    inner.pending.store(false, Ordering::SeqCst);

    // Invoke the callback
    (inner.callback)();
}

// PtyWakeup is Send + Sync because inner is Arc<...> with Send+Sync contents
unsafe impl Send for PtyWakeup {}
unsafe impl Sync for PtyWakeup {}
```

### Step 3: Export `PtyWakeup` from terminal crate

Location: `crates/terminal/src/lib.rs`

Add:
```rust
mod pty_wakeup;
pub use pty_wakeup::PtyWakeup;
```

### Step 4: Modify `PtyHandle` to accept optional wakeup callback

Update `PtyHandle::spawn()` to optionally take a `PtyWakeup` handle. When provided,
the reader thread calls `wakeup.signal()` after sending data to the channel.

Location: `crates/terminal/src/pty.rs`

Changes:
1. Add `use crate::pty_wakeup::PtyWakeup;`
2. Add new method `spawn_with_wakeup()` that takes `PtyWakeup`
3. The reader thread clone the wakeup and calls `signal()` after send

```rust
/// Spawns a command in a new PTY with run-loop wakeup support.
///
/// Same as `spawn()`, but signals `wakeup` whenever PTY output arrives,
/// allowing the main thread to poll and render promptly.
pub fn spawn_with_wakeup(
    cmd: &str,
    args: &[&str],
    cwd: &Path,
    rows: u16,
    cols: u16,
    wakeup: PtyWakeup,
) -> std::io::Result<Self> {
    // ... same setup as spawn() ...

    // In reader thread:
    let reader_thread = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(TerminalEvent::PtyOutput(buf[..n].to_vec())).is_err() {
                        break;
                    }
                    // Signal main thread to wake and poll
                    wakeup.signal();
                }
                Err(e) => {
                    let _ = tx.send(TerminalEvent::PtyError(e));
                    break;
                }
            }
        }
    });

    // ...
}
```

### Step 5: Add `spawn_shell_with_wakeup` to TerminalBuffer

Location: `crates/terminal/src/terminal_buffer.rs`

Add methods that accept `PtyWakeup`:
```rust
/// Spawns a shell with run-loop wakeup support.
pub fn spawn_shell_with_wakeup(
    &mut self,
    shell: &str,
    cwd: &Path,
    wakeup: PtyWakeup,
) -> std::io::Result<()> {
    self.spawn_command_with_wakeup(shell, &[], cwd, wakeup)
}

/// Spawns a command with run-loop wakeup support.
pub fn spawn_command_with_wakeup(
    &mut self,
    cmd: &str,
    args: &[&str],
    cwd: &Path,
    wakeup: PtyWakeup,
) -> std::io::Result<()> {
    let (cols, rows) = self.size;
    let handle = PtyHandle::spawn_with_wakeup(cmd, args, cwd, rows as u16, cols as u16, wakeup)?;
    self.pty = Some(handle);
    Ok(())
}
```

### Step 6: Store wakeup factory in EditorState

The callback needs access to `EditorController` (via `Rc<RefCell<>>`), which is
created in `main.rs`. We store a callback factory in `EditorState` that workspace
code can use when spawning terminals.

Location: `crates/editor/src/editor_state.rs`

Add field and methods:
```rust
/// Factory for creating PTY wakeup callbacks.
/// Set by main.rs after controller creation.
pty_wakeup_factory: Option<Arc<dyn Fn() -> PtyWakeup + Send + Sync>>,

pub fn set_pty_wakeup_factory(
    &mut self,
    factory: impl Fn() -> PtyWakeup + Send + Sync + 'static,
) {
    self.pty_wakeup_factory = Some(Arc::new(factory));
}

pub fn create_pty_wakeup(&self) -> Option<PtyWakeup> {
    self.pty_wakeup_factory.as_ref().map(|f| f())
}
```

### Step 7: Add handle_pty_wakeup to EditorController

Location: `crates/editor/src/main.rs`

Add method:
```rust
// Chunk: docs/chunks/terminal_pty_wakeup - PTY data arrival handler
/// Called when PTY data arrives (via dispatch_async from reader thread).
///
/// Polls all agents/terminals for output and renders if dirty.
fn handle_pty_wakeup(&mut self) {
    let terminal_dirty = self.state.poll_agents();
    if terminal_dirty.is_dirty() {
        self.state.dirty_region.merge(terminal_dirty);
    }
    self.render_if_dirty();
}
```

### Step 8: Wire up wakeup factory in main.rs setup

Location: `crates/editor/src/main.rs`, in `setup_window()` after controller creation

```rust
// Set up PTY wakeup factory for terminal tabs
// The callback runs on main thread via dispatch_async when PTY data arrives
{
    let wakeup_controller = controller.clone();
    controller.borrow_mut().state.set_pty_wakeup_factory(move || {
        let ctrl = wakeup_controller.clone();
        PtyWakeup::new(move || {
            ctrl.borrow_mut().handle_pty_wakeup();
        })
    });
}
```

### Step 9: Use wakeup when spawning terminal tabs

Location: wherever terminal tabs are spawned (trace from Cmd+Shift+T handler)

This is likely in `Workspace::spawn_terminal_tab()` or similar. When calling
`TerminalBuffer::spawn_shell()`, use the wakeup-enabled version:

```rust
// Get wakeup from state if available
if let Some(wakeup) = state.create_pty_wakeup() {
    terminal.spawn_shell_with_wakeup(shell, cwd, wakeup)?;
} else {
    // Fallback to non-wakeup version
    terminal.spawn_shell(shell, cwd)?;
}
```

We need to thread the `create_pty_wakeup()` capability through the spawn chain.
Options:
- Pass `&EditorState` to spawn
- Pass `Option<PtyWakeup>` directly
- Store wakeup factory in Workspace

**Preferred:** Pass `Option<PtyWakeup>` directly to keep Workspace independent.

### Step 10: Update workspace terminal spawn to accept wakeup

Location: `crates/editor/src/workspace.rs`

Modify spawn_terminal_tab or equivalent to accept optional wakeup:
```rust
pub fn spawn_terminal_tab_with_wakeup(
    &mut self,
    cwd: &Path,
    wakeup: Option<PtyWakeup>,
) -> std::io::Result<()> {
    // ...
    if let Some(w) = wakeup {
        terminal.spawn_shell_with_wakeup(shell, cwd, w)?;
    } else {
        terminal.spawn_shell(shell, cwd)?;
    }
    // ...
}
```

### Step 11: Thread wakeup through handle_key_buffer terminal spawn

Location: `crates/editor/src/editor_state.rs`, in the Cmd+Shift+T handler

When the terminal spawn keybinding is detected:
```rust
let wakeup = self.create_pty_wakeup();
workspace.spawn_terminal_tab_with_wakeup(cwd, wakeup)?;
```

### Step 12: Add integration test for latency

Create a test that verifies PTY output arrives promptly. Note: This test may
not be perfectly reliable due to thread scheduling, but it validates the basic
mechanism.

Location: `crates/terminal/tests/wakeup_integration.rs`

```rust
//! Integration test for PTY wakeup mechanism.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use lite_edit_terminal::{PtyWakeup, TerminalBuffer};

#[test]
fn test_pty_wakeup_signals_on_output() {
    // Track when wakeup is signaled
    let signaled = Arc::new(AtomicBool::new(false));
    let signal_time = Arc::new(AtomicU64::new(0));

    let signaled_clone = signaled.clone();
    let signal_time_clone = signal_time.clone();

    let wakeup = PtyWakeup::new(move || {
        signaled_clone.store(true, Ordering::SeqCst);
        signal_time_clone.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            Ordering::SeqCst,
        );
    });

    // Create terminal and spawn echo command
    let mut terminal = TerminalBuffer::new(80, 24, 1000);
    terminal
        .spawn_command_with_wakeup("echo", &["hello"], std::path::Path::new("/tmp"), wakeup)
        .expect("spawn failed");

    // Wait for signal (with timeout)
    let start = Instant::now();
    while !signaled.load(Ordering::SeqCst) {
        if start.elapsed() > Duration::from_millis(100) {
            panic!("Wakeup was not signaled within 100ms");
        }
        std::thread::sleep(Duration::from_micros(100));
    }

    // Success: wakeup was signaled
}
```

### Step 13: Update code_paths in GOAL.md frontmatter

Location: `docs/chunks/terminal_pty_wakeup/GOAL.md`

Update:
```yaml
code_paths:
  - crates/terminal/Cargo.toml
  - crates/terminal/src/lib.rs
  - crates/terminal/src/pty.rs
  - crates/terminal/src/pty_wakeup.rs
  - crates/terminal/src/terminal_buffer.rs
  - crates/editor/src/main.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/workspace.rs
  - crates/terminal/tests/wakeup_integration.rs
```

## Dependencies

- `dispatch2` crate v0.3 (add to `crates/terminal/Cargo.toml`)
- Existing `crossbeam-channel` in terminal crate (already present)
- The `terminal_input_render_bug` chunk must be complete (ACTIVE status confirms this)

## Risks and Open Questions

1. **dispatch2 API ergonomics:** Using `dispatch_async_f` with raw pointers
   requires careful memory management. The Arc::into_raw/from_raw pattern is
   correct but needs review. If dispatch2 exposes a safer closure-based API,
   prefer that.

2. **Recursive borrow panics:** The wakeup callback borrows `EditorController`
   via `Rc<RefCell<>>`. If dispatch_async delivers during an existing borrow,
   this panics. Mitigations:
   - dispatch_async runs callbacks in a fresh call frame
   - NSRunLoop processes dispatch callbacks between event handlers, not nested
   - Could add try_borrow_mut check as defensive fallback

3. **Test flakiness:** The latency test depends on thread scheduling. May need:
   - Generous timeout (100ms in test, much better than 500ms)
   - Skip on slow CI with feature flag
   - Accept occasional flakes

4. **Agent terminals vs standalone terminals:** The spawn path may differ for
   agent terminals (spawned via AgentRunner) vs user terminals (Cmd+Shift+T).
   Need to trace both paths and wire wakeup through both. Check `agent.rs`.

5. **Idle CPU usage:** Verify that when no PTY output is occurring, no CPU is
   consumed. The wakeup is purely reactive (signal on data arrival).

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
