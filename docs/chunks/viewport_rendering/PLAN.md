<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk connects the text buffer (`crates/buffer`) to the glyph renderer (`crates/editor`) through a viewport abstraction. The viewport is the mapping layer identified in the H1 investigation traces — it determines which buffer lines are visible on screen and enables efficient incremental rendering via dirty region tracking.

### Key architectural decisions

1. **Viewport is core state, not a subsystem.** Per the investigation, the viewport sits between command execution and rendering: focus targets mutate it (scroll offset changes), and the render loop reads it (which lines to render). It's a small struct stored in the eventual `EditorContext`.

2. **DirtyRegion is the render-side complement to DirtyLines.** The buffer's `DirtyLines` enum tracks buffer-coordinate changes. The viewport translates this to `DirtyRegion` (screen-coordinate changes). This separation follows the humble view architecture: the buffer knows nothing about screen coordinates.

3. **Cursor rendering is part of viewport rendering.** The cursor position comes from the buffer; the viewport maps it to screen coordinates; the renderer draws it as a visual indicator.

4. **No input handling yet.** This chunk connects buffer → viewport → renderer. The next chunk (editable_buffer) wires up input events.

### Building on existing code

- **`lite_edit_buffer::TextBuffer`**: Provides `line_content(line)`, `line_count()`, `cursor_position()`, and `DirtyLines` from mutations.
- **`GlyphBuffer::update()`**: Currently takes `&[&str]`. Will be modified to accept a line iterator with a range, enabling partial updates.
- **`Renderer::set_content()`**: Currently takes `&[&str]`. Will be replaced with `set_buffer()` + `render_viewport()`.

### Testing strategy

Per TESTING_PHILOSOPHY.md:
- **Pure logic is testable**: `Viewport` (scroll clamping, visible range calculation), `DirtyRegion` (merge semantics), viewport-to-buffer coordinate mapping.
- **Platform code is humble**: The actual Metal draw calls are not unit-tested; we verify visually and via integration tests.
- **TDD applies**: Write tests for viewport calculations and dirty region merging before implementing.

## Sequence

### Step 1: Define the DirtyRegion enum

Create `crates/editor/src/dirty_region.rs` with the `DirtyRegion` enum as specified in the GOAL:

```rust
pub enum DirtyRegion {
    None,
    Lines { from: usize, to: usize },
    FullViewport,
}
```

Implement merge semantics:
- `Lines(a,b) + Lines(c,d)` → `Lines(min(a,c), max(b,d))`
- `Any + FullViewport` → `FullViewport`
- `None + x` → `x`

Add comprehensive unit tests for merge behavior (per investigation H3 analysis).

**Location**: `crates/editor/src/dirty_region.rs`

### Step 2: Define the Viewport struct

Create `crates/editor/src/viewport.rs` with the `Viewport` struct:

```rust
pub struct Viewport {
    /// First visible buffer line (0-indexed)
    pub scroll_offset: usize,
    /// Number of lines that fit in the viewport (computed from window height / line height)
    visible_lines: usize,
    /// Line height in pixels (for computing visible_lines)
    line_height: f32,
}
```

Implement methods:
- `new(line_height: f32)` — create with no content
- `update_size(window_height: f32)` — recompute `visible_lines` when window resizes
- `visible_range(&self, buffer_line_count: usize) -> Range<usize>` — returns `scroll_offset..min(scroll_offset + visible_lines, buffer_line_count)`
- `scroll_to(&mut self, line: usize, buffer_line_count: usize)` — clamps scroll_offset to valid bounds
- `ensure_visible(&mut self, line: usize, buffer_line_count: usize) -> bool` — scrolls if needed to bring `line` into view, returns whether scroll_offset changed
- `buffer_line_to_screen_line(&self, buffer_line: usize) -> Option<usize>` — returns screen Y if visible, None otherwise

Add unit tests for:
- `visible_range` at buffer start, middle, end
- `visible_range` when buffer has fewer lines than viewport
- `scroll_to` clamping
- `ensure_visible` scrolling behavior

**Location**: `crates/editor/src/viewport.rs`

### Step 3: Implement DirtyLines → DirtyRegion conversion

Add a method to convert buffer-space `DirtyLines` to screen-space `DirtyRegion` given a viewport:

```rust
impl Viewport {
    pub fn dirty_lines_to_region(
        &self,
        dirty: &DirtyLines,
        buffer_line_count: usize,
    ) -> DirtyRegion {
        // Map buffer line indices to screen indices via scroll_offset
        // Lines outside the viewport produce DirtyRegion::None
        // FromLineToEnd that touches visible range → FullViewport
    }
}
```

