// Chunk: docs/chunks/pty_wakeup_reentrant - Event channel for unified event queue
//! Event channel for the unified event queue architecture.
//!
//! This module provides the sender/receiver pair for the editor event queue.
//! All event sources (NSView callbacks, PTY reader thread, blink timer, window
//! delegate) send events through this channel, and a single drain loop processes
//! them sequentially.
//!
//! # Design
//!
//! We use `std::sync::mpsc` because:
//! - The PTY reader is the only background thread producer
//! - `mpsc::Sender` is `Send` (can be used from the PTY thread)
//! - `mpsc::Receiver` is `!Send` (main thread only - which is what we want)
//!
//! The `EventSender` wrapper provides typed convenience methods and implements
//! `WakeupSignal` from the input crate for cross-crate PTY wakeup.

use std::sync::mpsc::{self, Receiver, SendError, Sender};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lite_edit_input::{KeyEvent, MouseEvent, ScrollDelta, WakeupSignal};

use crate::editor_event::EditorEvent;

/// Sender half of the event channel.
///
/// This is cloneable and `Send`, so it can be:
/// - Cloned and stored in NSView callbacks (for key/mouse/scroll events)
/// - Cloned and passed to the blink timer callback
/// - Wrapped in `Arc` and passed to the PTY reader thread
///
/// The sender also holds a callback for waking the run loop when events
/// are sent from background threads (like the PTY reader).
#[derive(Clone)]
pub struct EventSender {
    inner: Arc<EventSenderInner>,
}

struct EventSenderInner {
    /// The actual mpsc sender
    sender: Sender<EditorEvent>,
    /// Callback to wake the run loop (called after sending from bg thread)
    run_loop_waker: Box<dyn Fn() + Send + Sync>,
    /// Whether a wakeup is already pending (debouncing)
    wakeup_pending: AtomicBool,
}

/// Receiver half of the event channel.
///
/// This is `!Send` and stays on the main thread. The drain loop owns this
/// and processes events sequentially.
pub struct EventReceiver {
    receiver: Receiver<EditorEvent>,
}

/// Creates a new event channel pair.
///
/// # Arguments
/// * `run_loop_waker` - Callback to wake the main run loop when events arrive
///   from background threads. This is called after sending `PtyWakeup` events.
///   For events sent from the main thread (NSView callbacks), the run loop is
///   already awake so this isn't strictly needed, but we call it anyway for
///   consistency.
///
/// # Returns
/// A tuple of `(EventSender, EventReceiver)`. The sender can be cloned and
/// distributed to event sources, while the receiver stays with the drain loop.
pub fn create_event_channel(run_loop_waker: impl Fn() + Send + Sync + 'static) -> (EventSender, EventReceiver) {
    let (sender, receiver) = mpsc::channel();

    let event_sender = EventSender {
        inner: Arc::new(EventSenderInner {
            sender,
            run_loop_waker: Box::new(run_loop_waker),
            wakeup_pending: AtomicBool::new(false),
        }),
    };

    let event_receiver = EventReceiver { receiver };

    (event_sender, event_receiver)
}

impl EventSender {
    /// Sends a key event to the channel.
    pub fn send_key(&self, event: KeyEvent) -> Result<(), SendError<EditorEvent>> {
        let result = self.inner.sender.send(EditorEvent::Key(event));
        (self.inner.run_loop_waker)();
        result
    }

    /// Sends a mouse event to the channel.
    pub fn send_mouse(&self, event: MouseEvent) -> Result<(), SendError<EditorEvent>> {
        let result = self.inner.sender.send(EditorEvent::Mouse(event));
        (self.inner.run_loop_waker)();
        result
    }

    /// Sends a scroll event to the channel.
    pub fn send_scroll(&self, delta: ScrollDelta) -> Result<(), SendError<EditorEvent>> {
        let result = self.inner.sender.send(EditorEvent::Scroll(delta));
        (self.inner.run_loop_waker)();
        result
    }

