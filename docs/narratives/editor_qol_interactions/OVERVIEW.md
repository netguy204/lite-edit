---
status: ACTIVE
advances_trunk_goal: "Required Properties: Standard text editor interaction patterns"
proposed_chunks:
  - prompt: "Add mouse click to position cursor: convert mouse down events in MetalView to buffer (line, col) coordinates using font metrics and viewport scroll offset, then set the cursor position. Requires plumbing mouse events from MetalView through EditorController to EditorState/BufferFocusTarget."
    chunk_directory: mouse_click_cursor
    depends_on: []
  - prompt: "Add Home/End and Ctrl+A/Ctrl+E keybindings for line start/end cursor movement. Home and Ctrl+A move to beginning of line, End and Ctrl+E move to end of line. The Command enum and resolve_command already support these — verify they work end-to-end and that the NSEvent conversion in MetalView correctly translates Ctrl+key combinations (currently convert_key may swallow Ctrl+A/E as control characters)."
    chunk_directory: line_nav_keybindings
    depends_on: []
  - prompt: "Add text selection model to TextBuffer: track an optional selection anchor alongside the cursor. When anchor is set, the range between anchor and cursor is the selection. Add methods: set_selection_anchor, clear_selection, selected_range, selected_text. Mutation operations (insert_char, delete_backward, etc.) should delete the selection first when one exists."
    chunk_directory: text_selection_model
    depends_on: []
  - prompt: "Add mouse drag selection: on mouse down set the selection anchor at the click position, on mouse moved (drag) update the cursor to extend the selection, on mouse up finalize. Requires forwarding mouse moved/up events from MetalView and implementing handle_mouse in BufferFocusTarget."
    chunk_directory: mouse_drag_selection
    depends_on: [0, 2]
  - prompt: "Add selection rendering: highlight selected text with a background color in the renderer. When a selection exists, draw colored quads behind the selected character cells before drawing the glyphs."
    chunk_directory: text_selection_rendering
    depends_on: [2]
  - prompt: "Add Cmd+A to select entire buffer, Cmd+C to copy selection to macOS pasteboard, and Cmd+V to paste from macOS pasteboard at cursor (replacing selection if active). Requires interfacing with NSPasteboard for clipboard access."
    chunk_directory: clipboard_operations
    depends_on: [2]
  - prompt: "Add Cmd+K to delete from cursor to end of line (kill-line). Add a delete_to_line_end method on TextBuffer that removes characters from the cursor to the end of the current line and returns DirtyLines. Wire it through the Command enum and resolve_command in BufferFocusTarget."
    chunk_directory: kill_line
    depends_on: []
  - prompt: "Add Shift+Arrow key selection: holding Shift with arrow keys drops an anchor and extends selection. Shift+Left/Right/Up/Down, Shift+Home/End, Shift+Cmd+arrows all extend selection. Selection persists after Shift release until dismissed by plain movement, click, or mutation."
    chunk_directory: shift_arrow_selection
    depends_on: [2]
created_after:
  - editable_buffer
  - viewport_rendering
---

## Advances Trunk Goal

This narrative advances the editor toward standard text editor interaction patterns that users expect: mouse-based cursor positioning and text selection, common keyboard shortcuts for line navigation (Home/End, Ctrl+A/E), clipboard operations (Cmd+C/V), select-all (Cmd+A), and kill-line (Cmd+K). These are quality-of-life essentials without which the editor is keyboard-navigation-only with no selection or clipboard support.

## Driving Ambition

lite-edit currently supports basic typing, arrow key movement, and Emacs-style Ctrl+A/E and Home/End line navigation (already wired in `resolve_command`). However, there's no mouse interaction, no text selection, no clipboard support, and no kill-line. This narrative adds the interaction primitives that make the editor feel like a real editor:

- **Mouse click** positions the cursor at the clicked location
- **Mouse drag** selects text between the press and release points
- **Cmd+A** selects the entire buffer
- **Cmd+C** copies selected text to the system clipboard
- **Cmd+V** pastes from the system clipboard (replacing selection if active)
- **Cmd+K** deletes from cursor to end of line
- **Home/Ctrl+A** and **End/Ctrl+E** move to line start/end (verify existing wiring works end-to-end)

## Chunks

1. **Mouse click cursor positioning** — Convert mouse down events to buffer coordinates using font metrics and viewport offset, set cursor position. Plumb mouse events from MetalView through EditorController to BufferFocusTarget.

2. **Verify Home/End and Ctrl+A/E keybindings** — The Command enum already maps these. Verify the full pipeline works, especially that MetalView's `convert_key` doesn't swallow Ctrl+A/E as control characters before they reach `resolve_command`.

3. **Text selection model** — Add selection anchor to TextBuffer. Track optional anchor; range between anchor and cursor is the selection. Mutations delete selection first when present.

4. **Mouse drag selection** — On mouse down set anchor, on drag extend cursor/selection, on mouse up finalize. Depends on chunks 1 and 3.

5. **Selection rendering** — Draw highlighted background behind selected text in the renderer. Depends on chunk 3.

6. **Cmd+A, Cmd+C, Cmd+V (select-all, copy, paste)** — Cmd+A selects entire buffer. Cmd+C copies selection to NSPasteboard. Cmd+V pastes at cursor (replacing selection). Depends on chunk 3.

7. **Cmd+K kill-line** — Delete from cursor to end of line. Add `delete_to_line_end` to TextBuffer, wire through Command enum.

8. **Shift+Arrow key selection** — Holding Shift with arrow keys drops an anchor and extends the selection. Selection persists after Shift release until dismissed by plain movement, click, or mutation. Depends on chunk 3.

## Completion Criteria

When complete, a user can:
- Click anywhere in the buffer to place the cursor
- Use Home/Ctrl+A and End/Ctrl+E to jump to line boundaries
- Click and drag to select text, or Cmd+A to select all
- Copy selected text with Cmd+C and paste with Cmd+V
- Delete from cursor to end of line with Cmd+K
- All selection operations integrate with macOS system clipboard
- Selected text is visually highlighted in the editor
