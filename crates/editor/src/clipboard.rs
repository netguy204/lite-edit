// Chunk: docs/chunks/clipboard_operations - Cmd+A, Cmd+C, Cmd+V clipboard operations
//!
//! Clipboard integration for macOS via NSPasteboard.
//!
//! This module provides a thin wrapper around NSPasteboard for copy/paste operations.
//! It is intentionally minimal ("humble object" pattern) - all business logic
//! stays in the focus target.
//!
//! ## Test isolation
//!
//! Under `cfg(test)` the real NSPasteboard is never touched. Instead, a
//! `thread_local!` string acts as a mock clipboard. This prevents unit tests
//! from contaminating the developer's system clipboard (which would cause paste
//! operations in the live editor to produce test strings such as "hello").

// ── production clipboard (NSPasteboard) ──────────────────────────────────────

#[cfg(not(test))]
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
#[cfg(not(test))]
use objc2_foundation::NSString;

/// Writes text to the macOS general pasteboard.
///
/// Clears existing contents before writing. No-op if the write fails
/// (clipboard operations are best-effort).
#[cfg(not(test))]
pub fn copy_to_clipboard(text: &str) {
    unsafe {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let ns_string = NSString::from_str(text);
        pasteboard.setString_forType(&ns_string, NSPasteboardTypeString);
    }
}

/// Reads text from the macOS general pasteboard.
///
/// Returns `None` if the pasteboard contains no string data.
#[cfg(not(test))]
pub fn paste_from_clipboard() -> Option<String> {
    unsafe {
        let pasteboard = NSPasteboard::generalPasteboard();
        let ns_string = pasteboard.stringForType(NSPasteboardTypeString)?;
        Some(ns_string.to_string())
    }
}

// ── test clipboard (thread-local mock) ───────────────────────────────────────

#[cfg(test)]
use std::cell::RefCell;

#[cfg(test)]
thread_local! {
    /// In-process clipboard used by all unit tests on the current thread.
    /// Never touches NSPasteboard, so the developer's system clipboard is
    /// untouched regardless of how many times cargo test runs.
    static MOCK_CLIPBOARD: RefCell<Option<String>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub fn copy_to_clipboard(text: &str) {
    MOCK_CLIPBOARD.with(|c| *c.borrow_mut() = Some(text.to_string()));
}

#[cfg(test)]
pub fn paste_from_clipboard() -> Option<String> {
    MOCK_CLIPBOARD.with(|c| c.borrow().clone())
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_clipboard_roundtrip() {
        copy_to_clipboard("hello, test");
        assert_eq!(paste_from_clipboard(), Some("hello, test".to_string()));
    }

    #[test]
    fn test_mock_clipboard_empty_when_reset() {
        MOCK_CLIPBOARD.with(|c| *c.borrow_mut() = None);
        assert_eq!(paste_from_clipboard(), None);
    }

    #[test]
    fn test_mock_clipboard_overwrites() {
        copy_to_clipboard("first");
        copy_to_clipboard("second");
        assert_eq!(paste_from_clipboard(), Some("second".to_string()));
    }

    #[test]
    fn test_mock_clipboard_large_content() {
        // Verify no truncation for arbitrarily large content.
        let big = "a".repeat(1_000_000);
        copy_to_clipboard(&big);
        let result = paste_from_clipboard().unwrap();
        assert_eq!(result.len(), 1_000_000);
    }
}
