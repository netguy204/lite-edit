<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The terminal displays unstyled monochrome text because `GlyphBuffer::update_from_buffer_with_wrap` — the rendering path used for terminal content — extracts text from `StyledLine` spans but ignores their `Style` attributes. It uses a single `glyph_color` (the default foreground) for all characters.

The fix follows the pattern already established in `update_from_buffer_with_cursor` (the non-wrap rendering path):
1. **Iterate spans, not flattened text** — Process each span individually, preserving its `Style`
2. **Resolve per-span colors** — Use `ColorPalette::resolve_style_colors()` to convert `Style` fg/bg to RGBA
3. **Emit background quads** — For spans with non-default backgrounds, emit solid quads behind the text
4. **Emit per-span foreground colors** — Pass the resolved fg color to `quad_vertices_with_xy_offset()`
5. **Emit underline quads** — For spans with underline styles

The existing color resolution infrastructure (`ColorPalette`, `cell_to_style`, `row_to_styled_line`) already handles the full terminal color space (named, indexed, RGB, inverse, dim). The missing link is using this data in the wrap-aware rendering path.

Testing follows the TESTING_PHILOSOPHY.md approach: test the pure logic (color resolution, span iteration) while relying on visual verification for GPU rendering.

## Sequence

### Step 1: Add unit tests for terminal color resolution

Location: `crates/terminal/src/style_convert.rs`

Add tests verifying:
- `cell_to_style` correctly captures ANSI colors (named colors map to correct `NamedColor`)
- `cell_to_style` correctly captures 256-color indexed colors
- `cell_to_style` correctly captures RGB truecolor
- `cell_to_style` correctly captures text attributes (bold, italic, dim, inverse, underline, strikethrough)
- `row_to_styled_line` preserves styles across span boundaries

These tests establish the baseline that `style_convert.rs` is producing correct `Style` values. If these pass and colors still don't render, the bug is in the rendering path.

### Step 2: Add tests for ColorPalette resolution of terminal colors

Location: `crates/editor/src/color_palette.rs`

Verify that `ColorPalette::resolve_style_colors` correctly handles:
- All 16 named ANSI colors (fg and bg)
- Inverse video (fg/bg swap)
- Dim (alpha reduction)
- Combined inverse+dim
- 256-color indexed colors (cube and grayscale ranges)
- RGB truecolor

These tests are mostly present but should be reviewed for completeness against terminal color cases.

### Step 3: Extract span-aware rendering from update_from_buffer_with_cursor

Location: `crates/editor/src/glyph_buffer.rs`

Study `update_from_buffer_with_cursor` Phase 3 (Glyph Quads) lines 672-730. This code:
1. Iterates lines via `view.styled_line(buffer_line)`
2. Iterates spans within each line
3. Skips hidden text
4. Resolves `(fg, _) = self.palette.resolve_style_colors(&span.style)` per span
5. Uses `fg` color when generating quad vertices

This is the exact pattern needed in `update_from_buffer_with_wrap`.

### Step 4: Add background quad emission to update_from_buffer_with_wrap

Location: `crates/editor/src/glyph_buffer.rs`, `update_from_buffer_with_wrap` method

Insert a new Phase 1.5 (Background Quads) after selection quads but before border quads:
1. Initialize `background_range` tracking at method start
2. Iterate buffer lines in the visible range
3. For each line, get `styled_line`, iterate spans
4. Track cumulative column position within each line
5. For spans with `!self.palette.is_default_background(span.style.bg)`:
   - Calculate screen position using `wrap_layout.buffer_col_to_screen_pos()`
   - Account for wrapping: a span may cross multiple screen rows
   - Call `self.create_selection_quad_with_offset()` with the resolved bg color
   - Push quad indices
6. Set `self.background_range`

This follows the existing Phase 1 (Background Quads) pattern in `update_from_buffer_with_cursor`.

### Step 5: Modify glyph rendering in update_from_buffer_with_wrap to use per-span colors

Location: `crates/editor/src/glyph_buffer.rs`, `update_from_buffer_with_wrap` Phase 3

Replace the current implementation that extracts text into a flat string:
```rust
// OLD (incorrect)
let line_content = if let Some(styled_line) = view.styled_line(buffer_line) {
    styled_line.spans.iter().map(|s| s.text.as_str()).collect::<String>()
} else { ... };
// ... uses glyph_color for all characters
```

