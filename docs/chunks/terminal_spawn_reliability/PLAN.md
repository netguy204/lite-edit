# Implementation Plan

## Approach

This chunk addresses two related bugs that create a feedback loop: PTY fd leaks cause spawn failures, and spawn error swallowing hides the problem from users.

**Bug 1: Error swallowing in `new_terminal_tab`**

Currently, `new_terminal_tab` (at `crates/editor/src/editor_state.rs:4987-4989`) logs spawn errors to stderr and proceeds to create a non-functional tab with `pty: None`. Users see a "Terminal" tab that accepts no input and shows no output.

The fix introduces a new `TabBuffer::Error` variant that renders an error message and offers retry functionality — analogous to Chrome's "Aw, Snap!" error page.

**Bug 2: PTY fd leaks in `PtyHandle::Drop`**

Currently, `PtyHandle::Drop` (at `crates/terminal/src/pty.rs:371-386`) kills the child process but detaches the reader thread without joining. The reader thread may still hold the PTY master fd, preventing the kernel from releasing the PTY device. This can cause subsequent `openpty` calls to fail with `ENXIO`.

The fix adds a brief timed join (100ms) to the Drop impl. In the common case, the reader thread exits promptly after the process is killed and the PTY hits EOF. The timeout bounds worst-case blocking while still ensuring timely fd release in normal operation.

**Testing approach**: Per TESTING_PHILOSOPHY.md, we'll test the error state rendering semantics (not the Metal/GPU output) and verify PTY cleanup behavior through PTY lifecycle tests. The contention experiment from the investigation serves as a regression benchmark.

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk USES the renderer subsystem indirectly. The new `TabBuffer::Error` variant must implement `BufferView` so it can be rendered through the existing pipeline. No changes to the renderer itself are needed — we're just providing a new buffer type that the existing polymorphic rendering handles transparently.

## Sequence

### Step 1: Add `TabBuffer::Error` variant

Add a new variant to the `TabBuffer` enum in `crates/editor/src/workspace.rs`:

```rust
pub enum TabBuffer {
    File(TextBuffer),
    Terminal(TerminalBuffer),
    AgentTerminal,
    Error { message: String },  // NEW
}
```

Update all match arms throughout the file to handle this variant:
- `as_buffer_view()` / `as_buffer_view_mut()`: Return an `ErrorBuffer` impl
- `as_text_buffer()` / `as_terminal_buffer()`: Return `None`
- `Debug` impl: Format appropriately

Location: `crates/editor/src/workspace.rs`

### Step 2: Implement `ErrorBuffer` struct with `BufferView`

Create a minimal struct that implements `BufferView` for rendering the error state:

```rust
pub struct ErrorBuffer {
    message: String,
}

impl BufferView for ErrorBuffer {
    fn line_count(&self) -> usize { 3 }  // Title, blank, message
    fn styled_line(&self, line: usize) -> Option<StyledLine> {
        match line {
            0 => Some(StyledLine::plain("Failed to create terminal")),
            1 => Some(StyledLine::empty()),
            2 => Some(StyledLine::plain(&self.message)),
            _ => None
        }
    }
    fn line_len(&self, line: usize) -> usize { ... }
    fn take_dirty(&mut self) -> DirtyLines { DirtyLines::None }
    fn is_editable(&self) -> bool { false }
    fn cursor_info(&self) -> Option<CursorInfo> { None }
}
```

Location: `crates/editor/src/workspace.rs` (or new file `crates/editor/src/error_buffer.rs` if cleaner)

### Step 3: Add `Tab::new_error` constructor

Add a constructor for error tabs analogous to `new_file` and `new_terminal`:

```rust
impl Tab {
    pub fn new_error(id: TabId, error_message: String, label: String, line_height: f32) -> Self {
        Self {
            id,
            label,
            buffer: TabBuffer::Error { message: error_message },
            kind: TabKind::Terminal,  // Same visual treatment as terminal
            // ...other fields
        }
    }
}
```

Location: `crates/editor/src/workspace.rs`

### Step 4: Modify `new_terminal_tab` to use error state on spawn failure

Change the error handling in `new_terminal_tab` from:

```rust
if let Err(e) = spawn_result {
    eprintln!("Failed to spawn shell: {}", e);
}
// proceed to create tab with pty: None
```

To:

