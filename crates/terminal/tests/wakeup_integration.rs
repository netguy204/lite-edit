// Chunk: docs/chunks/terminal_pty_wakeup - Run-loop wakeup for PTY output
// Chunk: docs/chunks/pty_wakeup_reliability - Direct CFRunLoop signaling tests
//! Integration test for PTY wakeup mechanism.
//!
//! These tests verify that the `PtyWakeup` mechanism correctly signals
//! when PTY data arrives, enabling low-latency terminal output rendering.
//!
//! # Architecture
//!
//! `PtyWakeup` wraps a `WakeupSignal` (typically `EventSender`) and calls
//! `signal()` directly from the PTY reader thread. This test uses a mock
//! `WakeupSignal` implementation to verify the signaling behavior.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use lite_edit_terminal::{PtyWakeup, TerminalBuffer, WakeupSignal};

/// A mock WakeupSignal that counts how many times it's signaled.
struct MockWakeupSignal {
    count: Arc<AtomicUsize>,
}

impl MockWakeupSignal {
    fn new() -> (Self, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        (Self { count: count.clone() }, count)
    }
}

impl WakeupSignal for MockWakeupSignal {
    fn signal(&self) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

/// Tests that `PtyWakeup::signal()` calls the underlying WakeupSignal.
#[test]
fn test_pty_wakeup_signal_calls_wakeup_signal() {
    let (mock, count) = MockWakeupSignal::new();
    let wakeup = PtyWakeup::with_signal(Box::new(mock));

    assert_eq!(count.load(Ordering::SeqCst), 0);

    wakeup.signal();

    assert_eq!(count.load(Ordering::SeqCst), 1);
}

/// Tests that PtyWakeup can be created and cloned.
#[test]
fn test_pty_wakeup_clone() {
    let (mock, count) = MockWakeupSignal::new();
    let wakeup1 = PtyWakeup::with_signal(Box::new(mock));
    let wakeup2 = wakeup1.clone();

    // Both should share the same underlying signal
    wakeup1.signal();
    wakeup2.signal();

    assert_eq!(count.load(Ordering::SeqCst), 2);
}

/// Tests that PtyWakeup is Send + Sync (can be passed to other threads).
#[test]
fn test_pty_wakeup_is_send_sync() {
    let (mock, count) = MockWakeupSignal::new();
    let wakeup = PtyWakeup::with_signal(Box::new(mock));

    // Clone and send to another thread
    let wakeup_clone = wakeup.clone();
    let handle = std::thread::spawn(move || {
        // Signal from background thread should not panic
        wakeup_clone.signal();
    });

    handle.join().expect("thread panicked");

    assert_eq!(count.load(Ordering::SeqCst), 1);
}

/// Tests that spawning with wakeup works and the wakeup is signaled.
#[test]
fn test_spawn_command_with_wakeup() {
    let (mock, count) = MockWakeupSignal::new();
    let wakeup = PtyWakeup::with_signal(Box::new(mock));

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

    // The wakeup should have been signaled at least once when output arrived
    assert!(count.load(Ordering::SeqCst) >= 1, "wakeup should have been signaled");
}

/// Tests that spawning shell with wakeup works.
///
/// This test is ignored by default because shell startup timing is highly
/// environment-dependent. Run manually with `--ignored` when needed.
#[test]
#[ignore]
fn test_spawn_shell_with_wakeup() {
    let (mock, count) = MockWakeupSignal::new();
    let wakeup = PtyWakeup::with_signal(Box::new(mock));

    // Create terminal and spawn shell with wakeup
    let mut terminal = TerminalBuffer::new(80, 24, 1000);
    let result = terminal.spawn_shell_with_wakeup(
        std::path::Path::new("/tmp"),
        wakeup,
    );

    assert!(result.is_ok(), "spawn_shell_with_wakeup failed: {:?}", result.err());

    // Wait for the shell to start and produce output (with timeout).
    // Shell startup can be slow on loaded systems or when sourcing profile files.
    let mut got_signal = false;
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(50));
        terminal.poll_events();
        if count.load(Ordering::SeqCst) >= 1 {
            got_signal = true;
            break;
        }
    }

    assert!(got_signal, "wakeup should have been signaled when shell started");
}

/// Tests that multiple rapid signals all call the underlying WakeupSignal.
///
/// Note: The WakeupSignal implementation (EventSender) handles debouncing.
/// PtyWakeup itself no longer debounces - it passes through all signals.
#[test]
fn test_pty_wakeup_passthrough_no_debouncing() {
    let (mock, count) = MockWakeupSignal::new();
    let wakeup = PtyWakeup::with_signal(Box::new(mock));

    // Rapidly signal multiple times
    for _ in 0..10 {
        wakeup.signal();
    }

    // All signals should pass through to the underlying WakeupSignal
    // (EventSender handles debouncing, not PtyWakeup)
    assert_eq!(count.load(Ordering::SeqCst), 10);
}

/// Tests that signaling from multiple threads works correctly.
#[test]
fn test_pty_wakeup_concurrent_signals() {
    let (mock, count) = MockWakeupSignal::new();
    let wakeup = PtyWakeup::with_signal(Box::new(mock));

    let mut handles = Vec::new();

    // Spawn 4 threads that each signal 25 times
    for _ in 0..4 {
        let wakeup_clone = wakeup.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..25 {
                wakeup_clone.signal();
            }
        }));
    }

    for handle in handles {
        handle.join().expect("thread panicked");
    }

    // All 100 signals should have been delivered
    assert_eq!(count.load(Ordering::SeqCst), 100);
}
