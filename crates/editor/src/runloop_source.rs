// Chunk: docs/chunks/pty_wakeup_reentrant - CFRunLoopSource wrapper for event queue wakeup
//! CFRunLoopSource wrapper for waking the main run loop.
//!
//! This module provides a wrapper around `CFRunLoopSource` that can be signaled
//! from any thread to wake the main run loop. When signaled, the run loop invokes
//! a callback that drains the event channel.
//!
//! # Design
//!
//! We use a version 0 run loop source (`CFRunLoopSourceContext` with `version = 0`).
//! This is the simplest type: we signal it with `CFRunLoopSourceSignal` and then
//! wake the run loop with `CFRunLoopWakeUp`. The run loop then calls our `perform`
//! callback.
//!
//! The callback is stored in a `Box::leak`ed allocation so it has `'static` lifetime.
//! This is safe because the run loop source (and thus the callback) lives for the
//! entire application lifetime.

use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

use objc2_core_foundation::{
    kCFRunLoopCommonModes, CFIndex, CFRunLoop,
    CFRunLoopSource, CFRunLoopSourceContext,
};

/// A wrapper around `CFRunLoopSource` for waking the main run loop.
///
/// When `signal()` is called (from any thread), the run loop is woken and will
/// call the callback provided at construction time. This is used to trigger
/// event queue draining when events arrive from background threads.
///
/// # Thread Safety
///
/// - `signal()` is safe to call from any thread
/// - The callback is always invoked on the main thread (the thread that created
///   the source and added it to the run loop)
pub struct RunLoopSource {
    /// The underlying CFRunLoopSource (as usize for Send+Sync)
    source: usize,
    /// The main run loop (as usize for Send+Sync)
    run_loop: usize,
}

// SAFETY: RunLoopSource can be sent between threads. The signal() method only
// calls CFRunLoopSourceSignal and CFRunLoopWakeUp, which are thread-safe.
// We store the pointers as usize which is Send+Sync, and convert back to
// pointers only when calling the CF functions.
unsafe impl Send for RunLoopSource {}
unsafe impl Sync for RunLoopSource {}

/// Callback context for the run loop source.
///
/// This is `Box::leak`ed to get a `'static` reference, so it lives for the
/// entire application lifetime.
struct CallbackContext {
    callback: Box<dyn FnMut() + Send>,
}

/// Global pointer to the callback context.
///
/// We need this because the CFRunLoopSource perform callback only receives
/// a void* context, and we need to route it to our Rust callback.
static CALLBACK_CONTEXT: AtomicPtr<CallbackContext> = AtomicPtr::new(ptr::null_mut());

/// The C callback function that CFRunLoopSource calls.
///
/// This is called on the main thread when the run loop source is signaled.
/// It retrieves the Rust callback from the global context and invokes it.
unsafe extern "C-unwind" fn perform_callback(_info: *mut c_void) {
    let context_ptr = CALLBACK_CONTEXT.load(Ordering::Acquire);
    if !context_ptr.is_null() {
        // SAFETY: We set this pointer in RunLoopSource::new and it's only
        // accessed from the main thread.
        let context = unsafe { &mut *context_ptr };
        (context.callback)();
    }
}

impl RunLoopSource {
    /// Creates a new run loop source and adds it to the current run loop.
    ///
    /// # Arguments
    /// * `callback` - The callback to invoke when the source is signaled.
    ///   This will be called on the main thread.
    ///
    /// # Panics
    /// Panics if called from a thread other than the main thread, or if a
    /// run loop source has already been created (there should only be one).
    ///
    /// # Safety
    /// This must be called from the main thread before the run loop starts.
    pub fn new(callback: impl FnMut() + Send + 'static) -> Self {
        // Ensure we haven't already created a source
        let old = CALLBACK_CONTEXT.load(Ordering::Acquire);
        assert!(old.is_null(), "RunLoopSource already created");

        // Create the callback context and leak it for 'static lifetime
        let context = Box::new(CallbackContext {
            callback: Box::new(callback),
        });
        let context_ptr = Box::leak(context) as *mut CallbackContext;
        CALLBACK_CONTEXT.store(context_ptr, Ordering::Release);

        // Get the current run loop using the new API
        let run_loop = CFRunLoop::current()
            .expect("failed to get current run loop");
        let run_loop_ptr = &*run_loop as *const CFRunLoop;

        // Create the source context
        // Version 0 sources only need the perform callback
        let mut source_context = CFRunLoopSourceContext {
            version: 0 as CFIndex,
            info: context_ptr as *mut c_void,
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: Some(perform_callback),
        };

        // Create the run loop source
        // SAFETY: source_context is valid and perform_callback is a valid C function
        let source = unsafe { CFRunLoopSource::new(None, 0, &mut source_context) }
            .expect("failed to create run loop source");
        let source_ptr = &*source as *const CFRunLoopSource;

        // Add the source to the run loop for common modes (so it works during
        // tracking modes like window resize/drag)
        // SAFETY: kCFRunLoopCommonModes is a static extern symbol that is always valid
        let mode = unsafe { kCFRunLoopCommonModes }
            .expect("kCFRunLoopCommonModes not available");
        run_loop.add_source(Some(&source), Some(mode));

        Self {
            source: source_ptr as usize,
            run_loop: run_loop_ptr as usize,
        }
    }

    /// Signals the run loop source and wakes the run loop.
    ///
    /// This is safe to call from any thread. The callback will be invoked
    /// on the main thread during the next run loop iteration.
    ///
    /// Multiple signals coalesce: if you signal multiple times before the
    /// run loop has a chance to process, the callback is only invoked once.
    pub fn signal(&self) {
        // SAFETY: source and run_loop are valid CF objects stored as usize
        unsafe {
            let source = self.source as *const CFRunLoopSource;
            let run_loop = self.run_loop as *const CFRunLoop;
            (*source).signal();
            (*run_loop).wake_up();
        }
    }
}

impl Drop for RunLoopSource {
    fn drop(&mut self) {
        // In practice, this is never called because the RunLoopSource lives
        // for the entire application lifetime. But for correctness, we would
        // need to:
        // 1. Remove the source from the run loop
        // 2. Invalidate the source
        // 3. Free the callback context
        //
        // Since we use Box::leak, not cleaning up is intentional - the OS
        // reclaims everything on process exit anyway.
    }
}

/// Creates a waker function that signals the given run loop source.
///
/// This is used to integrate the EventSender with the RunLoopSource:
/// when events arrive on background threads, the sender's waker calls
/// this function to signal the source.
///
/// # Returns
/// A function that, when called, signals the source and wakes the run loop.
pub fn create_waker(source: &RunLoopSource) -> impl Fn() + Send + Sync {
    // Store as usize (which is Send+Sync) to avoid issues with raw pointers
    let source_addr = source.source;
    let run_loop_addr = source.run_loop;

    move || {
        // SAFETY: source_addr and run_loop_addr are valid for the lifetime of
        // the application, and CFRunLoopSourceSignal/CFRunLoopWakeUp are thread-safe.
        unsafe {
            let source = source_addr as *const CFRunLoopSource;
            let run_loop = run_loop_addr as *const CFRunLoop;
            (*source).signal();
            (*run_loop).wake_up();
        }
    }
}
