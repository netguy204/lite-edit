---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
code_references:
- ref: crates/editor/src/editor_state.rs#EditorState::new_terminal_tab
  implements: "Pane-aware terminal dimension calculation and sync_pane_viewports() call after terminal creation"
- ref: crates/editor/src/editor_state.rs#test_terminal_initial_sizing_in_split_pane
  implements: "Test verifying terminal columns match pane width in horizontal split"
- ref: crates/editor/src/editor_state.rs#test_terminal_initial_sizing_in_vertical_split
  implements: "Test verifying terminal rows match pane height in vertical split"
- ref: crates/editor/src/editor_state.rs#test_terminal_initial_sizing_in_single_pane
  implements: "Regression test verifying single-pane layout still works correctly"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- terminal_flood_starvation
---

# Chunk Goal

## Minor Goal

Fix terminal tabs spawned in split panes receiving incorrect initial dimensions. When a new terminal is opened in a pane that is part of a multi-pane layout, the terminal's PTY is sized using the full window content area rather than the active pane's actual dimensions. This causes incorrect soft-wrapping and broken scroll-to-bottom behavior until a resize event (such as moving the tab to another pane and back) triggers `sync_pane_viewports()`.

This directly impacts the GOAL.md required property that terminal tabs should be "indistinguishable in navigation and visual treatment from file buffer tabs" — a terminal that renders with wrong wrapping and can't scroll to bottom is a broken experience.

### Root Cause

`EditorState::new_terminal_tab()` (`editor_state.rs:~3144`) computes terminal dimensions using `self.view_width - RAIL_WIDTH` and `self.view_height - TAB_BAR_HEIGHT` — the full window content area. In a split layout, the active pane is only a fraction of that area. The PTY is spawned via `TerminalBuffer::new(cols, rows, ...)` and `spawn_shell()` with these oversized dimensions, and `TIOCSWINSZ` is set accordingly.

After the tab is added, `sync_active_tab_viewport()` is called but is a no-op for terminal tabs. `sync_pane_viewports()` — which correctly iterates all panes and resizes terminals to match their actual pane dimensions — is **not** called after terminal creation.

Moving a tab between panes (`Cmd+Shift+Arrow`) does call `sync_pane_viewports()`, which corrects the sizing. This is why the workaround of moving the tab away and back resolves the issue.

## Success Criteria

- A new terminal tab opened in a split pane receives correct `cols` and `rows` matching its pane's actual dimensions, not the full window dimensions
- The PTY's `TIOCSWINSZ` reflects the actual pane size from the moment the shell starts
- Soft-wrapping in the terminal matches the visible pane width without needing to move the tab
- The terminal viewport can scroll to the bottom immediately after creation
- Existing behavior for single-pane layouts is unaffected (the full content area is already correct in that case)