```rust
match spawn_result {
    Ok(()) => {
        // Create normal terminal tab
        let tab = Tab::new_terminal(tab_id, terminal, label, line_height);
        workspace.add_tab(tab);
    }
    Err(e) => {
        // Create error tab instead
        let tab = Tab::new_error(tab_id, e.to_string(), label, line_height);
        workspace.add_tab(tab);
    }
}
```

Location: `crates/editor/src/editor_state.rs` (~line 4929-5020)

### Step 5: Implement retry mechanism for error tabs

Add a method to detect and retry error tabs:

```rust
impl EditorState {
    pub fn retry_terminal_tab(&mut self) {
        // Check if active tab is an error tab
        // If so, replace it with a new terminal spawn attempt
    }
}
```

Wire this to a keyboard shortcut (e.g., Enter on an error tab) or add a rendered "Press Enter to retry" hint.

Location: `crates/editor/src/editor_state.rs`

### Step 6: Add timed join to `PtyHandle::Drop`

Modify the Drop impl to join the reader thread with a timeout:

```rust
impl Drop for PtyHandle {
    fn drop(&mut self) {
        // Kill the process first
        let _ = self.child.kill();

        // Join the reader thread with a brief timeout.
        // The thread should exit promptly once the PTY hits EOF.
        // If it doesn't exit in time, we detach to avoid blocking forever.
        if let Some(handle) = self.reader_thread.take() {
            // Use a timed park/join pattern
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_millis(100);

            // Spawn a helper thread to join the reader and signal completion
            // OR use thread::park_timeout with a check pattern
            // OR check if portable_pty provides a better mechanism
        }
    }
}
```

**Implementation note**: Rust's standard library doesn't provide `JoinHandle::join_timeout()`. Options:
1. Use a `crossbeam_channel` with timeout to signal thread exit (cleanest)
2. Use `std::thread::park_timeout` with a shared atomic
3. Accept blocking but add a comment noting it's bounded by process kill

Prefer option 1 since `crossbeam_channel` is already a dependency.

Location: `crates/terminal/src/pty.rs` (~line 371-386)

### Step 7: Write tests for error tab rendering

Test that error tabs:
- Return correct `line_count()` and `styled_line()` values
- Have `is_editable() == false`
- Have `cursor_info() == None`
- Are created when `spawn_shell` fails

```rust
#[test]
fn test_error_tab_buffer_view() {
    let error_buf = ErrorBuffer::new("openpty: ENXIO");
    assert_eq!(error_buf.line_count(), 3);
    assert!(error_buf.styled_line(0).is_some());
    assert!(!error_buf.is_editable());
}
```

Location: `crates/editor/src/workspace.rs` (test module)

### Step 8: Write tests for PTY cleanup

Test that PTY handles release resources properly:

```rust
#[test]
fn test_pty_handle_drop_joins_reader() {
    // Create a PTY, verify it spawns
    // Drop it, verify the reader thread is joined (not just detached)
    // This is tricky to test directly - may need to verify via fd counting
}
```

Also verify the contention experiment from the investigation doesn't regress:
- Spawning 10 shells simultaneously should succeed (per investigation, 25 started failing)

Location: `crates/terminal/src/pty.rs` (test module)

### Step 9: Update GOAL.md code_paths

Update the chunk's GOAL.md frontmatter with the files touched:

```yaml
code_paths:
- crates/editor/src/workspace.rs
- crates/editor/src/editor_state.rs
- crates/terminal/src/pty.rs
```

Location: `docs/chunks/terminal_spawn_reliability/GOAL.md`

## Risks and Open Questions

1. **JoinHandle timeout pattern**: Rust doesn't have `join_timeout()`. The cleanest solution uses `crossbeam_channel` to signal thread completion with a timeout, but this adds complexity to the reader thread. Alternative: accept blocking but ensure the process kill guarantees eventual thread exit.

2. **Retry UX**: The success criteria mention "offers a retry action" but don't specify the UX. Options:
   - Keyboard shortcut (Enter on error tab)
   - Rendered clickable text
   - Both

   Will implement keyboard-only initially for simplicity.

3. **Error message formatting**: The raw `io::Error` message may be cryptic (e.g., "Device not configured" for ENXIO). Consider adding context like "Failed to create terminal: {error}. Try closing some terminal tabs."

4. **TabKind for error tabs**: Using `TabKind::Terminal` for error tabs means they'll be counted in `terminal_tab_count()`. May want a `TabKind::Error` variant instead, though this has broader implications for tab bar rendering.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->