<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Replace the Phase 5 label rendering in `LeftRailGlyphBuffer::update()` with identicon grid generation. The identicon algorithm hashes the workspace label with SHA-256 and derives:
1. A foreground color (HSL from bytes 0-3, converted to RGB)
2. A 5×5 vertically-symmetric grid pattern (from bytes 4-5)

Each identicon cell is rendered as a colored rectangle quad using the existing `create_rect_quad()` infrastructure. "On" cells use the derived foreground color; "off" cells use 1/5 brightness for a cohesive background. The status indicator dot continues to overlay the identicon as it does today.

The algorithm is taken directly from the validated prototype in `docs/investigations/workspace_identity/prototypes/identicon_gen.py` and transliterated to Rust.

**Testing approach (per TESTING_PHILOSOPHY.md):**
- The hash→color and hash→grid derivation functions are pure and fully testable without Metal
- Unit tests verify deterministic output for known inputs (e.g., "untitled" always produces the same color and pattern)
- Unit tests verify that similar workspace names produce visually distinct identicons
- Existing geometry tests continue to pass unchanged

## Sequence

### Step 1: Add SHA-256 dependency

Add the `sha2` crate to `crates/editor/Cargo.toml` for SHA-256 hashing.

**Location:** `crates/editor/Cargo.toml`

```toml
sha2 = "0.10"
```

### Step 2: Implement identicon color derivation

Create a pure function that takes a SHA-256 digest and returns an RGBA color array.

Algorithm (from prototype):
- Hue: bytes 0-1 combined as u16, mod 360
- Saturation: byte 2 mapped to range [0.5, 0.8]
- Lightness: byte 3 mapped to range [0.4, 0.65]
- Convert HSL to RGB

**Location:** `crates/editor/src/left_rail.rs` — add new function `identicon_color_from_hash`

```rust
/// Derives an RGBA foreground color from a SHA-256 hash.
///
/// Algorithm:
/// - Hue: bytes 0-1 (little-endian u16) mod 360
/// - Saturation: byte 2 mapped to [0.5, 0.8]
/// - Lightness: byte 3 mapped to [0.4, 0.65]
fn identicon_color_from_hash(hash: &[u8; 32]) -> [f32; 4] {
    // ...
}
```

Also implement an HSL-to-RGB helper function (colorsys equivalent).

### Step 3: Implement identicon grid derivation

Create a pure function that takes a SHA-256 digest and returns a 5×5 boolean grid representing the pattern.

Algorithm (from prototype):
- Extract 15 bits from bytes 4-5 (little-endian u16)
- For each row 0..5 and column 0..3:
  - Bit index = row * 3 + col
  - Cell is "on" if bit is set
  - Mirror: grid[row][4-col] = grid[row][col]

**Location:** `crates/editor/src/left_rail.rs` — add new function `identicon_grid_from_hash`

```rust
/// Derives a 5×5 vertically-symmetric grid pattern from a SHA-256 hash.
///
/// Returns a [[bool; 5]; 5] where true = filled cell, false = background cell.
fn identicon_grid_from_hash(hash: &[u8; 32]) -> [[bool; 5]; 5] {
    // ...
}
```

### Step 4: Replace Phase 5 label rendering with identicon quads

Modify the Phase 5 section of `LeftRailGlyphBuffer::update()` to:
1. Hash each workspace label with SHA-256
2. Derive the foreground color and grid pattern
3. Calculate cell size: `cell_size = (tile_width - padding) / 5`
4. For each cell in the 5×5 grid:
   - Compute cell position within the tile
   - If cell is "on": use foreground color
   - If cell is "off": use dimmed color (1/5 brightness)
   - Generate a quad via `create_rect_quad()`

Update capacity estimation: each workspace now generates up to 25 quads (instead of 3 label glyphs).

Rename `label_range` to `identicon_range` throughout the struct and its accessors for clarity.

**Location:** `crates/editor/src/left_rail.rs`, lines 389-419 (Phase 5 section)

### Step 5: Add unit tests for hash→color derivation

Write tests verifying:
1. Determinism: same input always produces same output
2. Known values: "untitled" produces a specific expected color (snapshot test)
3. Range validity: output RGB values are in [0.0, 1.0]

**Location:** `crates/editor/src/left_rail.rs`, in `#[cfg(test)] mod tests`

```rust
#[test]
fn test_identicon_color_deterministic() { ... }

#[test]
fn test_identicon_color_known_input() { ... }

#[test]
fn test_identicon_color_valid_range() { ... }
```

### Step 6: Add unit tests for hash→grid derivation

Write tests verifying:
1. Determinism: same input always produces same grid
2. Vertical symmetry: grid[row][col] == grid[row][4-col] for all rows
3. Known values: "untitled" produces a specific expected pattern (snapshot test)

**Location:** `crates/editor/src/left_rail.rs`, in `#[cfg(test)] mod tests`

```rust
#[test]
fn test_identicon_grid_deterministic() { ... }

#[test]
fn test_identicon_grid_symmetric() { ... }

#[test]
fn test_identicon_grid_known_input() { ... }
```

### Step 7: Add unit tests for similar-name differentiation

Write tests verifying that similar workspace names produce visually distinct identicons:
- "project-alpha" vs "project-beta" (same prefix)
- "untitled" vs "untitled-2" (sequential)
- "feature/auth" vs "feature/ui" (same path prefix)

**Location:** `crates/editor/src/left_rail.rs`, in `#[cfg(test)] mod tests`

```rust
#[test]
fn test_similar_names_produce_distinct_identicons() { ... }
```

### Step 8: Run existing tests and verify no regressions

Run `cargo test` to ensure:
- All geometry tests pass unchanged
- All status color tests pass unchanged
- New identicon tests pass

**Command:** `cargo test -p lite-edit`

## Dependencies

- `sha2` crate (version 0.10) for SHA-256 hashing
- No chunk dependencies — this chunk modifies existing infrastructure in place

## Risks and Open Questions

- **HSL to RGB conversion accuracy:** The prototype uses Python's `colorsys.hls_to_rgb`. Need to verify Rust implementation matches. Mitigated by snapshot tests with known inputs.
- **Cell size at non-retina:** At 48px tile with 4px padding per side, cells are ~8px. This is fine on retina but may look blocky on non-retina. The investigation deemed this acceptable, but visual verification is warranted.
- **Performance:** 25 quads per workspace (vs ~3 previously) increases vertex count. With typical workspace counts (3-10), this adds 60-220 vertices — negligible for GPU rendering. No optimization needed.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