    /// Sends a PTY wakeup event to the channel and wakes the run loop.
    ///
    /// This is called from the PTY reader thread when data arrives.
    /// Includes debouncing: if a wakeup is already pending, skips the send
    /// to avoid flooding the channel with duplicate wakeup events.
    pub fn send_pty_wakeup(&self) -> Result<(), SendError<EditorEvent>> {
        // Debouncing: only send if not already pending
        if self.inner.wakeup_pending.swap(true, Ordering::SeqCst) {
            return Ok(()); // Already pending, skip
        }

        let result = self.inner.sender.send(EditorEvent::PtyWakeup);

        // Wake the run loop so it drains the channel
        (self.inner.run_loop_waker)();

        result
    }

    // Chunk: docs/chunks/terminal_flood_starvation - Manual wakeup for budget overflow
    /// Sends a PTY wakeup event unconditionally, bypassing debouncing.
    ///
    /// This is called by the drain loop when a terminal hits its byte budget
    /// and has more data pending. Unlike `send_pty_wakeup()`, this method:
    /// - Does NOT check or modify the `wakeup_pending` flag
    /// - Always sends an event to guarantee a follow-up cycle
    ///
    /// This is safe because:
    /// 1. We only send a follow-up when budget is exhausted AND more data exists
    /// 2. The drain loop will clear `wakeup_pending` after processing
    /// 3. The follow-up ensures all PTY data is eventually processed
    pub fn send_pty_wakeup_followup(&self) -> Result<(), SendError<EditorEvent>> {
        let result = self.inner.sender.send(EditorEvent::PtyWakeup);

        // Wake the run loop so it drains the channel
        (self.inner.run_loop_waker)();

        result
    }

    /// Clears the wakeup pending flag.
    ///
    /// Called by the drain loop after processing a `PtyWakeup` event.
    /// This allows new PTY data to trigger another wakeup.
    pub fn clear_wakeup_pending(&self) {
        self.inner.wakeup_pending.store(false, Ordering::SeqCst);
    }

    /// Sends a cursor blink event to the channel.
    pub fn send_cursor_blink(&self) -> Result<(), SendError<EditorEvent>> {
        let result = self.inner.sender.send(EditorEvent::CursorBlink);
        (self.inner.run_loop_waker)();
        result
    }

    /// Sends a resize event to the channel.
    pub fn send_resize(&self) -> Result<(), SendError<EditorEvent>> {
        let result = self.inner.sender.send(EditorEvent::Resize);
        (self.inner.run_loop_waker)();
        result
    }

    /// Sends a file drop event to the channel.
    ///
    /// This is called when files are dropped onto the view via drag-and-drop.
    // Chunk: docs/chunks/dragdrop_file_paste - File drop event sender
    pub fn send_file_drop(&self, paths: Vec<String>) -> Result<(), SendError<EditorEvent>> {
        let result = self.inner.sender.send(EditorEvent::FileDrop(paths));
        (self.inner.run_loop_waker)();
        result
    }
}

// Implement WakeupSignal so EventSender can be used by the terminal crate
impl WakeupSignal for EventSender {
    fn signal(&self) {
        // Ignore send errors (channel might be closed during shutdown)
        let _ = self.send_pty_wakeup();
    }
}

impl EventReceiver {
    /// Attempts to receive an event without blocking.
    ///
    /// Returns `Some(event)` if an event is available, `None` otherwise.
    pub fn try_recv(&self) -> Option<EditorEvent> {
        self.receiver.try_recv().ok()
    }

    /// Drains all pending events from the channel.
    ///
    /// This is the main entry point for the drain loop. It returns an
    /// iterator that yields all events currently in the channel without
    /// blocking.
    pub fn drain(&self) -> impl Iterator<Item = EditorEvent> + '_ {
        std::iter::from_fn(|| self.try_recv())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn test_send_key_event() {
        let waker_called = Arc::new(AtomicUsize::new(0));
        let waker_called_clone = waker_called.clone();

        let (sender, receiver) = create_event_channel(move || {
            waker_called_clone.fetch_add(1, Ordering::SeqCst);
        });

        let event = KeyEvent::char('a');
        sender.send_key(event.clone()).unwrap();

        let received = receiver.try_recv().unwrap();
        match received {
            EditorEvent::Key(e) => assert_eq!(e, event),
            _ => panic!("Expected Key event"),
        }
    }

