<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is a **pure code-movement refactor** — no logic changes. The goal is to break up
`renderer.rs` (~3000 LOC, ~120KB) into focused modules organized by rendering phase,
improving maintainability and potentially instruction cache locality.

**Strategy**: Extract cohesive function groups into separate modules, leaving the
`Renderer` struct definition and top-level orchestration in `mod.rs`. Each module
will receive the functions that logically belong together, preserving all existing
signatures and behaviors.

**Module decomposition** (based on analysis of the existing code):

```
renderer/
├── mod.rs              // Renderer struct, new(), public API, render_with_editor() orchestration
├── constants.rs        // Color constants (BACKGROUND_COLOR, TEXT_COLOR, etc.)
├── content.rs          // render_text(), update_glyph_buffer*() - text line rendering
├── tab_bar.rs          // draw_tab_bar(), draw_pane_tab_bar() - tab bar UI rendering
├── left_rail.rs        // draw_left_rail() - workspace tiles rendering
├── overlay.rs          // draw_selector_overlay(), draw_confirm_dialog() - modal overlays
├── find_strip.rs       // draw_find_strip(), draw_find_strip_in_pane() - find-in-file UI
├── panes.rs            // render_pane(), draw_pane_frames() - multi-pane layout rendering
├── welcome.rs          // draw_welcome_screen(), draw_welcome_screen_in_pane()
└── scissor.rs          // Scissor rect helpers (selector_list_scissor_rect, etc.)
```

**Key principles**:
1. **No behavioral changes** — all existing public APIs remain identical
2. **Preserve chunk backreferences** — move comments with their associated code
3. **Internal visibility** — helper functions become `pub(super)` or `pub(crate)` as needed
4. **Shared state via &mut self** — functions that mutate renderer state stay as methods

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk directly IMPLEMENTS the
  renderer subsystem by reorganizing its core file structure. The subsystem documents
  the rendering pipeline's invariants (Atlas Availability, Single Frame Contract,
  Screen-Space Consistency, Layering Contract). This refactor preserves all invariants
  — we're only changing file organization, not rendering logic.

- The subsystem's `code_references` in its OVERVIEW.md frontmatter will need updating
  after this refactor to reference `renderer/mod.rs#Renderer` instead of `renderer.rs#Renderer`.

## Sequence

### Step 1: Create the renderer module directory structure

Create `crates/editor/src/renderer/` directory with placeholder `mod.rs`.

Location: `crates/editor/src/renderer/mod.rs`

### Step 2: Extract constants to constants.rs

Move all color constants and the `Uniforms` struct to a dedicated constants module:
- `BACKGROUND_COLOR`
- `TEXT_COLOR`
- `SELECTION_COLOR`
- `BORDER_COLOR`
- `PANE_DIVIDER_COLOR`
- `FOCUSED_PANE_BORDER_COLOR`
- `Uniforms` struct

These are used across multiple rendering functions.

Location: `crates/editor/src/renderer/constants.rs`

### Step 3: Extract scissor rect helpers to scissor.rs

Move the scissor rect utility functions:
- `selector_list_scissor_rect()`
- `full_viewport_scissor_rect()`
- `buffer_content_scissor_rect()`
- `pane_scissor_rect()`
- `pane_content_scissor_rect()`

Location: `crates/editor/src/renderer/scissor.rs`

### Step 4: Extract text content rendering to content.rs

Move functions related to buffer content rendering:
- `update_glyph_buffer()`
- `update_glyph_buffer_with_cursor_visible()`
- `render_text()`
- `set_content()` (if still used)
- `render()` (the basic render without editor context)
- `render_dirty()`
- `apply_mutation()`

These will need access to `&mut self` fields: `glyph_buffer`, `atlas`, `font`, `viewport`, `cursor_visible`.

Location: `crates/editor/src/renderer/content.rs`

### Step 5: Extract tab bar rendering to tab_bar.rs

Move tab bar drawing functions:
- `draw_tab_bar()`
- `draw_pane_tab_bar()`

Location: `crates/editor/src/renderer/tab_bar.rs`

### Step 6: Extract left rail rendering to left_rail.rs

