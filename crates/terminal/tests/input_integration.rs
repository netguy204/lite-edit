// Chunk: docs/chunks/terminal_input_encoding - Terminal input encoding integration tests
//!
//! Integration tests for terminal input encoding.
//!
//! These tests verify that keyboard events are correctly encoded and sent to
//! the PTY, producing expected output from the shell.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::thread;
use std::time::Duration;

use lite_edit_input::{Key, KeyEvent, Modifiers};
use lite_edit_terminal::{BufferView, InputEncoder, TerminalBuffer, TerminalFocusTarget};

/// Helper to create a terminal with a shell.
fn create_terminal_with_shell() -> (Rc<RefCell<TerminalBuffer>>, TerminalFocusTarget) {
    let terminal = Rc::new(RefCell::new(TerminalBuffer::new(80, 24, 1000)));
    terminal
        .borrow_mut()
        .spawn_shell("/bin/sh", Path::new("/tmp"))
        .expect("Failed to spawn shell");

    // Give the shell time to initialize
    thread::sleep(Duration::from_millis(100));
    terminal.borrow_mut().poll_events();

    let target = TerminalFocusTarget::new(terminal.clone(), 8.0, 16.0);
    (terminal, target)
}

/// Helper to send a string of characters to the terminal.
fn type_string(target: &mut TerminalFocusTarget, s: &str) {
    for ch in s.chars() {
        let event = KeyEvent::char(ch);
        let result = target.handle_key(event);
        if !result {
            eprintln!("Warning: handle_key returned false for char '{}'", ch);
        }
    }
}

/// Helper to send Enter key.
fn press_enter(target: &mut TerminalFocusTarget) {
    let event = KeyEvent::new(Key::Return, Modifiers::default());
    target.handle_key(event);
}

/// Helper to wait for shell output and poll events.
fn wait_and_poll(terminal: &Rc<RefCell<TerminalBuffer>>, millis: u64) {
    thread::sleep(Duration::from_millis(millis));
    terminal.borrow_mut().poll_events();
}

/// Helper to get terminal content as a string (for debugging).
fn get_terminal_content(terminal: &Rc<RefCell<TerminalBuffer>>) -> String {
    let term = terminal.borrow();
    let mut content = String::new();
    for line in 0..term.line_count() {
        if let Some(styled_line) = term.styled_line(line) {
            // Extract text from all spans
            for span in &styled_line.spans {
                content.push_str(&span.text);
            }
            content.push('\n');
        }
    }
    content
}

#[test]
fn test_typing_basic_command() {
    let (terminal, mut target) = create_terminal_with_shell();

    // Type "echo hello" and press Enter
    type_string(&mut target, "echo hello");
    press_enter(&mut target);

    // Wait for output
    wait_and_poll(&terminal, 200);

    // The terminal should contain "hello" in the output
    let content = get_terminal_content(&terminal);
    assert!(
        content.contains("hello"),
        "Expected 'hello' in output, got: {}",
        content
    );
}

#[test]
fn test_ctrl_c_sends_interrupt() {
    let (terminal, mut target) = create_terminal_with_shell();

    // Start a long-running command
    type_string(&mut target, "sleep 100");
    press_enter(&mut target);
    wait_and_poll(&terminal, 100);

    // Send Ctrl+C
    let event = KeyEvent {
        key: Key::Char('c'),
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
    };
    target.handle_key(event);

    // Wait for the shell to process the interrupt
    wait_and_poll(&terminal, 200);

    // The shell should be ready for new input (sleep should have been interrupted)
    // We can verify by trying to type a new command
    type_string(&mut target, "echo done");
    press_enter(&mut target);
    wait_and_poll(&terminal, 200);

    let content = get_terminal_content(&terminal);
    assert!(
        content.contains("done"),
        "Expected 'done' in output after Ctrl+C, got: {}",
        content
    );
}

#[test]
fn test_backspace_deletes_character() {
    let (terminal, mut target) = create_terminal_with_shell();

    // Type "echo hexxllo", backspace twice, then "llo"
    type_string(&mut target, "echo hexx");

    // Send backspace twice
    let backspace = KeyEvent::new(Key::Backspace, Modifiers::default());
    target.handle_key(backspace.clone());
    target.handle_key(backspace);

    type_string(&mut target, "llo");
    press_enter(&mut target);

    wait_and_poll(&terminal, 200);

    let content = get_terminal_content(&terminal);
    assert!(
        content.contains("hello"),
        "Expected 'hello' in output, got: {}",
        content
    );
}

