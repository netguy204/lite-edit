---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/workspace.rs
code_references:
  - ref: crates/editor/src/workspace.rs#Workspace::poll_standalone_terminals
    implements: "Primary→alt screen transition viewport reset"
narrative: null
investigation: terminal_scroll_viewport
subsystems:
  - subsystem_id: viewport_scroll
    relationship: uses
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- terminal_size_accuracy
---

# Chunk Goal

## Minor Goal

Fix viewport scroll position not resetting when terminal enters alternate screen.

When a terminal switches from primary to alt-screen (e.g., vim, htop, less),
the `poll_terminals` auto-follow code in `workspace.rs` has no handler for the
`!was_alt_screen && now_alt_screen` transition. The viewport's `scroll_offset_px`
from the primary screen carries over, pointing far past the alt-screen's
`line_count` (which is just `screen_lines`, typically ~40). This produces an empty
`visible_range` and nothing renders.

Reproduction: open a terminal, cat a large file (causes scrollback/scrolling),
then run vim. Vim's screen is invisible and cursor appears at window top.
Any prior vertical scrolling triggers this — even if followed by `clear`.

## Success Criteria

- After scrolling in a terminal (e.g., `cat` a large file), opening vim renders its full welcome screen correctly
- Opening htop, less, or any alt-screen program after scrolling renders correctly
- Existing alt→primary transition (exiting vim) continues to work — viewport snaps to bottom of primary screen
- Primary screen auto-follow (new output while at bottom) continues to work
- Fresh terminals (no prior scrolling) continue to work unchanged