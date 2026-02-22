<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Extend the fuzzy matching in `query_fuzzy()` to score against the **full relative path** instead of just the filename. The key insight is that matching against the path should be additive—if a query matches directory segments but not the filename, the file should still appear in results. However, filename matches must remain primary so that exact hits don't get drowned by incidental path matches.

Strategy:
1. **Two-pass scoring**: Score the query against both the filename and the full path. The final score combines these with a heavy bias toward filename matches (e.g., filename score * 2 + path score).
2. **Fallback to path-only**: If the query doesn't match the filename at all but does match the full path, use the path score (allowing directory-based filtering).
3. **Preserve existing heuristics**: The prefix bonus, consecutive-run bonus, and shorter-filename bonus continue to apply to filename matches, ensuring current behavior is retained.

This approach builds on the existing `score_match()` and `find_match_positions()` functions. A new helper `score_path_match()` will handle the path-aware scoring, and `query_fuzzy()` will call both.

Following the TESTING_PHILOSOPHY.md TDD approach, we'll write failing tests first for the new path-matching behavior, then implement.

## Sequence

### Step 1: Write failing tests for path-segment matching

Create test cases in the `#[cfg(test)]` module that verify the new behavior:

1. `test_query_directory_name_matches_files_within` — Typing `file_search` should match files under `docs/chunks/file_search_path_matching/`.
2. `test_query_partial_path_matches` — Typing `chunks/term` should match files under `docs/chunks/terminal_tab_spawn/`.
3. `test_filename_matches_still_rank_highest` — When a query matches both a filename prefix and a path segment, the filename match should score higher.
4. `test_path_only_match_returns_results` — A query that matches only directory segments (not the filename) still returns results.

Location: `crates/editor/src/file_index.rs` in the `mod tests` block.

### Step 2: Create `score_path_match()` helper

Add a new function that scores a query against a full relative path string (e.g., `docs/chunks/file_search_path_matching/GOAL.md`). This function:

- Converts the path to a string and lowercases it
- Uses `find_match_positions()` to find subsequence matches
- Applies a base score and the consecutive-run bonus (reusing logic from `score_match`)
- Does NOT apply the filename-prefix bonus or shorter-length bonus (those are filename-specific)

Signature:
```rust
fn score_path_match(query: &str, path: &Path) -> Option<u32>
```

Location: `crates/editor/src/file_index.rs`

### Step 3: Update `query_fuzzy()` to combine filename and path scores

Modify the `query_fuzzy()` method to:

1. For each path in the cache:
   - Compute `filename_score = score_match(query, filename)` (existing behavior)
   - Compute `path_score = score_path_match(query, path)`
2. Compute final score:
   - If `filename_score.is_some()`: use `filename_score * 2 + path_score.unwrap_or(0)`
   - Else if `path_score.is_some()`: use `path_score` as the sole score
   - Else: filter out the path (no match)
3. Return results sorted by descending score, then alphabetically by path.

This ensures:
- Filename matches dominate (2× weight)
- Path-only matches still appear (users can type directory names)
- Both bonuses stack when query matches both

Location: `crates/editor/src/file_index.rs#FileIndex::query_fuzzy`

### Step 4: Verify existing tests still pass

Run `cargo test` in `crates/editor` to confirm:
- All existing scoring tests pass (no regression in filename-first ranking)
- New path-matching tests pass

### Step 5: Add edge-case tests

Add tests for:
- Query with `/` characters (e.g., `src/main`) — should match paths containing that sequence
- Empty path components — shouldn't panic or behave unexpectedly
- Very long paths — no performance regression for typical query lengths

Location: `crates/editor/src/file_index.rs` in the `mod tests` block.

## Risks and Open Questions

- **Score overflow**: Combining `filename_score * 2 + path_score` could overflow `u32` in extreme cases. The current scoring is bounded (base 100 + bonuses capped by string length), so this is unlikely, but we should add a `.saturating_add()` to be safe.
- **Performance**: Scoring every path twice (filename + full path) doubles the work. For typical codebase sizes (10K–100K files), this should be negligible since it's just subsequence matching on in-memory strings, but worth watching.
- **Slash handling**: Users may type `/` expecting directory separators. On Windows the cache uses `\`. This chunk assumes macOS/Linux (per lite-edit's target), so forward slashes should work. If cross-platform support is needed later, normalize separators before matching.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->