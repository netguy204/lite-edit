<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The welcome screen will be implemented as a **virtual overlay** that renders in place of buffer content when a file tab contains an empty `TextBuffer`. This approach avoids modifying the actual buffer content and integrates cleanly with the existing rendering pipeline.

**Key design decisions:**

1. **Detection**: Check if the active tab is a file tab (`TabKind::File`) with an empty buffer (`buffer.is_empty()`). The welcome screen is never shown for terminal tabs.

2. **Rendering**: Create a new `WelcomeScreenBuffer` struct (similar to `SelectorGlyphBuffer`, `TabBarGlyphBuffer`) that generates glyph quads for the ASCII art logo, editor name, and hotkey table. This reuses the existing `GlyphBuffer`/`GlyphLayout` infrastructure.

3. **Centering**: Calculate the welcome content dimensions (width in chars, height in lines) and offset rendering to center it both horizontally and vertically within the content viewport (accounting for `RAIL_WIDTH` and `TAB_BAR_HEIGHT` offsets).

4. **Colors**: Use Catppuccin Mocha accent colors from `color_palette.rs`:
   - Logo: Gradient using lavender (#b4befe), mauve (#cba6f7), and blue (#89b4fa)
   - Editor name: Bright white (#cdd6f4)
   - Key combos: Blue (#89b4fa)
   - Descriptions: Dimmed text (Subtext1: #bac2de)

5. **Dismissal**: The welcome screen disappears when:
   - Any printable character is typed (handled naturally—character inserts into buffer, which becomes non-empty)
   - A file is opened into the tab (replaces buffer content)
   - The tab switches away and back (re-evaluated on each render)

6. **No state machine needed**: The welcome screen is purely a function of buffer state. Empty buffer → show welcome. Non-empty buffer → normal render.

## Subsystem Considerations

No subsystems are directly relevant to this chunk. The viewport_scroll subsystem deals with scroll offset management, but the welcome screen will be centered content that doesn't scroll.

## Sequence

### Step 1: Define welcome screen content constants

Create a new module `welcome_screen.rs` in `crates/editor/src/` containing:
- ASCII art feather logo as a `const` array of `&str` lines
- Per-line color specifications for the logo gradient
- Editor name and tagline strings
- Hotkey definitions as a `const` array of `(category, &[(&str, &str)])` tuples

Location: `crates/editor/src/welcome_screen.rs`

### Step 2: Create WelcomeScreenGlyphBuffer struct

Define a glyph buffer similar to `SelectorGlyphBuffer` that:
- Takes `GlyphLayout` for positioning
- Has `update()` method accepting viewport dimensions and line height
- Computes centered position based on content size vs viewport size
- Emits colored quads for logo, name, tagline, and hotkey table
- Tracks separate quad ranges for each color category

Location: `crates/editor/src/welcome_screen.rs`

### Step 3: Add is_welcome_screen_visible helper

Add a method to `Tab` or a helper function that checks:
- `tab.kind == TabKind::File`
- `tab.as_text_buffer().map_or(false, |b| b.is_empty())`

This encapsulates the welcome screen visibility logic.

Location: `crates/editor/src/workspace.rs`

### Step 4: Add welcome screen rendering to Renderer

Extend `Renderer` to:
1. Add `welcome_screen_buffer: Option<WelcomeScreenGlyphBuffer>` field
2. In `render_with_editor()`, after setting content offsets:
   - Check if active tab should show welcome screen
   - If yes, call `draw_welcome_screen()` instead of `update_glyph_buffer()` + `render_text()`
3. Implement `draw_welcome_screen()` method that:
   - Initializes/updates `welcome_screen_buffer`
   - Renders the welcome content using the glyph pipeline

Location: `crates/editor/src/renderer.rs`

### Step 5: Add accent color constants

Add Catppuccin Mocha accent colors to use for the welcome screen:
- Lavender: #b4befe → [0.706, 0.745, 0.996, 1.0]
- Mauve: #cba6f7 → [0.796, 0.651, 0.969, 1.0]
- Blue: #89b4fa → [0.537, 0.706, 0.980, 1.0]
- Subtext1: #bac2de → [0.729, 0.761, 0.871, 1.0]

These can be defined in `welcome_screen.rs` or referenced from `color_palette.rs`.

Location: `crates/editor/src/welcome_screen.rs`

### Step 6: Add module to lib.rs

Register the new module in the editor crate's `lib.rs`.

Location: `crates/editor/src/lib.rs`

### Step 7: Test welcome screen rendering

Verify manually (and with a smoke test if feasible):
- Launch editor with no file args → welcome screen appears
- Type any character → welcome screen disappears, character appears in buffer
- Cmd+T (new tab) → new tab shows welcome screen
- Open file → welcome screen replaced with file content
- Terminal tabs never show welcome screen

Location: Manual testing + optional `crates/editor/tests/welcome_test.rs`

---

**BACKREFERENCE COMMENTS**

Add at module level in `welcome_screen.rs`:
```rust
// Chunk: docs/chunks/welcome_screen - Vim-style welcome/intro screen on empty tabs
```

## Dependencies

This chunk depends on:
- **terminal_cmd_backspace** (ACTIVE): Terminal functionality must be complete
- **terminal_paste_render** (ACTIVE): Terminal paste rendering complete
- **terminal_viewport_init** (ACTIVE): Terminal viewport initialization complete

These are listed in `created_after` and represent prior shipped work, not implementation blockers.

No external library dependencies.

## Risks and Open Questions

1. **Logo design**: The ASCII art feather logo needs to be designed. Risk: may require iteration to look good at typical terminal font sizes. Mitigation: Start with a simple ~10-15 line design; can refine later.

2. **Hotkey accuracy**: The hotkey list should match actual current bindings. Need to audit `buffer_target.rs` and `editor_state.rs` to ensure accuracy.

3. **Viewport size edge cases**: Very small viewports (< 40 cols or < 20 lines) may not fit the welcome content. Decision: Simply render what fits; content will be clipped. No special handling needed.

4. **Performance**: Rendering the welcome screen should be negligible (~100 quads max). No concerns.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->