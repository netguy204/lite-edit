---
status: FUTURE
ticket: null
parent_chunk: null
code_paths: []
code_references: []
narrative: null
investigation: tiling_pane_layout
subsystems: []
friction_entries: []
bug_type: null
depends_on: ["tiling_workspace_integration"]
created_after: ["welcome_screen_startup"]
---

# Chunk Goal

## Minor Goal

Update the renderer to support multiple panes. Each pane is rendered independently within its screen rectangle — its own tab bar, content area, cursor, and selection. Panes are visually separated by divider lines. The focused pane has a subtle visual indicator.

Currently the renderer assumes a single content area occupying the full window (minus left rail). This chunk generalizes the render loop to iterate over pane rectangles computed by the layout algorithm, rendering each pane as an independent unit with its own geometry.

This is the chunk that makes pane splitting visible to the user.

## Success Criteria

- **Per-pane rendering**:
  - The renderer computes `PaneRect` values from the pane tree via `calculate_pane_rects()` each frame (or when the layout changes).
  - For each pane rect, the renderer draws:
    - The pane's tab bar at the top of the pane rect (using the pane's own tabs, active tab index, and tab bar scroll offset).
    - The pane's content area below the tab bar (using the pane's active tab's buffer view and viewport).
    - The cursor and selection for the pane's active tab.
    - The left rail is rendered once for the whole window (not per-pane). Line numbers, if present, are per-pane.
  - Each pane's rendering is clipped to its rectangle. Content, glyphs, and selections do not bleed into adjacent panes.

- **Clip rectangles**:
  - Metal scissor rects (or equivalent clipping mechanism) constrain each pane's draw calls to its `PaneRect` bounds.
  - Overlapping glyphs at pane boundaries are cleanly clipped.

- **Divider lines**:
  - A 1-pixel line is rendered between adjacent panes (at the boundary of their rectangles).
  - Divider color is visually distinct from both the background and the tab bar (e.g., a subtle border color from the color palette).

- **Focused pane indicator**:
  - The focused pane (the one receiving keyboard input) has a visual indicator distinguishing it from unfocused panes. This could be a colored border, a different tab bar background tint, or a subtle highlight — the exact treatment is an implementation choice, but it must be immediately apparent which pane is active.

- **Pane-local geometry**:
  - Each pane's content renderer receives pane-local dimensions (width, height of the content area within that pane). `pixel_to_buffer_position` operates on pane-local coordinates — it never sees window-global offsets. This is enforced by the coordinate pipeline established in `tiling_workspace_integration`.

- **Single-pane rendering unchanged**:
  - With one pane (no splits), the visual output is identical to before. No divider lines, no focus indicator (unnecessary with one pane). The single pane fills the content area as it does today.

- **Welcome screen and overlays**:
  - The welcome screen renders in the focused pane if its active tab is empty.
  - Selector overlay (file picker) and find strip render relative to the focused pane, not the full window.
