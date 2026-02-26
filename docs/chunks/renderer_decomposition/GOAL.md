---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/renderer/mod.rs
- crates/editor/src/renderer/constants.rs
- crates/editor/src/renderer/scissor.rs
- crates/editor/src/renderer/content.rs
- crates/editor/src/renderer/tab_bar.rs
- crates/editor/src/renderer/left_rail.rs
- crates/editor/src/renderer/overlay.rs
- crates/editor/src/renderer/find_strip.rs
- crates/editor/src/renderer/panes.rs
- crates/editor/src/renderer/welcome.rs
- docs/subsystems/renderer/OVERVIEW.md
code_references:
  - ref: crates/editor/src/renderer/mod.rs
    implements: "Top-level module orchestration, Renderer struct, public API, render_with_editor entry point"
  - ref: crates/editor/src/renderer/mod.rs#Renderer
    implements: "Metal renderer struct with command queue, font, atlas, glyph buffers, viewport"
  - ref: crates/editor/src/renderer/mod.rs#Renderer::render_with_editor
    implements: "Primary render entry point orchestrating all rendering phases"
  - ref: crates/editor/src/renderer/mod.rs#Renderer::render_with_confirm_dialog
    implements: "Render entry point with confirm dialog overlay"
  - ref: crates/editor/src/renderer/constants.rs
    implements: "Color constants (BACKGROUND_COLOR, TEXT_COLOR, SELECTION_COLOR, BORDER_COLOR, etc.) and Uniforms struct"
  - ref: crates/editor/src/renderer/scissor.rs
    implements: "Scissor rect helpers for clipping (full_viewport, buffer_content, pane, selector_list)"
  - ref: crates/editor/src/renderer/content.rs
    implements: "Text content rendering: update_glyph_buffer, render_text, set_content"
  - ref: crates/editor/src/renderer/content.rs#Renderer::render_text
    implements: "Multi-pass text rendering (background, selection, borders, glyphs, underlines, cursor)"
  - ref: crates/editor/src/renderer/tab_bar.rs
    implements: "Tab bar rendering: draw_tab_bar (global), draw_pane_tab_bar (per-pane)"
  - ref: crates/editor/src/renderer/left_rail.rs
    implements: "Left rail (workspace tiles) rendering: draw_left_rail"
  - ref: crates/editor/src/renderer/overlay.rs
    implements: "Overlay rendering: draw_selector_overlay, draw_confirm_dialog"
  - ref: crates/editor/src/renderer/find_strip.rs
    implements: "Find strip rendering: draw_find_strip (full viewport), draw_find_strip_in_pane (multi-pane)"
  - ref: crates/editor/src/renderer/panes.rs
    implements: "Multi-pane layout rendering: render_pane, draw_pane_frames, configure_viewport_for_pane"
  - ref: crates/editor/src/renderer/welcome.rs
    implements: "Welcome screen rendering: draw_welcome_screen, draw_welcome_screen_in_pane"
narrative: null
investigation: null
subsystems:
- subsystem_id: renderer
  relationship: implements
friction_entries: []
bug_type: null
depends_on: []
created_after:
- typescript_highlight_layering
---

# Chunk Goal

## Minor Goal

`renderer.rs` has grown to 116K LOC — a single file containing layout calculation, tab bar rendering, content rendering, cursor/selection rendering, overlay rendering, and Metal command encoding. This is both a maintainability problem and a performance problem: the instruction cache on Apple Silicon is typically 128-192KB, and a monolithic render function with all its callees likely exceeds L1 I-cache capacity, causing I-cache misses on the hot path.

Decompose `renderer.rs` into focused modules, each responsible for a single rendering phase:

```
renderer/
├── mod.rs              // Top-level render() orchestration (~500 LOC)
├── layout.rs           // Pane rect computation from BSP tree
├── content.rs          // Text line rendering (styled lines → quads)
├── tab_bar_render.rs   // Tab bar UI rendering
├── overlay.rs          // Find bar, selector, dialog rendering
├── cursor.rs           // Cursor and selection highlight rendering
└── metal_pass.rs       // Metal command buffer setup, encoding, present
```

This is a mechanical refactor — no logic changes, just moving code to focused modules.

**Key files**: `crates/editor/src/renderer.rs` (116K LOC to decompose), possibly `crates/editor/src/glyph_buffer.rs` (92K LOC, same pattern)

**Origin**: Architecture review recommendation #7 (P1 — Maintainability/Performance). See `ARCHITECTURE_REVIEW.md`.

## Success Criteria

- `renderer.rs` replaced by `renderer/` module directory
- Top-level `mod.rs` is <1000 LOC, orchestrating calls to sub-modules
- Each sub-module is a focused rendering phase (<15K LOC each)
- No logic changes — pure code movement refactor
- All existing rendering tests pass without modification
- Visual output byte-identical before and after (verified by screenshot comparison or manual QA)
- Compilation succeeds with no new warnings
- Consider applying same pattern to `glyph_buffer.rs` (92K LOC) if time permits

## Rejected Ideas

<!-- DELETE THIS SECTION when the goal is confirmed if there were no rejected
ideas.

This is where the back-and-forth between the agent and the operator is recorded
so that future agents understand why we didn't do something.

If there were rejected ideas in the development of this GOAL with the operator,
list them here with the reason they were rejected.

Example:

### Store the queue in redis

We could store the queue in redis instead of a file. This would allow us to scale the queue to multiple nodes.

Rejected because: The queue has no meaning outside the current session.

---

-->