---
status: ONGOING
trigger: "Multiple failed attempts to fix terminal scrolling. Three distinct symptoms: (1) ls wrapping artifacts ('light bars') and inability to scroll to bottom, (2) clear positions cursor above viewport, (3) only cat works correctly. Pane splits exacerbate issues."
proposed_chunks:
  - prompt: |
      Fix viewport scroll position not resetting when terminal enters alternate screen.

      When a terminal switches from primary to alt-screen (e.g., vim, htop, less),
      the poll_terminals auto-follow code in workspace.rs has no handler for the
      `!was_alt_screen && now_alt_screen` transition. The viewport's scroll_offset_px
      from the primary screen carries over, pointing far past the alt-screen's
      line_count (which is just screen_lines, typically ~40). This produces an empty
      visible_range and nothing renders.

      Reproduction: open a terminal, cat a large file (causes scrollback/scrolling),
      then run vim. Vim's screen is invisible and cursor appears at window top.
      Any prior vertical scrolling triggers this — even if followed by `clear`.

      Fix: in the poll_terminals match block in workspace.rs (~line 1408), add a
      branch for `!was_alt_screen && now_alt_screen` that resets the viewport:
      `viewport.scroll_to_bottom(terminal.line_count())`. Since alt-screen line_count
      equals screen_lines and screen_lines <= visible_lines, this effectively sets
      scroll_offset_px to 0.
    chunk_directory:
    depends_on: []
created_after: ["terminal_shell_flakiness"]
---

<!--
DO NOT DELETE THIS COMMENT until the investigation reaches a terminal status.
This documents the frontmatter schema and guides investigation workflow.

STATUS VALUES:
- ONGOING: Investigation is active; exploration and analysis in progress
- SOLVED: The investigation question has been answered. If proposed_chunks exist,
  implementation work remains—SOLVED indicates the investigation is complete, not
  that all resulting work is done.
- NOTED: Findings documented but no action required; kept for future reference
- DEFERRED: Investigation paused; may be revisited later when conditions change

TRIGGER:
- Brief description of what prompted this investigation
- Examples:
  - "Test failures in CI after dependency upgrade"
  - "User reported slow response times on dashboard"
  - "Exploring whether GraphQL would simplify our API"
- The trigger naturally captures whether this is an issue (problem to solve)
  or a concept (opportunity to explore)

PROPOSED_CHUNKS:
- Starts empty; entries are added if investigation reveals actionable work
- Each entry records a chunk prompt for work that should be done
- Format: list of {prompt, chunk_directory, depends_on} where:
  - prompt: The proposed chunk prompt text
  - chunk_directory: Populated when/if the chunk is actually created via /chunk-create
  - depends_on: Optional array of integer indices expressing implementation dependencies.

    SEMANTICS (null vs empty distinction):
    | Value           | Meaning                                 | Oracle behavior |
    |-----------------|----------------------------------------|-----------------|
    | omitted/null    | "I don't know dependencies for this"  | Consult oracle  |
    | []              | "Explicitly has no dependencies"       | Bypass oracle   |
    | [0, 2]          | "Depends on prompts at indices 0 & 2"  | Bypass oracle   |

    - Indices are zero-based and reference other prompts in this same array
    - At chunk-create time, index references are translated to chunk directory names
    - Use `[]` when you've analyzed the chunks and determined they're independent
    - Omit the field when you don't have enough context to determine dependencies
- Unlike narrative chunks (which are planned upfront), these emerge from investigation findings
-->

## Trigger

Multiple prior attempts to fix terminal scrolling have been unsuccessful. Three distinct symptoms observed:

### Symptom 1: `cat` large file — works correctly
- Normal scroll ability, correct viewport positioning
- Meaningful soft wrapping, both natural and from pane narrowing
- This is the **control case** — proves basic rendering works

### Symptom 2: `ls` — broken wrapping and scroll
- Wrapping leaves "light bars" (colored fragments) on the left side of the pane
- Cannot scroll to the bottom of content
- Adding a pane changes the wrapping but doesn't fix the scroll-to-bottom issue

### Symptom 3: `clear` — cursor above viewport
- After running `clear`, cursor is above the top of the visible screen
- Cursor "peeks out" under the tab bar
- Happens regardless of whether the `ls` broken state was reached first

## Success Criteria

