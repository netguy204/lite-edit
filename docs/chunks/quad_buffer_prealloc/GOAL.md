---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/glyph_buffer.rs
- crates/editor/src/selector_overlay.rs
- crates/editor/src/left_rail.rs
- crates/editor/src/tab_bar.rs
- crates/editor/src/welcome_screen.rs
- crates/editor/src/pane_frame_buffer.rs
- crates/editor/src/confirm_dialog.rs
code_references:
- ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer
  implements: "Persistent vertex/index buffers for main text rendering"
- ref: crates/editor/src/selector_overlay.rs#SelectorGlyphBuffer
  implements: "Persistent vertex/index buffers for command palette overlay"
- ref: crates/editor/src/selector_overlay.rs#FindStripGlyphBuffer
  implements: "Persistent vertex/index buffers for find/replace strip"
- ref: crates/editor/src/left_rail.rs#LeftRailGlyphBuffer
  implements: "Persistent vertex/index buffers for line number rail"
- ref: crates/editor/src/tab_bar.rs#TabBarGlyphBuffer
  implements: "Persistent vertex/index buffers for tab bar"
- ref: crates/editor/src/welcome_screen.rs#WelcomeScreenGlyphBuffer
  implements: "Persistent vertex/index buffers for welcome screen"
- ref: crates/editor/src/pane_frame_buffer.rs#PaneFrameBuffer
  implements: "Persistent vertex/index buffers for pane frame/dividers"
- ref: crates/editor/src/confirm_dialog.rs#ConfirmDialogGlyphBuffer
  implements: "Persistent vertex/index buffers for confirmation dialogs"
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

The quad buffer (`Vec<Quad>`) used for glyph rendering is rebuilt every frame. If the Vec is created as a local variable inside the render function, it allocates on every frame — a typical 40-line × 80-char viewport produces ~3,200 quads at ~64 bytes each = ~200KB of quad data. Full-viewport redraws (resize, overlay toggle, scroll) trigger fresh allocation.

Move the quad buffer(s) to persistent state and use `clear()` (which retains capacity) instead of creating new Vecs each frame. After the first frame, no heap allocation occurs for quad emission. Same treatment for background quad buffers and any other per-frame Vec allocations in the render path.

**Key files**: `crates/editor/src/glyph_buffer.rs` (quad emission), `crates/editor/src/renderer.rs` (render loop that may create local Vecs)

**Origin**: Architecture review recommendation #8 (P1 — Performance). See `ARCHITECTURE_REVIEW.md`.

## Success Criteria

- Quad buffers (`Vec<Quad>`, background quads, etc.) are persistent across frames
- `clear()` used at frame start instead of creating new Vecs
- After first full render, zero heap allocations from quad buffer growth during steady-state rendering
- No visual artifacts from buffer reuse
- Measurable: heap allocation count during steady-state typing reduced (verify with Instruments Allocations or a simple allocation counter)
- Full-viewport redraws (resize, tab switch) reuse existing capacity without reallocation
- Memory not leaked — buffers are bounded by maximum viewport size