Add unit tests for:
- Single dirty line inside viewport → `Lines { from, to }`
- Single dirty line outside viewport → `None`
- `FromLineToEnd` starting inside viewport → `FullViewport`
- `FromLineToEnd` starting below viewport → `None`
- Range spanning into viewport → appropriate `Lines` region

**Location**: `crates/editor/src/viewport.rs`

### Step 4: Add the lite-edit-buffer crate as a dependency

Update `crates/editor/Cargo.toml` to depend on the buffer crate:

```toml
[dependencies]
lite-edit-buffer = { path = "../buffer" }
```

Verify the build compiles with both crates.

**Location**: `crates/editor/Cargo.toml`

### Step 5: Extend GlyphBuffer for viewport-aware rendering

Modify `GlyphBuffer::update()` to accept a viewport and buffer, rendering only visible lines:

```rust
impl GlyphBuffer {
    pub fn update_from_buffer(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        buffer: &TextBuffer,
        viewport: &Viewport,
    ) {
        let range = viewport.visible_range(buffer.line_count());
        // Build quads only for lines in range
        // Adjust y-coordinate: screen_row = buffer_line - scroll_offset
    }
}
```

Update the existing `update(&[&str])` method to use this internally with a trivial viewport, or deprecate it.

**Location**: `crates/editor/src/glyph_buffer.rs`

### Step 6: Add cursor rendering to GlyphBuffer

Extend `GlyphBuffer` to render the cursor:

```rust
impl GlyphBuffer {
    pub fn update_from_buffer_with_cursor(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        buffer: &TextBuffer,
        viewport: &Viewport,
        cursor_visible: bool,  // For future blink support
    ) {
        // Existing glyph rendering...

        // Add cursor quad if cursor is in visible range
        let (cursor_line, cursor_col) = buffer.cursor_position();
        if let Some(screen_line) = viewport.buffer_line_to_screen_line(cursor_line) {
            // Render cursor as a solid rectangle at (cursor_col, screen_line)
            // Use a special "cursor" glyph or render as a colored quad
        }
    }
}
```

Cursor rendering approach:
- Draw a solid vertical bar (line cursor) or filled rectangle (block cursor)
- Reuse the glyph pipeline with a solid-color texture region or a separate cursor quad
- For simplicity, start with a block cursor that inverts/highlights the character cell

**Location**: `crates/editor/src/glyph_buffer.rs`

### Step 7: Refactor Renderer to use Viewport and TextBuffer

Modify `Renderer` to hold a `Viewport` and render from a `TextBuffer`:

```rust
pub struct Renderer {
    // ... existing fields ...
    viewport: Viewport,
    buffer: Option<TextBuffer>,  // Or a reference/Rc
}

impl Renderer {
    pub fn new(view: &MetalView) -> Self {
        // ... existing init ...
        let viewport = Viewport::new(font.metrics.line_height as f32);
        Self {
            // ...
            viewport,
            buffer: None,
        }
    }

    pub fn set_buffer(&mut self, buffer: TextBuffer) {
        self.buffer = Some(buffer);
        // Mark full viewport dirty on initial load
    }

    pub fn update_viewport_size(&mut self, window_height: f32) {
        self.viewport.update_size(window_height);
    }

    pub fn render(&mut self, view: &MetalView) {
        // ... existing render setup ...

        if let Some(buffer) = &self.buffer {
            self.glyph_buffer.update_from_buffer_with_cursor(
                &self.device,
                &self.atlas,
                buffer,
                &self.viewport,
                true,  // cursor visible
            );
        }

        // ... existing draw calls ...
    }
}
```

**Location**: `crates/editor/src/renderer.rs`

### Step 8: Update main.rs to use TextBuffer with viewport

Replace the hardcoded `DEMO_TEXT` with a `TextBuffer` containing 100+ lines:

```rust
fn setup_window(&self, mtm: MainThreadMarker) {
    // ... existing window creation ...

    let mut renderer = Renderer::new(&metal_view);

    // Create a buffer with substantial content
    let demo_content = generate_demo_content();  // 100+ lines
    let buffer = TextBuffer::from_str(&demo_content);
    renderer.set_buffer(buffer);

    // Update viewport size based on window
    let frame = metal_view.frame();
    renderer.update_viewport_size((frame.size.height * metal_view.scale_factor()) as f32);

    // ... existing render and setup ...
}
```