1. **Root-cause all three symptoms** — determine whether they share a common cause or are independent
2. **Verify with diagnostic data** — log the actual values of `terminal.screen_lines()` vs `viewport.visible_lines()` and `terminal.cols` vs `WrapLayout.cols_per_row` at runtime
3. **Fix confirmed** — all three symptoms eliminated: `ls` renders cleanly with correct scroll, `clear` positions cursor correctly, and `cat` continues to work

## Testable Hypotheses

### H1: Terminal rows (screen_lines) != viewport visible_lines due to f64/f32 precision mismatch

- **Rationale**: Terminal resize computes `rows = (pane_content_height as f64 / line_height_f64).floor()` but viewport computes `visible_rows = (pane_content_height_f32 / line_height_f32).floor()`. The f64→f32 conversion of line_height could cause different floor() results. If `screen_lines > visible_lines`, then `scroll_to_bottom()` overshoots, placing the cursor above the viewport — exactly matching symptom 3 (`clear`).
- **Test**: Add diagnostic logging to `sync_pane_viewports` that prints both values. If they ever differ, this is confirmed.
- **Status**: NOT CONFIRMED — runtime diagnostics showed rows matched (term=35, vp_visible=35) at the tested window size. May still apply at other sizes.

### H2: Terminal cols != renderer cols_per_row due to same f64/f32 mismatch

- **Rationale**: Same precision issue applies to columns. Terminal: `cols = (pane_width as f64 / advance_width_f64).floor()`. Renderer WrapLayout: `cols_per_row = (pane_width_f32 / (advance_width_f64 as f32)).floor()`. If terminal has more cols than the renderer, `ls` output formatted for N cols gets wrapped at N-1 cols by the renderer, producing garbled column layout and the "light bars" (first chars of wrapped continuations appearing at x=0).
- **Test**: Same diagnostic logging, comparing terminal.size().0 vs WrapLayout::new(pane_width, &metrics).cols_per_row(). The terminal_size_accuracy tests pass with specific font metrics but may miss real-world values.
- **Status**: NOT THE ROOT CAUSE — runtime diagnostics showed cols matched (term=113, wrap_cols=113) at the tested window size where `ls` still double-spaced. The actual cause was tab characters in terminal grid cells (see H6).

### H3: "Can't scroll to bottom" is caused by line_count inflation from screen_lines > visible_lines

- **Rationale**: `line_count = cold + history + screen_lines`. `scroll_to_bottom` computes `max_offset = (line_count - visible_lines) * line_height`. If `screen_lines > visible_lines`, the max scroll position shows the last `visible_lines` rows of content, but the cursor (at row 0-ish of the screen) is ABOVE the viewport. The user sees empty rows at the bottom and can't scroll down to where the prompt actually is.
- **Test**: If H1 is confirmed, this follows automatically. Also verify by checking if scroll_to_bottom positions the cursor outside the visible range.
- **Status**: NOT CONFIRMED — H1 was not confirmed at the tested window size. May still apply at other sizes.

### H4: The difference only manifests at certain window sizes / pane splits

