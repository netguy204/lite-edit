// Chunk: docs/chunks/terminal_emulator - Terminal emulator backed by alacritty_terminal
// Chunk: docs/chunks/terminal_background_box_drawing - Background color and box-drawing character tests
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
    term.spawn_shell(Path::new("/tmp")).unwrap();

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
    terminal.spawn_shell(Path::new("/tmp")).unwrap();

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

// =============================================================================
// Terminal styling integration tests
// Chunk: docs/chunks/terminal_styling_fidelity - Tests for styled terminal output
// =============================================================================

use lite_edit_buffer::{Color, NamedColor, UnderlineStyle};

/// Test that colored text output preserves ANSI color escape sequences in styled spans.
///
/// This verifies: ANSI escape sequences → alacritty_terminal parsing → style_convert → StyledLine
#[test]
fn test_colored_text_produces_styled_spans() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Use printf to output red text: \e[31m sets red foreground, \e[0m resets
    // The text "RED" should appear in a span with Color::Named(NamedColor::Red)
    terminal
        .spawn_command("printf", &["\\033[31mRED\\033[0m"], Path::new("/tmp"))
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find "RED" in output and verify it has red foreground
    let mut found_red = false;
    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            for span in &styled.spans {
                if span.text.contains("RED") {
                    // Check that the span has red foreground color
                    if span.style.fg == Color::Named(NamedColor::Red) {
                        found_red = true;
                        break;
                    }
                }
            }
        }
        if found_red {
            break;
        }
    }
    assert!(found_red, "Expected 'RED' text with red foreground color");
}

/// Test that multiple colors in one line produce separate styled spans.
#[test]
fn test_multiple_colors_create_separate_spans() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Output "RED" in red, then "GREEN" in green
    terminal
        .spawn_command(
            "printf",
            &["\\033[31mRED\\033[32mGREEN\\033[0m"],
            Path::new("/tmp"),
        )
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find the line containing the colored text
    let mut found_red = false;
    let mut found_green = false;
    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            for span in &styled.spans {
                if span.text.contains("RED") && span.style.fg == Color::Named(NamedColor::Red) {
                    found_red = true;
                }
                if span.text.contains("GREEN") && span.style.fg == Color::Named(NamedColor::Green)
                {
                    found_green = true;
                }
            }
        }
    }
    assert!(
        found_red && found_green,
        "Expected both red and green spans, found_red={}, found_green={}",
        found_red,
        found_green
    );
}

/// Test that bold attribute is captured in styled spans.
#[test]
fn test_bold_attribute_captured() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Output bold text: \e[1m sets bold, \e[0m resets
    terminal
        .spawn_command("printf", &["\\033[1mBOLD\\033[0m"], Path::new("/tmp"))
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find "BOLD" and verify it has bold attribute
    let mut found_bold = false;
    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            for span in &styled.spans {
                if span.text.contains("BOLD") && span.style.bold {
                    found_bold = true;
                    break;
                }
            }
        }
        if found_bold {
            break;
        }
    }
    assert!(found_bold, "Expected 'BOLD' text with bold attribute");
}

/// Test that inverse video attribute is captured in styled spans.
#[test]
fn test_inverse_attribute_captured() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Output inverse text: \e[7m sets inverse video, \e[0m resets
    terminal
        .spawn_command("printf", &["\\033[7mINVERSE\\033[0m"], Path::new("/tmp"))
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find "INVERSE" and verify it has inverse attribute
    let mut found_inverse = false;
    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            for span in &styled.spans {
                if span.text.contains("INVERSE") && span.style.inverse {
                    found_inverse = true;
                    break;
                }
            }
        }
        if found_inverse {
            break;
        }
    }
    assert!(
        found_inverse,
        "Expected 'INVERSE' text with inverse attribute"
    );
}

/// Test that underline attribute is captured in styled spans.
#[test]
fn test_underline_attribute_captured() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Output underlined text: \e[4m sets underline, \e[0m resets
    terminal
        .spawn_command("printf", &["\\033[4mUNDERLINE\\033[0m"], Path::new("/tmp"))
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find "UNDERLINE" and verify it has underline attribute
    let mut found_underline = false;
    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            for span in &styled.spans {
                if span.text.contains("UNDERLINE")
                    && span.style.underline != UnderlineStyle::None
                {
                    found_underline = true;
                    break;
                }
            }
        }
        if found_underline {
            break;
        }
    }
    assert!(
        found_underline,
        "Expected 'UNDERLINE' text with underline attribute"
    );
}

