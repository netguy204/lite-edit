<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Build the input encoding layer that translates macOS keyboard and mouse events into terminal escape sequences and writes them to the PTY stdin. This chunk bridges the existing input infrastructure (`KeyEvent`, `MouseEvent`, `Modifiers` in `crates/editor/src/input.rs`) with the terminal emulator's PTY (`TerminalBuffer::write_input()` in `crates/terminal/src/terminal_buffer.rs`).

**Architecture:**

1. **Input encoder module** (`crates/terminal/src/input_encoder.rs`): A pure, stateless module that maps `(KeyEvent/MouseEvent, TermMode)` → `Vec<u8>`. The encoder queries active terminal modes (via `Term::mode()`) to decide encoding:
   - `APP_CURSOR` → use SS3 sequences (`\x1bO`) vs CSI (`\x1b[`) for arrow keys
   - `BRACKETED_PASTE` → wrap pasted text in `\x1b[200~`...`\x1b[201~`
   - `SGR_MOUSE` → use SGR encoding (`\x1b[<btn;x;yM`) vs legacy X10/normal modes

2. **Terminal focus target** (`crates/terminal/src/terminal_target.rs`): Implements the `FocusTarget` trait for terminal tabs. When a terminal tab is focused, keystrokes route through this target → encoder → PTY stdin, rather than mutating a `TextBuffer`.

3. **Extended Key enum** (`crates/editor/src/input.rs`): Add missing key variants needed for full terminal interaction: F1-F12, Insert.

4. **Integration**: Wire the terminal focus target into the workspace/tab system so `TabKind::Terminal` tabs use `TerminalFocusTarget` rather than `BufferFocusTarget`.

**Testing approach per TESTING_PHILOSOPHY.md:**

The input encoder is pure Rust with no platform dependencies — ideal for TDD. Tests verify encoding correctness by comparing encoder output against expected xterm escape sequences. The FocusTarget integration is tested by constructing a `TerminalBuffer` + context and asserting that PTY receives correct bytes.

## Sequence

### Step 1: Extend the Key enum with function keys and Insert

Add missing key variants to `crates/editor/src/input.rs`:
- `F1` through `F12`
- `Insert`

Also update `crates/editor/src/metal_view.rs` to translate macOS key codes (0x7A for F1, etc.) to these new variants.

Location: `crates/editor/src/input.rs`, `crates/editor/src/metal_view.rs`

### Step 2: Create the input encoder module with basic character encoding

Create `crates/terminal/src/input_encoder.rs` with:

```rust
pub struct InputEncoder;

impl InputEncoder {
    /// Encode a key event given active terminal modes.
    pub fn encode_key(event: &KeyEvent, modes: TermMode) -> Vec<u8>;

    /// Encode a mouse event given active terminal modes.
    pub fn encode_mouse(event: &MouseEvent, modes: TermMode) -> Vec<u8>;

    /// Encode pasted text, respecting bracketed paste mode.
    pub fn encode_paste(text: &str, modes: TermMode) -> Vec<u8>;
}
```

Start with the simplest encodings:
- **Printable ASCII**: Return the character as a single byte
- **Enter**: `\r` (0x0D)
- **Tab**: `\t` (0x09)
- **Escape**: `\x1b` (0x1B)
- **Backspace**: `\x7f` (DEL, which is what most modern terminals expect)

Write failing tests first per TDD philosophy.

Location: `crates/terminal/src/input_encoder.rs`

### Step 3: Implement control character encoding

