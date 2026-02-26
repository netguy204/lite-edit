<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The core issue is that `Vec<GlyphVertex>` and `Vec<u32>` buffers are created as local variables inside update methods each frame, causing heap allocation on every full-viewport redraw. The fix is to move these buffers to persistent struct fields and use `clear()` (which retains capacity) instead of creating new Vecs.

**Strategy**:

1. For each glyph buffer type (`GlyphBuffer`, `SelectorGlyphBuffer`, `FindStripGlyphBuffer`, `LeftRailGlyphBuffer`, `TabBarGlyphBuffer`, `WelcomeScreenGlyphBuffer`, `PaneFrameBuffer`, `ConfirmDialogGlyphBuffer`), add `vertices: Vec<GlyphVertex>` and `indices: Vec<u32>` fields to the struct.

2. In each `update*` method, replace:
   ```rust
   let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(estimated * 4);
   let mut indices: Vec<u32> = Vec::with_capacity(estimated * 6);
   ```
   with:
   ```rust
   self.vertices.clear();
   self.indices.clear();
   // Use reserve_exact only if needed for capacity
   if self.vertices.capacity() < estimated * 4 {
       self.vertices.reserve(estimated * 4 - self.vertices.len());
   }
   ```

3. After populating `self.vertices` and `self.indices`, create the Metal buffers from the persistent Vecs (this part remains the same — we still create new `MTLBuffer` objects each frame since Metal needs them, but the CPU-side Vec allocation is eliminated).

**Note on Metal Buffers**: The `MTLBuffer` objects are still created fresh each frame from the vertex/index data. This is intentional — the optimization targets the CPU-side Vec allocation, not the GPU buffer creation. The Metal buffer creation uses `newBufferWithBytes` which copies data to GPU memory.

**Testing approach**: Per TESTING_PHILOSOPHY.md, the GPU buffer creation is "humble view" code that can't be meaningfully unit-tested. However, we can:
- Verify the optimization through allocation profiling (Instruments Allocations)
- Add a compile-time assertion that `GlyphVertex` size matches `VERTEX_SIZE`
- Ensure no visual artifacts through manual testing

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk IMPLEMENTS a performance optimization within the renderer subsystem. The subsystem documents the glyph buffer pattern (`GlyphBuffer`, `GlyphVertex`, quad ranges) that we're optimizing. This chunk adds persistent CPU-side buffers to avoid per-frame allocation while preserving the existing quad emission and Metal buffer creation patterns.

## Sequence

### Step 1: Add persistent vertex/index buffers to GlyphBuffer

Add `vertices: Vec<GlyphVertex>` and `indices: Vec<u32>` fields to `GlyphBuffer` struct.

Location: `crates/editor/src/glyph_buffer.rs`

Changes:
- Add two new fields to `GlyphBuffer` struct
- Update `GlyphBuffer::new()` to initialize with `Vec::new()` (zero allocation until first use)
- Add backreference comment for this chunk

### Step 2: Refactor GlyphBuffer::update_from_lines to use persistent buffers

Modify `update_from_lines` to use `self.vertices.clear()` and `self.indices.clear()` instead of creating new Vecs.

Location: `crates/editor/src/glyph_buffer.rs` (~line 414-415)

Changes:
- Replace `let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(...)` with `self.vertices.clear(); self.vertices.reserve(...)`
- Replace `let mut indices: Vec<u32> = Vec::with_capacity(...)` with `self.indices.clear(); self.indices.reserve(...)`
- Update references from `vertices` to `self.vertices` and `indices` to `self.indices`
- At end, create Metal buffers from `self.vertices` and `self.indices`

### Step 3: Refactor GlyphBuffer::update_from_buffer_with_cursor to use persistent buffers

Same pattern as Step 2 for the more complex viewport-aware update method.

Location: `crates/editor/src/glyph_buffer.rs` (~line 608-609)

### Step 4: Refactor GlyphBuffer::update_from_buffer_wrapped to use persistent buffers

Same pattern for the line-wrap-aware update method.

Location: `crates/editor/src/glyph_buffer.rs` (~line 1242-1243)

### Step 5: Add persistent buffers to SelectorGlyphBuffer

Add `vertices: Vec<GlyphVertex>` and `indices: Vec<u32>` fields.

Location: `crates/editor/src/selector_overlay.rs`

