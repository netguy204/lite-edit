// Chunk: docs/chunks/file_change_events - Self-write suppression
//!
//! Self-write suppression for file change events.
//!
//! When the editor saves a file, the filesystem watcher will detect that change
//! and potentially trigger a reload/merge flow. To avoid this, we temporarily
//! suppress file change events for paths we're about to write.
//!
//! The suppression is time-limited: if no FileChanged event arrives within the
//! TTL (default: 1 second), the suppression entry expires and future changes
//! are processed normally. This prevents stale suppressions from masking
//! legitimate external edits.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Default suppression TTL in milliseconds.
///
/// If no FileChanged event arrives within this time, the suppression expires.
/// 1 second is generous enough to cover slow filesystems while not masking
/// external edits made shortly after our save.
pub const DEFAULT_SUPPRESSION_TTL_MS: u64 = 1000;

/// Registry of paths whose file change events should be suppressed.
///
/// Use `suppress()` before writing to a file, and `check()` when receiving
/// a FileChanged event. If `check()` returns true, the event should be ignored.
pub struct FileChangeSuppression {
    /// Map from absolute path to suppression expiry time
    suppressions: HashMap<PathBuf, Instant>,
    /// TTL for suppression entries
    ttl: Duration,
}

impl FileChangeSuppression {
    /// Creates a new suppression registry with the default TTL (1 second).
    pub fn new() -> Self {
        Self::with_ttl(DEFAULT_SUPPRESSION_TTL_MS)
    }

    /// Creates a new suppression registry with a custom TTL.
    ///
    /// # Arguments
    ///
    /// * `ttl_ms` - TTL in milliseconds for suppression entries
    pub fn with_ttl(ttl_ms: u64) -> Self {
        Self {
            suppressions: HashMap::new(),
            ttl: Duration::from_millis(ttl_ms),
        }
    }

    /// Marks a path for suppression.
    ///
    /// Call this immediately before writing to a file. The suppression will
    /// expire after the TTL if not consumed by a `check()` call.
    ///
    /// # Arguments
    ///
    /// * `path` - The absolute path being written
    pub fn suppress(&mut self, path: PathBuf) {
        let expiry = Instant::now() + self.ttl;
        self.suppressions.insert(path, expiry);
    }

    /// Checks if a file change event should be suppressed.
    ///
    /// If the path was marked for suppression and the TTL hasn't expired,
    /// returns true and removes the suppression entry (one-shot behavior).
    ///
    /// Also cleans up expired entries opportunistically.
    ///
    /// # Arguments
    ///
    /// * `path` - The path from the FileChanged event (absolute)
    ///
    /// # Returns
    ///
    /// `true` if the event should be suppressed (was our own write),
    /// `false` if the event should be processed (external change).
    pub fn check(&mut self, path: &Path) -> bool {
        let now = Instant::now();

        // Clean up expired entries opportunistically
        self.clean_expired(now);

        // Check if this path is suppressed
        if let Some(expiry) = self.suppressions.remove(path) {
            if now < expiry {
                // Still within TTL - suppress this event
                return true;
            }
            // Expired - don't suppress (the remove already happened)
        }

        false
    }

    /// Returns the number of active (non-expired) suppression entries.
    ///
    /// Useful for testing and diagnostics.
    pub fn active_count(&self) -> usize {
        let now = Instant::now();
        self.suppressions.values().filter(|&exp| now < *exp).count()
    }

    /// Cleans up expired suppression entries.
    fn clean_expired(&mut self, now: Instant) {
        self.suppressions.retain(|_, expiry| now < *expiry);
    }
}

impl Default for FileChangeSuppression {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_registry_is_empty() {
        let registry = FileChangeSuppression::new();
        assert_eq!(registry.active_count(), 0);
    }

    #[test]
    fn test_suppress_adds_entry() {
        let mut registry = FileChangeSuppression::new();
        registry.suppress(PathBuf::from("/test/file.rs"));
        assert_eq!(registry.active_count(), 1);
    }

    #[test]
    fn test_check_returns_true_for_suppressed() {
        let mut registry = FileChangeSuppression::new();
        let path = PathBuf::from("/test/file.rs");

        registry.suppress(path.clone());

        assert!(registry.check(&path));
    }

    #[test]
    fn test_check_returns_false_for_unknown() {
        let mut registry = FileChangeSuppression::new();
        let path = PathBuf::from("/test/unknown.rs");

        assert!(!registry.check(&path));
    }

    #[test]
    fn test_check_removes_suppression_one_shot() {
        let mut registry = FileChangeSuppression::new();
        let path = PathBuf::from("/test/file.rs");

        registry.suppress(path.clone());

        // First check consumes the suppression
        assert!(registry.check(&path));

        // Second check returns false (suppression was consumed)
        assert!(!registry.check(&path));
    }

    #[test]
    fn test_check_returns_false_after_ttl_expires() {
        // Use a very short TTL for testing
        let mut registry = FileChangeSuppression::with_ttl(1); // 1ms TTL
        let path = PathBuf::from("/test/file.rs");

        registry.suppress(path.clone());

        // Wait for TTL to expire
        std::thread::sleep(Duration::from_millis(10));

        // Check should return false (expired)
        assert!(!registry.check(&path));
    }

    #[test]
    fn test_multiple_paths() {
        let mut registry = FileChangeSuppression::new();
        let path1 = PathBuf::from("/test/file1.rs");
        let path2 = PathBuf::from("/test/file2.rs");

        registry.suppress(path1.clone());
        registry.suppress(path2.clone());

        assert_eq!(registry.active_count(), 2);

        // Check path1
        assert!(registry.check(&path1));
        assert_eq!(registry.active_count(), 1);

        // Check path2
        assert!(registry.check(&path2));
        assert_eq!(registry.active_count(), 0);
    }

    #[test]
    fn test_resuppress_extends_ttl() {
        let mut registry = FileChangeSuppression::with_ttl(50); // 50ms TTL
        let path = PathBuf::from("/test/file.rs");

        registry.suppress(path.clone());

        // Wait 30ms
        std::thread::sleep(Duration::from_millis(30));

        // Re-suppress (extends the TTL)
        registry.suppress(path.clone());

        // Wait another 30ms (total 60ms from first suppress, but only 30ms from second)
        std::thread::sleep(Duration::from_millis(30));

        // Should still be suppressed (second suppress extended TTL)
        assert!(registry.check(&path));
    }

    #[test]
    fn test_clean_expired_removes_old_entries() {
        let mut registry = FileChangeSuppression::with_ttl(1); // 1ms TTL
        let path1 = PathBuf::from("/test/file1.rs");
        let path2 = PathBuf::from("/test/file2.rs");

        registry.suppress(path1.clone());

        // Wait for path1's TTL to expire
        std::thread::sleep(Duration::from_millis(10));

        // Add path2 (fresh)
        registry.suppress(path2.clone());

        // Check an unrelated path to trigger cleanup
        registry.check(Path::new("/unrelated"));

        // path1 should be cleaned up, path2 should remain
        assert_eq!(registry.active_count(), 1);
    }

    #[test]
    fn test_default() {
        let registry = FileChangeSuppression::default();
        assert_eq!(registry.active_count(), 0);
    }
}
