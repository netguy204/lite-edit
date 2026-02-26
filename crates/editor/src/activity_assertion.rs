// Chunk: docs/chunks/app_nap_activity_assertions - Activity assertion wrapper
//! Activity assertion management for App Nap prevention.
//!
//! This module wraps macOS's NSProcessInfo activity assertion API to
//! communicate terminal activity state to the operating system.
//!
//! When an activity assertion is held, macOS will not put the process
//! into App Nap, ensuring responsive terminal output. When released,
//! macOS is free to nap the process if it's backgrounded and idle.
//!
//! # Usage
//!
//! ```ignore
//! let mut assertion = ActivityAssertion::new();
//!
//! // When terminal activity begins
//! assertion.hold(mtm);
//!
//! // When activity ceases (after ~2 seconds of quiescence)
//! assertion.release();
//! ```

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{
    ns_string, MainThreadMarker, NSActivityOptions, NSObjectProtocol, NSProcessInfo,
};

/// Manages a single NSProcessInfo activity assertion.
///
/// When held, the assertion prevents App Nap for latency-sensitive work.
/// Call `hold()` when terminal activity begins and `release()` when idle.
///
/// The assertion uses `NSActivityUserInitiated` which:
/// - Prevents App Nap
/// - Allows display and system idle sleep (we're not a video player)
/// - Indicates work responding to user interaction
pub struct ActivityAssertion {
    /// The current activity token, if held.
    token: Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

impl ActivityAssertion {
    /// Creates a new ActivityAssertion without holding an assertion.
    pub fn new() -> Self {
        Self { token: None }
    }

    /// Begins an activity assertion if not already held.
    ///
    /// This tells macOS that the process is doing latency-sensitive work
    /// (terminal output) and should not be napped.
    ///
    /// # Arguments
    ///
    /// * `_mtm` - MainThreadMarker to ensure we're on the main thread
    pub fn hold(&mut self, _mtm: MainThreadMarker) {
        if self.token.is_some() {
            // Already holding an assertion
            return;
        }

        // Get the shared NSProcessInfo instance
        let process_info = NSProcessInfo::processInfo();

        // Begin the activity assertion with UserInitiated options.
        // UserInitiated prevents App Nap while allowing display/system sleep.
        let options = NSActivityOptions::UserInitiated;
        let reason = ns_string!("Terminal activity");

        // This is a standard NSProcessInfo API call.
        // The returned token is retained and must be passed to endActivity: to release.
        let token = process_info.beginActivityWithOptions_reason(options, reason);

        self.token = Some(token);
    }

    /// Ends the activity assertion if held.
    ///
    /// This tells macOS that the process is now idle and may be napped
    /// if backgrounded.
    pub fn release(&mut self) {
        if let Some(token) = self.token.take() {
            // Get the shared NSProcessInfo instance
            let process_info = NSProcessInfo::processInfo();

            // End the activity assertion
            // SAFETY: This is a standard NSProcessInfo API call.
            // The token was obtained from beginActivityWithOptions_reason.
            unsafe {
                process_info.endActivity(&token);
            }
        }
    }

    /// Returns whether an assertion is currently held.
    pub fn is_held(&self) -> bool {
        self.token.is_some()
    }
}

impl Default for ActivityAssertion {
    fn default() -> Self {
        Self::new()
    }
}

// Ensure the assertion is released when dropped
impl Drop for ActivityAssertion {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_without_assertion() {
        let assertion = ActivityAssertion::new();
        assert!(!assertion.is_held());
    }

    #[test]
    fn test_default_creates_without_assertion() {
        let assertion = ActivityAssertion::default();
        assert!(!assertion.is_held());
    }

    // Note: The following tests require MainThreadMarker which can only be
    // obtained when running on the main thread. In practice, these are
    // integration tests that verify App Nap behavior through Activity Monitor.
    //
    // The hold/release tests are included here for completeness but may need
    // to be run in a context where MainThreadMarker is available (e.g., in
    // the actual application or with a test harness that sets up the main thread).

    #[test]
    fn test_double_release_is_safe() {
        let mut assertion = ActivityAssertion::new();
        // Release without holding should not panic
        assertion.release();
        assertion.release();
        assert!(!assertion.is_held());
    }

    // Tests that require MainThreadMarker - these can be run manually
    // or in an integration test context.
    //
    // #[test]
    // fn test_hold_acquires_assertion() {
    //     let mtm = MainThreadMarker::new().expect("must be on main thread");
    //     let mut assertion = ActivityAssertion::new();
    //     assertion.hold(mtm);
    //     assert!(assertion.is_held());
    // }
    //
    // #[test]
    // fn test_release_releases_assertion() {
    //     let mtm = MainThreadMarker::new().expect("must be on main thread");
    //     let mut assertion = ActivityAssertion::new();
    //     assertion.hold(mtm);
    //     assertion.release();
    //     assert!(!assertion.is_held());
    // }
    //
    // #[test]
    // fn test_double_hold_is_idempotent() {
    //     let mtm = MainThreadMarker::new().expect("must be on main thread");
    //     let mut assertion = ActivityAssertion::new();
    //     assertion.hold(mtm);
    //     assertion.hold(mtm); // Should not create a second assertion
    //     assert!(assertion.is_held());
    // }
}