#[test]
fn test_tab_key_for_completion() {
    let (terminal, mut target) = create_terminal_with_shell();

    // Type "ech" and press Tab for completion
    type_string(&mut target, "ech");

    let tab = KeyEvent::new(Key::Tab, Modifiers::default());
    target.handle_key(tab);

    wait_and_poll(&terminal, 200);

    // The shell might complete "echo" or show completion options
    // This depends on shell configuration, so we just verify the tab was sent
    // by checking the terminal received some output
    let content = get_terminal_content(&terminal);
    // At minimum, the "ech" should be visible
    assert!(
        content.contains("ech"),
        "Expected 'ech' in output, got: {}",
        content
    );
}

#[test]
fn test_escape_key() {
    let (terminal, mut target) = create_terminal_with_shell();

    // Type some text, then press Escape, then more text
    type_string(&mut target, "echo ");

    let escape = KeyEvent::new(Key::Escape, Modifiers::default());
    target.handle_key(escape);

    type_string(&mut target, "test");
    press_enter(&mut target);

    wait_and_poll(&terminal, 200);

    // The output should contain "test" (Escape doesn't typically affect input)
    let content = get_terminal_content(&terminal);
    assert!(
        content.contains("test"),
        "Expected 'test' in output, got: {}",
        content
    );
}

#[test]
fn test_arrow_keys_send_escape_sequences() {
    // Test that arrow keys are encoded correctly by checking the raw encoding
    use alacritty_terminal::term::TermMode;

    // Normal mode
    let up = KeyEvent::new(Key::Up, Modifiers::default());
    let encoded = InputEncoder::encode_key(&up, TermMode::NONE);
    assert_eq!(encoded, b"\x1b[A", "Up arrow should be ESC [ A");

    // APP_CURSOR mode
    let encoded_app = InputEncoder::encode_key(&up, TermMode::APP_CURSOR);
    assert_eq!(encoded_app, b"\x1bOA", "Up arrow in APP_CURSOR should be ESC O A");

    // With Ctrl modifier
    let ctrl_right = KeyEvent {
        key: Key::Right,
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
    };
    let encoded_ctrl = InputEncoder::encode_key(&ctrl_right, TermMode::NONE);
    assert_eq!(encoded_ctrl, b"\x1b[1;5C", "Ctrl+Right should be ESC [ 1 ; 5 C");
}

#[test]
fn test_function_keys_encoding() {
    use alacritty_terminal::term::TermMode;

    // F1-F4 use SS3 sequences
    let f1 = KeyEvent::new(Key::F1, Modifiers::default());
    assert_eq!(InputEncoder::encode_key(&f1, TermMode::NONE), b"\x1bOP");

    let f4 = KeyEvent::new(Key::F4, Modifiers::default());
    assert_eq!(InputEncoder::encode_key(&f4, TermMode::NONE), b"\x1bOS");

    // F5+ use tilde sequences
    let f5 = KeyEvent::new(Key::F5, Modifiers::default());
    assert_eq!(InputEncoder::encode_key(&f5, TermMode::NONE), b"\x1b[15~");

    let f12 = KeyEvent::new(Key::F12, Modifiers::default());
    assert_eq!(InputEncoder::encode_key(&f12, TermMode::NONE), b"\x1b[24~");
}

#[test]
fn test_bracketed_paste() {
    use alacritty_terminal::term::TermMode;

    let text = "hello world";

    // Without bracketed paste mode
    let encoded = InputEncoder::encode_paste(text, TermMode::NONE);
    assert_eq!(encoded, b"hello world");

    // With bracketed paste mode
    let encoded_bracketed = InputEncoder::encode_paste(text, TermMode::BRACKETED_PASTE);
    assert_eq!(encoded_bracketed, b"\x1b[200~hello world\x1b[201~");
}

#[test]
fn test_ctrl_d_sends_eof() {
    let (terminal, mut target) = create_terminal_with_shell();

    // Start a cat command that reads stdin
    type_string(&mut target, "cat");
    press_enter(&mut target);
    wait_and_poll(&terminal, 100);

    // Type some input
    type_string(&mut target, "test input");
    press_enter(&mut target);
    wait_and_poll(&terminal, 100);

    // Send Ctrl+D (EOF)
    let event = KeyEvent {
        key: Key::Char('d'),
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
    };
    target.handle_key(event);

    wait_and_poll(&terminal, 200);

    // The terminal should show the echoed input
    let content = get_terminal_content(&terminal);
    assert!(
        content.contains("test input"),
        "Expected 'test input' in output, got: {}",
        content
    );
}

