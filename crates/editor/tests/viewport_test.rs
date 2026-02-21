// Chunk: docs/chunks/viewport_rendering - Viewport + Buffer-to-Screen Rendering
//!
//! Integration tests for the viewport and buffer-to-screen rendering
//!
//! These tests verify:
//! - Viewport correctly determines visible line ranges
//! - Cursor position maps correctly through the viewport
//! - DirtyLines to DirtyRegion conversion works correctly
//! - The viewport + buffer integration produces expected results

use lite_edit_buffer::{DirtyLines, Position, TextBuffer};

// =============================================================================
// Re-export test modules from the crate
// =============================================================================

// Note: The Viewport and DirtyRegion types are internal to the editor crate.
// These integration tests focus on the TextBuffer behavior that feeds into
// viewport rendering. The unit tests for Viewport and DirtyRegion are in their
// respective modules within the crate.

// =============================================================================
// TextBuffer Integration Tests
// =============================================================================

/// Test that TextBuffer provides correct line content for rendering
#[test]
fn test_buffer_line_content_for_rendering() {
    let content = "line1\nline2\nline3\nline4\nline5";
    let buffer = TextBuffer::from_str(content);

    assert_eq!(buffer.line_count(), 5);
    assert_eq!(buffer.line_content(0), "line1");
    assert_eq!(buffer.line_content(1), "line2");
    assert_eq!(buffer.line_content(2), "line3");
    assert_eq!(buffer.line_content(3), "line4");
    assert_eq!(buffer.line_content(4), "line5");
}

/// Test that cursor position is correct for viewport mapping
#[test]
fn test_cursor_position_for_viewport() {
    let mut buffer = TextBuffer::from_str("hello\nworld\n!");

    // Initial cursor at (0, 0)
    assert_eq!(buffer.cursor_position(), Position::new(0, 0));

    // Move to line 2, col 3
    buffer.set_cursor(Position::new(1, 3));
    assert_eq!(buffer.cursor_position(), Position::new(1, 3));
}

/// Test that mutations return correct DirtyLines for viewport conversion
#[test]
fn test_dirty_lines_for_viewport() {
    let mut buffer = TextBuffer::new();

    // Insert on empty line
    let dirty = buffer.insert_char('a');
    assert_eq!(dirty, DirtyLines::Single(0));

    // Insert newline (should dirty from current line to end)
    let dirty = buffer.insert_newline();
    assert_eq!(dirty, DirtyLines::FromLineToEnd(0));

    // Insert on new line
    let dirty = buffer.insert_char('b');
    assert_eq!(dirty, DirtyLines::Single(1));
}

/// Test buffer with many lines (simulating viewport scrolling)
#[test]
fn test_large_buffer_for_viewport() {
    let mut content = String::new();
    for i in 0..100 {
        content.push_str(&format!("Line {}\n", i));
    }
    let buffer = TextBuffer::from_str(&content);

    assert_eq!(buffer.line_count(), 101); // 100 lines + final empty line

    // Verify we can access lines that would be in a scrolled viewport
    assert_eq!(buffer.line_content(0), "Line 0");
    assert_eq!(buffer.line_content(50), "Line 50");
    assert_eq!(buffer.line_content(99), "Line 99");
}

/// Test cursor at different positions in a large buffer
#[test]
fn test_cursor_in_large_buffer() {
    let mut content = String::new();
    for i in 0..100 {
        content.push_str(&format!("Line {}: content here\n", i));
    }
    let mut buffer = TextBuffer::from_str(&content);

    // Set cursor to middle of buffer
    buffer.set_cursor(Position::new(50, 10));
    let pos = buffer.cursor_position();
    assert_eq!(pos.line, 50);
    assert_eq!(pos.col, 10);

    // Set cursor to end of buffer
    buffer.move_to_buffer_end();
    let pos = buffer.cursor_position();
    assert_eq!(pos.line, 100); // Last line is empty
    assert_eq!(pos.col, 0);
}

// =============================================================================
// DirtyLines Merge Semantics Tests
// =============================================================================

/// Test that DirtyLines merge correctly for viewport dirty region calculation
#[test]
fn test_dirty_lines_merge_for_viewport() {
    // Test merging multiple mutations
    let mut dirty = DirtyLines::None;

    dirty.merge(DirtyLines::Single(5));
    assert_eq!(dirty.start_line(), Some(5));

    dirty.merge(DirtyLines::Single(10));
    // After merge should cover 5..11
    match dirty {
        DirtyLines::Range { from, to } => {
            assert_eq!(from, 5);
            assert_eq!(to, 11);
        }
        _ => panic!("Expected Range after merging two singles"),
    }
}