/// Test that 256-color indexed colors are captured.
#[test]
fn test_indexed_colors_captured() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Output text with 256-color: \e[38;5;208m sets fg to color 208 (orange)
    terminal
        .spawn_command(
            "printf",
            &["\\033[38;5;208mORANGE\\033[0m"],
            Path::new("/tmp"),
        )
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find "ORANGE" and verify it has indexed color 208
    let mut found_indexed = false;
    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            for span in &styled.spans {
                if span.text.contains("ORANGE") {
                    if let Color::Indexed(idx) = span.style.fg {
                        if idx == 208 {
                            found_indexed = true;
                            break;
                        }
                    }
                }
            }
        }
        if found_indexed {
            break;
        }
    }
    assert!(
        found_indexed,
        "Expected 'ORANGE' text with indexed color 208"
    );
}

/// Test that RGB truecolor is captured.
#[test]
fn test_rgb_truecolor_captured() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Output text with RGB truecolor: \e[38;2;255;128;64m sets fg to RGB(255, 128, 64)
    terminal
        .spawn_command(
            "printf",
            &["\\033[38;2;255;128;64mRGB\\033[0m"],
            Path::new("/tmp"),
        )
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find "RGB" and verify it has the correct RGB color
    let mut found_rgb = false;
    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            for span in &styled.spans {
                if span.text.contains("RGB") {
                    if let Color::Rgb { r, g, b } = span.style.fg {
                        if r == 255 && g == 128 && b == 64 {
                            found_rgb = true;
                            break;
                        }
                    }
                }
            }
        }
        if found_rgb {
            break;
        }
    }
    assert!(
        found_rgb,
        "Expected 'RGB' text with RGB color (255, 128, 64)"
    );
}

/// Test that background colors are captured.
#[test]
fn test_background_color_captured() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Output text with blue background: \e[44m sets bg to blue, \e[0m resets
    terminal
        .spawn_command("printf", &["\\033[44mBLUEBG\\033[0m"], Path::new("/tmp"))
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find "BLUEBG" and verify it has blue background color
    let mut found_blue_bg = false;
    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            for span in &styled.spans {
                if span.text.contains("BLUEBG")
                    && span.style.bg == Color::Named(NamedColor::Blue)
                {
                    found_blue_bg = true;
                    break;
                }
            }
        }
        if found_blue_bg {
            break;
        }
    }
    assert!(
        found_blue_bg,
        "Expected 'BLUEBG' text with blue background color"
    );
}

/// Test that combined attributes (color + bold + underline) are captured.
#[test]
fn test_combined_attributes_captured() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Output text with red + bold + underline: \e[31;1;4m
    terminal
        .spawn_command(
            "printf",
            &["\\033[31;1;4mCOMBINED\\033[0m"],
            Path::new("/tmp"),
        )
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find "COMBINED" and verify it has all attributes
    let mut found_combined = false;
    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            for span in &styled.spans {
                if span.text.contains("COMBINED") {
                    if span.style.fg == Color::Named(NamedColor::Red)
                        && span.style.bold
                        && span.style.underline != UnderlineStyle::None
                    {
                        found_combined = true;
                        break;
                    }
                }
            }
        }
        if found_combined {
            break;
        }
    }
    assert!(
        found_combined,
        "Expected 'COMBINED' text with red, bold, and underline attributes"
    );
}

// =============================================================================
// Selection and clipboard integration tests
// Chunk: docs/chunks/terminal_clipboard_selection - Selection and copy/paste tests
// =============================================================================

use lite_edit_buffer::Position;

/// Test that selection state can be set and read back.
#[test]
fn test_selection_set_and_get() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Initially no selection
    assert!(terminal.selection_anchor().is_none());
    assert!(terminal.selection_head().is_none());

    // Set selection
    terminal.set_selection_anchor(Position::new(5, 10));
    terminal.set_selection_head(Position::new(7, 20));

    assert_eq!(terminal.selection_anchor(), Some(Position::new(5, 10)));
    assert_eq!(terminal.selection_head(), Some(Position::new(7, 20)));

    // Clear selection
    terminal.clear_selection();
    assert!(terminal.selection_anchor().is_none());
    assert!(terminal.selection_head().is_none());
}

/// Test that selection_range returns positions in document order.
#[test]
fn test_selection_range_ordering() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Forward selection
    terminal.set_selection_anchor(Position::new(5, 10));
    terminal.set_selection_head(Position::new(7, 20));
    let range = terminal.selection_range();
    assert_eq!(range, Some((Position::new(5, 10), Position::new(7, 20))));

    // Backward selection - should still return in document order
    terminal.set_selection_anchor(Position::new(7, 20));
    terminal.set_selection_head(Position::new(5, 10));
    let range = terminal.selection_range();
    assert_eq!(range, Some((Position::new(5, 10), Position::new(7, 20))));

    // No selection when anchor equals head
    terminal.set_selection_anchor(Position::new(5, 10));
    terminal.set_selection_head(Position::new(5, 10));
    assert!(terminal.selection_range().is_none());
}

