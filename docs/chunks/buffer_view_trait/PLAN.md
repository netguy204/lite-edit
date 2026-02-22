<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The `BufferView` trait establishes the contract between buffer implementations and the renderer. This is the keystone abstraction identified in the hierarchical_terminal_tabs investigation: the right unification point is at the **view layer**, not the storage layer.

**Strategy:**

1. **Define the trait and supporting types in the `buffer` crate.** The types (`BufferView`, `StyledLine`, `Span`, `Style`, `CursorInfo`, `Color`) are foundational vocabulary shared by all buffer implementations.

2. **Design `Style` for terminal-grade richness from day one.** The investigation emphasizes this: avoid breaking changes later by supporting all terminal attributes upfront (fg/bg color, bold, italic, dim, 5 underline variants + underline color, strikethrough, inverse, hidden).

3. **Keep the trait object-safe.** The goal explicitly requires `Box<dyn BufferView>` and `&dyn BufferView` usage. This means no generic methods, no associated types with complex bounds, and no `Self` in return positions.

4. **Implement `BufferView` for `TextBuffer` with trivial defaults.** Each line becomes a single `Span` with default `Style`. This proves the trait works and keeps existing behavior unchanged.

5. **Migrate the renderer to consume `BufferView`.** Replace the current `Option<TextBuffer>` with `Option<Box<dyn BufferView>>`. Update `GlyphBuffer::update_from_buffer_with_cursor()` to work with the trait instead of concrete `TextBuffer`.

6. **Preserve all existing rendering behavior.** The demo text must render identically. No visual changes should occur from this refactor.

**Testing Philosophy alignment:**

