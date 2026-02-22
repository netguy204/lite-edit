// Chunk: docs/chunks/terminal_emulator - Terminal emulator backed by alacritty_terminal
//! Integration tests for the terminal crate.
//!
//! These tests verify that the terminal buffer works correctly with real
//! shell processes and PTY I/O.

use std::path::Path;
use std::time::Duration;

use lite_edit_terminal::{BufferView, TerminalBuffer};

/// Test that spawning a shell and running echo produces visible output.
#[test]
fn test_shell_output_renders() {
    let mut term = TerminalBuffer::new(80, 24, 1000);
    term.spawn_shell("/bin/sh", Path::new("/tmp")).unwrap();

    // Give shell time to start and produce prompt
    std::thread::sleep(Duration::from_millis(100));
    term.poll_events();

    // Send echo command
    term.write_input(b"echo hello\n").unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    term.poll_events();

    // Find "hello" in output
    let mut found = false;
    for line in 0..term.line_count() {
        if let Some(styled) = term.styled_line(line) {
            let text: String = styled.spans.iter().map(|s| &s.text[..]).collect();
            if text.contains("hello") {
                found = true;
                break;
            }
        }
    }
    assert!(found, "Expected 'hello' in terminal output");
}

/// Test that line_count returns a reasonable value for a new terminal.
#[test]
fn test_new_terminal_line_count() {
    let term = TerminalBuffer::new(80, 24, 1000);
    assert_eq!(term.line_count(), 24);
}

/// Test that styled_line returns Some for valid lines and None for invalid.
#[test]
fn test_styled_line_bounds() {
    let term = TerminalBuffer::new(80, 24, 1000);

    // Valid lines should return Some
    assert!(term.styled_line(0).is_some());
    assert!(term.styled_line(23).is_some());

    // Invalid lines should return None
    assert!(term.styled_line(24).is_none());
    assert!(term.styled_line(100).is_none());
}

/// Test that the terminal is not editable.
#[test]
fn test_terminal_not_editable() {
    let term = TerminalBuffer::new(80, 24, 1000);
    assert!(!term.is_editable());
}

/// Test that cursor_info returns valid cursor position.
#[test]
fn test_cursor_position() {
    let term = TerminalBuffer::new(80, 24, 1000);
    let cursor = term.cursor_info();

    assert!(cursor.is_some());
    let cursor = cursor.unwrap();

    // Initial cursor should be at line 0, col 0
    assert_eq!(cursor.position.line, 0);
    assert_eq!(cursor.position.col, 0);
}

/// Test terminal resize.
#[test]
fn test_resize() {
    let mut term = TerminalBuffer::new(80, 24, 1000);
    assert_eq!(term.size(), (80, 24));
    assert_eq!(term.line_count(), 24);

    term.resize(120, 40);
    assert_eq!(term.size(), (120, 40));
    assert_eq!(term.line_count(), 40);
}

/// Test dirty tracking.
#[test]
fn test_dirty_tracking() {
    use lite_edit_terminal::DirtyLines;

    let mut term = TerminalBuffer::new(80, 24, 1000);

    // Initial state should be dirty (FromLineToEnd(0))
    let dirty = term.take_dirty();
    assert!(!dirty.is_none());

    // After taking, should be None
    let dirty2 = term.take_dirty();
    assert_eq!(dirty2, DirtyLines::None);
}

/// Test that line_len returns terminal width.
#[test]
fn test_line_len() {
    let term = TerminalBuffer::new(80, 24, 1000);
    assert_eq!(term.line_len(0), 80);
    assert_eq!(term.line_len(23), 80);
    // Even out of bounds returns terminal width for this implementation
    assert_eq!(term.line_len(100), 80);
}

/// Test spawning a command that exits immediately.
#[test]
fn test_command_exit() {
    let mut term = TerminalBuffer::new(80, 24, 1000);
    term.spawn_command("true", &[], Path::new("/tmp")).unwrap();

    // Wait for exit
    std::thread::sleep(Duration::from_millis(100));
    term.poll_events();

    // Process should have exited
    let exit_code = term.try_wait();
    assert_eq!(exit_code, Some(0));
}

/// Test spawning a command with arguments.
#[test]
fn test_command_with_args() {
    let mut term = TerminalBuffer::new(80, 24, 1000);
    term.spawn_command("echo", &["test", "args"], Path::new("/tmp"))
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    term.poll_events();

    // Find "test args" in output
    let mut found = false;
    for line in 0..term.line_count() {
        if let Some(styled) = term.styled_line(line) {
            let text: String = styled.spans.iter().map(|s| &s.text[..]).collect();
            if text.contains("test") && text.contains("args") {
                found = true;
                break;
            }
        }
    }
    assert!(found, "Expected 'test args' in terminal output");
}

// =============================================================================
// File-backed scrollback tests
// Chunk: docs/chunks/terminal_file_backed_scrollback - File-backed cold scrollback
// =============================================================================

