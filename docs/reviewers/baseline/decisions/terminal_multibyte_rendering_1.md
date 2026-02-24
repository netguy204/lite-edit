---
decision: FEEDBACK
summary: Non-BMP support implemented correctly, but width-aware rendering only applied to `update_from_buffer_with_cursor` - the `update_from_buffer_with_wrap` path (which terminal panes actually use) was not updated
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: Non-BMP characters (U+10000 and above) are rasterized and rendered instead of falling back to space glyphs

- **Status**: satisfied
- **Evidence**: `font.rs:210-256` implements UTF-16 surrogate pair calculation for characters > U+FFFF. Tests `test_non_bmp_characters_with_surrogate_pairs` and `test_non_bmp_egyptian_hieroglyphs` verify the lookup path works without panicking. The `glyph_atlas.rs` tests confirm ensure_glyph returns valid glyphs for non-BMP characters.

### Criterion 2: Wide characters render at the correct 2-cell width in the terminal pane

- **Status**: gap
- **Evidence**: Width-aware positioning was added to `update_from_buffer_with_cursor` (lines 420-460, 630-660, 715-772, 783-824) but NOT to `update_from_buffer_with_wrap`. Terminal panes are rendered via `renderer.rs:update_glyph_buffer` -> `update_from_buffer_with_wrap` (line 540). The wrap path still uses `span.text.chars().count()` (e.g., lines 1286, 1512, 1531, 1541, 1556, 1587, 1628) instead of width-aware counting.

### Criterion 3: Characters following a wide character are positioned correctly (no overlap, no gap)

- **Status**: gap
- **Evidence**: Same as Criterion 2 - the `update_from_buffer_with_wrap` function still uses `col += 1` (lines 1541, 1547, 1556, 1568, 1587) instead of advancing by character width.

### Criterion 4: Selection/highlight quads span the full display width of wide characters

- **Status**: gap
- **Evidence**: Background and underline phases in `update_from_buffer_with_wrap` use `span.text.chars().count()` (lines 1286, 1628) instead of the width-aware `span.text.chars().map(|c| c.width().unwrap_or(1)).sum()` that was applied to `update_from_buffer_with_cursor`.

### Criterion 5: The block cursor renders at the correct column position when wide characters are present

- **Status**: gap
- **Evidence**: Cursor positioning in wrapped mode depends on column tracking which is not width-aware in `update_from_buffer_with_wrap`.

### Criterion 6: The checkmark rendering defect from the screenshot is resolved

- **Status**: unclear
- **Evidence**: The checkmark character was not identified. PLAN.md Step 7 mentions this as an investigation task but no conclusion was documented. If it's a non-BMP character, Criterion 1 fixes it. If it's a width issue, Criteria 2-5 gaps apply.

### Criterion 7: Existing ASCII and narrow Unicode rendering is not regressed

- **Status**: satisfied
- **Evidence**: All 399 existing tests pass. The changes are additive - ASCII chars have width 1 so `c.width().unwrap_or(1)` returns 1, preserving existing behavior.

### Criterion 8: The fix applies to terminal pane rendering (editor buffer rendering is out of scope for this chunk)

- **Status**: gap
- **Evidence**: Terminal panes ARE rendered via `update_from_buffer_with_wrap` (renderer.rs line 540 calls this for ALL BufferView types including terminals). The changes were only applied to `update_from_buffer_with_cursor`, which is NOT used in the current architecture. The PLAN.md incorrectly identified `update_from_buffer_with_cursor` as the target function.

## Feedback Items

### Issue 1: Width-aware rendering not applied to the wrap path

- **ID**: issue-wrap-width
- **Location**: `crates/editor/src/glyph_buffer.rs:1161-1730` (`update_from_buffer_with_wrap`)
- **Concern**: The PLAN.md identified `update_from_buffer_with_cursor` as the target for width-aware changes, but all terminal pane rendering actually goes through `update_from_buffer_with_wrap`. The same width-aware patterns need to be applied there.
- **Suggestion**: Apply the same width-tracking changes to `update_from_buffer_with_wrap`:
  1. In Phase 1 (Background Quads, ~line 1286): Replace `span.text.chars().count()` with `span.text.chars().map(|c| c.width().unwrap_or(1)).sum()`
  2. In Phase 3 (Glyph Quads, ~lines 1512, 1531, 1541, 1547, 1556, 1568, 1587): Use `c.width().unwrap_or(1)` for column advancement
  3. In Phase 4 (Underline Quads, ~line 1628): Use width-aware counting
- **Severity**: functional
- **Confidence**: high

### Issue 2: Checkmark investigation not documented

- **ID**: issue-checkmark
- **Location**: `docs/chunks/terminal_multibyte_rendering/PLAN.md` Step 7
- **Concern**: The PLAN mentions identifying the checkmark character but no conclusion was documented in the Deviations section.
- **Suggestion**: Either identify and document the checkmark codepoint, or document that the investigation was inconclusive (acceptable if the underlying non-BMP/width fixes address it).
- **Severity**: style
- **Confidence**: medium
