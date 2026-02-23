// Chunk: docs/chunks/workspace_dir_picker - Directory picker for new workspace creation
//!
//! Directory picker integration for macOS via NSOpenPanel.
//!
//! This module provides a thin wrapper around NSOpenPanel for directory selection.
//! It is intentionally minimal ("humble object" pattern) - all business logic
//! stays in the caller.
//!
//! ## Test isolation
//!
//! Under `cfg(test)` the real NSOpenPanel is never touched. Instead, a
//! `thread_local!` option acts as a mock directory picker. This prevents unit tests
//! from opening modal dialogs during test runs.

use std::path::PathBuf;

// ── production directory picker (NSOpenPanel) ─────────────────────────────────

#[cfg(not(test))]
use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
#[cfg(not(test))]
use objc2_foundation::MainThreadMarker;

/// Opens a directory picker dialog and returns the selected directory.
///
/// Returns `Some(PathBuf)` with the selected directory, or `None` if the user
/// cancelled the dialog.
///
/// # Safety
///
/// This function must be called from the main thread. On macOS, UI operations
/// including NSOpenPanel must be performed on the main thread.
#[cfg(not(test))]
pub fn pick_directory() -> Option<PathBuf> {
    // Get the main thread marker - this is safe because we're called from
    // the main event loop (Cmd+N handler).
    let mtm = MainThreadMarker::new().expect("pick_directory must be called from main thread");

    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(false);
    panel.setCanChooseDirectories(true);
    panel.setAllowsMultipleSelection(false);

    let response = panel.runModal();
    if response == NSModalResponseOK {
        panel.URL().and_then(|url| url.path().map(|p| PathBuf::from(p.to_string())))
    } else {
        None
    }
}

// ── test directory picker (thread-local mock) ─────────────────────────────────

#[cfg(test)]
use std::cell::RefCell;

#[cfg(test)]
thread_local! {
    /// In-process directory picker mock used by all unit tests on the current thread.
    /// Never touches NSOpenPanel, so tests can run without modal dialogs.
    static MOCK_DIRECTORY: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Opens a directory picker dialog and returns the selected directory.
///
/// In test mode, returns the value set by `mock_set_next_directory()`.
/// The mock value is consumed after one call (returns None on subsequent calls
/// until set again).
#[cfg(test)]
pub fn pick_directory() -> Option<PathBuf> {
    MOCK_DIRECTORY.with(|d| d.borrow_mut().take())
}

/// Sets the directory that `pick_directory()` will return on its next call.
///
/// This function is only available in test builds.
#[cfg(test)]
pub fn mock_set_next_directory(dir: Option<PathBuf>) {
    MOCK_DIRECTORY.with(|d| *d.borrow_mut() = dir);
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_pick_directory_returns_set_value() {
        mock_set_next_directory(Some(PathBuf::from("/test/path")));
        assert_eq!(pick_directory(), Some(PathBuf::from("/test/path")));
    }

    #[test]
    fn test_mock_pick_directory_returns_none_by_default() {
        // Clear any previous value
        MOCK_DIRECTORY.with(|d| *d.borrow_mut() = None);
        assert_eq!(pick_directory(), None);
    }

    #[test]
    fn test_mock_pick_directory_consumes_value() {
        mock_set_next_directory(Some(PathBuf::from("/consumed")));

        // First call returns the value
        assert_eq!(pick_directory(), Some(PathBuf::from("/consumed")));

        // Second call returns None (value was consumed)
        assert_eq!(pick_directory(), None);
    }

    #[test]
    fn test_mock_pick_directory_can_be_reset() {
        mock_set_next_directory(Some(PathBuf::from("/first")));
        let _ = pick_directory(); // Consume

        mock_set_next_directory(Some(PathBuf::from("/second")));
        assert_eq!(pick_directory(), Some(PathBuf::from("/second")));
    }

    #[test]
    fn test_mock_pick_directory_none_value() {
        // Explicitly setting None should result in None
        mock_set_next_directory(None);
        assert_eq!(pick_directory(), None);
    }
}
