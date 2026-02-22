---
status: ACTIVE
ticket: null
parent_chunk: content_tab_bar
code_paths:
- crates/editor/src/tab_bar.rs
- crates/editor/src/workspace.rs
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/tab_bar.rs#TabInfo::from_tab
    implements: "Derives tab labels from associated_file for file tabs, falls back to 'Untitled' when no file"
  - ref: crates/editor/src/tab_bar.rs#TabBarGlyphBuffer::update
    implements: "Left-truncation of labels to preserve file extension (Phase 6: Tab Labels)"
  - ref: crates/editor/src/tab_bar.rs#disambiguate_labels
    implements: "Adds parent directory to labels when multiple tabs share the same filename"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_tab_bar_click
    implements: "Handles click-to-switch and close button clicks in the tab bar"
  - ref: crates/editor/src/editor_state.rs#EditorState::switch_tab
    implements: "Switches active buffer to the specified tab index"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- content_tab_bar
- tab_bar_layout_fixes
created_after:
- agent_lifecycle
- content_tab_bar
- terminal_input_encoding
- find_in_file
- cursor_wrap_scroll_alignment
- row_scroller_extract
- selector_row_scroller
---

# Chunk Goal

## Minor Goal

Three related improvements to the content tab bar introduced by `content_tab_bar`:

**1. Clickable tab titles**

Clicking a tab should switch the active buffer to that tab. The `handle_tab_bar_click` plumbing exists in `editor_state.rs` but has not been verified to work end-to-end (it depends on the coordinate fix in `tab_bar_layout_fixes`). This chunk ensures clicking a tab body reliably fires `switch_tab()` for the correct index.

**2. Labels derived from the buffer's observed file path**

Currently `Tab.label` is a snapshot string set at creation time and never updated. When a tab is backed by a file (`TabKind::File`, `associated_file: Some(path)`), the display label must always reflect the actual filename component of `associated_file`. The label should be recomputed from `Tab.associated_file` at render time rather than cached as a static string. For non-file tabs (terminals, agent output), the label continues to come from the static `Tab.label` field.

**3. Left-truncation when the label doesn't fit**

When a tab label must be shortened to fit within `TAB_MAX_WIDTH`, truncate from the left and prepend an ellipsis, not from the right. This preserves the file extension and the end of the filename — the parts that distinguish tabs — rather than truncating them away.

Example: `"…/src/very_long_module_name.rs"` → keep the tail, clip the head.

For the disambiguation case (two tabs with the same base filename), the disambiguated form (e.g., `"src/main.rs"`) is the label being left-truncated.

## Success Criteria

- Clicking a tab switches the content area to that tab's buffer
- Clicking the active tab is a no-op (no crash, no flicker)
- File tabs always display the `file_name()` component of their `associated_file` path, not a stale snapshot
- When `associated_file` is `None` (unsaved/untitled), the label falls back to `"Untitled"`
- When two file tabs share the same base filename, disambiguation includes the parent directory (existing behavior retained)
- When a label exceeds the available width, it is left-truncated with a leading `…` so the filename end is always visible
- Right-truncation is fully removed from the label rendering path
- All existing tab bar unit tests in `tab_bar.rs` continue to pass; truncation tests are updated to reflect left-truncation semantics

## Relationship to Parent

`content_tab_bar` introduced `Tab.label` as a static string and right-truncation in `TabBarGlyphBuffer`. This chunk:

- Changes label derivation for file tabs from static snapshot → derived from `associated_file` at render time
- Changes truncation direction from right → left
- Verifies and exercises the click-to-switch path end-to-end (after `tab_bar_layout_fixes` corrects coordinates)

The tab bar rendering structure, GPU buffer layout, geometry calculation, and keyboard shortcuts from `content_tab_bar` are all retained unchanged.