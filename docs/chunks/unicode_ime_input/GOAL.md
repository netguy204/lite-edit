---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/input/src/lib.rs
  - crates/editor/src/editor_event.rs
  - crates/editor/src/event_channel.rs
  - crates/editor/src/metal_view.rs
  - crates/editor/src/drain_loop.rs
  - crates/editor/src/buffer_target.rs
  - crates/buffer/src/text_buffer.rs
code_references:
  - ref: crates/input/src/lib.rs#TextInputEvent
    implements: "Text insertion event type for keyboard, IME, paste, dictation"
  - ref: crates/input/src/lib.rs#MarkedTextEvent
    implements: "IME marked text (composition) event type with selected range"
  - ref: crates/editor/src/editor_event.rs#EditorEvent::InsertText
    implements: "EditorEvent variant for text insertion from IME"
  - ref: crates/editor/src/editor_event.rs#EditorEvent::SetMarkedText
    implements: "EditorEvent variant for IME composition in progress"
  - ref: crates/editor/src/editor_event.rs#EditorEvent::UnmarkText
    implements: "EditorEvent variant for IME composition cancel"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_insert_text
    implements: "Send text insertion events through event channel"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_set_marked_text
    implements: "Send marked text events through event channel"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_unmark_text
    implements: "Send unmark text events through event channel"
  - ref: crates/editor/src/metal_view.rs#MetalView::__key_down
    implements: "Route keys through text input system vs bypass for shortcuts"
  - ref: crates/editor/src/metal_view.rs#MetalView::__insert_text
    implements: "NSTextInputClient insertText:replacementRange: method"
  - ref: crates/editor/src/metal_view.rs#MetalView::__set_marked_text
    implements: "NSTextInputClient setMarkedText:selectedRange:replacementRange: method"
  - ref: crates/editor/src/metal_view.rs#MetalView::__unmark_text
    implements: "NSTextInputClient unmarkText method"
  - ref: crates/editor/src/metal_view.rs#MetalView::__do_command_by_selector
    implements: "Handle action commands from text input system (Enter, Tab, etc.)"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::handle_insert_text
    implements: "Process InsertText events in drain loop"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::handle_set_marked_text
    implements: "Process SetMarkedText events in drain loop"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::handle_unmark_text
    implements: "Process UnmarkText events in drain loop"
  - ref: crates/buffer/src/text_buffer.rs#MarkedTextState
    implements: "Marked text state storage for IME composition"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::set_marked_text
    implements: "Set/update marked text with underline rendering"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::commit_marked_text
    implements: "Commit marked text as permanent buffer content"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::cancel_marked_text
    implements: "Cancel marked text without inserting"
  - ref: crates/buffer/src/lib.rs
    implements: "Export MarkedTextState for IME support"
narrative: null
investigation: null
subsystems:
  - subsystem_id: renderer
    relationship: uses
friction_entries: []
bug_type: null
depends_on: []
created_after:
- typescript_highlight_layering
---

# Chunk Goal

## Minor Goal

The MetalView NSView subclass currently handles raw key events (`keyDown:`) but does not implement the `NSTextInputClient` protocol. This means Input Method Editors (IME) for Chinese, Japanese, Korean, and other languages cannot function — these users literally cannot type in the editor.

Implement `NSTextInputClient` on the MetalView to support:
1. **Composed text input** (`insertText:replacementRange:`) — final text insertion from IME
2. **Marked text** (`setMarkedText:selectedRange:replacementRange:`) — in-progress IME composition with underline rendering
3. **Unmarking** (`unmarkText`) — commit or cancel composition

This also requires splitting the input event model: physical key events (for shortcuts, navigation) must be separate from text input events (from keyboard, IME, paste, dictation). The current `KeyEvent` conflates both.

**Key files**: `crates/editor/src/metal_view.rs` (NSTextInputClient impl), `crates/input/src/lib.rs` (new InputEvent variants), `crates/editor/src/drain_loop.rs` (handle new events), `crates/editor/src/buffer_target.rs` (process text insertion)

**Origin**: Architecture review recommendation #2 (P0 — Correctness). See `ARCHITECTURE_REVIEW.md`.

## Success Criteria

- `NSTextInputClient` protocol is implemented on MetalView
- Japanese IME (Hiragana → Kanji conversion) works end-to-end: type romaji, see marked text with underline, press Enter to commit kanji
- Chinese Pinyin IME works: type pinyin, select character from candidates, text is inserted
- Marked text renders with a distinct underline style in the buffer view
- Canceling IME composition (Escape) removes marked text without inserting
- Regular ASCII typing continues to work with no latency regression
- Dead key composition (e.g., Option+e then e → é on US keyboard) works
- Input events are split into `Key` (physical) and `InsertText`/`SetMarkedText`/`UnmarkText` variants

## Rejected Ideas

<!-- DELETE THIS SECTION when the goal is confirmed if there were no rejected
ideas.

This is where the back-and-forth between the agent and the operator is recorded
so that future agents understand why we didn't do something.

If there were rejected ideas in the development of this GOAL with the operator,
list them here with the reason they were rejected.

Example:

### Store the queue in redis

We could store the queue in redis instead of a file. This would allow us to scale the queue to multiple nodes.

Rejected because: The queue has no meaning outside the current session.

---

-->