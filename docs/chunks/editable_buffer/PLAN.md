# Implementation Plan

## Approach

This chunk implements the capstone of the editor core architecture: wiring keyboard input to the buffer and renderer through the drain-all-then-render main loop pattern established in the `editor_core_architecture` investigation.

The architecture follows the investigation's findings:

1. **FocusTarget trait** — Each focus target interprets its own input. The buffer's focus target owns a stateless chord resolver that maps (modifiers, key) → Command. No state machine needed since all target chords are single-step modifier+key combinations.

2. **EditorContext** — Provides mutable access to buffer, viewport, and dirty region accumulator. Focus targets mutate state through this context.

3. **Drain-all-then-render loop** — Each NSRunLoop iteration: (1) drain all pending events, forwarding each to the active focus target which mutates buffer and accumulates dirty regions, (2) render once if dirty, (3) sleep until next event or timer. This ensures latency fairness.

4. **Cursor blink** — An NSTimer toggles cursor visibility every ~500ms. Reset on any keystroke.

We build on existing infrastructure from dependent chunks:
- `text_buffer`: TextBuffer with insert/delete/cursor movement and DirtyLines tracking
- `viewport_rendering`: Viewport, DirtyRegion, and Renderer with `apply_mutation()` and `render_dirty()`

Testing strategy per TESTING_PHILOSOPHY.md:
- **Pure Rust tests**: FocusTarget logic, EditorContext, chord resolution — all testable without Metal or macOS
- **Humble view**: MetalView event forwarding is thin shell (not unit-tested); the logic it calls IS tested
- The Elm-style architecture makes everything testable: construct state, call handle_key(), assert on buffer and dirty region

## Sequence

### Step 1: Define input event types

Create `crates/editor/src/input.rs` with types for input events:

```rust
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: Modifiers,
}

pub struct Modifiers {
    pub shift: bool,
    pub command: bool,
    pub option: bool,
    pub control: bool,
}

pub enum Key {
    Char(char),
    Backspace,
    Delete,
    Return,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Tab,
    Escape,
}
```

Also define `ScrollDelta` and `MouseEvent` stubs for future use.

Location: `crates/editor/src/input.rs`

### Step 2: Define the FocusTarget trait and Handled enum

Create `crates/editor/src/focus.rs`:

```rust
pub enum Handled {
    Yes,
    No,
}

pub trait FocusTarget {
    fn handle_key(&mut self, event: KeyEvent, ctx: &mut EditorContext) -> Handled;
    fn handle_scroll(&mut self, delta: ScrollDelta, ctx: &mut EditorContext);
    fn handle_mouse(&mut self, event: MouseEvent, ctx: &mut EditorContext);
}
```

This trait is the core abstraction from the investigation. Focus targets interpret their own input.

Location: `crates/editor/src/focus.rs`

### Step 3: Define EditorContext

Create `crates/editor/src/context.rs`:

```rust
pub struct EditorContext<'a> {
    pub buffer: &'a mut TextBuffer,
    pub viewport: &'a mut Viewport,
    pub dirty_region: &'a mut DirtyRegion,
}
```

EditorContext provides mutable access to core state. Focus targets mutate through it.

Add helper methods:
- `mark_dirty(dirty_lines: DirtyLines)` — converts buffer DirtyLines to screen DirtyRegion and merges
- `ensure_cursor_visible()` — calls `viewport.ensure_visible()` and marks dirty if scrolled

Location: `crates/editor/src/context.rs`

### Step 4: Implement BufferFocusTarget with stateless chord resolution

Create `crates/editor/src/buffer_target.rs`:

The buffer's focus target handles all basic editing keystrokes. Chord resolution is a pure function per H2 findings — no state machine needed.

```rust
pub struct BufferFocusTarget;

impl FocusTarget for BufferFocusTarget {
    fn handle_key(&mut self, event: KeyEvent, ctx: &mut EditorContext) -> Handled {
        match resolve_command(&event) {
            Some(cmd) => {
                self.execute_command(cmd, ctx);
                Handled::Yes
            }
            None => Handled::No,
        }
    }
    // ...
}

fn resolve_command(event: &KeyEvent) -> Option<Command> {
    // Pure stateless function: (modifiers, key) → Option<Command>
    // Per H2: all chords are single-step modifier+key
}

enum Command {
    InsertChar(char),
    InsertNewline,
    DeleteBackward,
    DeleteForward,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveToLineStart,
    MoveToLineEnd,
    MoveToBufferStart,
    MoveToBufferEnd,
    SelectAll, // Placeholder: move cursor to start, then end
}
```

