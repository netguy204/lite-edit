# Implementation Plan

## Approach

This chunk fixes slow mouse wheel scrolling by distinguishing between trackpad (precise, pixel-level) and mouse wheel (line-based) scroll deltas in `convert_scroll_event()`. The fix is localized to `metal_view.rs`, keeping all downstream scroll handlers device-agnostic.

**Strategy:**

The root cause is that `convert_scroll_event()` uses the raw `scrollingDeltaX()`/`scrollingDeltaY()` values for all input devices. These methods return:
- **Trackpad**: Precise pixel deltas (e.g., 15.3, -8.7) — already in the units scroll consumers expect
- **Mouse wheel**: Line-based deltas (e.g., 1.0, -3.0) — need conversion to pixel deltas

The fix checks `NSEvent::hasPreciseScrollingDeltas()`:
- When `true` (trackpad): Use deltas as-is (current behavior preserved)
- When `false` (mouse wheel): Multiply deltas by a line height constant to convert line-counts to pixels

This is the same approach used by other macOS editors (VS Code, Sublime Text, etc.) and follows [Apple's documentation](https://developer.apple.com/documentation/appkit/nsevent/hasprecisescrollingdeltas) which explicitly describes this pattern.

**Line height constant:**

The `MetalView` doesn't have access to font metrics. Rather than adding complexity to pass metrics through, we use a constant `DEFAULT_LINE_HEIGHT_PX = 20.0`. This matches the project's typical line height (tests use 16.0, production fonts are slightly larger) and provides ~1 line of scroll per mouse wheel tick, matching typical editor behavior.

**Testing approach per TESTING_PHILOSOPHY.md:**

The `convert_scroll_event()` method is platform code that operates on `NSEvent` objects, which cannot be easily constructed in tests. Per the testing philosophy, we:
1. Keep the logic simple and self-evidently correct (a single conditional with multiplication)
2. Verify behavior through manual testing with both mouse and trackpad
3. Document the visual verification in the plan

This follows the "humble view" principle — the platform integration is thin, and the scroll arithmetic is already well-tested in `RowScroller`.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport scroll subsystem. The `ScrollDelta` type and scroll consumers (`set_scroll_offset_px`, etc.) are part of this subsystem. This chunk does not modify subsystem code — it only ensures that `ScrollDelta.dy` values arriving from mouse wheel events are in the correct units (pixels) that the subsystem expects.

No deviations discovered. The change is fully compatible with the subsystem's contract that scroll deltas are in pixels.

## Sequence

### Step 1: Add hasPreciseScrollingDeltas check in convert_scroll_event

Modify `convert_scroll_event()` in `crates/editor/src/metal_view.rs` to:

1. Call `event.hasPreciseScrollingDeltas()` to distinguish input device type
2. When `false` (mouse wheel), multiply `dx` and `dy` by `DEFAULT_LINE_HEIGHT_PX` (20.0)
3. When `true` (trackpad), use deltas unchanged (current behavior)

Add a constant at the top of the impl block:
```rust
/// Default line height for mouse wheel scroll conversion.
/// Mouse wheel events report line-based deltas; we convert to pixels
/// using this constant. Matches typical editor line heights.
const DEFAULT_LINE_HEIGHT_PX: f64 = 20.0;
```

Add a chunk backreference to the modified method:
```rust
// Chunk: docs/chunks/scroll_wheel_speed - Mouse wheel vs trackpad delta handling
```

**Location:** `crates/editor/src/metal_view.rs#MetalView::convert_scroll_event`

### Step 2: Update GOAL.md code_paths

Add `crates/editor/src/metal_view.rs` to the `code_paths` field in GOAL.md.

**Location:** `docs/chunks/scroll_wheel_speed/GOAL.md`

### Step 3: Visual verification

Verify the fix manually with both input devices:

1. **Mouse wheel test:**
   - Open the editor with a long file (100+ lines)
   - Scroll using a mouse wheel
   - Verify: Each tick scrolls approximately one line (not 1-3 pixels as before)
   - Verify: Scrolling feels responsive, similar to VS Code/Sublime Text

2. **Trackpad test:**
   - Scroll using trackpad with natural scrolling enabled
   - Verify: Smooth, pixel-precise scrolling unchanged from previous behavior
   - Verify: Momentum scrolling works naturally

3. **Mixed usage test:**
   - Alternate between mouse wheel and trackpad
   - Verify: Both devices work correctly without confusion or state leakage

4. **Selector/picker test:**
   - Open the file picker (Cmd-P)
   - Scroll the results list with mouse wheel and trackpad
   - Verify: Both input devices work correctly

## Risks and Open Questions

1. **Line height mismatch**: Using a fixed 20.0px constant may feel slightly off if the user has configured a different font size. This is an acceptable tradeoff — the scroll experience will be "approximately right" rather than pixel-perfect, and 20px is a reasonable default. A future enhancement could pass actual line height to the view layer if needed.

2. **Mouse DPI variance**: Different mice have different scroll wheel sensitivities at the hardware level. macOS normalizes this somewhat, but very high-DPI mice might scroll faster. The 1-line-per-tick target is a reasonable baseline.

3. **Horizontal scrolling**: The fix also applies to `dx` for horizontal scrolling. This is correct behavior — horizontal scrolling with a mouse tilt-wheel should also be scaled. However, most text editors don't support horizontal scrolling, so this code path may not be exercised.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->