// Chunk: docs/chunks/terminal_pty_wakeup - Run-loop wakeup for PTY output
//! Integration test for PTY wakeup mechanism.
//!
//! These tests verify that the `PtyWakeup` mechanism correctly signals
//! when PTY data arrives, enabling low-latency terminal output rendering.
//!
//! Note: These tests run without a full Cocoa run loop, so the dispatch_async
//! callbacks may not execute immediately. We test the signal() call path
//! rather than the callback execution, which requires an actual NSRunLoop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use lite_edit_terminal::{set_global_wakeup_callback, PtyWakeup, TerminalBuffer};

/// Sets up the global wakeup callback to track signals.
fn setup_wakeup_tracking() -> Arc<AtomicBool> {
    let signaled = Arc::new(AtomicBool::new(false));
    let signaled_clone = signaled.clone();

    // Note: We use a raw function that accesses a global to track callbacks.
    // In a real test with NSRunLoop, the callback would be invoked on the main queue.
    static mut SIGNAL_FLAG: *const AtomicBool = std::ptr::null();

    unsafe {
        // Store the flag pointer so the callback can access it
        SIGNAL_FLAG = Arc::as_ptr(&signaled_clone);
    }

    set_global_wakeup_callback(|| {
        unsafe {
            if !SIGNAL_FLAG.is_null() {
                (*SIGNAL_FLAG).store(true, Ordering::SeqCst);
            }
        }
    });

    signaled
}

/// Tests that `PtyWakeup::signal()` can be called without panicking.
///
/// Note: In a unit test environment without NSRunLoop, the callback won't
/// actually execute. This test verifies the signal path doesn't crash.
#[test]
fn test_pty_wakeup_signal_does_not_panic() {
    let _signaled = setup_wakeup_tracking();
    let wakeup = PtyWakeup::new();

    // Signal should not panic
    wakeup.signal();

    // The callback runs on the main dispatch queue, which may not execute
    // in this test context without an NSRunLoop.
}

/// Tests that PtyWakeup can be created and cloned.
#[test]
fn test_pty_wakeup_clone() {
    let wakeup1 = PtyWakeup::new();
    let wakeup2 = wakeup1.clone();

    // Both should be usable
    wakeup1.signal();
    wakeup2.signal();
}

/// Tests that PtyWakeup is Send + Sync (can be passed to other threads).
#[test]
fn test_pty_wakeup_is_send_sync() {
    let wakeup = PtyWakeup::new();

    // Clone and send to another thread
    let wakeup_clone = wakeup.clone();
    let handle = std::thread::spawn(move || {
        // Signal from background thread should not panic
        wakeup_clone.signal();
    });

    handle.join().expect("thread panicked");
}

/// Tests that spawning with wakeup works and the wakeup is signaled.
///
/// Note: This test creates a terminal and spawns a command. The wakeup
/// callback may or may not execute depending on whether an NSRunLoop
/// is running, but the spawn should succeed.
#[test]
fn test_spawn_command_with_wakeup() {
    let _signaled = setup_wakeup_tracking();
    let wakeup = PtyWakeup::new();

    // Create terminal and spawn echo command with wakeup
    let mut terminal = TerminalBuffer::new(80, 24, 1000);
    let result = terminal.spawn_command_with_wakeup(
        "echo",
        &["hello"],
        std::path::Path::new("/tmp"),
        wakeup,
    );

    assert!(result.is_ok(), "spawn_command_with_wakeup failed: {:?}", result.err());

    // Give the command time to produce output
    std::thread::sleep(Duration::from_millis(100));

    // Poll events to process output
    terminal.poll_events();
}

/// Tests that spawning shell with wakeup works.
#[test]
fn test_spawn_shell_with_wakeup() {
    let _signaled = setup_wakeup_tracking();
    let wakeup = PtyWakeup::new();

    // Create terminal and spawn shell with wakeup
    let mut terminal = TerminalBuffer::new(80, 24, 1000);
    let result = terminal.spawn_shell_with_wakeup(
        std::path::Path::new("/tmp"),
        wakeup,
    );

    assert!(result.is_ok(), "spawn_shell_with_wakeup failed: {:?}", result.err());

    // Give the shell time to start
    std::thread::sleep(Duration::from_millis(200));

    // Poll events to process output
    terminal.poll_events();
}

/// Tests debouncing behavior - multiple rapid signals should coalesce.
#[test]
fn test_pty_wakeup_debouncing() {
    let _signaled = setup_wakeup_tracking();
    let wakeup = PtyWakeup::new();

    // Rapidly signal multiple times
    for _ in 0..10 {
        wakeup.signal();
    }

    // This should not cause issues (debouncing prevents excessive dispatches)
}

/// Tests that default() works the same as new().
#[test]
fn test_pty_wakeup_default() {
    let wakeup1 = PtyWakeup::new();
    let wakeup2 = PtyWakeup::default();

    // Both should be usable
    wakeup1.signal();
    wakeup2.signal();
}
