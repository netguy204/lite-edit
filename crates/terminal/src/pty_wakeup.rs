// Chunk: docs/chunks/terminal_pty_wakeup - Run-loop wakeup for PTY output
// Chunk: docs/chunks/pty_wakeup_reentrant - WakeupSignal trait-based signaling
// Chunk: docs/chunks/pty_wakeup_reliability - Direct CFRunLoop signaling (no GCD)
//! Run-loop wakeup signaling for PTY output.
//!
//! When the PTY reader thread receives data, it needs to wake the main thread's
//! event loop so that poll_agents() runs promptly.
//!
//! # Architecture
//!
//! `PtyWakeup` holds a `Box<dyn WakeupSignal>` (typically an `EventSender`) and
//! calls `signal()` directly from the PTY reader thread when data arrives.
//!
//! The signaling path is:
//!
//! ```text
//! PTY reader thread
//!   → PtyWakeup::signal()
//!     → WakeupSignal::signal()           [calls EventSender::send_pty_wakeup()]
//!       → mpsc channel send
//!       → CFRunLoopSourceSignal + CFRunLoopWakeUp  [thread-safe per Apple docs]
//! ```
//!
//! # Thread Safety
//!
//! This design calls `CFRunLoopSourceSignal()` and `CFRunLoopWakeUp()` directly
//! from the PTY reader thread. Both functions are explicitly documented by Apple
//! as thread-safe. This eliminates the previous GCD indirection which introduced
//! non-deterministic timing gaps that could lose wakeups.
//!
//! # Debouncing
//!
//! Debouncing (at-most-one-wakeup-per-drain-cycle) is handled entirely by
//! `EventSender::send_pty_wakeup()` using its `wakeup_pending` atomic flag.
//! This consolidates the debounce logic in one place and eliminates double-
//! debounce suppression races.

use std::sync::Arc;

use lite_edit_input::WakeupSignal;

/// Handle for waking the main run-loop from the PTY reader thread.
///
/// This is constructed on the main thread and passed to `PtyHandle`. When the
/// reader thread receives PTY output, it calls `signal()` which wakes the main
/// thread's event loop.
///
/// # Thread Safety
///
/// `PtyWakeup` is `Send + Sync` and can be safely called from the PTY reader
/// thread. The underlying `WakeupSignal::signal()` method calls thread-safe
/// CFRunLoop functions directly without GCD intermediation.
///
/// # Construction
///
/// Use `with_signal` to create a wakeup handle:
///
/// ```ignore
/// let sender = event_sender.clone();
/// let wakeup = PtyWakeup::with_signal(Box::new(sender));
/// ```
#[derive(Clone)]
pub struct PtyWakeup {
    inner: Arc<PtyWakeupInner>,
}

/// Inner state shared between clones of PtyWakeup.
struct PtyWakeupInner {
    /// The wakeup signal implementation (typically EventSender)
    signal: Arc<dyn WakeupSignal>,
}

impl PtyWakeup {
    /// Creates a new wakeup handle with a WakeupSignal implementation.
    ///
    /// The signal's `signal()` method will be called directly from the PTY
    /// reader thread when data arrives. The signal must be thread-safe.
    ///
    /// # Arguments
    /// * `signal` - The wakeup signal implementation (typically an EventSender)
    ///
    /// # Example
    /// ```ignore
    /// let sender = event_sender.clone();
    /// let wakeup = PtyWakeup::with_signal(Box::new(sender));
    /// ```
    pub fn with_signal(signal: Box<dyn WakeupSignal>) -> Self {
        Self {
            inner: Arc::new(PtyWakeupInner {
                signal: Arc::from(signal),
            }),
        }
    }

    /// Signals the main thread that PTY data is available.
    ///
    /// This method is called from the PTY reader thread when data arrives.
    /// It calls `WakeupSignal::signal()` directly, which:
    /// 1. Sends a `PtyWakeup` event to the mpsc channel (debounced)
    /// 2. Calls `CFRunLoopSourceSignal()` + `CFRunLoopWakeUp()` (thread-safe)
    ///
    /// # Thread Safety
    ///
    /// Safe to call from any thread. The underlying CFRunLoop functions are
    /// documented by Apple as thread-safe, and the mpsc sender is `Send`.
    ///
    /// # Debouncing
    ///
    /// Debouncing is handled by `EventSender::send_pty_wakeup()` using its
    /// `wakeup_pending` atomic flag. Multiple rapid calls from the PTY reader
    /// thread coalesce into at-most-one wakeup per drain cycle.
    pub fn signal(&self) {
        self.inner.signal.signal();
    }
}

// PtyWakeup is Send + Sync because:
// - inner is Arc<PtyWakeupInner>
// - PtyWakeupInner::signal is Arc<dyn WakeupSignal> where WakeupSignal: Send + Sync
unsafe impl Send for PtyWakeup {}
unsafe impl Sync for PtyWakeup {}
