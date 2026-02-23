---
status: SOLVED
trigger: "All workspaces currently display as 'unt' — no way to distinguish one from another in the sidebar"
proposed_chunks:
  - prompt: "Implement workspace identicon rendering in the left rail. Hash workspace labels with SHA-256 to derive a foreground color (hue from bytes 0-1, saturation from byte 2, lightness from byte 3) and a 5×5 vertically-symmetric grid pattern (from bytes 4-5). Replace the current 3-char label rendering in LeftRailGlyphBuffer::update() with identicon quad generation using the existing create_rect_quad infrastructure. Include dimmed off-cells (1/5 foreground brightness) for a cohesive tile background. Keep the status indicator dot overlay. See docs/investigations/workspace_identity/prototypes/identicon_gen.py for the exact algorithm."
    chunk_directory: workspace_identicon
    depends_on: []
created_after: ["tiling_pane_layout"]
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

Every workspace in the sidebar currently renders as the string "unt" — they are completely indistinguishable from one another. The sidebar has very limited real estate for workspace representation, so the solution must work at small sizes. The question is how to make each workspace rapidly and reliably identifiable within that constraint. Identicons (hash-derived symmetric graphics) are one promising technique, but the problem space is worth exploring before committing to an approach.

## Success Criteria

1. Determine whether identicons are sufficiently distinguishable and memorable at the available sidebar size (48×48px tiles in a 56px-wide rail)
2. Understand what hash input to use (workspace name, path, UUID, etc.) and whether the resulting visual variety is adequate across typical workspace counts (3–10)
3. Determine the implementation approach for the GPU-rendered Metal pipeline (vertex generation in left_rail.rs)
4. Produce a clear recommendation: adopt identicons, adopt an alternative, or combine techniques — with enough specificity to write an implementation chunk

## Testable Hypotheses

### H1: Identicons rendered in a small grid (5×5 or 3×3) are visually distinct at 48px tile size

- **Rationale**: Identicons work well in web UIs at ~32px. This is a GPU-rendered app (Metal) with 48×48px tiles, giving real pixels to work with.
- **Test**: Generate sample identicons at target size, evaluate distinguishability across 12 test names.
- **Status**: VERIFIED — Both 5×5 and 3×3 grids produce clearly distinct patterns. Color + pattern together provide two independent recognition channels. Even at 1x (48px) in the simulated rail, tiles are distinguishable at a glance. The 5×5 grid is richer but the 3×3 is more "readable" at small sizes. See `prototypes/hybrid_5x5_48px.png`, `prototypes/hybrid_3x3_48px.png`, `prototypes/simulated_rail.png`.

### H2: Workspace names produce enough hash entropy that similar names don't collide visually

- **Rationale**: Users might name workspaces similarly (project-a, project-b). If the algorithm maps similar strings to similar visuals, the scheme fails.
- **Test**: Hash plausibly similar workspace names and compare for near-collisions.
- **Status**: VERIFIED — SHA-256 produces completely different patterns for similar names. "project-alpha" through "project-delta" all have distinct colors AND patterns. "untitled" vs "untitled-2" are completely different. "workspace-1" vs "workspace-2" are completely different. The cryptographic hash guarantees this property. See `prototypes/identicon_comparison_2x.png`.

### H3: Color alone (colored square + initial letter) might be sufficient without a full identicon

- **Rationale**: Color is the fastest pre-attentive visual channel. A colored square with an initial letter might be simpler to implement and equally effective.
- **Test**: Mock up colored-initial approach alongside identicons and compare.
- **Status**: FALSIFIED — The colored-initial approach fails badly when workspace names share prefixes. All four "project-*" workspaces show "P" in similar hue ranges. "feature/auth" and "feature/ui" both show "F" in similar teal. "workspace-1" and "workspace-2" are nearly identical. Color alone doesn't provide enough differentiation; the pattern is essential. See `prototypes/colored_initial_comparison.png`.

## Exploration Log

### 2026-02-22: Initial prototyping

Built two prototype scripts to test all three hypotheses:

1. **`prototypes/identicon_gen.py`** — Generates comparison sheets for 5×5 identicons, 2× zoom identicons, and colored-initial alternative. Tested 12 workspace names including deliberately similar groups (project-alpha/beta/gamma/delta, feature/auth vs feature/ui, untitled vs untitled-2, workspace-1 vs workspace-2).