Encode Ctrl+key combinations:
- `Ctrl+A` → `0x01`, `Ctrl+B` → `0x02`, ..., `Ctrl+Z` → `0x1A`
- `Ctrl+C` → `0x03` (SIGINT)
- `Ctrl+D` → `0x04` (EOF)
- `Ctrl+[` → `0x1B` (same as Escape)
- `Ctrl+\` → `0x1C`, `Ctrl+]` → `0x1D`, `Ctrl+^` → `0x1E`, `Ctrl+_` → `0x1F`

The general rule: for letters A-Z, control code = (char - 'A' + 1) when Ctrl is held.

Location: `crates/terminal/src/input_encoder.rs`

### Step 4: Implement arrow key encoding with APP_CURSOR mode awareness

Arrow keys have mode-dependent encoding:
- **Normal mode** (APP_CURSOR not set):
  - Up: `\x1b[A`, Down: `\x1b[B`, Right: `\x1b[C`, Left: `\x1b[D`
- **Application cursor mode** (APP_CURSOR set):
  - Up: `\x1bOA`, Down: `\x1bOB`, Right: `\x1bOC`, Left: `\x1bOD`

Add tests that verify both modes produce correct sequences.

Location: `crates/terminal/src/input_encoder.rs`

### Step 5: Implement modifier+arrow encoding

When Shift, Ctrl, or Alt modifiers are held with arrow keys, use the xterm extended format:
```
\x1b[1;{modifier}A  (Up with modifier)
\x1b[1;{modifier}B  (Down with modifier)
\x1b[1;{modifier}C  (Right with modifier)
\x1b[1;{modifier}D  (Left with modifier)
```

Where modifier code is:
- Shift: 2
- Alt: 3
- Shift+Alt: 4
- Ctrl: 5
- Shift+Ctrl: 6
- Alt+Ctrl: 7
- Shift+Alt+Ctrl: 8

Location: `crates/terminal/src/input_encoder.rs`

### Step 6: Implement function key encoding (F1-F12)

Function keys use longer escape sequences:
- F1: `\x1bOP`, F2: `\x1bOQ`, F3: `\x1bOR`, F4: `\x1bOS`
- F5: `\x1b[15~`, F6: `\x1b[17~`, F7: `\x1b[18~`, F8: `\x1b[19~`
- F9: `\x1b[20~`, F10: `\x1b[21~`, F11: `\x1b[23~`, F12: `\x1b[24~`

Note the gap at 16 and 22 — these are historical legacy from VT220 keycodes.

With modifiers, F5+ use the form: `\x1b[15;{modifier}~`

Location: `crates/terminal/src/input_encoder.rs`

### Step 7: Implement navigation key encoding (Home, End, PageUp, PageDown, Insert, Delete)

These keys use the `\x1b[{code}~` format:
- Home: `\x1b[1~` or `\x1b[H` (depends on terminal mode)
- End: `\x1b[4~` or `\x1b[F`
- Insert: `\x1b[2~`
- Delete: `\x1b[3~`
- PageUp: `\x1b[5~`
- PageDown: `\x1b[6~`

With modifiers: `\x1b[{code};{modifier}~`

For APP_CURSOR mode, Home and End may use `\x1bOH` and `\x1bOF`.

Location: `crates/terminal/src/input_encoder.rs`

### Step 8: Implement bracketed paste encoding

When `BRACKETED_PASTE` mode is active:
- Wrap pasted text in `\x1b[200~` (start) and `\x1b[201~` (end)
- This allows TUI apps to distinguish pasted text from typed input

```rust
pub fn encode_paste(text: &str, modes: TermMode) -> Vec<u8> {
    if modes.contains(TermMode::BRACKETED_PASTE) {
        let mut result = b"\x1b[200~".to_vec();
        result.extend_from_slice(text.as_bytes());
        result.extend_from_slice(b"\x1b[201~");
        result
    } else {
        text.as_bytes().to_vec()
    }
}
```

Location: `crates/terminal/src/input_encoder.rs`

### Step 9: Implement mouse encoding (X10 and Normal modes)

Mouse events are encoded when `MOUSE_REPORT_CLICK` or related modes are active.

**X10 mode** (basic click reporting):
- Format: `\x1b[M{button}{x+32}{y+32}`
- Button byte: 0 = left, 1 = middle, 2 = right, 3 = release
- Coordinates are 1-indexed and offset by 32 to make them printable

**Normal mode** (extended):
- Same format but includes modifier bits in button byte
- Button byte: (modifier_bits << 2) | button
- Modifier bits: 4 = Shift, 8 = Alt, 16 = Ctrl

Location: `crates/terminal/src/input_encoder.rs`

### Step 10: Implement SGR mouse encoding

When `SGR_MOUSE` mode is active, use the modern SGR format:
```
\x1b[<{button};{x};{y}M  (press)
\x1b[<{button};{x};{y}m  (release)
```

Benefits over legacy encoding:
- Coordinates can exceed 223 (not limited to printable ASCII)
- Explicit press/release distinction
- Button encoding includes modifiers

Location: `crates/terminal/src/input_encoder.rs`

### Step 11: Implement mouse motion and drag encoding

When `MOUSE_MOTION` or `MOUSE_DRAG` modes are active:
- **MOUSE_MOTION**: Report all mouse movement
- **MOUSE_DRAG**: Report movement only while button is held

Use the same encoding format (X10/Normal or SGR) but with:
- Button byte has bit 5 (32) set to indicate motion
- For drag: button byte includes which button is held

Location: `crates/terminal/src/input_encoder.rs`

### Step 12: Create TerminalFocusTarget

Create `crates/terminal/src/terminal_target.rs` implementing `FocusTarget`:

```rust
pub struct TerminalFocusTarget {
    /// Reference to the terminal buffer for mode queries and writing
    terminal: Rc<RefCell<TerminalBuffer>>,
}

impl FocusTarget for TerminalFocusTarget {
    fn handle_key(&mut self, event: KeyEvent, ctx: &mut EditorContext) -> Handled {
        let modes = self.terminal.borrow().term_mode();
        let bytes = InputEncoder::encode_key(&event, modes);
        if !bytes.is_empty() {
            let _ = self.terminal.borrow_mut().write_input(&bytes);
            Handled::Yes
        } else {
            Handled::No
        }
    }

    fn handle_scroll(&mut self, delta: ScrollDelta, ctx: &mut EditorContext) {
        // Scroll viewport through scrollback (doesn't go to PTY)
        // Implementation depends on viewport integration
    }

    fn handle_mouse(&mut self, event: MouseEvent, ctx: &mut EditorContext) {
        let modes = self.terminal.borrow().term_mode();
        if modes.intersects(MOUSE_REPORT_CLICK | MOUSE_MOTION | MOUSE_DRAG) {
            let bytes = InputEncoder::encode_mouse(&event, modes);
            if !bytes.is_empty() {
                let _ = self.terminal.borrow_mut().write_input(&bytes);
            }
        }
    }
}
```

Location: `crates/terminal/src/terminal_target.rs`

### Step 13: Add term_mode() accessor to TerminalBuffer

Expose the terminal mode flags for the encoder to query:

```rust
impl TerminalBuffer {
    /// Returns the current terminal mode flags.
    pub fn term_mode(&self) -> TermMode {
        self.term.mode()
    }
}
```

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 14: Wire terminal target into the workspace/tab system

Update `crates/editor/src/workspace.rs`:
1. Add `Terminal(TerminalBuffer)` variant to `TabBuffer` enum
2. Update tab dispatch logic to use `TerminalFocusTarget` when `TabKind::Terminal`

This may require updating the editor's main event loop to construct the appropriate focus target based on the active tab's kind.

Location: `crates/editor/src/workspace.rs`, potentially `crates/editor/src/main.rs` or the relevant event dispatch code

### Step 15: Write integration tests

Create integration tests that:
1. Spawn a real shell in `TerminalBuffer`
2. Send key events through `TerminalFocusTarget`
3. Verify expected behavior (e.g., typing "echo test" + Enter produces "test" in output)

Additional tests:
- Ctrl+C interrupts a running command (observable via exit behavior)
- Arrow keys work in readline (send up-arrow, verify history recall)

Location: `crates/terminal/tests/input_integration.rs`

### Step 16: Add clipboard paste support

Wire clipboard paste (Cmd+V) through the terminal target:
1. In `TerminalFocusTarget::handle_key()`, detect Cmd+V
2. Read clipboard contents via the existing clipboard module
3. Encode using `InputEncoder::encode_paste()` with BRACKETED_PASTE awareness
4. Write to PTY

Location: `crates/terminal/src/terminal_target.rs`

## Dependencies

- **terminal_emulator chunk**: Must be ACTIVE. This chunk depends on `TerminalBuffer`, `PtyHandle`, and the `BufferView` implementation being in place.
- **alacritty_terminal 0.25**: Already a dependency; provides `TermMode` flags.
- **crates/editor input infrastructure**: `KeyEvent`, `MouseEvent`, `Modifiers` types exist.
- **FocusTarget trait**: Defined in `crates/editor/src/focus.rs`.

## Risks and Open Questions

1. **macOS keycode mapping completeness**: The current `metal_view.rs` key conversion may miss some edge cases (e.g., international keyboards, dead keys). Will need manual testing on non-US keyboards to verify.

2. **Alt+key encoding ambiguity**: On macOS, Option (Alt) is used for special characters (e.g., Option+E for accent). Need to decide whether to send raw character or Alt+key escape sequence. Most terminal emulators use an "option sends meta" preference; initially we'll treat Option as Alt for terminal encoding.

3. **Kitty keyboard protocol**: The investigation mentions KITTY_KEYBOARD_PROTOCOL mode. This is a more modern encoding scheme that avoids some ambiguities. For initial implementation, we'll skip this and use standard xterm encoding. Can be added as a follow-up enhancement.

4. **Mouse coordinate translation**: Mouse events arrive in pixel coordinates. We need to translate to terminal cell coordinates using font metrics (glyph width, line height). This requires access to the renderer's metrics, which may need threading through the context.

5. **Focus target ownership model**: `TerminalFocusTarget` needs a reference to `TerminalBuffer`. The current architecture may use `Rc<RefCell<>>` or the target may be stored alongside the buffer. Need to verify the ownership model works with the existing editor architecture.

6. **Terminal scrollback interaction**: When scrolled up in scrollback, typing should snap viewport to bottom. The scroll handling in `TerminalFocusTarget` needs to coordinate with viewport state.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
