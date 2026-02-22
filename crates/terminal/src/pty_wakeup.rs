// Chunk: docs/chunks/terminal_pty_wakeup - Run-loop wakeup for PTY output
//! Run-loop wakeup signaling for PTY output.
//!
//! When the PTY reader thread receives data, it needs to wake the main thread's
//! NSRunLoop so that poll_agents() runs promptly. This module provides the
//! cross-thread signaling mechanism using GCD's dispatch_async.
//!
//! The implementation dispatches an empty closure to the main queue. This wakes
//! the NSRunLoop, and the closure invokes a global callback that was registered
//! at app startup.

use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Arc;

use dispatch2::DispatchQueue;

/// Global callback pointer that gets invoked when PTY data arrives.
/// Set once at app startup via `set_global_wakeup_callback`.
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
pub fn set_global_wakeup_callback(callback: WakeupCallbackFn) {
    WAKEUP_CALLBACK.store(callback as *mut (), Ordering::SeqCst);
}

/// Handle for waking the main run-loop from the PTY reader thread.
///
/// This is constructed on the main thread and passed to `PtyHandle`. When the
/// reader thread receives PTY output, it calls `signal()` which dispatches a
/// callback to the main queue.
///
/// Includes debouncing: multiple rapid signals coalesce into one callback.
#[derive(Clone)]
pub struct PtyWakeup {
    inner: Arc<PtyWakeupInner>,
}

struct PtyWakeupInner {
    /// True if a dispatch is pending (prevents duplicate dispatches)
    pending: AtomicBool,
}

impl PtyWakeup {
    /// Creates a new wakeup handle.
    ///
    /// The global callback (set via `set_global_wakeup_callback`) will be invoked
    /// on the main thread when PTY data arrives.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(PtyWakeupInner {
                pending: AtomicBool::new(false),
            }),
        }
    }

    /// Signals the main thread that PTY data is available.
    ///
    /// This dispatches the global callback to the main queue asynchronously.
    /// Safe to call from any thread.
    ///
    /// Debouncing: If a signal is already pending, this is a no-op.
    pub fn signal(&self) {
        // Only dispatch if not already pending
        if self.inner.pending.swap(true, Ordering::SeqCst) {
            return; // Already pending, skip
        }

        let inner = Arc::clone(&self.inner);

        // Use dispatch2's high-level API to dispatch to the main queue
        DispatchQueue::main().exec_async(move || {
            // Clear pending flag BEFORE invoking callback
            // This allows new signals during callback execution to trigger another dispatch
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

impl Default for PtyWakeup {
    fn default() -> Self {
        Self::new()
    }
}

// PtyWakeup is Send + Sync because inner is Arc<...> with atomic contents only
unsafe impl Send for PtyWakeup {}
unsafe impl Sync for PtyWakeup {}