Command execution:
- Calls appropriate TextBuffer methods
- Converts DirtyLines to DirtyRegion via `ctx.mark_dirty()`
- Calls `ctx.ensure_cursor_visible()` for cursor movements

Location: `crates/editor/src/buffer_target.rs`

### Step 5: Add unit tests for BufferFocusTarget

Test the focus target in isolation without Metal or macOS:

```rust
#[test]
fn test_typing_hello() {
    let mut buffer = TextBuffer::new();
    let mut viewport = Viewport::new(16.0);
    viewport.update_size(160.0);
    let mut dirty = DirtyRegion::None;
    let mut ctx = EditorContext { buffer: &mut buffer, viewport: &mut viewport, dirty_region: &mut dirty };
    let mut target = BufferFocusTarget;

    target.handle_key(KeyEvent::char('H'), &mut ctx);
    target.handle_key(KeyEvent::char('i'), &mut ctx);

    assert_eq!(ctx.buffer.content(), "Hi");
    assert_eq!(ctx.buffer.cursor_position(), Position::new(0, 2));
    assert!(ctx.dirty_region.is_dirty());
}
```

Test cases:
- Typing characters inserts them at cursor
- Backspace deletes backward, joins lines
- Delete key deletes forward, joins lines
- Arrow keys move cursor
- Enter inserts newline
- Cursor movement past viewport scrolls (ensure_cursor_visible)
- Multiple events accumulate dirty regions correctly