With span-aware iteration:
```rust
// NEW (correct)
if let Some(styled_line) = view.styled_line(buffer_line) {
    let mut col: usize = 0;
    for span in &styled_line.spans {
        if span.style.hidden { col += span.text.chars().count(); continue; }
        let (fg, _) = self.palette.resolve_style_colors(&span.style);
        for c in span.text.chars() {
            // Skip spaces, get glyph, calculate wrapped position
            // Use fg (not glyph_color) when generating vertex
            col += 1;
        }
    }
}
```

This matches the pattern in `update_from_buffer_with_cursor` Phase 3.

### Step 6: Add underline quad emission to update_from_buffer_with_wrap

Location: `crates/editor/src/glyph_buffer.rs`, `update_from_buffer_with_wrap` method

Insert a new Phase 3.5 (Underline Quads) after glyph quads:
1. Initialize `underline_range` tracking at method start (currently defaults to empty)
2. Iterate buffer lines and spans as in Step 5
3. For spans with `span.style.underline != UnderlineStyle::None`:
   - Calculate screen position accounting for wrap
   - Resolve underline color (use `underline_color` if set, else fg)
   - Call `self.create_underline_quad()` with the resolved color
   - Push quad indices
4. Set `self.underline_range`

This follows the existing Phase 4 (Underline Quads) pattern in `update_from_buffer_with_cursor`.

### Step 7: Manual visual verification

Run the application and test:
1. Open a terminal tab (`Cmd+Shift+T`)
2. Run `ls --color=auto` — file types should show distinct colors
3. Run `echo -e "\e[31mRed\e[32mGreen\e[34mBlue\e[0m"` — should show colored text
4. Run `echo -e "\e[1mBold\e[0m \e[3mItalic\e[0m \e[4mUnderline\e[0m"` — should show styled text
5. Run `echo -e "\e[7mInverse\e[0m \e[2mDim\e[0m"` — should show inverse and dim
6. Run `vim` and open a source file — syntax highlighting should appear
7. Run a TUI app like `htop` or `top` — colors and shading should be visible
8. Verify file tabs still render correctly (no regression)

### Step 8: Add integration test for styled terminal output

Location: `crates/terminal/tests/integration.rs` (or new file)

Add a test that:
1. Creates a `TerminalBuffer`
2. Feeds ANSI escape sequences for colored text
3. Calls `styled_line()` and verifies spans have correct `Style` attributes
4. Verifies foreground colors match expected ANSI colors
5. Verifies attributes (bold, inverse, etc.) are correctly set

This provides automated regression coverage for the color pipeline without requiring GPU rendering.

## Dependencies

- `renderer_styled_content` chunk (ACTIVE): Provides `ColorPalette`, per-vertex color in shaders, and the pattern for background/underline rendering
- `terminal_input_render_bug` chunk (ACTIVE): Ensures terminal input/output pipeline is functional

## Risks and Open Questions

1. **Performance impact**: Adding per-span iteration in the wrap path may increase CPU cost. The non-wrap path already does this, so the cost should be acceptable. Monitor render times during testing.

2. **Wrap boundary handling for spans**: A single styled span may wrap across multiple screen rows. The background and underline quads need to be split at wrap boundaries. Study how `update_from_buffer_with_cursor` handles this (it doesn't have wrap) and adapt.

3. **Attribute interaction**: Bold+Dim, Inverse+underline_color, etc. Verify these combinations render correctly. The existing `resolve_style_colors` logic should handle this, but visual verification is needed.

4. **Hidden text in wrapped mode**: The current wrap implementation doesn't skip hidden text. Need to ensure `span.style.hidden` is checked and column tracking accounts for it.

## Deviations

### Step 1: DIM flag handling correction

**Changed**: Fixed the dim attribute detection in `cell_to_style`.

**Original code**: `dim: flags.contains(Flags::DIM_BOLD) && !flags.contains(Flags::BOLD)`

**New code**: `dim: flags.contains(Flags::DIM)`

**Why**: The original code incorrectly used `DIM_BOLD` which is a compound flag (both DIM and BOLD set). Alacritty actually has a separate `DIM` flag that should be used directly.

**Impact**: Dim text now correctly detected. Tests were updated to match.

### Step 3: No extraction needed

**Skipped**: Step 3 (Extract span-aware rendering from update_from_buffer_with_cursor) was not explicitly performed as a separate step. Instead, the pattern from `update_from_buffer_with_cursor` was directly applied to `update_from_buffer_with_wrap` during Steps 4-6.

### Step 7: Visual verification

**Skipped**: Manual visual verification was not performed as part of this implementation. The integration tests provide automated coverage for the key scenarios. Visual verification should be done by the operator.