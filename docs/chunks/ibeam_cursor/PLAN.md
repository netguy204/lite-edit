# Implementation Plan

## Approach

This chunk implements the standard macOS cursor behavior where an I-beam cursor appears when the mouse hovers over an editable text area. This is purely a visual affordance at the NSView layer — it signals to users that the area is editable, following macOS human interface guidelines.

**Strategy:**

The implementation uses the standard macOS cursor management API via `resetCursorRects`. When the window system needs to know what cursor to display, it calls `resetCursorRects` on each view. We override this method to call `addCursorRect:cursor:` with the view's bounds and `NSCursor.iBeam`.

This is a minimal change that follows the established patterns in `MetalView`:

1. **Add `resetCursorRects` override to MetalView** — Within the `define_class!` macro, add a method override that calls `addCursorRect:cursor:` with `self.bounds()` and `NSCursor::iBeam()`.

2. **Import NSCursor** — Add the necessary import from `objc2_app_kit`.

**Testing approach per TESTING_PHILOSOPHY.md:**

This change falls under the "Humble View Architecture" category — it's platform shell code that directly manipulates macOS cursor management and cannot be meaningfully unit-tested in isolation. Per the testing philosophy:

> "GPU rendering, macOS window management, and Metal pipeline setup involve visual output and platform state that can't be meaningfully asserted in a unit test. For these, write the implementation first, verify visually..."

Visual verification will confirm:
- I-beam cursor appears when mouse enters the view
- Arrow cursor returns when mouse exits the view
- I-beam is maintained during mouse movement and drag operations
- No functional regressions in click, drag, or scroll behavior

## Subsystem Considerations

No subsystems are relevant to this change. The work is isolated to a single NSView method override with no cross-cutting concerns.

## Sequence

### Step 1: Add NSCursor import to metal_view.rs

Add `NSCursor` to the imports from `objc2_app_kit`.

**Location:** `crates/editor/src/metal_view.rs`

**Details:**
- Find the existing `use objc2_app_kit::...` line
- Add `NSCursor` to the imported types

### Step 2: Override resetCursorRects in MetalView

Add the `resetCursorRects` method override within the `define_class!` macro.

**Location:** `crates/editor/src/metal_view.rs`, inside the `impl MetalView` block within `define_class!`

**Details:**
- Add a new method with `#[unsafe(method(resetCursorRects))]` attribute
- The method should:
  1. Call `discardCursorRects()` on self to clear any existing cursor rects
  2. Get the view's bounds via `self.bounds()`
  3. Get the I-beam cursor via `NSCursor::iBeam()`
  4. Call `self.addCursorRect:cursor:` with the bounds and I-beam cursor

**Code pattern:**
```rust
// Chunk: docs/chunks/ibeam_cursor - I-beam cursor over editable area
/// Sets up cursor rects to display I-beam cursor over the editable area
#[unsafe(method(resetCursorRects))]
fn __reset_cursor_rects(&self) {
    // Clear existing cursor rects
    self.discardCursorRects();
    // Add I-beam cursor for the entire view bounds
    unsafe {
        self.addCursorRect_cursor(self.bounds(), &NSCursor::iBeam());
    }
}
```

**Note:** The exact method name for `addCursorRect:cursor:` in objc2 bindings may be `addCursorRect_cursor` or similar — verify against objc2-app-kit API.

### Step 3: Visual verification

Build and run the editor to verify:

1. **I-beam on entry:** Mouse cursor changes to I-beam when entering the MetalView bounds
2. **Arrow on exit:** Mouse cursor reverts to arrow when leaving the MetalView bounds
3. **I-beam during movement:** I-beam is maintained while moving within the view
4. **I-beam during drag:** I-beam is maintained during mouse drag operations
5. **No regressions:** Click, drag, and scroll behaviors remain functional

**Verification command:** `cargo run` in `crates/editor`

## Dependencies

**No blocking dependencies.** The change uses:

- `objc2-app-kit` (already in `Cargo.toml`) for `NSCursor` and `NSView` methods
- The existing `MetalView` class infrastructure from `metal_surface` chunk
- The `define_class!` pattern already established for NSView overrides

The chunks listed in `created_after` (`mouse_drag_selection`, `shift_arrow_selection`, `text_selection_rendering`, `viewport_scrolling`) are ordering constraints for delivery, not implementation dependencies — this chunk doesn't depend on their code.

## Risks and Open Questions

1. **objc2-app-kit API for NSCursor:** The exact method names in the objc2 Rust bindings may differ from the Objective-C API names. For example, `addCursorRect:cursor:` may be exposed as `addCursorRect_cursor` or similar. Verify the exact API during implementation.

2. **Cursor rects and bounds updates:** When the view resizes, macOS should automatically invalidate cursor rects and call `resetCursorRects` again. However, if the cursor doesn't update correctly during window resizing, we may need to explicitly call `invalidateCursorRectsForView:` in `setFrameSize:`. This is unlikely to be needed but worth monitoring.

3. **Cursor during scroll:** The I-beam cursor should persist during scroll wheel events since we're using cursor rects (not tracking areas). Verify this during testing.

4. **Layer-backed view considerations:** Since MetalView uses `wantsLayer = true`, confirm that cursor rect behavior is unaffected by layer backing. The macOS documentation doesn't indicate any issues with layer-backed views and cursor rects.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->