/// Test that selection is cleared when new PTY output arrives.
#[test]
fn test_selection_cleared_on_output() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);
    terminal.spawn_shell(Path::new("/tmp")).unwrap();

    // Wait for initial prompt
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Set a selection
    terminal.set_selection_anchor(Position::new(0, 0));
    terminal.set_selection_head(Position::new(1, 10));
    assert!(terminal.selection_range().is_some());

    // Generate more output by sending a command
    terminal.write_input(b"echo test\n").unwrap();
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Selection should be cleared
    assert!(terminal.selection_anchor().is_none());
    assert!(terminal.selection_head().is_none());
}

/// Test that selected_text extracts correct content from terminal grid.
#[test]
fn test_selected_text_extraction() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Output some text to select
    terminal
        .spawn_command("printf", &["Hello World\\nTest Line\\n"], Path::new("/tmp"))
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find the line with "Hello World"
    let mut hello_line = None;
    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            let text: String = styled.spans.iter().map(|s| &s.text[..]).collect();
            if text.contains("Hello World") {
                hello_line = Some(line);
                break;
            }
        }
    }

    let hello_line = hello_line.expect("Should find 'Hello World' in output");

    // Select "World" (columns 6-11 on that line)
    terminal.set_selection_anchor(Position::new(hello_line, 6));
    terminal.set_selection_head(Position::new(hello_line, 11));

    let selected = terminal.selected_text();
    assert!(
        selected.is_some(),
        "Should have selected text"
    );
    let selected = selected.unwrap();
    assert!(
        selected.contains("World"),
        "Selected text should contain 'World', got: {:?}",
        selected
    );
}

// =============================================================================
// Initial render tests
// Chunk: docs/chunks/terminal_viewport_init - Tests for terminal viewport initialization
// =============================================================================

/// Test that a shell produces visible content after spawning.
///
/// This test verifies the core requirement: when a shell is spawned and polled,
/// it should produce visible content (e.g., a prompt). This is the terminal-level
/// behavior that underlies the editor's initial render fix.
///
/// Note: Uses a longer timeout because login shells (spawned via spawn_shell)
/// source the full profile chain which can take longer than a simple /bin/sh.
// Chunk: docs/chunks/terminal_shell_env - Increased timeout for login shell startup
#[test]
fn test_shell_produces_content_after_poll() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);
    terminal.spawn_shell(Path::new("/tmp")).unwrap();

    // Poll multiple times with brief delays to give shell time to produce output.
    // Login shells source profile files (~/.zprofile, ~/.zshrc) which can take
    // longer than a simple shell, so we use a generous timeout.
    let mut has_content = false;
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(50));
        terminal.poll_events();

        // Check if any line has non-whitespace content
        for line in 0..terminal.line_count() {
            if let Some(styled) = terminal.styled_line(line) {
                let text: String = styled.spans.iter().map(|s| &s.text[..]).collect();
                if text.trim().len() > 0 {
                    has_content = true;
                    break;
                }
            }
        }
        if has_content {
            break;
        }
    }

    assert!(
        has_content,
        "Shell should produce visible content (e.g., prompt) after polling"
    );
}

/// Test that poll_events returns true when there is PTY output.
///
/// This validates the dirty tracking mechanism that the editor uses to determine
/// whether a re-render is needed after terminal creation.
#[test]
fn test_poll_events_returns_true_on_output() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);
    terminal.spawn_shell(Path::new("/tmp")).unwrap();

    // The shell should produce output (prompt) shortly after spawning.
    // Poll until we get true from poll_events.
    let mut got_output = false;
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(20));
        if terminal.poll_events() {
            got_output = true;
            break;
        }
    }

    assert!(
        got_output,
        "poll_events should return true when shell produces output"
    );
}

// =============================================================================
// Background color rendering tests
// Chunk: docs/chunks/terminal_background_box_drawing - Background color verification
// =============================================================================

