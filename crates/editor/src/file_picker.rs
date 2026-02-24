// Chunk: docs/chunks/file_open_picker - File picker for opening files via Cmd+O
//!
//! File picker integration for macOS via NSOpenPanel.
//!
//! This module provides a thin wrapper around NSOpenPanel for file selection.
//! It is intentionally minimal ("humble object" pattern) - all business logic
//! stays in the caller.
//!
//! ## Test isolation
//!
//! Under `cfg(test)` the real NSOpenPanel is never touched. Instead, a
//! `thread_local!` option acts as a mock file picker. This prevents unit tests
//! from opening modal dialogs during test runs.

use std::path::PathBuf;

// ── production file picker (NSOpenPanel) ─────────────────────────────────────

#[cfg(not(test))]
use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
#[cfg(not(test))]
use objc2_foundation::MainThreadMarker;

/// Opens a file picker dialog and returns the selected file.
///
/// Returns `Some(PathBuf)` with the selected file, or `None` if the user
/// cancelled the dialog.
///
/// # Safety
///
/// This function must be called from the main thread. On macOS, UI operations
/// including NSOpenPanel must be performed on the main thread.
#[cfg(not(test))]
pub fn pick_file() -> Option<PathBuf> {
    // Get the main thread marker - this is safe because we're called from
    // the main event loop (Cmd+O handler).
    let mtm = MainThreadMarker::new().expect("pick_file must be called from main thread");

    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(false);

    let response = panel.runModal();
    if response == NSModalResponseOK {
        panel.URL().and_then(|url| url.path().map(|p| PathBuf::from(p.to_string())))
    } else {
        None
    }
}

// ── test file picker (thread-local mock) ─────────────────────────────────────

#[cfg(test)]
use std::cell::RefCell;

#[cfg(test)]
thread_local! {
    /// In-process file picker mock used by all unit tests on the current thread.
    /// Never touches NSOpenPanel, so tests can run without modal dialogs.
    static MOCK_FILE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Opens a file picker dialog and returns the selected file.
///
/// In test mode, returns the value set by `mock_set_next_file()`.
/// The mock value is consumed after one call (returns None on subsequent calls
/// until set again).
#[cfg(test)]
pub fn pick_file() -> Option<PathBuf> {
    MOCK_FILE.with(|f| f.borrow_mut().take())
}

/// Sets the file that `pick_file()` will return on its next call.
///
/// This function is only available in test builds.
#[cfg(test)]
pub fn mock_set_next_file(file: Option<PathBuf>) {
    MOCK_FILE.with(|f| *f.borrow_mut() = file);
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_pick_file_returns_set_value() {
        mock_set_next_file(Some(PathBuf::from("/test/path.txt")));
        assert_eq!(pick_file(), Some(PathBuf::from("/test/path.txt")));
    }

    #[test]
    fn test_mock_pick_file_returns_none_by_default() {
        // Clear any previous value
        MOCK_FILE.with(|f| *f.borrow_mut() = None);
        assert_eq!(pick_file(), None);
    }

    #[test]
    fn test_mock_pick_file_consumes_value() {
        mock_set_next_file(Some(PathBuf::from("/consumed.txt")));

        // First call returns the value
        assert_eq!(pick_file(), Some(PathBuf::from("/consumed.txt")));

        // Second call returns None (value was consumed)
        assert_eq!(pick_file(), None);
    }

    #[test]
    fn test_mock_pick_file_can_be_reset() {
        mock_set_next_file(Some(PathBuf::from("/first.txt")));
        let _ = pick_file(); // Consume

        mock_set_next_file(Some(PathBuf::from("/second.txt")));
        assert_eq!(pick_file(), Some(PathBuf::from("/second.txt")));
    }

    #[test]
    fn test_mock_pick_file_none_value() {
        // Explicitly setting None should result in None
        mock_set_next_file(None);
        assert_eq!(pick_file(), None);
    }
}