Changes:
- Add fields to `SelectorGlyphBuffer` struct (~line 216)
- Update `new()` to initialize fields
- Refactor `update_from_widget()` (~line 342-343) to use `clear()` instead of new Vecs

### Step 6: Add persistent buffers to FindStripGlyphBuffer

Same pattern as Step 5.

Location: `crates/editor/src/selector_overlay.rs`

Changes:
- Add fields to `FindStripGlyphBuffer` struct (~line 758)
- Update `new()` to initialize fields
- Refactor `update()` (~line 850) to use `clear()` instead of new Vecs

### Step 7: Add persistent buffers to LeftRailGlyphBuffer

Same pattern.

Location: `crates/editor/src/left_rail.rs`

Changes:
- Add fields to `LeftRailGlyphBuffer` struct (~line 292)
- Update `new()` to initialize fields
- Refactor `update()` (~line 397-398) to use `clear()` instead of new Vecs

### Step 8: Add persistent buffers to TabBarGlyphBuffer

Same pattern.

Location: `crates/editor/src/tab_bar.rs`

Changes:
- Add fields to `TabBarGlyphBuffer` struct
- Update `new()` to initialize fields
- Refactor `update()` (~line 661-662) to use `clear()` instead of new Vecs

### Step 9: Add persistent buffers to WelcomeScreenGlyphBuffer

Same pattern.

Location: `crates/editor/src/welcome_screen.rs`

Changes:
- Add fields to `WelcomeScreenGlyphBuffer` struct (~line 312)
- Update `new()` to initialize fields
- Refactor `update()` (~line 399) to use `clear()` instead of new Vecs

### Step 10: Add persistent buffers to PaneFrameBuffer

Same pattern.

Location: `crates/editor/src/pane_frame_buffer.rs`

Changes:
- Add fields to struct
- Update `new()` to initialize fields
- Refactor `update()` (~line 364) to use `clear()` instead of new Vecs

### Step 11: Add persistent buffers to ConfirmDialogGlyphBuffer

Same pattern.

Location: `crates/editor/src/confirm_dialog.rs`

Changes:
- Add fields to `ConfirmDialogGlyphBuffer` struct
- Update `new()` to initialize fields
- Refactor `update()` (~line 1075) to use `clear()` instead of new Vecs

### Step 12: Address rendered_buffer_lines Vec in GlyphBuffer

The `update_from_buffer_wrapped` method also creates a local `rendered_buffer_lines: Vec<usize>` that grows during iteration.

Location: `crates/editor/src/glyph_buffer.rs` (~line 1192)

Changes:
- Add `rendered_buffer_lines: Vec<usize>` field to `GlyphBuffer`
- Use `self.rendered_buffer_lines.clear()` instead of creating new Vec

### Step 13: Address positions Vec in GlyphBuffer

There's also a `positions: Vec<usize>` at ~line 2179 that may need similar treatment.

Location: `crates/editor/src/glyph_buffer.rs`

Changes:
- Evaluate if this is in a hot path; if so, add as persistent field
- If it's only used rarely (e.g., one-time calculations), leave as-is

### Step 14: Build and verify

- Run `cargo build` to ensure all changes compile
- Run `cargo test` to verify no regressions
- Manual visual verification that rendering still works correctly

### Step 15: Performance verification

- Use Instruments Allocations to measure heap allocation reduction during steady-state rendering
- Compare allocation counts for:
  - Single character typing (should show minimal allocations)
  - Full viewport redraw (resize) — should now reuse existing capacity

## Dependencies

None. This is a pure refactoring of existing code with no external dependencies.

## Risks and Open Questions

1. **Memory growth bounding**: The persistent buffers will retain their high-water-mark capacity. If a user briefly views an extremely large viewport (e.g., during a resize animation), the buffers will grow and stay large. This should be acceptable since:
   - Capacity is bounded by maximum viewport size (~200KB per buffer type)
   - Memory is released when the `GlyphBuffer` is dropped
   - If needed, we could add a `shrink_to_fit()` call on viewport resize events

2. **Borrow checker interactions**: The refactoring changes `vertices`/`indices` from local variables to `self` fields. Need to ensure no borrow conflicts arise when passing `&self.vertices` to Metal buffer creation while `self` is still mutable.

3. **Thread safety**: `GlyphBuffer` and related types are currently not `Send`/`Sync` (due to `Retained<MTLBuffer>`). Adding persistent Vecs doesn't change this, but we should verify the render path remains single-threaded.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->