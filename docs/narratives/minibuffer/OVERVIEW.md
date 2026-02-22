---
status: DRAFTING
advances_trunk_goal: null
proposed_chunks:
  - prompt: >
      Extract a `MiniBuffer` struct — a self-contained single-line editing model
      that wraps `TextBuffer` and delegates all key events through `BufferFocusTarget`.
      It must support the full editing affordance set (character insert, backspace,
      forward delete, Alt+Backspace word kill, Ctrl+K kill-line, Ctrl+A/E line
      navigation, Option+Left/Right word jump, Shift+Arrow selection, Cmd+A
      select-all, Cmd+C copy, Cmd+V paste). Because it is single-line only,
      Return and Down/Up arrow produce no effect (or are explicitly suppressed).
      MiniBuffer exposes: `content()`, `cursor_col() -> usize`, and
      `selection_range() -> Option<(usize, usize)>`. It lives in a new file
      `crates/editor/src/mini_buffer.rs`.
    chunk_directory: mini_buffer_model
    depends_on: []

  - prompt: >
      Port the file picker to use `MiniBuffer` for its query-input field.
      Replace the raw String mutation in `SelectorWidget` (the `query` field,
      `Backspace` branch, and printable-char branch of `handle_key`) with a
      `MiniBuffer` instance. `SelectorWidget::query()` now delegates to
      `mini_buffer.content()`. All existing `SelectorWidget` tests must still
      pass. The selector overlay rendering already reads `widget.query()` and
      will pick up the change automatically; no rendering changes are needed.
    chunk_directory: file_picker_mini_buffer
    depends_on: [0]

  - prompt: >
      Add a find-in-file (Cmd+F) feature. Pressing Cmd+F opens a single-line
      minibuffer strip at the bottom of the viewport. As the user types a search
      query, the editor performs a forward case-insensitive search from the
      current cursor position, selects the first match in the main buffer, and
      scrolls it into view — but the minibuffer retains keyboard focus. Pressing
      Enter advances to the next match (wrapping around at buffer end). Pressing
      Escape dismisses the minibuffer and returns focus to the main buffer,
      leaving the cursor at the current match position. Pressing Cmd+F while
      already open is a no-op (does not close). The minibuffer uses the
      `MiniBuffer` model for its input, so all standard editing affordances
      (word-jump, kill, selection, etc.) work inside it. The minibuffer strip
      is rendered at the bottom of the screen as a 1-line-tall tinted row with
      a search icon label ("find:") followed by the MiniBuffer content and a
      blinking cursor. Match selection in the main buffer uses the existing
      `TextBuffer` selection machinery.
    chunk_directory: find_in_file
    depends_on: [0]

created_after: ["file_buffer_association", "word_forward_delete"]
---

## Advances Trunk Goal

This narrative advances the editor's core editing experience by introducing a
reusable, fully-featured single-line editing primitive (the minibuffer) and two
concrete use-cases that demonstrate its value: the file-picker query field and a
live find-in-file tool.

## Driving Ambition

Right now the file picker's query field is a stripped-down input — it supports
only character append and backspace. Every other text-editing affordance that the
main buffer offers (word jumping, selection, kill-line, clipboard) is missing
from that narrow input. This is jarring: the muscle memory that works in the
editor body doesn't work inside the picker.

The deeper problem is that we have no reusable concept of a "single-line text
input with full editor affordances". Without that primitive, every future
narrowfield input (find-in-file, command palette, rename-file prompt, etc.) will
be built ad hoc and will again lack the full affordance set.

This narrative introduces the **minibuffer**: a single-line editing model that
composes the existing `TextBuffer` + `BufferFocusTarget` machinery and constrains
it to one line. The file picker query field is ported to use it, and a new
Cmd+F find-in-file surface demonstrates the concept with interactive, live-as-you-type
search.

Success looks like:

- Typing inside the file picker query box supports Option+Left, Ctrl+K, Shift+Right,
  Cmd+A, Cmd+V and every other affordance the main buffer has.
- Pressing Cmd+F opens a bottom strip; as the user types, the nearest match in
  the file is highlighted and scrolled into view while the minibuffer retains
  focus; Enter advances to the next match; Escape returns focus to the main buffer.
- No editing logic is duplicated: the minibuffer reuses the existing
  `TextBuffer` and `BufferFocusTarget` rather than reimplementing character
  manipulation.

## Chunks

### Chunk 1 — `MiniBuffer` model

Extract a `MiniBuffer` struct in `crates/editor/src/mini_buffer.rs`. It owns a
`TextBuffer` (single-line invariant enforced by filtering Return events) and
delegates all key events through `BufferFocusTarget`. It exposes:

```
content() -> &str
cursor_col() -> usize
selection_range() -> Option<(usize, usize)>
handle_key(event: &KeyEvent)
```

The single-line invariant: Return, Up, Down, and Cmd+Up/Down are treated as
no-ops inside `MiniBuffer`. All other affordances pass through unmodified to
the existing focus-target logic.

### Chunk 2 — Port file picker query to `MiniBuffer`

Replace the raw `query: String` field inside `SelectorWidget` with a
`MiniBuffer`. The public `query()` accessor delegates to `mini_buffer.content()`.
All existing selector tests must pass without modification because the observable
interface (`query()` returns a `&str`, `handle_key` accepts a `&KeyEvent`) is
unchanged.

The payoff: the file picker's query input immediately gains word-jump,
kill-line, selection, and clipboard support — for free, from the reuse.

### Chunk 3 — Find-in-file (Cmd+F)

Add a new `EditorFocus::FindInFile` variant. Cmd+F transitions from Buffer focus
to FindInFile focus, opening a one-line rendering strip at the bottom of the
viewport. The strip is labeled `find:` and shows the minibuffer content with a
blinking cursor.

Live search: whenever the minibuffer content changes (any key that modifies the
query), perform a forward case-insensitive search from the cursor position before
the overlay opened, find the first match, and set the main buffer's selection to
that match range (+ scroll into view). The minibuffer still owns keyboard focus.

Enter: advance to the next match starting just after the end of the current
match (wrap around if needed).

Escape: dismiss the strip, clear `EditorFocus::FindInFile`, set focus back to
`Buffer`. The main buffer's cursor and selection remain at the last-found match
position.

Cmd+F while already open: no-op.

## Completion Criteria

When complete:

1. Every text-editing affordance available in the main buffer is also available
   inside the file picker query field.
2. Pressing Cmd+F opens a find strip at the bottom of the editor; typing a
   search term live-selects the nearest forward match in the main buffer.
3. Pressing Enter in the find strip advances to the next match (with wrap).
4. Pressing Escape dismisses the find strip and returns focus to the main buffer,
   leaving the cursor at the last match.
5. The `MiniBuffer` struct contains no bespoke character-manipulation logic — it
   delegates entirely to the existing `TextBuffer` and `BufferFocusTarget`.
