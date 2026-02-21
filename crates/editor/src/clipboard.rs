// Chunk: docs/chunks/clipboard_operations - Cmd+A, Cmd+C, Cmd+V clipboard operations
//!
//! Clipboard integration for macOS via NSPasteboard.
//!
//! This module provides a thin wrapper around NSPasteboard for copy/paste operations.
//! It is intentionally minimal ("humble object" pattern) - all business logic
//! stays in the focus target.

use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::NSString;

/// Writes text to the macOS general pasteboard.
///
/// This clears the existing pasteboard contents before writing.
pub fn copy_to_clipboard(text: &str) {
    unsafe {
        let pasteboard = NSPasteboard::generalPasteboard();

        // Clear existing contents and declare we're writing a string
        pasteboard.clearContents();

        // Create NSString from our Rust string
        let ns_string = NSString::from_str(text);

        // Write the string to the pasteboard
        // setString:forType: returns BOOL but we ignore errors here
        // as clipboard operations are best-effort
        pasteboard.setString_forType(&ns_string, NSPasteboardTypeString);
    }
}

/// Reads text from the macOS general pasteboard.
///
/// Returns `None` if the pasteboard does not contain string data.
pub fn paste_from_clipboard() -> Option<String> {
    unsafe {
        let pasteboard = NSPasteboard::generalPasteboard();

        // Try to get string data from the pasteboard
        let ns_string = pasteboard.stringForType(NSPasteboardTypeString)?;

        // Convert NSString to Rust String
        Some(ns_string.to_string())
    }
}

#[cfg(test)]
mod tests {
    // Note: Clipboard operations are not unit tested per the testing philosophy's
    // "humble object" pattern. This module is a thin FFI wrapper with no logic.
    // Integration testing happens via manual verification or UI tests.
}