    #[test]
    fn test_send_pty_wakeup_debouncing() {
        let waker_called = Arc::new(AtomicUsize::new(0));
        let waker_called_clone = waker_called.clone();

        let (sender, receiver) = create_event_channel(move || {
            waker_called_clone.fetch_add(1, Ordering::SeqCst);
        });

        // Send multiple wakeups rapidly
        sender.send_pty_wakeup().unwrap();
        sender.send_pty_wakeup().unwrap(); // Should be debounced
        sender.send_pty_wakeup().unwrap(); // Should be debounced

        // Only one event should be in the channel
        let mut count = 0;
        while receiver.try_recv().is_some() {
            count += 1;
        }
        assert_eq!(count, 1);

        // Waker should have been called only once
        assert_eq!(waker_called.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_clear_wakeup_pending() {
        let (sender, receiver) = create_event_channel(|| {});

        sender.send_pty_wakeup().unwrap();
        sender.clear_wakeup_pending();

        // Now another wakeup should go through
        sender.send_pty_wakeup().unwrap();

        let mut count = 0;
        while receiver.try_recv().is_some() {
            count += 1;
        }
        assert_eq!(count, 2);
    }

    #[test]
    fn test_drain_all_events() {
        let (sender, receiver) = create_event_channel(|| {});

        sender.send_key(KeyEvent::char('a')).unwrap();
        sender.send_key(KeyEvent::char('b')).unwrap();
        sender.send_cursor_blink().unwrap();

        let events: Vec<_> = receiver.drain().collect();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_wakeup_signal_trait() {
        let (sender, receiver) = create_event_channel(|| {});

        // Use the WakeupSignal trait method
        WakeupSignal::signal(&sender);

        let event = receiver.try_recv().unwrap();
        assert!(matches!(event, EditorEvent::PtyWakeup));
    }

    #[test]
    fn test_send_key_calls_waker() {
        let waker_called = Arc::new(AtomicUsize::new(0));
        let waker_called_clone = waker_called.clone();

        let (sender, _receiver) = create_event_channel(move || {
            waker_called_clone.fetch_add(1, Ordering::SeqCst);
        });

        sender.send_key(KeyEvent::char('a')).unwrap();

        assert_eq!(waker_called.load(Ordering::SeqCst), 1, "Waker should be called after send_key");
    }

    #[test]
    fn test_send_mouse_calls_waker() {
        use lite_edit_input::{MouseEventKind, Modifiers};

        let waker_called = Arc::new(AtomicUsize::new(0));
        let waker_called_clone = waker_called.clone();

        let (sender, _receiver) = create_event_channel(move || {
            waker_called_clone.fetch_add(1, Ordering::SeqCst);
        });

        let event = MouseEvent {
            kind: MouseEventKind::Moved,
            position: (10.0, 20.0),
            modifiers: Modifiers { shift: false, command: false, option: false, control: false },
            click_count: 0,
        };
        sender.send_mouse(event).unwrap();

        assert_eq!(waker_called.load(Ordering::SeqCst), 1, "Waker should be called after send_mouse");
    }

    #[test]
    fn test_send_scroll_calls_waker() {
        let waker_called = Arc::new(AtomicUsize::new(0));
        let waker_called_clone = waker_called.clone();

        let (sender, _receiver) = create_event_channel(move || {
            waker_called_clone.fetch_add(1, Ordering::SeqCst);
        });

        sender.send_scroll(ScrollDelta { dx: 0.0, dy: 10.0, mouse_position: None }).unwrap();

        assert_eq!(waker_called.load(Ordering::SeqCst), 1, "Waker should be called after send_scroll");
    }

    #[test]
    fn test_send_cursor_blink_calls_waker() {
        let waker_called = Arc::new(AtomicUsize::new(0));
        let waker_called_clone = waker_called.clone();

        let (sender, _receiver) = create_event_channel(move || {
            waker_called_clone.fetch_add(1, Ordering::SeqCst);
        });

        sender.send_cursor_blink().unwrap();

        assert_eq!(waker_called.load(Ordering::SeqCst), 1, "Waker should be called after send_cursor_blink");
    }

    #[test]
    fn test_send_resize_calls_waker() {
        let waker_called = Arc::new(AtomicUsize::new(0));
        let waker_called_clone = waker_called.clone();

        let (sender, _receiver) = create_event_channel(move || {
            waker_called_clone.fetch_add(1, Ordering::SeqCst);
        });

        sender.send_resize().unwrap();

        assert_eq!(waker_called.load(Ordering::SeqCst), 1, "Waker should be called after send_resize");
    }

    #[test]
    fn test_send_file_drop() {
        let waker_called = Arc::new(AtomicUsize::new(0));
        let waker_called_clone = waker_called.clone();

        let (sender, receiver) = create_event_channel(move || {
            waker_called_clone.fetch_add(1, Ordering::SeqCst);
        });

        let paths = vec!["/path/to/file.txt".to_string(), "/another/path.txt".to_string()];
        sender.send_file_drop(paths.clone()).unwrap();

        assert_eq!(waker_called.load(Ordering::SeqCst), 1, "Waker should be called after send_file_drop");

        let event = receiver.try_recv().unwrap();
        match event {
            EditorEvent::FileDrop(received_paths) => {
                assert_eq!(received_paths, paths);
            }
            _ => panic!("Expected FileDrop event"),
        }
    }

    // Chunk: docs/chunks/terminal_flood_starvation - Tests for send_pty_wakeup_followup

    #[test]
    fn test_send_pty_wakeup_followup_bypasses_debouncing() {
        let waker_called = Arc::new(AtomicUsize::new(0));
        let waker_called_clone = waker_called.clone();

        let (sender, receiver) = create_event_channel(move || {
            waker_called_clone.fetch_add(1, Ordering::SeqCst);
        });

        // First, send a regular wakeup which sets the pending flag
        sender.send_pty_wakeup().unwrap();

        // Now send a followup - this should bypass debouncing and send another event
        sender.send_pty_wakeup_followup().unwrap();

        // Both events should be in the channel
        let mut count = 0;
        while receiver.try_recv().is_some() {
            count += 1;
        }
        assert_eq!(count, 2, "Both wakeup and followup should be sent");

        // Waker should have been called twice
        assert_eq!(waker_called.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_send_pty_wakeup_followup_does_not_affect_pending_flag() {
        let (sender, receiver) = create_event_channel(|| {});

        // Send followup first (no pending flag set)
        sender.send_pty_wakeup_followup().unwrap();

        // Now regular wakeup should still work (pending flag not affected by followup)
        sender.send_pty_wakeup().unwrap();

        // Both should be received
        let mut count = 0;
        while receiver.try_recv().is_some() {
            count += 1;
        }
        assert_eq!(count, 2);
    }

    #[test]
    fn test_send_pty_wakeup_followup_always_sends() {
        let waker_called = Arc::new(AtomicUsize::new(0));
        let waker_called_clone = waker_called.clone();

        let (sender, receiver) = create_event_channel(move || {
            waker_called_clone.fetch_add(1, Ordering::SeqCst);
        });

        // Send multiple followups in a row - all should go through
        sender.send_pty_wakeup_followup().unwrap();
        sender.send_pty_wakeup_followup().unwrap();
        sender.send_pty_wakeup_followup().unwrap();

        let mut count = 0;
        while receiver.try_recv().is_some() {
            count += 1;
        }
        assert_eq!(count, 3, "All followup wakeups should be sent");
        assert_eq!(waker_called.load(Ordering::SeqCst), 3);
    }
}
