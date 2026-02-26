// Chunk: docs/chunks/file_change_events - File change debouncing
//!
//! Debouncing logic for file change events.
//!
//! When a file change is detected, we wait for a brief period (the debounce window)
//! before emitting the event. If another change arrives for the same file within
//! this window, the timer resets. This coalesces rapid successive writes (e.g.,
//! from editors that write files in multiple operations) into a single event.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Default debounce window in milliseconds.
pub const DEFAULT_DEBOUNCE_MS: u64 = 100;

/// Debounces file change events, coalescing rapid successive writes.
///
/// When a file change is registered, the debouncer waits for the debounce window
/// to elapse before emitting. If another change arrives for the same path within
/// the window, the timer resets.
///
/// This is a pure data structure with no I/O, making it easy to test.
/// The watcher thread calls `register()` on each event and periodically
/// calls `flush_ready()` to get paths ready to emit.
pub struct FileChangeDebouncer {
    /// Pending paths and when they were last changed
    pending: HashMap<PathBuf, Instant>,
    /// Debounce window duration
    debounce_duration: Duration,
}

impl FileChangeDebouncer {
    /// Creates a new debouncer with the given debounce window.
    ///
    /// # Arguments
    ///
    /// * `debounce_ms` - The debounce window in milliseconds
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            pending: HashMap::new(),
            debounce_duration: Duration::from_millis(debounce_ms),
        }
    }

    /// Creates a new debouncer with the default debounce window (100ms).
    pub fn with_default() -> Self {
        Self::new(DEFAULT_DEBOUNCE_MS)
    }

    /// Register a file change event.
    ///
    /// Updates the timestamp for the given path. If the path is already pending,
    /// the timestamp is reset (extending the debounce window).
    ///
    /// This method does NOT return paths to emit - use `flush_ready()` for that.
    ///
    /// # Arguments
    ///
    /// * `path` - The path that changed
    /// * `now` - The current timestamp (passed in for testability)
    pub fn register(&mut self, path: PathBuf, now: Instant) {
        self.pending.insert(path, now);
    }

    /// Check for paths whose debounce window has expired.
    ///
    /// Returns paths that are ready to emit (their last change was more than
    /// `debounce_ms` ago). These paths are removed from the pending set.
    ///
    /// # Arguments
    ///
    /// * `now` - The current timestamp (passed in for testability)
    ///
    /// # Returns
    ///
    /// A vector of paths ready to be emitted as FileChanged events.
    pub fn flush_ready(&mut self, now: Instant) -> Vec<PathBuf> {
        let mut ready = Vec::new();
        let debounce = self.debounce_duration;

        self.pending.retain(|path, last_change| {
            if now.duration_since(*last_change) >= debounce {
                ready.push(path.clone());
                false // Remove from pending
            } else {
                true // Keep in pending
            }
        });

        ready
    }

    /// Returns the number of pending paths.
    ///
    /// Useful for testing and diagnostics.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Returns true if there are no pending paths.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_debouncer_is_empty() {
        let debouncer = FileChangeDebouncer::new(100);
        assert!(debouncer.is_empty());
        assert_eq!(debouncer.pending_count(), 0);
    }

    #[test]
    fn test_register_adds_pending() {
        let mut debouncer = FileChangeDebouncer::new(100);
        let now = Instant::now();
        let path = PathBuf::from("/test/file.rs");

        debouncer.register(path.clone(), now);

        assert!(!debouncer.is_empty());
        assert_eq!(debouncer.pending_count(), 1);
    }

    #[test]
    fn test_single_event_not_emitted_immediately() {
        let mut debouncer = FileChangeDebouncer::new(100);
        let now = Instant::now();
        let path = PathBuf::from("/test/file.rs");

        debouncer.register(path.clone(), now);

        // Immediately flushing should return nothing
        let ready = debouncer.flush_ready(now);
        assert!(ready.is_empty());
        assert_eq!(debouncer.pending_count(), 1); // Still pending
    }

    #[test]
    fn test_event_emitted_after_debounce_window() {
        let mut debouncer = FileChangeDebouncer::new(100);
        let now = Instant::now();
        let path = PathBuf::from("/test/file.rs");

        debouncer.register(path.clone(), now);

        // Flush after debounce window
        let later = now + Duration::from_millis(100);
        let ready = debouncer.flush_ready(later);

        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], path);
        assert!(debouncer.is_empty()); // Removed from pending
    }

    #[test]
    fn test_rapid_writes_coalesce() {
        let mut debouncer = FileChangeDebouncer::new(100);
        let now = Instant::now();
        let path = PathBuf::from("/test/file.rs");

        // First write at t=0
        debouncer.register(path.clone(), now);

        // Second write at t=50ms (resets the timer)
        let t50 = now + Duration::from_millis(50);
        debouncer.register(path.clone(), t50);

        // Check at t=100ms (100ms after first write, but only 50ms after second)
        let t100 = now + Duration::from_millis(100);
        let ready = debouncer.flush_ready(t100);
        assert!(ready.is_empty(), "Should not emit yet - timer was reset");

        // Check at t=150ms (100ms after second write)
        let t150 = now + Duration::from_millis(150);
        let ready = debouncer.flush_ready(t150);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], path);
    }

    #[test]
    fn test_different_files_tracked_independently() {
        let mut debouncer = FileChangeDebouncer::new(100);
        let now = Instant::now();
        let path1 = PathBuf::from("/test/file1.rs");
        let path2 = PathBuf::from("/test/file2.rs");

        // File 1 at t=0
        debouncer.register(path1.clone(), now);

        // File 2 at t=50ms
        let t50 = now + Duration::from_millis(50);
        debouncer.register(path2.clone(), t50);

        assert_eq!(debouncer.pending_count(), 2);

        // At t=100ms, only file1 should be ready
        let t100 = now + Duration::from_millis(100);
        let ready = debouncer.flush_ready(t100);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], path1);
        assert_eq!(debouncer.pending_count(), 1);

        // At t=150ms, file2 should be ready
        let t150 = now + Duration::from_millis(150);
        let ready = debouncer.flush_ready(t150);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], path2);
        assert!(debouncer.is_empty());
    }

    #[test]
    fn test_flush_ready_with_no_pending() {
        let mut debouncer = FileChangeDebouncer::new(100);
        let now = Instant::now();

        let ready = debouncer.flush_ready(now);
        assert!(ready.is_empty());
    }

    #[test]
    fn test_boundary_exactly_at_debounce() {
        let mut debouncer = FileChangeDebouncer::new(100);
        let now = Instant::now();
        let path = PathBuf::from("/test/file.rs");

        debouncer.register(path.clone(), now);

        // Exactly at the debounce boundary should emit
        let t100 = now + Duration::from_millis(100);
        let ready = debouncer.flush_ready(t100);
        assert_eq!(ready.len(), 1);
    }

    #[test]
    fn test_just_before_debounce() {
        let mut debouncer = FileChangeDebouncer::new(100);
        let now = Instant::now();
        let path = PathBuf::from("/test/file.rs");

        debouncer.register(path.clone(), now);

        // Just before debounce boundary should not emit
        let t99 = now + Duration::from_millis(99);
        let ready = debouncer.flush_ready(t99);
        assert!(ready.is_empty());
    }

    #[test]
    fn test_default_debounce_ms() {
        let debouncer = FileChangeDebouncer::with_default();
        // Default is 100ms, just verify it works
        assert!(debouncer.is_empty());
    }

    #[test]
    fn test_multiple_files_ready_at_once() {
        let mut debouncer = FileChangeDebouncer::new(100);
        let now = Instant::now();

        // Register 3 files at the same time
        for i in 0..3 {
            debouncer.register(PathBuf::from(format!("/test/file{}.rs", i)), now);
        }

        assert_eq!(debouncer.pending_count(), 3);

        // All should be ready at t=100ms
        let t100 = now + Duration::from_millis(100);
        let ready = debouncer.flush_ready(t100);
        assert_eq!(ready.len(), 3);
        assert!(debouncer.is_empty());
    }
}
