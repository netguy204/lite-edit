---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/file_index.rs
code_references:
  - ref: crates/editor/src/file_index.rs#FileIndex::query_fuzzy
    implements: "Two-pass scoring combining filename and path scores with filename 2× weight"
  - ref: crates/editor/src/file_index.rs#score_path_match
    implements: "Path-aware fuzzy matching against full relative path with consecutive-run bonus"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- tab_bar_content_clip
- click_scroll_fraction_alignment
---

# Chunk Goal

## Minor Goal

The file search picker currently only matches the query against the **filename** component of each path (via `path.file_name()`). This means typing a directory name like `file_search` returns no results even though files exist under that path.

This chunk extends `query_fuzzy` in `crates/editor/src/file_index.rs` so that the fuzzy match runs against the **full relative path** (e.g. `docs/chunks/file_search_path_matching/GOAL.md`), not just the filename. This lets users narrow results by typing directory segments—particularly useful for finding chunk goal files by typing the chunk name.

The scoring heuristics should still favor filename matches (shorter paths, prefix bonuses on the filename portion) so that exact filename hits aren't drowned out by incidental path segment matches.

## Success Criteria

- Typing a directory name (e.g. `file_search_path`) in the file picker returns files within that directory
- Typing a partial path like `chunks/terminal` matches files under `docs/chunks/terminal_tab_spawn/`
- Pure filename queries still rank filename-prefix matches highest (no regression in current behavior)
- `score_match` accepts or is complemented by a path-aware variant that scores against the full relative path string
- Existing tests in `file_index.rs` continue to pass; new tests cover path-segment matching scenarios