- **Rationale**: The f64/f32 precision difference only causes a floor() discrepancy when the quotient is very close to an integer boundary. This is window-size dependent, which would explain why `cat` works (it doesn't depend on row/col accuracy for its output format) but `ls` (which formats based on terminal cols) breaks.
- **Test**: Try resizing the window by 1px in each direction and see if symptoms appear/disappear.
- **Status**: MOOT — H2 was disproved as the root cause of `ls` double-spacing. The precision mismatch may still exist at certain window sizes but was not the observed bug.

### H5 (NEW): Alt-screen viewport scroll position not reset on primary→alt transition

- **Rationale**: `workspace.rs` poll_terminals handles `alt→primary` and `primary auto-follow` but not `primary→alt`. When entering alt-screen after scrolling, scroll_offset_px stays high while line_count drops to screen_lines (~40). visible_range becomes empty.
- **Test**: Cat large file, run vim. Vim is invisible. Fresh terminal + vim works.
- **Status**: CONFIRMED — code analysis and manual reproduction

### H6: Tab characters in terminal grid cells inflate visual width in renderer

- **Rationale**: Alacritty stores `'\t'` in the first cell of a tab stop expansion (remaining cells are spaces). `row_to_styled_line()` passed `'\t'` through literally. The renderer's `char_visual_width('\t', col)` expands tabs using TAB_WIDTH=4, but terminal tab stops are 8 columns. With multiple tabs per `ls` row, the visual width exceeds `cols_per_row` (= terminal.cols), causing `screen_rows_for_line() = 2` → double-spacing. `ls` uses tabs to separate columns in multi-column mode, making it uniquely affected.
- **Test**: Dump terminal grid content after `ls` — confirmed tab characters present. Fix `'\t'` → `' '` in `row_to_styled_line()`.
- **Status**: CONFIRMED — ROOT CAUSE of `ls` double-spacing. Fix applied in `style_convert.rs:154`, verified by user.

## Exploration Log

### 2026-03-03: Diagnostic logging added (prior session)

Added `eprintln!` diagnostics in two places:
- `editor_state.rs:sync_pane_viewports` — logs terminal cols/rows vs viewport visible_lines vs WrapLayout cols_per_row, and f64 vs f32 row calculations
- `workspace.rs:poll_terminals` — logs auto-follow scroll state (was_bottom, was_alt, now_alt, line_count, scroll_px, cursor position) before and after scroll updates

### 2026-03-03: "Terminal-width + \n causes double spacing" theory — DEAD END

Hypothesis: lines exactly terminal-width followed by `\n` would cause double spacing due to pending-wrap + line-feed interaction (cursor advances twice). Tested with `printf` printing exactly `$COLUMNS` characters + newline. Did NOT reproduce double spacing. Theory invalidated — the kernel's `onlcr` PTY setting converts `\n` to `\r\n`, and the `\r` clears the pending wrap state.

### 2026-03-03: Observations narrowing the ls issue

Key observations from manual testing:
1. `ls` multi-column mode damages terminal state (double spacing, light bars)
2. `ls` single-column mode (narrow window) does NOT damage terminal state
3. Widening window after multi-column `ls` shows no visible soft re-wrapping — the `ls` content was NOT wrapped at the terminal grid level
4. `vim` does not show double spacing — only `ls` on primary screen
5. `cat` large file works correctly (control case)

Observations 1+2 indicate the damage correlates with line width relative to terminal width. Observation 3 proves the terminal grid rows are intact (no terminal-level wrapping). This pointed toward renderer-level wrapping.

### 2026-03-03: Alt-screen scroll bug discovered

Observation: vim is invisible after any prior vertical scrolling in the terminal. Opening vim in a fresh terminal works fine. Cat a large file then vim → nothing visible, cursor at top.

Code analysis of `workspace.rs:poll_terminals` (~line 1408) revealed: the auto-follow code handles `alt→primary` transition and `primary auto-follow when at bottom`, but has NO handler for `primary→alt` transition (`!was_alt_screen && now_alt_screen`). The viewport's `scroll_offset_px` from primary screen carries over. With large scrollback, `first_visible_line` far exceeds alt-screen's `line_count` (just `screen_lines`), producing an empty `visible_range`. Nothing renders.

### 2026-03-03: Root cause of ls wrapping identified via code tracing

Traced the full rendering pipeline:
1. `content.rs:52` — ALL buffer content (including terminal) goes through `update_from_buffer_with_wrap()`, which applies WrapLayout soft-wrapping
2. `row_to_styled_line()` in `style_convert.rs` includes ALL terminal cells including trailing spaces/nulls, producing StyledLines of visual width = `terminal.cols`
3. The wrapped renderer computes `rows_for_line = ceil(visual_width / cols_per_row)` for each line
4. If `cols_per_row < terminal.cols`, every terminal line occupies 2 screen rows

Terminal cols are computed with f64 arithmetic: `floor(pane_width as f64 / advance_width_f64)`. WrapLayout cols_per_row uses f32: `floor(pane_width_f32 / (advance_width as f32))`. The f64→f32 cast of advance_width can cause `cols_per_row` to be 1 less than `terminal.cols` at certain window sizes.

This analysis was on the right track (terminal lines going through soft-wrapping renderer) but identified the wrong cause of width inflation. See next entry for the actual root cause.

### 2026-03-03: Runtime diagnostics disprove f64/f32 mismatch at tested size

Runtime diagnostic output at the tested window size showed:
- `term=113x35, vp_visible=35, wrap_cols=113`
- Cols and rows matched exactly — no f64/f32 precision discrepancy at this size
- `ls` still double-spaced despite matching cols → H2 is NOT the root cause

### 2026-03-03: Tab characters in terminal grid — ACTUAL ROOT CAUSE

Added LINE dump diagnostic to print terminal grid content after `ls`. Output revealed tab characters (`'\t'`) stored in grid cells:
```
[LINE]  1 len=113 trimmed=79 "all_grab.log\t   \t       \t       \t       task-crm-sync-data"
```

Alacritty stores `'\t'` in the first cell of each tab stop expansion. `row_to_styled_line()` converted `' '` and `'\0'` to spaces but passed `'\t'` through literally. The renderer's `char_visual_width('\t', col)` expanded each tab using TAB_WIDTH=4, inflating visual width beyond `cols_per_row` (= terminal.cols = 113). With multiple tabs per `ls` row, visual width reached ~116+, causing `screen_rows_for_line() = 2` → double-spacing.

Fix: Added `|| ch == '\t'` to the space conversion condition in `style_convert.rs:row_to_styled_line()`. User confirmed fix eliminates `ls` double-spacing.

## Findings

### Verified Findings

- **Alt-screen scroll bug confirmed**: `workspace.rs` poll_terminals auto-follow code has no handler for primary→alt screen transition. The viewport scroll_offset_px carries over from primary screen, causing empty visible_range on alt-screen. (Evidence: code analysis of workspace.rs:1408-1414, confirmed by reproduction: cat large file then vim → invisible)

- **Terminal content goes through soft-wrapping renderer**: ALL buffer content, including terminal, renders via `update_from_buffer_with_wrap()` (content.rs:52), which applies WrapLayout wrapping using `cols_per_row`. Terminal lines always have visual width = `terminal.cols` due to trailing space/null cells in `row_to_styled_line()`. (Evidence: code tracing through content.rs → glyph_buffer.rs → style_convert.rs)

- **Terminal cols and WrapLayout cols_per_row use different precision**: Terminal: `floor(pane_width_f64 / advance_width_f64)`. WrapLayout: `floor(pane_width_f32 / (advance_width_f32))`. The f64→f32 cast of advance_width can produce different floor() results. However, runtime diagnostics showed these matched at the tested window size (both 113). The mismatch may still occur at other sizes. (Evidence: code at editor_state.rs:920 vs wrap_layout.rs:57-60; runtime diagnostics showed no mismatch)

- **Tab characters in terminal grid cause `ls` double-spacing (FIXED)**: Alacritty stores `'\t'` in the first cell of tab stop expansions. `row_to_styled_line()` passed these through literally. The renderer's `char_visual_width('\t', col)` expanded tabs using TAB_WIDTH=4, inflating visual width beyond `cols_per_row`, causing every full-width `ls` line to wrap to 2 screen rows. Fix: convert `'\t'` to `' '` in `row_to_styled_line()` at `style_convert.rs:154`. (Evidence: LINE dump showing `'\t'` in grid cells; fix confirmed by user)

### Hypotheses/Opinions

- The f64/f32 precision mismatch (H1-H4) was not the cause of the observed `ls` bug but may still cause rendering issues at window sizes where floor() results diverge. Worth monitoring but not a priority fix.

- The `clear` symptom (cursor above viewport) has not been investigated since the tab fix. It may have been a secondary effect of the same rendering pipeline issue, or may be independently caused by the rows mismatch (H1/H3). Needs retesting.

## Proposed Chunks

1. **Reset viewport on alt-screen entry**: In `workspace.rs` poll_terminals, add a branch for `!was_alt_screen && now_alt_screen` that calls `viewport.scroll_to_bottom(terminal.line_count())`. This resets scroll_offset_px to 0 since alt-screen line_count = screen_lines ≤ visible_lines.
   - Priority: High
   - Dependencies: None
   - Notes: One-line fix in the existing match block at workspace.rs:1408. See H5 and exploration log entry for full analysis.

## Resolution Rationale

<!--
GUIDANCE:

When marking this investigation as SOLVED, NOTED, or DEFERRED, explain why.
This captures the decision-making for future reference.

Questions to answer:
- What evidence supports this resolution?
- If SOLVED: What was the answer or solution?
- If NOTED: Why is no action warranted? What would change this assessment?
- If DEFERRED: What conditions would trigger revisiting? What's the cost of delay?

Example (SOLVED):
Root cause was identified (unbounded ImageCache) and fix is straightforward (LRU eviction).
Chunk created to implement the fix. Investigation complete.

Example (NOTED):
GraphQL migration would require significant investment (estimated 3-4 weeks) with
marginal benefits for our use case. Our REST API adequately serves current needs.
Would revisit if: (1) we add mobile clients needing flexible queries, or
(2) API versioning becomes unmanageable.

Example (DEFERRED):
Investigation blocked pending vendor response on their API rate limits. Cannot
determine feasibility of proposed integration without this information.
Expected response by 2024-02-01; will revisit then.
-->