Location: `crates/editor/src/buffer_target.rs` (in #[cfg(test)] module)

### Step 6: Extend MetalView with key event handling

Modify `crates/editor/src/metal_view.rs` to override `keyDown:` and convert NSEvent to KeyEvent.

Add Objective-C method overrides:
- `keyDown:` — extract key code, characters, modifier flags; convert to KeyEvent
- `acceptsFirstResponder` — return YES so view receives key events
- `flagsChanged:` — capture modifier key changes (for future use)

The view itself doesn't handle the event — it calls a callback/delegate. We'll use a Rust closure or function pointer stored in ivars.

Add to MetalViewIvars:
```rust
key_handler: RefCell<Option<Box<dyn Fn(KeyEvent)>>>,
```

Add method:
```rust
pub fn set_key_handler(&self, handler: impl Fn(KeyEvent) + 'static);
```

Location: `crates/editor/src/metal_view.rs`

### Step 7: Create EditorState to hold all mutable state

Create `crates/editor/src/editor_state.rs`:

```rust
pub struct EditorState {
    pub buffer: TextBuffer,
    pub viewport: Viewport,
    pub dirty_region: DirtyRegion,
    pub focus_target: BufferFocusTarget,
    pub cursor_visible: bool,
}

impl EditorState {
    pub fn handle_key(&mut self, event: KeyEvent) {
        let mut ctx = EditorContext {
            buffer: &mut self.buffer,
            viewport: &mut self.viewport,
            dirty_region: &mut self.dirty_region,
        };
        self.focus_target.handle_key(event, &mut ctx);
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_region.is_dirty()
    }

    pub fn take_dirty_region(&mut self) -> DirtyRegion {
        std::mem::take(&mut self.dirty_region)
    }
}
```

This consolidates state that the main loop needs.

Location: `crates/editor/src/editor_state.rs`

### Step 8: Implement cursor blink timer

Add cursor blink to EditorState:
- `last_keystroke_time: Instant`
- `cursor_blink_on: bool`

Add method to toggle cursor blink:
```rust
pub fn toggle_cursor_blink(&mut self) -> DirtyRegion {
    self.cursor_visible = !self.cursor_visible;
    // Return dirty region for cursor line only
}
```

The timer will be an NSTimer created in the AppDelegate. On fire:
1. If time since last keystroke < threshold, reset cursor to visible
2. Otherwise toggle visibility
3. Mark cursor line dirty

Location: `crates/editor/src/editor_state.rs`

### Step 9: Rewire main.rs with drain-all-then-render loop

This is the critical architectural change. The main loop must:

1. **Setup**: Create EditorState with demo buffer, create MetalView with key handler
2. **Key handler**: Forward KeyEvent to `editor_state.handle_key()`
3. **Timer callback**: Call `editor_state.toggle_cursor_blink()`
4. **Render trigger**: After event processing, check if dirty and call `renderer.render_dirty()`

The challenge: NSRunLoop doesn't expose a "drain all events then render" API directly. We'll use:
- `nextEventMatchingMask:untilDate:inMode:dequeue:` with `[NSDate distantPast]` to drain events non-blocking
- Loop until no more events
- Then render if dirty
- Then call `run` with a short timeout or use `runUntilDate:`

Actually, the simpler approach: use standard Cocoa patterns where:
- keyDown: triggers immediately through the key handler
- The key handler mutates EditorState and sets needs_display
- windowDidUpdate: or a display link triggers render

But per the investigation, we want the drain-all-then-render pattern for latency fairness. The way to achieve this on macOS:
- Install a `CFRunLoopObserver` for `kCFRunLoopBeforeWaiting`
- In the observer callback, render if dirty
- This runs after all events are processed but before sleeping

This is the correct pattern: events accumulate dirty regions, observer renders once before sleep.

Alternative: use `setNeedsDisplay:` and let the view's `drawRect:` method trigger rendering. But this integrates with AppKit's display cycle rather than our Metal rendering.

For this implementation, we'll use the observer pattern:
1. Key events forward to EditorState
2. EditorState accumulates dirty regions
3. Before-waiting observer triggers render if dirty

Location: `crates/editor/src/main.rs`

### Step 10: Wire up NSTimer for cursor blink

Create an NSTimer in AppDelegate that fires every 500ms:

```rust
let timer = NSTimer::scheduledTimerWithTimeInterval_repeats_block(
    0.5,
    true,
    &block![|_timer| {
        // Toggle cursor blink
        // Mark dirty
        // Trigger render (or set needs_display)
    }]
);
```

On keystroke, record `Instant::now()`. In timer callback, if recently typed, keep cursor solid. Otherwise toggle.

Location: `crates/editor/src/main.rs`

### Step 11: Update Renderer to use EditorState

Modify the Renderer to work with EditorState:
- Take `&EditorState` in render methods
- Use `state.cursor_visible` for cursor rendering

Actually, the Renderer already has `set_cursor_visible()`. Just ensure it's called appropriately.

Location: `crates/editor/src/renderer.rs`

### Step 12: Implement viewport scrolling via cursor

In EditorContext or BufferFocusTarget, after cursor movement:
- Call `viewport.ensure_visible(cursor.line, buffer.line_count())`
- If it scrolled, mark FullViewport dirty

This ensures the cursor stays visible when moving with arrow keys or adding newlines.

Already have `ensure_visible()` in Viewport. Just need to wire it up in the command execution.

Location: `crates/editor/src/buffer_target.rs` (in execute_command)

### Step 13: Integration test: typing demo

Create a smoke test that:
1. Creates EditorState
2. Simulates typing "Hello\nWorld" via handle_key
3. Verifies buffer content
4. Verifies dirty region accumulated correctly

Location: `crates/editor/tests/typing_test.rs`

### Step 14: Manual testing checklist

Before considering this chunk complete:
- [ ] Launch app, type characters, see them appear
- [ ] Use Backspace to delete characters
- [ ] Use Delete key (if available)
- [ ] Arrow keys move cursor
- [ ] Enter creates new line
- [ ] Cursor blinks when idle (~500ms)
- [ ] Cursor stays solid while typing
- [ ] Typing at end of viewport scrolls down
- [ ] Moving cursor up past viewport scrolls up
- [ ] No visual glitches during sustained typing

## Dependencies

**Required chunks (already complete):**
- `text_buffer`: TextBuffer with insert/delete/cursor movement and DirtyLines
- `glyph_rendering`: Glyph atlas and text rendering pipeline
- `viewport_rendering`: Viewport, DirtyRegion, viewport-aware rendering

**External dependencies (already available):**
- `objc2`, `objc2_app_kit`, `objc2_foundation`: Cocoa bindings for event handling
- `block2`: For NSTimer block callbacks

## Risks and Open Questions

1. **NSEvent key code mapping**: Need to correctly map NSEvent keycodes to our Key enum. macOS uses virtual key codes (integers) that differ from character codes. Arrow keys, backspace, delete, etc. have specific codes we need to handle.

2. **First responder chain**: MetalView must become first responder to receive key events. If it doesn't accept first responder status, keys might go elsewhere. Will test this explicitly.

3. **CFRunLoopObserver complexity**: The before-waiting observer pattern is the correct architecture but adds complexity. If it proves difficult, fallback is simpler: render immediately after key handling. This sacrifices latency fairness for simplicity, but full viewport redraws are <1ms anyway.

4. **Modifier key handling**: Shift affects character case (handled by NSEvent.characters), but Cmd/Opt/Ctrl affect command resolution. Need to extract modifiers correctly.

5. **Text input vs key events**: For future international keyboard support, we may need to use NSTextInputClient protocol instead of raw keyDown. For this chunk, raw keyDown is sufficient for ASCII input.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->