Add scroll offset mutation for testing (temporary, manual):
- The buffer should display starting from line 0
- Programmatically changing `renderer.viewport.scroll_offset` should display different slices

**Location**: `crates/editor/src/main.rs`

### Step 9: Add dirty region tracking to the render loop

Implement partial rendering based on `DirtyRegion`:

```rust
impl Renderer {
    pub fn apply_mutation(&mut self, dirty_lines: DirtyLines) -> DirtyRegion {
        if let Some(buffer) = &self.buffer {
            self.viewport.dirty_lines_to_region(&dirty_lines, buffer.line_count())
        } else {
            DirtyRegion::None
        }
    }

    pub fn render_dirty(&mut self, view: &MetalView, dirty: &DirtyRegion) {
        match dirty {
            DirtyRegion::None => {
                // No redraw needed
            }
            DirtyRegion::FullViewport | DirtyRegion::Lines { .. } => {
                // For now, always do full redraw
                // Optimization: only rebuild affected line quads
                self.render(view);
            }
        }
    }
}
```

Note: True partial re-rendering (only updating specific GPU buffer regions) is an optimization. For this chunk, we rebuild the full glyph buffer but the dirty region tracking is in place for future optimization. The H3 analysis confirmed full viewport redraws are <1ms, so this is acceptable.

**Location**: `crates/editor/src/renderer.rs`

### Step 10: Write integration tests for viewport rendering

Create tests that verify the viewport correctly displays buffer content:

```rust
// Test that viewport shows correct lines
#[test]
fn test_viewport_displays_visible_lines() {
    let buffer = TextBuffer::from_str(&"line1\nline2\nline3\n...".repeat(50));
    let mut viewport = Viewport::new(16.0);
    viewport.update_size(160.0);  // 10 visible lines

    let range = viewport.visible_range(buffer.line_count());
    assert_eq!(range, 0..10);

    viewport.scroll_to(20, buffer.line_count());
    let range = viewport.visible_range(buffer.line_count());
    assert_eq!(range, 20..30);
}

// Test that cursor position maps correctly through viewport
#[test]
fn test_cursor_screen_position() {
    let mut buffer = TextBuffer::from_str("line1\nline2\nline3");
    buffer.set_cursor(Position::new(1, 2));

    let viewport = Viewport::new(16.0);
    let screen_line = viewport.buffer_line_to_screen_line(buffer.cursor_position().line);
    assert_eq!(screen_line, Some(1));
}
```

**Location**: `crates/editor/tests/viewport_test.rs`

### Step 11: Manual visual verification

Create a test scenario that can be verified visually:

1. Launch the editor with a 100+ line buffer
2. Verify text renders starting from line 0
3. Modify scroll_offset programmatically (via test code or debug UI)
4. Verify the displayed content shifts correctly
5. Verify cursor renders at the correct position
6. Verify that window resize recomputes visible lines

Document the verification steps in test comments.

**Location**: `crates/editor/tests/smoke_test.rs` (extend existing)

## Dependencies

- **text_buffer chunk** (ACTIVE): Provides `TextBuffer`, `DirtyLines`, `Position`
- **glyph_rendering chunk** (ACTIVE): Provides `GlyphBuffer`, `GlyphAtlas`, `Renderer`, `Font`
- **metal_surface chunk** (ACTIVE): Provides `MetalView` and Metal infrastructure

No new external dependencies needed — this chunk integrates existing crates.

## Risks and Open Questions

1. **Cursor rendering approach**: Should the cursor be a special glyph in the atlas or a separate colored quad? Starting with a block cursor rendered as an inverted cell is simplest. If visual quality is poor, may need a separate cursor rendering path.

2. **Buffer ownership**: The `Renderer` currently owns the `TextBuffer`. In the full architecture (editable_buffer chunk), the buffer will be owned by `EditorContext` and passed by reference. This chunk uses owned storage as a stepping stone.

3. **Partial rendering optimization**: The dirty region tracking is implemented but actual partial GPU buffer updates are deferred. If full viewport rebuilds prove too slow (unlikely per H3), this would need to change.

4. **Scroll offset mutation**: Without input handling, scroll offset changes are programmatic. The API is designed to support the drain-all-then-render loop from editable_buffer.

5. **Line height precision**: The viewport uses pixel-space line height from font metrics. Ensure consistent handling of floating-point rounding to avoid off-by-one errors in visible line calculation.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->