Move left rail (workspace tiles) rendering:
- `draw_left_rail()`
- `left_rail_width()` (accessor method)

Location: `crates/editor/src/renderer/left_rail.rs`

### Step 7: Extract overlay rendering to overlay.rs

Move overlay/modal rendering functions:
- `draw_selector_overlay()`
- `draw_confirm_dialog()`
- `render_with_selector()` (if separate from main orchestration)
- `render_with_confirm_dialog()`

Location: `crates/editor/src/renderer/overlay.rs`

### Step 8: Extract find strip rendering to find_strip.rs

Move find-in-file UI rendering:
- `draw_find_strip()`
- `draw_find_strip_in_pane()`

Location: `crates/editor/src/renderer/find_strip.rs`

### Step 9: Extract pane rendering to panes.rs

Move multi-pane layout rendering:
- `render_pane()`
- `draw_pane_frames()`
- `configure_viewport_for_pane()`

Location: `crates/editor/src/renderer/panes.rs`

### Step 10: Extract welcome screen rendering to welcome.rs

Move welcome screen rendering:
- `draw_welcome_screen()`
- `draw_welcome_screen_in_pane()`

Location: `crates/editor/src/renderer/welcome.rs`

### Step 11: Consolidate mod.rs with Renderer struct and orchestration

The main `mod.rs` will contain:
- All module declarations (`mod constants; mod content; ...`)
- Re-exports as needed (`pub use constants::*;`)
- The `Renderer` struct definition
- `Renderer::new()`
- Public API methods: `viewport_mut()`, `viewport()`, `font_metrics()`, etc.
- Main entry point: `render_with_editor()` which orchestrates calls to sub-modules
- Any viewport-related methods that don't fit elsewhere

Location: `crates/editor/src/renderer/mod.rs`

### Step 12: Update imports in dependent files

Update files that use the renderer:
- `crates/editor/src/main.rs`: `mod renderer;` stays the same, `use crate::renderer::Renderer;` should still work
- `crates/editor/src/drain_loop.rs`: `use crate::renderer::Renderer;` should still work

Verify all public exports are accessible.

### Step 13: Verify compilation and run tests

- Run `cargo check` to verify all imports resolve correctly
- Run `cargo build` to ensure compilation succeeds
- Run `cargo test` to verify no regressions

### Step 14: Verify visual correctness

- Launch the editor and verify:
  - Text rendering works correctly
  - Tab bar renders properly
  - Left rail renders properly
  - Selector overlay works
  - Confirm dialog works
  - Find strip works
  - Multi-pane layout works
  - Welcome screen renders

### Step 15: Update subsystem documentation

Update `docs/subsystems/renderer/OVERVIEW.md` frontmatter to reference the new
module structure:
- `crates/editor/src/renderer/mod.rs#Renderer` (instead of `renderer.rs#Renderer`)

---

**BACKREFERENCE COMMENTS**

All existing chunk backreferences in the code will be preserved exactly as they
appear. No new backreferences will be added since this is a pure code movement
refactor. Each module file should retain the chunk comments that were associated
with the functions moved into it.

## Dependencies

No external dependencies. This is a pure refactor of existing code.

## Risks and Open Questions

1. **Method visibility boundaries**: Some private methods become pub(super) when
   extracted. This is safe since module boundaries control visibility, but we must
   ensure no unintended public API exposure.

2. **Shared mutable state**: Many methods mutate `&mut self`. The extraction pattern
   must either:
   - Keep methods as `impl Renderer` in sub-modules (preferred), or
   - Pass required state as parameters (more verbose but more explicit)

   Decision: Keep as `impl Renderer` methods in sub-modules to minimize API churn.

3. **Cyclic imports**: If modules reference each other (e.g., `content.rs` needs
   `scissor.rs`), we need to ensure the import graph is acyclic. Constants and
   scissor helpers should be leaf modules with no dependencies on other renderer
   sub-modules.

4. **glyph_buffer.rs mentioned in goal**: The goal suggests potentially decomposing
   `glyph_buffer.rs` (92KB) as well. This is marked as "if time permits" and is
   **out of scope for this chunk**. A separate chunk should be created for that work.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->