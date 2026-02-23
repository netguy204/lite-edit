---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/left_rail.rs
  - crates/editor/Cargo.toml
code_references:
  - ref: crates/editor/src/left_rail.rs#hsl_to_rgb
    implements: "HSL to RGB color conversion for identicon foreground colors"
  - ref: crates/editor/src/left_rail.rs#identicon_color_from_hash
    implements: "Derives RGBA foreground color from SHA-256 hash (hue/sat/light from bytes 0-3)"
  - ref: crates/editor/src/left_rail.rs#identicon_grid_from_hash
    implements: "Derives 5×5 vertically-symmetric grid pattern from hash bytes 4-5"
  - ref: crates/editor/src/left_rail.rs#hash_workspace_label
    implements: "SHA-256 hashing of workspace labels for identicon generation"
  - ref: crates/editor/src/left_rail.rs#LeftRailGlyphBuffer::identicon_range
    implements: "Quad range accessor for identicon rendering phase"
  - ref: crates/editor/src/left_rail.rs#LeftRailGlyphBuffer::update
    implements: "Phase 5 identicon quad generation (25 cells per workspace)"
narrative: null
investigation: workspace_identity
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- dirty_region_wrap_aware
- macos_app_bundle
---

# Chunk Goal

## Minor Goal

Replace the current 3-character label rendering in workspace tiles with hash-derived identicon graphics, making each workspace visually unique and rapidly identifiable in the left rail.

Currently all workspaces display "unt" (first 3 chars of "untitled") and are completely indistinguishable. This chunk implements vertically-symmetric 5×5 identicons where a SHA-256 hash of the workspace label determines both the foreground color and the grid pattern. The approach was validated in `docs/investigations/workspace_identity/` — identicons are clearly distinguishable at the 48px tile size, even for similar workspace names.

## Success Criteria

- Each workspace tile in the left rail displays a unique 5×5 vertically-symmetric identicon derived from the workspace label via SHA-256
- The identicon foreground color is derived from hash bytes (hue from bytes 0-1, saturation from byte 2, lightness from byte 3)
- The 5×5 grid pattern is derived from hash bytes 4-5 (15 bits for the left half + center, mirrored)
- "Off" cells render at ~1/5 foreground brightness for a cohesive tile background
- The status indicator dot remains visible overlaid on the identicon
- Similar workspace names (e.g., "project-alpha" vs "project-beta", "untitled" vs "untitled-2") produce visually distinct identicons
- The existing left rail unit tests continue to pass
- New unit tests verify the hash→color and hash→grid derivation produces expected outputs for known inputs

## Rejected Ideas

### Color + initial letter approach

Use a hash-derived background color with the first letter of the workspace name displayed large in the center.

Rejected because: Investigation H3 showed this fails when workspace names share prefixes — a very common pattern (e.g., all "project-*" workspaces show "P" in similar hues, all "feature/*" show "F"). The pattern component is essential for differentiation.

### 3×3 grid instead of 5×5

Use a coarser 3×3 grid for bolder, more readable shapes at small size.

Rejected because: 3×3 has only 64 possible patterns vs 32,768 for 5×5, making visual collisions far more likely as workspace count grows. At 48px, 5×5 cells are ~8px each, which is readable on retina displays.