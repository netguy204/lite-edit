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