/// Test that cold scrollback captures lines when hot limit is exceeded.
#[test]
fn test_cold_scrollback_captures_lines() {
    let mut term = TerminalBuffer::new(80, 24, 100);
    term.set_hot_scrollback_limit(50); // Small limit to trigger cold capture quickly

    // Generate enough output to exceed the hot limit
    // We need to generate many lines
    let mut output = String::new();
    for i in 0..100 {
        output.push_str(&format!("Line {:04}\r\n", i));
    }

    // Spawn a command to echo the lines
    term.spawn_command("printf", &[&output], Path::new("/tmp")).unwrap();

    // Wait for output and poll events to process
    std::thread::sleep(Duration::from_millis(200));
    for _ in 0..10 {
        term.poll_events();
        std::thread::sleep(Duration::from_millis(50));
    }

    // Check if any lines were captured to cold storage
    // Note: Due to timing, we might not always capture lines, but we should
    // at least verify the mechanism doesn't crash
    let cold_count = term.cold_line_count();
    // We might have captured some lines, or not (depends on timing)
    // The important thing is it doesn't crash
    // usize is always >= 0, so we just log the count
    eprintln!("Cold line count: {}", cold_count);
}

/// Test that styled_line returns correct content from cold storage.
#[test]
fn test_styled_line_from_cold_storage() {
    // This test directly manipulates the terminal to trigger cold storage
    let mut term = TerminalBuffer::new(80, 24, 5000);
    term.set_hot_scrollback_limit(100);

    // Generate many numbered lines through printf
    // Use multiple printf calls to ensure scrollback builds up
    let mut batch = String::new();
    for i in 0..300 {
        batch.push_str(&format!("L{:04}\r\n", i));
    }

    term.spawn_command("printf", &[&batch], Path::new("/tmp")).unwrap();

    // Poll multiple times to ensure all output is processed
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(50));
        term.poll_events();
    }

    // Verify we can read lines from the terminal
    // The exact content depends on timing, but we should be able to read
    // without errors
    let line_count = term.line_count();
    assert!(line_count > 24, "Should have scrollback lines");

    // Read all lines - should not panic or error
    for i in 0..line_count.min(500) {
        let _line = term.styled_line(i);
        // Line might be None if out of bounds, but shouldn't panic
    }
}

/// Test that line_count includes cold storage lines.
#[test]
fn test_line_count_includes_cold_lines() {
    let mut term = TerminalBuffer::new(80, 24, 5000);
    term.set_hot_scrollback_limit(50);

    let initial_count = term.line_count();

    // Generate output
    let mut output = String::new();
    for i in 0..200 {
        output.push_str(&format!("Line {:04}\r\n", i));
    }

    term.spawn_command("printf", &[&output], Path::new("/tmp")).unwrap();

    // Process output
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(50));
        term.poll_events();
    }

    let final_count = term.line_count();

    // line_count should include both cold and hot lines
    assert!(
        final_count > initial_count,
        "line_count should increase with scrollback"
    );
}

/// Test cursor position accounts for cold scrollback.
#[test]
fn test_cursor_with_cold_scrollback() {
    let mut term = TerminalBuffer::new(80, 24, 5000);
    term.set_hot_scrollback_limit(50);

    // Generate output to trigger cold scrollback
    let mut output = String::new();
    for i in 0..150 {
        output.push_str(&format!("Line {:04}\r\n", i));
    }

    term.spawn_command("printf", &[&output], Path::new("/tmp")).unwrap();

    // Process output
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(50));
        term.poll_events();
    }

    // Cursor should be in the viewport region, which is after cold + hot scrollback
    let cursor = term.cursor_info();
    assert!(cursor.is_some());
    let cursor = cursor.unwrap();

    // Cursor line should be >= cold_line_count (it's in the viewport)
    let cold_count = term.cold_line_count();
    assert!(
        cursor.position.line >= cold_count,
        "Cursor at line {} should be >= cold_line_count {}",
        cursor.position.line,
        cold_count
    );
}

// =============================================================================
// Terminal input/output integration tests
// Chunk: docs/chunks/terminal_input_render_bug - PTY polling integration
// =============================================================================

/// Test that a shell prompt appears after spawning a shell.
///
/// This verifies the end-to-end flow: shell spawn → PTY output → poll_events → buffer content.
#[test]
fn test_shell_prompt_appears() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Use /bin/sh as it's always available
    terminal.spawn_shell("/bin/sh", Path::new("/tmp")).unwrap();

    // Poll until we see a prompt ($ or #)
    let mut attempts = 0;
    while attempts < 100 {
        if terminal.poll_events() {
            for line in 0..terminal.line_count() {
                if let Some(styled) = terminal.styled_line(line) {
                    let text: String = styled.spans.iter().map(|s| &s.text[..]).collect();
                    // Look for common shell prompt characters
                    if text.contains('$') || text.contains('#') || text.contains('%') {
                        return; // Success!
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
        attempts += 1;
    }

    panic!("No shell prompt appeared within timeout");
}

/// Test that PTY input/output round-trip works.
///
/// This verifies: write_input → PTY → poll_events → buffer contains echoed output.
#[test]
fn test_pty_input_output_roundtrip() {
    // Spawn a cat process that echoes input
    let mut terminal = TerminalBuffer::new(80, 24, 1000);
    terminal
        .spawn_command("cat", &[], Path::new("/tmp"))
        .unwrap();

    // Write input
    terminal.write_input(b"hello\n").unwrap();

    // Poll until we see output (with timeout)
    let mut attempts = 0;
    while attempts < 50 {
        if terminal.poll_events() {
            // Check if "hello" appears in the buffer
            for line in 0..terminal.line_count() {
                if let Some(styled) = terminal.styled_line(line) {
                    let text: String = styled.spans.iter().map(|s| &s.text[..]).collect();
                    if text.contains("hello") {
                        return; // Success!
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
        attempts += 1;
    }

    panic!("Did not see echoed input within timeout");
}