Per `docs/trunk/TESTING_PHILOSOPHY.md`:
- **TDD for behavioral code**: The `Style` type and `BufferView` trait have meaningful behavior to test (merging dirty lines, generating styled lines, etc.).
- **Humble view architecture**: The renderer remains humble — it reads `BufferView` and produces pixels. Testing focuses on the model layer (`TextBuffer`'s `BufferView` implementation).
- **Goal-driven test design**: Tests verify the success criteria directly: object-safety, dirty tracking flow, line content correctness.

## Sequence

### Step 1: Define the `Color` type

Create the `Color` enum supporting all terminal color modes. This is a leaf type with no dependencies.

Location: `crates/buffer/src/buffer_view.rs` (new file)

```rust
/// Terminal color representation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    /// Default foreground/background (terminal decides)
    #[default]
    Default,
    /// Named ANSI colors (0-15)
    Named(NamedColor),
    /// 256-color palette index
    Indexed(u8),
    /// 24-bit RGB color
    Rgb { r: u8, g: u8, b: u8 },
}

/// The 16 standard ANSI colors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedColor {
    Black, Red, Green, Yellow, Blue, Magenta, Cyan, White,
    BrightBlack, BrightRed, BrightGreen, BrightYellow,
    BrightBlue, BrightMagenta, BrightCyan, BrightWhite,
}
```

**Tests:** Unit tests for `Color::Default`, `Color::Rgb`, `Color::Indexed`, `Color::Named` construction and equality.

### Step 2: Define the `UnderlineStyle` enum

Create the underline style enum with 5 variants as specified in the goal.

Location: `crates/buffer/src/buffer_view.rs`

```rust
/// Underline rendering style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnderlineStyle {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}
```

**Tests:** Basic construction tests (these are trivial, but establish the API).

### Step 3: Define the `Style` struct

Create the full terminal-grade `Style` struct with all attributes. Use sensible defaults (no styling = default fg, default bg, no attributes).

Location: `crates/buffer/src/buffer_view.rs`

```rust
/// Terminal-grade text styling attributes
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Style {
    /// Foreground color
    pub fg: Color,
    /// Background color
    pub bg: Color,
    /// Bold weight
    pub bold: bool,
    /// Italic slant
    pub italic: bool,
    /// Dim/faint intensity
    pub dim: bool,
    /// Underline style
    pub underline: UnderlineStyle,
    /// Underline color (None = use fg color)
    pub underline_color: Option<Color>,
    /// Strikethrough line
    pub strikethrough: bool,
    /// Inverse video (swap fg/bg at render time)
    pub inverse: bool,
    /// Hidden text (don't render glyphs)
    pub hidden: bool,
}
```

**Tests:**
- `Style::default()` produces unstyled (Default colors, no attributes)
- Builder pattern tests if we add one (optional)
- Equality tests for identical and different styles

### Step 4: Define `Span` and `StyledLine`

Create the span-based line representation that the renderer consumes.

Location: `crates/buffer/src/buffer_view.rs`

```rust
/// A contiguous run of text with uniform styling
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    /// The text content of this span
    pub text: String,
    /// The style applied to this text
    pub style: Style,
}

impl Span {
    /// Creates a new span with the given text and style
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self { text: text.into(), style }
    }

    /// Creates an unstyled span (default style)
    pub fn plain(text: impl Into<String>) -> Self {
        Self { text: text.into(), style: Style::default() }
    }
}

/// A line as the renderer sees it — a sequence of styled spans
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StyledLine {
    /// The spans comprising this line
    pub spans: Vec<Span>,
}

impl StyledLine {
    /// Creates a new styled line from spans
    pub fn new(spans: Vec<Span>) -> Self {
        Self { spans }
    }

    /// Creates a line with a single unstyled span
    pub fn plain(text: impl Into<String>) -> Self {
        Self { spans: vec![Span::plain(text)] }
    }

    /// Creates an empty line
    pub fn empty() -> Self {
        Self { spans: vec![] }
    }
}
```

**Tests:**
- `Span::plain("hello")` produces unstyled span
- `StyledLine::plain("hello")` produces single-span line
- `StyledLine::empty()` has no spans

### Step 5: Define `CursorShape` and `CursorInfo`

Create the cursor representation for rendering different cursor styles.

Location: `crates/buffer/src/buffer_view.rs`

```rust
use crate::types::Position;

/// Cursor shape for rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    /// Solid block cursor (default for most terminals)
    #[default]
    Block,
    /// Vertical line cursor (insert mode)
    Beam,
    /// Horizontal line at bottom of cell
    Underline,
    /// Cursor is hidden
    Hidden,
}

/// Information about the cursor for rendering
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorInfo {
    /// Position in the buffer (line, column)
    pub position: Position,
    /// Visual shape of the cursor
    pub shape: CursorShape,
    /// Whether the cursor should blink
    pub blinking: bool,
}

impl CursorInfo {
    /// Creates a new cursor info
    pub fn new(position: Position, shape: CursorShape, blinking: bool) -> Self {
        Self { position, shape, blinking }
    }

    /// Creates a default block cursor at the given position
    pub fn block(position: Position) -> Self {
        Self { position, shape: CursorShape::Block, blinking: true }
    }
}
```

**Tests:**
- `CursorInfo::block(pos)` produces block cursor at position
- Default shape is Block

### Step 6: Define the `BufferView` trait

Create the core trait that unifies buffer implementations for rendering.

Location: `crates/buffer/src/buffer_view.rs`

```rust
use crate::types::DirtyLines;

/// A buffer as seen by the renderer — the view abstraction that unifies
/// file buffers and terminal buffers.
///
/// This trait is object-safe: it can be used as `&dyn BufferView` or
/// `Box<dyn BufferView>`.
// Chunk: docs/chunks/buffer_view_trait - BufferView trait definition
pub trait BufferView {
    /// Returns the total number of lines available for display.
    fn line_count(&self) -> usize;

    /// Returns a styled representation of the given line.
    ///
    /// Returns `None` if the line index is out of bounds.
    fn styled_line(&self, line: usize) -> Option<StyledLine>;

    /// Drains accumulated dirty state since last call.
    ///
    /// Returns which lines need re-rendering. After calling this,
    /// the buffer's dirty state is reset.
    fn take_dirty(&mut self) -> DirtyLines;

    /// Returns whether this buffer accepts direct text input.
    ///
    /// - `true` for file editing buffers (TextBuffer)
    /// - `false` for terminal buffers (input goes to PTY stdin instead)
    fn is_editable(&self) -> bool;

    /// Returns cursor information for display.
    ///
    /// Returns `None` if the buffer has no cursor (e.g., read-only view).
    fn cursor_info(&self) -> Option<CursorInfo>;
}
```

**Tests:**
- Compile-time test: verify trait is object-safe by constructing `Box<dyn BufferView>` with a mock implementation
- The real behavioral tests come in Step 7 with `TextBuffer`

### Step 7: Implement `BufferView` for `TextBuffer`

Add the trait implementation for the existing `TextBuffer` type. Each line returns a single unstyled span. Dirty tracking flows through `take_dirty()`.

Location: `crates/buffer/src/text_buffer.rs`

**Key changes:**
1. Add a `dirty_lines: DirtyLines` field to `TextBuffer` to accumulate dirty state
2. Have mutation methods update this field (merge new dirty state)
3. Implement `BufferView` methods

```rust
impl BufferView for TextBuffer {
    fn line_count(&self) -> usize {
        self.line_count() // existing method
    }

    fn styled_line(&self, line: usize) -> Option<StyledLine> {
        if line >= self.line_count() {
            return None;
        }
        let content = self.line_content(line);
        Some(StyledLine::plain(content))
    }

    fn take_dirty(&mut self) -> DirtyLines {
        std::mem::take(&mut self.dirty_lines)
    }

    fn is_editable(&self) -> bool {
        true
    }

    fn cursor_info(&self) -> Option<CursorInfo> {
        Some(CursorInfo::block(self.cursor_position()))
    }
}
```

**Implementation detail:** Currently, `TextBuffer` mutation methods return `DirtyLines` directly. We need to also accumulate into an internal field for `take_dirty()`. Options:
- **Option A**: Add a `dirty_lines` field, have mutations merge into it, AND return the specific mutation's dirty lines (for immediate response).
- **Option B**: Have mutations only accumulate, and callers use `take_dirty()` at render time.

**Chosen approach: Option A.** This maintains backward compatibility — existing code that uses the return value continues to work. The `dirty_lines` field provides the drain-all-then-render pattern needed by `BufferView`.

**Tests (TDD):**
1. `styled_line(0)` on buffer with content "hello" returns `StyledLine { spans: [Span { text: "hello", style: default }] }`
2. `styled_line(1)` on single-line buffer returns `None`
3. `line_count()` matches existing behavior
4. `take_dirty()` returns accumulated dirty state and resets to `DirtyLines::None`
5. `is_editable()` returns `true`
6. `cursor_info()` returns block cursor at cursor position

### Step 8: Export types from `buffer` crate

Update the crate's `lib.rs` to export the new types publicly.

Location: `crates/buffer/src/lib.rs`

```rust
mod buffer_view;

pub use buffer_view::{
    BufferView, Color, CursorInfo, CursorShape,
    NamedColor, Span, Style, StyledLine, UnderlineStyle,
};
```

### Step 9: Update `GlyphBuffer` to accept `BufferView`

Modify `GlyphBuffer::update_from_buffer_with_cursor()` to work with `&dyn BufferView` instead of `&TextBuffer`.

Location: `crates/editor/src/glyph_buffer.rs`

**Changes:**
- Change parameter type from `&TextBuffer` to `&dyn BufferView`
- Use `view.styled_line(line)` to get line content
- For now, extract plain text from the single span (since `TextBuffer` produces unstyled content and the renderer doesn't yet handle per-span styles)
- Cursor position comes from `view.cursor_info()` instead of direct access

**Note:** This step intentionally doesn't change rendering behavior. We extract text from `StyledLine` spans and render with the existing uniform text color. Per-cell styling is a future chunk (renderer_styled_content).

### Step 10: Update `Renderer` to use `BufferView`

Modify the renderer to store and work with `Box<dyn BufferView>` instead of `Option<TextBuffer>`.

Location: `crates/editor/src/renderer.rs`

**Changes:**
- Replace `buffer: Option<TextBuffer>` with `buffer: Option<Box<dyn BufferView>>`
- Update `set_buffer()` to accept `impl BufferView + 'static` or take `Box<dyn BufferView>` directly
- The `update_glyph_buffer()` method calls `self.buffer.as_ref()?.styled_line(...)` and passes to the glyph buffer
- The `apply_mutation()` method can be simplified or removed — dirty tracking now lives in `BufferView::take_dirty()`

**Backward compatibility approach:** Create a `set_text_buffer()` helper that boxes a `TextBuffer` and calls the generic `set_buffer()`. This keeps the `main.rs` initialization simple.

### Step 11: Update call sites in `main.rs` and `context.rs`

Update all code that interacts with the renderer's buffer to work with the new abstraction.

Locations: `crates/editor/src/main.rs`, `crates/editor/src/context.rs`

**Key changes:**
- Buffer initialization: `TextBuffer::new()` → boxed → `renderer.set_buffer()`
- Direct `buffer_mut()` access patterns need review — for file editing, we still need mutable access to `TextBuffer`. Options:
  - Downcast `&dyn BufferView` to `&TextBuffer` when needed (ergonomic but couples to concrete type)
  - Keep a separate `TextBuffer` reference in `EditorContext` alongside the `BufferView` in the renderer (cleaner separation)

**Chosen approach:** The `EditorContext` owns the `TextBuffer` directly (for editing operations), and the renderer receives a shared reference via the `BufferView` trait. The renderer holds a reference to the context's buffer, not its own copy.

**Actually, revisiting:** The current design has `Renderer` own the buffer. For the BufferView abstraction to work cleanly with future terminal buffers (which the renderer would also display), the renderer should hold `Option<&dyn BufferView>` or `Option<Box<dyn BufferView>>`.

**Refined approach for this chunk:** Keep the renderer owning the buffer for now. Change the type to `Box<dyn BufferView>`. For editing, downcast to `&mut TextBuffer`. This is acceptable because this chunk only implements `BufferView` for `TextBuffer`. Future terminal integration will need to revisit ownership.

### Step 12: Integration testing

Verify the application still runs correctly with the refactored code.

**Manual testing:**
- Launch the editor
- Verify demo text renders identically to before the refactor
- Verify cursor renders correctly
- Verify typing updates the display
- Verify dirty region tracking works (partial updates)

**Automated tests:**
- Add an integration test that constructs a renderer with a TextBuffer-backed BufferView and verifies the glyph buffer is populated correctly

## Dependencies

- This chunk depends on chunks listed in `created_after` being complete (merged to trunk).
- No new external crates required — all types are defined in the workspace.

## Risks and Open Questions

1. **Downcast ergonomics.** The renderer currently needs mutable access to the buffer for editing. With `BufferView` being a trait object, we'd need to downcast to `TextBuffer` for mutation. This is acceptable for this chunk but worth noting. Alternative: add mutation methods to the trait (but this couples the trait to editing semantics that terminals don't have).

2. **String allocation in `styled_line()`.** Each call to `styled_line()` allocates a new `String` in the `Span`. For rendering, this happens every frame for visible lines. The investigation benchmarks showed this is acceptable (< 0.25% of frame budget), but it's worth monitoring. Future optimization: return `Cow<str>` or a borrowed view.

3. **Dirty tracking semantic change.** Currently, `TextBuffer` mutations return `DirtyLines` immediately. With `BufferView::take_dirty()`, the pattern changes to drain-all-then-render. The renderer already uses this pattern (`render_dirty()` checks dirty region), so this aligns well. But call sites that relied on the return value need to be audited.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION, not at planning time. -->