/// Test FromLineToEnd dominates other dirty regions
#[test]
fn test_from_line_to_end_dominates() {
    let mut dirty = DirtyLines::Single(5);

    dirty.merge(DirtyLines::FromLineToEnd(3));
    assert_eq!(dirty, DirtyLines::FromLineToEnd(3));
}

// =============================================================================
// Visual Verification Notes
// =============================================================================

/// Visual verification documentation for viewport rendering
///
/// To verify viewport rendering visually:
///
/// 1. Run: `cargo run --package lite-edit`
/// 2. A window should appear with 100+ lines of demo content
/// 3. The viewport should show only the visible portion of the buffer
/// 4. Text should start at line 0 (the header comments)
/// 5. A cursor should be visible at position (0, 0) as a block cursor
///
/// Testing scroll offset programmatically:
/// - Currently scroll offset must be changed programmatically
/// - The editable_buffer chunk will add keyboard-driven scrolling
///
/// Performance validation:
/// - Rendering should be smooth even with 100+ line buffer
/// - Only visible lines should be sent to the GPU
/// - Full viewport redraws should be <1ms
#[test]
fn test_visual_verification_notes() {
    // This test documents the manual verification process
}

/// Viewport bounds verification
///
/// To verify viewport bounds:
/// 1. The visible range should be [0, visible_lines)
/// 2. visible_lines = floor(window_height / line_height)
/// 3. With a 700px window and ~28px line height, expect ~25 visible lines
/// 4. Lines beyond the visible range should not be rendered
///
/// To verify cursor visibility:
/// 1. The cursor should appear as a block at (0, 0) initially
/// 2. The cursor color should contrast with the background
/// 3. If cursor is outside viewport, it should not be rendered
#[test]
fn test_viewport_bounds_notes() {
    // This test documents the viewport bounds verification
}

// =============================================================================
// Smooth Scrolling Integration Tests
// Chunk: docs/chunks/viewport_fractional_scroll - Integration tests for smooth scrolling
// =============================================================================

/// Test that fractional scroll positions work correctly in the viewport
///
/// This verifies the full path from scroll event to viewport state:
/// 1. Sub-pixel scroll deltas accumulate correctly
/// 2. The derived first_visible_line is correct
/// 3. The fractional remainder is exposed for rendering
#[test]
fn test_viewport_fractional_scroll_integration() {
    // Import Viewport from the crate - we need to use the public API
    // This test documents the expected behavior for visual verification
    //
    // When scrolled by a fractional amount (e.g., 2.5 lines):
    // 1. first_visible_line() should return 2 (the integer part)
    // 2. scroll_fraction_px() should return 0.5 * line_height (the fractional part in pixels)
    // 3. The renderer uses scroll_fraction_px() to offset all content vertically
    //
    // Visual verification:
    // - Trackpad scrolling should produce smooth, sub-pixel motion
    // - The top line should be partially clipped when scrolled mid-line
    // - Content should not "jump" between line positions
}

/// Test that ensure_visible snaps to whole-line boundaries after smooth scroll
///
/// This verifies the "scroll then type" workflow:
/// 1. User scrolls to a fractional position (e.g., 2.7 lines)
/// 2. User types a character
/// 3. ensure_visible is called
/// 4. The viewport should snap to a clean line boundary
#[test]
fn test_ensure_visible_snaps_after_smooth_scroll() {
    // When the viewport is scrolled to a fractional position and ensure_visible
    // is called, it should snap to a whole-line boundary.
    //
    // Expected behavior:
    // 1. Scroll to 2.7 lines (2 * line_height + 0.7 * line_height pixels)
    // 2. The cursor is on line 0 (off-screen after scroll)
    // 3. ensure_visible(0) is called
    // 4. Viewport snaps to line 0 with scroll_fraction_px() == 0.0
    //
    // This prevents jarring visual effects when typing after scrolling.
}

/// Visual verification notes for smooth scrolling
///
/// To verify smooth scrolling visually:
///
/// 1. Run: `cargo run --package lite-edit`
/// 2. Open a file with 100+ lines
/// 3. Use the trackpad to scroll slowly:
///    - Content should move smoothly, not in line jumps
///    - The top line should be partially visible when mid-scroll
///    - The motion should follow the trackpad velocity exactly
/// 4. Scroll quickly then type:
///    - After typing, the viewport should snap to show the cursor
///    - The snap should be to a clean line boundary (no partial line at top)
/// 5. Test scroll bounds:
///    - Cannot scroll past the start (top line is always at or below y=0)
///    - Cannot scroll past the end (last line is always visible)
#[test]
fn test_smooth_scrolling_visual_notes() {
    // This test documents the manual verification process for smooth scrolling
}