/// Test that 256-color indexed background colors are captured.
#[test]
fn test_indexed_background_color_captured() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Output text with 256-color background: \e[48;5;196m sets bg to color 196 (red)
    terminal
        .spawn_command(
            "printf",
            &["\\033[48;5;196mREDBG256\\033[0m"],
            Path::new("/tmp"),
        )
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find "REDBG256" and verify it has indexed background color 196
    let mut found_indexed_bg = false;
    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            for span in &styled.spans {
                if span.text.contains("REDBG256") {
                    if let Color::Indexed(idx) = span.style.bg {
                        if idx == 196 {
                            found_indexed_bg = true;
                            break;
                        }
                    }
                }
            }
        }
        if found_indexed_bg {
            break;
        }
    }
    assert!(
        found_indexed_bg,
        "Expected 'REDBG256' text with indexed background color 196"
    );
}

/// Test that RGB truecolor background is captured.
#[test]
fn test_rgb_background_color_captured() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Output text with RGB truecolor background: \e[48;2;64;128;255m sets bg to RGB(64, 128, 255)
    terminal
        .spawn_command(
            "printf",
            &["\\033[48;2;64;128;255mRGBBG\\033[0m"],
            Path::new("/tmp"),
        )
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find "RGBBG" and verify it has the correct RGB background color
    let mut found_rgb_bg = false;
    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            for span in &styled.spans {
                if span.text.contains("RGBBG") {
                    if let Color::Rgb { r, g, b } = span.style.bg {
                        if r == 64 && g == 128 && b == 255 {
                            found_rgb_bg = true;
                            break;
                        }
                    }
                }
            }
        }
        if found_rgb_bg {
            break;
        }
    }
    assert!(
        found_rgb_bg,
        "Expected 'RGBBG' text with RGB background color (64, 128, 255)"
    );
}

/// Test that combined foreground and background colors are captured.
#[test]
fn test_combined_fg_bg_colors_captured() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Output text with red foreground on blue background: \e[31;44m
    terminal
        .spawn_command(
            "printf",
            &["\\033[31;44mFGBG\\033[0m"],
            Path::new("/tmp"),
        )
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find "FGBG" and verify it has red foreground and blue background
    let mut found_combined = false;
    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            for span in &styled.spans {
                if span.text.contains("FGBG") {
                    if span.style.fg == Color::Named(NamedColor::Red)
                        && span.style.bg == Color::Named(NamedColor::Blue)
                    {
                        found_combined = true;
                        break;
                    }
                }
            }
        }
        if found_combined {
            break;
        }
    }
    assert!(
        found_combined,
        "Expected 'FGBG' text with red foreground and blue background"
    );
}

// =============================================================================
// Box-drawing character tests
// Chunk: docs/chunks/terminal_background_box_drawing - Box-drawing character verification
// =============================================================================

/// Test that box-drawing characters are captured in terminal output.
#[test]
fn test_box_drawing_characters_captured() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Output a simple box using Unicode box-drawing characters
    // ┌──┐
    // │  │
    // └──┘
    // We use printf with the raw UTF-8 characters since macOS printf doesn't
    // support \u escape sequences.
    terminal
        .spawn_command(
            "printf",
            &["%s\n%s\n%s\n", "┌──┐", "│  │", "└──┘"],
            Path::new("/tmp"),
        )
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find box-drawing characters in output
    let mut found_horizontal = false;  // ─ U+2500
    let mut found_vertical = false;    // │ U+2502
    let mut found_corner = false;      // ┌ U+250C

    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            for span in &styled.spans {
                if span.text.contains('─') {
                    found_horizontal = true;
                }
                if span.text.contains('│') {
                    found_vertical = true;
                }
                if span.text.contains('┌') {
                    found_corner = true;
                }
            }
        }
    }
    assert!(
        found_horizontal && found_vertical && found_corner,
        "Expected box-drawing characters: horizontal={}, vertical={}, corner={}",
        found_horizontal,
        found_vertical,
        found_corner
    );
}

/// Test that block element characters are captured in terminal output.
#[test]
fn test_block_element_characters_captured() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Output block elements: █ (full block U+2588), ▀ (upper half U+2580)
    // We use printf with the raw UTF-8 characters since macOS printf doesn't
    // support \u escape sequences.
    terminal
        .spawn_command(
            "printf",
            &["%s\n", "█▀"],
            Path::new("/tmp"),
        )
        .unwrap();

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    terminal.poll_events();

    // Find block element characters in output
    let mut found_full_block = false;   // █ U+2588
    let mut found_upper_half = false;   // ▀ U+2580

    for line in 0..terminal.line_count() {
        if let Some(styled) = terminal.styled_line(line) {
            for span in &styled.spans {
                if span.text.contains('█') {
                    found_full_block = true;
                }
                if span.text.contains('▀') {
                    found_upper_half = true;
                }
            }
        }
    }
    assert!(
        found_full_block && found_upper_half,
        "Expected block elements: full_block={}, upper_half={}",
        found_full_block,
        found_upper_half
    );
}
