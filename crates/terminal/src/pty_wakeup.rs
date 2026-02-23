// Chunk: docs/chunks/terminal_pty_wakeup - Run-loop wakeup for PTY output
// Chunk: docs/chunks/pty_wakeup_reentrant - WakeupSignal trait-based signaling
//! Run-loop wakeup signaling for PTY output.
//!
//! When the PTY reader thread receives data, it needs to wake the main thread's
//! event loop so that poll_agents() runs promptly. This module provides two
//! wakeup mechanisms:
//!
//! 1. **WakeupSignal trait** (new): The editor provides an `EventSender` that
//!    implements `WakeupSignal`. PtyWakeup holds a `Box<dyn WakeupSignal>` and
//!    calls `signal()` when data arrives. This is the preferred approach as it
//!    integrates with the unified event queue.
//!
//! 2. **Global callback** (legacy): A global callback registered via
//!    `set_global_wakeup_callback`. This is kept for backward compatibility
//!    but is superseded by the WakeupSignal approach.
//!
//! The `PtyWakeup` type includes debouncing: multiple rapid signals coalesce
//! into one wakeup.

use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Arc;

use dispatch2::DispatchQueue;
use lite_edit_input::WakeupSignal;

/// Global callback pointer that gets invoked when PTY data arrives.
/// Set once at app startup via `set_global_wakeup_callback`.
///
/// **Note**: This is the legacy mechanism. Prefer using `PtyWakeup::with_signal`
/// which takes a `Box<dyn WakeupSignal>` directly.
static WAKEUP_CALLBACK: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

/// Type alias for the global wakeup callback function.
type WakeupCallbackFn = fn();

/// Sets the global wakeup callback that will be invoked when PTY data arrives.
///
/// This should be called once at app startup from the main thread.
/// The callback will be invoked on the main thread via dispatch_async.
///
/// # Safety
/// This function must be called from the main thread before any PTY is spawned.
/// The callback function must be valid for the lifetime of the application.
///
/// **Note**: This is the legacy mechanism. Prefer using `PtyWakeup::with_signal`
/// which doesn't require global state.
pub fn set_global_wakeup_callback(callback: WakeupCallbackFn) {
    WAKEUP_CALLBACK.store(callback as *mut (), Ordering::SeqCst);
}

/// Handle for waking the main run-loop from the PTY reader thread.
///
/// This is constructed on the main thread and passed to `PtyHandle`. When the
/// reader thread receives PTY output, it calls `signal()` which wakes the main
/// thread's event loop.
///
/// Includes debouncing: multiple rapid signals coalesce into one callback.
///
/// # Construction
///
/// Prefer `with_signal` which accepts a `Box<dyn WakeupSignal>` directly:
///
/// ```ignore
/// let sender = event_sender.clone();
/// let wakeup = PtyWakeup::with_signal(Box::new(sender));
/// ```
///
/// The legacy `new()` constructor uses the global callback mechanism.
#[derive(Clone)]
pub struct PtyWakeup {
    inner: Arc<PtyWakeupInner>,
}

/// Inner state shared between clones of PtyWakeup.
struct PtyWakeupInner {
    /// True if a dispatch is pending (prevents duplicate dispatches)
    pending: AtomicBool,
    /// The wakeup signal implementation (None for legacy global callback mode)
    signal: Option<Arc<dyn WakeupSignal>>,
}

impl PtyWakeup {
    /// Creates a new wakeup handle using the global callback mechanism (legacy).
    ///
    /// The global callback (set via `set_global_wakeup_callback`) will be invoked
    /// on the main thread when PTY data arrives.
    ///
    /// **Note**: Prefer `with_signal` which doesn't require global state.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(PtyWakeupInner {
                pending: AtomicBool::new(false),
                signal: None,
            }),
        }
    }

    /// Creates a new wakeup handle with a custom WakeupSignal implementation.
    ///
    /// This is the preferred construction method. The signal's `signal()` method
    /// will be called when PTY data arrives.
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
                pending: AtomicBool::new(false),
                signal: Some(Arc::from(signal)),
            }),
        }
    }

    /// Signals the main thread that PTY data is available.
    ///
    /// Safe to call from any thread.
    ///
    /// Debouncing: If a signal is already pending, this is a no-op.
    pub fn signal(&self) {
        // Only signal if not already pending
        if self.inner.pending.swap(true, Ordering::SeqCst) {
            return; // Already pending, skip
        }

        if let Some(ref signal) = self.inner.signal {
            // New mechanism: use WakeupSignal trait directly
            let signal = Arc::clone(signal);
            let inner = Arc::clone(&self.inner);

            // We still need to clear the pending flag on the main thread.
            // The WakeupSignal::signal() sends an event to the channel, and
            // the drain loop will clear the flag after processing.
            // But for compatibility with the debounce semantics, we clear it here.
            DispatchQueue::main().exec_async(move || {
                // Clear pending flag BEFORE invoking signal
                inner.pending.store(false, Ordering::SeqCst);
                signal.signal();
            });
        } else {
            // Legacy mechanism: use global callback
            let inner = Arc::clone(&self.inner);

            DispatchQueue::main().exec_async(move || {
                // Clear pending flag BEFORE invoking callback
                inner.pending.store(false, Ordering::SeqCst);

                // Invoke the global callback
                let callback_ptr = WAKEUP_CALLBACK.load(Ordering::SeqCst);
                if !callback_ptr.is_null() {
                    let callback: WakeupCallbackFn = unsafe { std::mem::transmute(callback_ptr) };
                    callback();
                }
            });
        }
    }
}

impl Default for PtyWakeup {
    fn default() -> Self {
        Self::new()
    }
}

// PtyWakeup is Send + Sync because:
// - inner is Arc<...>
// - PtyWakeupInner::pending is AtomicBool
// - PtyWakeupInner::signal is Option<Arc<dyn WakeupSignal>> where WakeupSignal: Send + Sync
unsafe impl Send for PtyWakeup {}
unsafe impl Sync for PtyWakeup {}
