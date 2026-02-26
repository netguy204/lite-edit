<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk implements IME (Input Method Editor) support for Chinese, Japanese, Korean, and other languages that require composition. The core change is implementing `NSTextInputClient` protocol on MetalView and splitting the input event model to distinguish physical key events (for shortcuts) from text input events (from IME, paste, dictation).

**Key insight from ARCHITECTURE_REVIEW.md**: The current `KeyEvent` conflates "the user pressed a physical key" with "the user wants to insert text." For IME to work, these must be separate. Physical keys like arrows, escape, and modifier-key combinations should continue as `KeyEvent`, while text from IME composition, paste, and dictation should flow through new text input events.

**Architecture**:
1. **NSTextInputClient protocol** on MetalView receives IME callbacks from macOS
2. **New InputEvent variants** in `crates/input/src/lib.rs` for `InsertText`, `SetMarkedText`, `UnmarkText`
3. **New EditorEvent variants** to carry these through the event channel
4. **Marked text state** in TextBuffer to track in-progress composition
5. **Underline rendering** for marked text using existing `UnderlineStyle::Single`

The humble view architecture (per TESTING_PHILOSOPHY.md) means:
- The NSTextInputClient impl is a thin shell forwarding to EventSender
- Marked text state lives in TextBuffer (testable without platform)
- Rendering logic uses existing `Style.underline` for marked text display

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk USES the renderer subsystem for marked text underline display. The underline rendering already exists (`UnderlineStyle::Single` in `buffer_view.rs`); we'll leverage it for marked text.

## Sequence

### Step 1: Add text input event variants to lite-edit-input

Extend the input types in `crates/input/src/lib.rs` to support IME:

```rust
/// Text insertion from keyboard, IME, paste, or dictation.
///
/// Unlike KeyEvent, this represents final text to insert, not physical keys.
#[derive(Debug, Clone, PartialEq)]
pub struct TextInputEvent {
    /// The text to insert
    pub text: String,
    /// Optional range to replace (for IME replacement). None = insert at cursor.
    pub replacement_range: Option<std::ops::Range<usize>>,
}

/// IME marked text (in-progress composition).
///
/// Marked text is displayed with an underline to indicate it's uncommitted.
/// The user can continue composing until they commit or cancel.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkedTextEvent {
    /// The composed text being input
    pub text: String,
    /// Selected range within the marked text (for IME cursor)
    pub selected_range: std::ops::Range<usize>,
    /// Range in buffer to replace (None = current marked text or cursor)
    pub replacement_range: Option<std::ops::Range<usize>>,
}
```

Location: `crates/input/src/lib.rs`

### Step 2: Add EditorEvent variants for text input

Extend `EditorEvent` in `crates/editor/src/editor_event.rs`:

```rust
/// Text insertion from IME or other text input sources
InsertText(TextInputEvent),

/// IME composition in progress (marked text)
SetMarkedText(MarkedTextEvent),

/// IME composition canceled or committed
UnmarkText,
```

Mark these as priority events in `is_priority_event()`.

Location: `crates/editor/src/editor_event.rs`

### Step 3: Add EventSender methods for text input events

Extend `EventSender` in `crates/editor/src/event_channel.rs`:

```rust
pub fn send_insert_text(&self, event: TextInputEvent) -> Result<(), SendError<EditorEvent>>
pub fn send_set_marked_text(&self, event: MarkedTextEvent) -> Result<(), SendError<EditorEvent>>
pub fn send_unmark_text(&self) -> Result<(), SendError<EditorEvent>>
```

Location: `crates/editor/src/event_channel.rs`

### Step 4: Add marked text state to TextBuffer

Add marked text tracking to `TextBuffer` in `crates/buffer/src/text_buffer.rs`:

```rust
/// State of IME marked text (in-progress composition).
#[derive(Debug, Clone, Default)]
pub struct MarkedTextState {
    /// The marked text content
    pub text: String,
    /// Start position in the buffer where marked text begins
    pub start: Position,
    /// Selected range within the marked text (for IME cursor display)
    pub selected_range: std::ops::Range<usize>,
}

impl TextBuffer {
    /// Returns the current marked text state, if any.
    pub fn marked_text(&self) -> Option<&MarkedTextState> { ... }

    /// Sets or updates the marked text.
    /// Returns the dirty lines affected.
    pub fn set_marked_text(&mut self, text: &str, selected_range: Range<usize>) -> DirtyLines { ... }

    /// Commits the marked text (inserts it permanently) and clears marked state.
    /// Returns the dirty lines affected.
    pub fn commit_marked_text(&mut self) -> DirtyLines { ... }

    /// Cancels the marked text (removes it) and clears marked state.
    /// Returns the dirty lines affected.
    pub fn cancel_marked_text(&mut self) -> DirtyLines { ... }
}
```

Implementation notes:
- When marked text is set, it replaces any existing marked text at the mark position
- Marked text is stored separately from the buffer content (not in the gap buffer)
- `styled_line()` must overlay marked text with underline style when rendering
- Cursor position during marking shows the IME cursor within marked text

Location: `crates/buffer/src/text_buffer.rs`

### Step 5: Implement NSTextInputClient protocol on MetalView

The key NSTextInputClient methods needed:

```objc
// REQUIRED methods:
- (void)insertText:(id)string replacementRange:(NSRange)replacementRange;
- (void)setMarkedText:(id)string selectedRange:(NSRange)selectedRange replacementRange:(NSRange)replacementRange;
- (void)unmarkText;
- (BOOL)hasMarkedText;
- (NSRange)markedRange;
- (NSRange)selectedRange;
- (NSAttributedString *)attributedSubstringForProposedRange:(NSRange)range actualRange:(NSRangePointer)actualRange;
- (NSUInteger)characterIndexForPoint:(NSPoint)point;
- (NSRect)firstRectForCharacterRange:(NSRange)range actualRange:(NSRangePointer)actualRange;
- (NSArray<NSAttributedStringKey> *)validAttributesForMarkedText;
```

Implementation in `crates/editor/src/metal_view.rs`:

1. Add `NSTextInputClient` protocol conformance to the `define_class!` macro
2. Implement each method, with the text-inserting methods forwarding to EventSender
3. Modify `__key_down` to call `interpretKeyEvents:` for regular text input, which routes through NSTextInputClient

The key behavior change: instead of converting keyDown directly to KeyEvent for printable characters, we call `interpretKeyEvents:` which lets macOS's text input system (including IME) process the keys and call back to our NSTextInputClient methods.

Location: `crates/editor/src/metal_view.rs`

### Step 6: Handle text input events in drain_loop

Add handlers in `EventDrainLoop` for the new events:

```rust
EditorEvent::InsertText(event) => self.handle_insert_text(event),
EditorEvent::SetMarkedText(event) => self.handle_set_marked_text(event),
EditorEvent::UnmarkText => self.handle_unmark_text(),
```

Each handler forwards to `EditorState` which routes to the appropriate focus target.

Location: `crates/editor/src/drain_loop.rs`

### Step 7: Process text input in buffer focus target

Update `buffer_target.rs` to handle text input:

1. Add a new method `handle_text_input(&mut self, text: &str, ctx: &mut EditorContext)` that inserts text using `ctx.buffer.insert_str()`
2. Add `handle_set_marked_text()` and `handle_unmark_text()` methods
3. Keep the existing `resolve_command()` for physical key events (arrows, modifiers, etc.)

The key distinction:
- Physical keys (Return, Tab, Backspace, arrows, Cmd+shortcuts) → `resolve_command()`
- Text insertion → `handle_text_input()` (no command resolution needed)

Location: `crates/editor/src/buffer_target.rs`

### Step 8: Render marked text with underline

Modify `TextBuffer::styled_line()` to overlay marked text styling:

1. When a line contains marked text, split spans at marked text boundaries
2. Apply `UnderlineStyle::Single` to the marked text spans
3. The IME cursor position within marked text can be shown using the selection mechanism

The renderer already supports underline rendering (via `UnderlineStyle`), so no changes needed there.

Location: `crates/buffer/src/text_buffer.rs` (in `styled_line()` implementation)

### Step 9: Update keyDown to route through text input system

Modify MetalView's `__key_down` to distinguish:

1. **Navigation/control keys**: Forward as KeyEvent (Escape, arrows, Backspace, etc.)
2. **Modifier-held keys**: Forward as KeyEvent (Cmd+X, Ctrl+A, etc.)
3. **Text input keys**: Call `interpretKeyEvents:` which routes through IME

```rust
#[unsafe(method(keyDown:))]
fn __key_down(&self, event: &NSEvent) {
    // Check for special keys and modifier combinations that should bypass text input
    let key_code = event.keyCode();
    let mods = event.modifierFlags();

    if self.is_navigation_or_command_key(key_code, mods) {
        // Forward as KeyEvent
        if let Some(key_event) = self.convert_key_event(event) {
            // ... send via EventSender
        }
    } else {
        // Route through macOS text input system (handles IME)
        self.interpretKeyEvents(&NSArray::from_slice(&[event]));
    }
}
```

Location: `crates/editor/src/metal_view.rs`

### Step 10: Write tests for marked text behavior

Tests for `TextBuffer` marked text operations (TDD approach):

```rust
#[test]
fn test_set_marked_text_basic() {
    let mut buf = TextBuffer::new();
    buf.set_marked_text("にほん", 0..6); // "にほん" (nihon)
    assert!(buf.marked_text().is_some());
    assert_eq!(buf.marked_text().unwrap().text, "にほん");
}

#[test]
fn test_commit_marked_text() {
    let mut buf = TextBuffer::new();
    buf.set_marked_text("日本", 0..2);
    buf.commit_marked_text();
    assert!(buf.marked_text().is_none());
    assert_eq!(buf.line_content(0), "日本");
}

#[test]
fn test_cancel_marked_text() {
    let mut buf = TextBuffer::new();
    buf.set_marked_text("test", 0..4);
    buf.cancel_marked_text();
    assert!(buf.marked_text().is_none());
    assert_eq!(buf.line_content(0), "");
}

#[test]
fn test_marked_text_styled_line_has_underline() {
    let mut buf = TextBuffer::new();
    buf.set_marked_text("abc", 0..3);
    let styled = buf.styled_line(0);
    // Verify the marked text has underline style
    assert!(styled.spans.iter().any(|s| s.style.underline == UnderlineStyle::Single));
}
```

Location: `crates/buffer/src/text_buffer.rs` (in `#[cfg(test)]` module)

### Step 11: Integration testing with manual verification

Since IME behavior requires macOS text input system, full integration testing is manual:

1. Switch to Japanese IME (Hiragana)
2. Type "nihon" → see "にほん" with underline (marked text)
3. Press Space to see candidate kanji → select 日本
4. Press Enter → marked text commits as "日本"
5. Press Escape during composition → marked text disappears

Also test:
- Dead keys: Option+e, then e → "é"
- Chinese Pinyin: type "zhong", select 中
- Regular ASCII typing still works without latency regression

---

**BACKREFERENCE COMMENTS**

Add backreference comments to implementations:
```rust
// Chunk: docs/chunks/unicode_ime_input - NSTextInputClient for IME support
```

## Dependencies

No external dependencies required. The implementation uses:
- Existing `objc2` and `objc2-app-kit` crates for NSTextInputClient protocol
- Existing underline rendering in the renderer subsystem
- Existing event channel architecture

## Risks and Open Questions

1. **NSTextInputClient protocol conformance in objc2**: Need to verify that `objc2`'s `define_class!` macro supports implementing Objective-C protocols with the `NSTextInputClient` methods. If not, may need unsafe manual protocol impl.

2. **Character position mapping for IME**: `characterIndexForPoint:` and `firstRectForCharacterRange:` require converting between screen coordinates and buffer positions. This is non-trivial with line wrapping. May return approximate values initially.

3. **Marked text spanning multiple lines**: If the user types a long composition that wraps, the underline rendering needs to handle multi-line marked text. Initial implementation may restrict marked text to single lines.

4. **Performance with rapid IME updates**: Each marked text update triggers re-rendering. Should verify this doesn't cause perceptible latency with fast typing in Chinese Pinyin (which updates on every keystroke).

5. **Interaction with selection**: What happens if the user has a selection when IME starts? Current plan: clear selection and start composition at cursor. May need refinement.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->
