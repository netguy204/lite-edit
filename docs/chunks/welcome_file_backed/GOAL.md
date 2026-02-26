---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/workspace.rs
code_references:
  - ref: crates/editor/src/workspace.rs#Editor::should_show_welcome_screen
    implements: "Welcome screen visibility excludes file-backed empty tabs"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- fallback_glyph_metrics
---

# Chunk Goal

## Minor Goal

When a buffer is backed by a file on disk (i.e., `Tab::associated_file` is `Some`), the welcome screen should not be displayed even if the buffer contents are empty. The welcome screen (logo, tagline, hotkey reference) is intended as an orientation aid for fresh, unassociated tabs — not as a replacement for viewing an empty file's actual (empty) contents.

Currently, `Editor::should_show_welcome_screen()` in `workspace.rs` returns `true` whenever the active tab is `TabKind::File` with an empty `TextBuffer`, regardless of whether the tab has an `associated_file`. This means opening or creating a zero-byte file incorrectly shows the welcome screen instead of an empty editing surface.

## Success Criteria

- `Editor::should_show_welcome_screen()` returns `false` when the active tab has `associated_file: Some(_)`, even if the buffer is empty
- Opening an existing empty file shows a blank editing surface, not the welcome screen
- New tabs created via Cmd+T (no associated file) continue to show the welcome screen as before
- Existing welcome screen tests updated or extended to cover the file-backed case