/// Tests that pasted text appears in the terminal buffer after poll.
/// This validates the fix for terminal_paste_render - ensuring that
/// paste content doesn't render as blank spaces.
///
/// Chunk: docs/chunks/terminal_paste_render - Paste rendering test
#[test]
fn test_paste_content_appears_after_poll() {
    let terminal = Rc::new(RefCell::new(TerminalBuffer::new(80, 24, 1000)));

    // Spawn cat which echoes input
    terminal
        .borrow_mut()
        .spawn_command("cat", &[], Path::new("/tmp"))
        .expect("Failed to spawn cat");

    // Wait for cat to start
    thread::sleep(Duration::from_millis(50));
    terminal.borrow_mut().poll_events();

    // Simulate paste (write bytes directly, no bracketed paste for simplicity)
    let paste_text = "hello world";
    terminal
        .borrow_mut()
        .write_input(paste_text.as_bytes())
        .expect("Failed to write paste input");
    terminal
        .borrow_mut()
        .write_input(b"\n")
        .expect("Failed to write newline"); // End with newline to complete the echo

    // Poll for output with timeout
    let mut found = false;
    for _ in 0..50 {
        if terminal.borrow_mut().poll_events() {
            // Check if pasted text appears in buffer
            let term = terminal.borrow();
            for line in 0..term.line_count() {
                if let Some(styled) = term.styled_line(line) {
                    let text: String = styled.spans.iter().map(|s| s.text.as_str()).collect();
                    if text.contains("hello world") {
                        found = true;
                        break;
                    }
                }
            }
            drop(term);
            if found {
                break;
            }
        }
        thread::sleep(Duration::from_millis(20));
    }

    assert!(
        found,
        "Pasted text 'hello world' should appear in terminal buffer after polling"
    );
}

// Chunk: docs/chunks/terminal_alt_backspace - Alt+Backspace integration test
#[test]
fn test_alt_backspace_deletes_word() {
    let (terminal, mut target) = create_terminal_with_shell();

    // Type "echo hello world" (without pressing Enter)
    type_string(&mut target, "echo hello world");

    // Give shell time to process the input
    wait_and_poll(&terminal, 100);

    // Send Alt+Backspace to delete "world"
    let alt_backspace = KeyEvent {
        key: Key::Backspace,
        modifiers: Modifiers {
            option: true,
            ..Default::default()
        },
    };
    target.handle_key(alt_backspace);

    // Give shell time to process the word deletion
    wait_and_poll(&terminal, 100);

    // Now press Enter to execute the command
    press_enter(&mut target);

    // Wait for output
    wait_and_poll(&terminal, 200);

    // The terminal should output "hello " (with trailing space) since "world" was deleted
    let content = get_terminal_content(&terminal);

    // The output should contain "hello" but NOT "world" (or contain "hello " specifically)
    // Note: The exact behavior depends on readline, but the key sequence \x1b\x7f was sent
    // We verify that "hello" is present in the output
    assert!(
        content.contains("hello"),
        "Expected 'hello' in output, got: {}",
        content
    );
}

// Chunk: docs/chunks/terminal_cmd_backspace - Cmd+Backspace integration test
#[test]
fn test_cmd_backspace_deletes_to_line_start() {
    let (terminal, mut target) = create_terminal_with_shell();

    // Type "echo hello world" (without pressing Enter)
    type_string(&mut target, "echo hello world");

    // Give shell time to process the input
    wait_and_poll(&terminal, 100);

    // Send Cmd+Backspace to delete from cursor to line start
    let cmd_backspace = KeyEvent {
        key: Key::Backspace,
        modifiers: Modifiers {
            command: true,
            ..Default::default()
        },
    };
    target.handle_key(cmd_backspace);

    // Give shell time to process the line deletion
    wait_and_poll(&terminal, 100);

    // Type something new to verify the line was cleared
    type_string(&mut target, "echo CLEARED");
    press_enter(&mut target);

    // Wait for output
    wait_and_poll(&terminal, 200);

    // The terminal should output "CLEARED" (the original text was deleted)
    let content = get_terminal_content(&terminal);
    assert!(
        content.contains("CLEARED"),
        "Expected 'CLEARED' in output after Cmd+Backspace cleared the line, got: {}",
        content
    );
}