2. **`prototypes/hybrid_gen.py`** — Generates identicons with dimmed "off" cells (subtle grid background), status indicator overlay, and a simulated left rail showing how 8 workspaces would look in the actual layout.

**Key observations:**
- 5×5 identicons at 48px are clearly distinguishable. Each cell is ~8px, which is fine on retina displays.
- 3×3 identicons are bolder/simpler — fewer possible patterns (64 vs 32768) but more readable at small size.
- The dimmed "off" cell background helps the pattern feel cohesive rather than floating.
- SHA-256 completely scrambles similar inputs — "project-alpha" vs "project-beta" produce unrelated colors and patterns.
- Colored-initial approach fails when names share prefixes, which is a common naming pattern.
- The simulated rail view confirms the approach works in context — 8 stacked identicons are all distinguishable.

**Implementation considerations for Metal rendering:**
- The identicon is just colored rectangles — maps directly to the existing `create_rect_quad` pattern in `left_rail.rs`.
- A 5×5 grid = 25 quads max per tile (plus background quad). Very lightweight.
- The hash can be computed once when a workspace is created and cached as a `[u8; 32]` on the `Workspace` struct.
- Grid pattern + color can be derived from the hash at render time (cheap bit operations).
- Could use `workspace.label` or `workspace.root_path` as the hash input. Label is simpler but path is more stable if labels change.

## Findings

### Verified Findings

- **5×5 identicons work at 48px tile size.** Both color and pattern are clearly distinguishable across 12 test names, including deliberately similar name groups. (Evidence: `prototypes/hybrid_5x5_48px.png`, `prototypes/simulated_rail.png`)
- **SHA-256 provides sufficient entropy.** Similar workspace names (project-alpha vs project-beta, untitled vs untitled-2) produce completely unrelated visual patterns. No near-collisions observed in testing. (Evidence: `prototypes/identicon_comparison_2x.png`)
- **Color-only approach is insufficient.** When workspace names share prefixes (a common pattern), both the initial letter and the hash-derived hue are too similar to distinguish. Pattern is the essential differentiator. (Evidence: `prototypes/colored_initial_comparison.png`)
- **Implementation maps cleanly to existing architecture.** The identicon is just colored rectangles, which maps directly to the `create_rect_quad` pattern already used in `left_rail.rs`. A 5×5 grid adds at most 25 quads per tile — negligible GPU cost.

### Hypotheses/Opinions

- **5×5 is probably better than 3×3** despite 3×3 being "bolder." The 5×5 grid has 32768 possible patterns vs 64 for 3×3, making visual collisions far less likely as workspace count grows. At 48px, 5×5 cells are ~8px each, which is fine on retina but might be tight on non-retina. Could offer 3×3 as a fallback.
- **Hash input should be `workspace.label`** rather than path, because the label is what the user chose and is the most semantically meaningful. If labels ever become editable, the identicon would change — which might actually be desirable (you renamed it, so it looks different now).
- **The dimmed "off" cell background** (1/5 brightness of fg color) looks better than transparent cells. It gives the identicon a cohesive tile feel rather than floating pixels. Recommend including this.

## Proposed Chunks

1. **Workspace identicon rendering**: Implement identicon generation and rendering in the left rail.
   - Add SHA-256 hashing of workspace label to derive color + 5×5 grid pattern
   - Replace the current 3-char label rendering in `LeftRailGlyphBuffer::update()` with identicon quad generation
   - Keep the status indicator dot overlay
   - Priority: High (directly fixes the "all workspaces look the same" problem)
   - Dependencies: None
   - Notes: Algorithm is straightforward — see `prototypes/identicon_gen.py` for the exact hash→color and hash→grid derivation. Maps to existing `create_rect_quad` infrastructure.

## Resolution Rationale

Investigation is SOLVED. All three hypotheses have been tested:

- **H1 VERIFIED**: 5×5 identicons are clearly distinguishable at 48px tile size
- **H2 VERIFIED**: SHA-256 ensures similar names produce completely different visuals
- **H3 FALSIFIED**: Color-only/initial-letter approach fails for common naming patterns

The recommendation is clear: implement 5×5 vertically-symmetric identicons using SHA-256 hashing of workspace labels. The algorithm is simple, the implementation maps directly to the existing Metal rendering infrastructure, and the GPU cost is negligible (25 quads per tile). A single proposed chunk captures all the implementation work needed.