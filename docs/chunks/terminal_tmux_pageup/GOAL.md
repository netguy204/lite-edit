---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/metal_view.rs
code_references:
  - ref: crates/editor/src/metal_view.rs#MetalView::__key_down
    implements: "Navigation key bypass routing (PageUp, PageDown, Home, End, Forward Delete)"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- app_nap_activity_assertions
- app_nap_blink_timer
- app_nap_file_watcher_pause
- highlight_text_source
- merge_conflict_render
- minibuffer_input
- terminal_single_pane_refresh
---

# Chunk Goal

## Minor Goal

Fix PageUp/PageDown so they work in tmux copy/scrollback mode running inside a lite-edit terminal pane. Currently, pressing PageUp in tmux scrollback mode does nothing.

### Investigation Findings

Code analysis shows the encoding is correct — `Key::PageUp` produces `ESC[5~` via `InputEncoder::encode_tilde_key(5)`, and the terminal tab path in `handle_key_buffer` (editor_state.rs:2186-2191) does call `terminal.write_input(&bytes)`. However, the key may never reach that code path.

**Suspected root cause: `keyDown:` routing gap.** In `metal_view.rs:310-314`, the `is_function_key` bypass check covers keyCodes `0x7A..=0x7F`, `0x60..=0x6F`, and `0x72`, but PageUp (0x74), PageDown (0x79), Home (0x73), and End (0x77) fall in the uncovered gap `0x70-0x79`. These keys go through `interpretKeyEvents:` → `doCommandBySelector:("pageUp:")` instead of the direct bypass path.

While `doCommandBySelector` does map `"pageUp:"` to `Key::PageUp`, this route through the macOS text input system may not fire the selector reliably in all keyboard/input method configurations. The bypass path (`convert_key_event`) would be more reliable for these navigation keys.

**Recommended fix:** Extend the `is_function_key` range or add explicit keyCode checks for PageUp (0x74), PageDown (0x79), Home (0x73), End (0x77), and Forward Delete (0x75) to route them through the direct bypass path, matching how Alacritty handles these keys.

### Key files

- `crates/editor/src/metal_view.rs` — `__key_down` bypass logic (line ~310)
- `crates/editor/src/metal_view.rs` — `__do_command_by_selector` (line ~550)
- `crates/terminal/src/input_encoder.rs` — `encode_tilde_key` (line ~176)
- `crates/editor/src/editor_state.rs` — `handle_key_buffer` terminal path (line ~2140)

## Success Criteria

- PageUp and PageDown work in tmux copy/scrollback mode (tmux enters copy mode, PageUp scrolls back).
- Home and End keys also work correctly in terminal panes (same routing gap).
- Verify with a PTY write trace or test that `ESC[5~` bytes actually reach the PTY when PageUp is pressed.
- Existing file-buffer PageUp/PageDown scrolling behavior is preserved.
- No regression for keys that currently work through `doCommandBySelector` (arrow keys, Return, Tab, Backspace, etc.).

