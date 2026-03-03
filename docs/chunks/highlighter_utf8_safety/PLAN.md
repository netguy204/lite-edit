<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The root cause is that `line_byte_range()` computes the line end as `self.line_offsets[line_idx + 1] - 1`, which subtracts 1 from the byte offset of the newline character. For multi-byte UTF-8 characters like `╔` (3 bytes: `\xe2\x95\x94`), subtracting 1 from the byte offset after such a character lands inside the multi-byte sequence, causing a panic when slicing.

**Strategy: Safe Character Boundary Adjustment**

Rather than blindly subtracting 1, we'll:
1. Create a helper function `safe_char_boundary(source, pos, direction)` that adjusts a byte position to the nearest valid character boundary
2. Use `str::is_char_boundary()` to validate positions before slicing
3. Apply this helper to all ~13 locations in `highlighter.rs` that perform `&self.source[start..end]` slicing

**Key insight:** The problem isn't just `line_byte_range()`. After edits, tree-sitter capture offsets can also become stale and point into the middle of multi-byte characters. The fix must handle both cases:
- **Line boundaries:** Ensure `line_byte_range()` always returns valid char boundaries
- **Capture offsets:** Clamp capture byte offsets to valid boundaries before slicing

**TDD approach:** Per TESTING_PHILOSOPHY.md, we'll write failing tests first that trigger the panic with multi-byte UTF-8 content, then implement the fix to make them pass.

## Sequence

### Step 1: Write failing regression tests

Create tests that reproduce the panic with multi-byte UTF-8 characters:

1. Test file containing box-drawing characters on multiple lines
2. Test editing a file that adds/removes characters before a multi-byte character
3. Test highlighting a line that ends with a multi-byte character (no trailing newline)

These tests must fail initially (panic) to verify we're testing the right bug.

Location: `crates/syntax/src/highlighter.rs` in the `#[cfg(test)]` module

Test cases:
```rust
#[test]
fn test_highlight_line_with_box_drawing_chars() {
    // Source with box-drawing characters that are 3 bytes each
    let source = "╔══════╗\n║ test ║\n╚══════╝";
    let hl = make_rust_highlighter(source).unwrap();
    // This should NOT panic
    let _ = hl.highlight_line(0);
    let _ = hl.highlight_line(1);
    let _ = hl.highlight_line(2);
}

#[test]
fn test_highlight_line_with_emoji() {
    let source = "fn main() { /* 🦀 */ }";
    let hl = make_rust_highlighter(source).unwrap();
    let _ = hl.highlight_line(0);
}

#[test]
fn test_edit_near_multibyte_char() {
    let source = "status: IMPLEMENTING";
    let mut hl = make_rust_highlighter(source).unwrap();
    // Replace "IMPLEMENTING" with "F" (shorter), then highlight
    let event = crate::edit::delete_event(source, 0, 8, "IMPLEMENTING".len());
    let new_source = "status: F";
    hl.edit(event, new_source);
    // If the file has multi-byte chars elsewhere, offsets shift
}
```

### Step 2: Create `safe_char_boundary` helper function

Add a private helper function that adjusts a byte position to the nearest valid character boundary:

```rust
// Chunk: docs/chunks/highlighter_utf8_safety - UTF-8 safe byte offset adjustment
/// Adjusts a byte position to the nearest valid character boundary.
///
/// When `round_down` is true, moves backward to the start of the character
/// containing `pos`. When false, moves forward to the next character boundary.
///
/// Returns `pos` unchanged if it's already a valid boundary.
fn safe_char_boundary(source: &str, pos: usize, round_down: bool) -> usize {
    if pos >= source.len() {
        return source.len();
    }
    if source.is_char_boundary(pos) {
        return pos;
    }
    if round_down {
        // Search backward for valid boundary
        let mut adjusted = pos;
        while adjusted > 0 && !source.is_char_boundary(adjusted) {
            adjusted -= 1;
        }
        adjusted
    } else {
        // Search forward for valid boundary
        let mut adjusted = pos;
        while adjusted < source.len() && !source.is_char_boundary(adjusted) {
            adjusted += 1;
        }
        adjusted
    }
}
```

Location: `crates/syntax/src/highlighter.rs`, near `build_line_offsets()`

### Step 3: Fix `line_byte_range()` to return valid char boundaries

Update `line_byte_range()` to ensure the returned end position is a valid character boundary:

Current code (line 1307-1321):
```rust
fn line_byte_range(&self, line_idx: usize) -> Option<(usize, usize)> {
    if line_idx >= self.line_offsets.len() {
        return None;
    }
    let start = self.line_offsets[line_idx];
    let end = if line_idx + 1 < self.line_offsets.len() {
        self.line_offsets[line_idx + 1] - 1  // BUG: May land inside multi-byte char
    } else {
        self.source.len()
    };
    Some((start, end))
}
```

Fixed code:
```rust
fn line_byte_range(&self, line_idx: usize) -> Option<(usize, usize)> {
    if line_idx >= self.line_offsets.len() {
        return None;
    }
    let start = self.line_offsets[line_idx];
    let end = if line_idx + 1 < self.line_offsets.len() {
        // Subtract 1 to exclude the newline, then adjust to valid char boundary
        let raw_end = self.line_offsets[line_idx + 1].saturating_sub(1);
        safe_char_boundary(&self.source, raw_end, true)  // Round down
    } else {
        self.source.len()
    };
    // Ensure start is also valid (should always be, but defensive)
    let start = safe_char_boundary(&self.source, start, true);
    Some((start, end.max(start)))  // Ensure end >= start
}
```

### Step 4: Fix `build_line_from_captures()` slicing sites

Update all `&self.source[start..end]` slicing in `build_line_from_captures()` (line 964-1135) to use safe boundaries.

Key locations to fix:
- Line 970: `let line_text = &self.source[line_start..line_end];`
- Line 1089: `let tail = &self.source[covered_until..actual_end];`
- Line 1100: `let gap_text = &self.source[covered_until..actual_start];`
- Line 1107: `let capture_text = &self.source[actual_start..actual_end];`
- Line 1121: `let remaining = &self.source[covered_until..line_end];`

For capture offsets, clamp to valid boundaries before slicing:
```rust
let actual_start = safe_char_boundary(&self.source, cap_start.max(line_start), true);
let actual_end = safe_char_boundary(&self.source, cap_end.min(line_end), false);
```

### Step 5: Fix `build_line_from_captures_impl()` slicing sites

Apply the same fix to `build_line_from_captures_impl()` (line 1161-1301):

- Line 1260: `let tail = &self.source[covered_until..actual_end];`
- Line 1270: `let gap_text = &self.source[covered_until..actual_start];`
- Line 1276: `let capture_text = &self.source[actual_start..actual_end];`
- Line 1289: `let remaining = &self.source[covered_until..line_end];`

Same pattern: ensure `covered_until`, `actual_start`, and `actual_end` are all valid char boundaries before slicing.

### Step 6: Fix injection region slicing

Fix the slicing in injection-related methods:

- Line 544: `let lang_text = &self.source[capture.node.start_byte()..capture.node.end_byte()];`
- Line 665: `let line_text = &self.source[line_start..line_end];` (in `highlight_single_line`)
- Line 881: `let region_source = &self.source[region.byte_range.clone()];`
- Line 938: `let region_source = &self.source[region.byte_range.clone()];`

For `region.byte_range`, validate and clamp:
```rust
let safe_start = safe_char_boundary(&self.source, region.byte_range.start, true);
let safe_end = safe_char_boundary(&self.source, region.byte_range.end, false);
let region_source = &self.source[safe_start..safe_end];
```

### Step 7: Verify tests pass

Run the test suite to confirm:
1. The new UTF-8 regression tests pass (no panics)
2. All existing tests still pass (no regressions in highlighting correctness)

```bash
cargo test -p syntax
```

### Step 8: Add additional edge case tests

Add tests for:
- CJK characters (Chinese/Japanese/Korean - typically 3 bytes UTF-8)
- Mixed ASCII and multi-byte content on the same line
- Lines consisting entirely of multi-byte characters
- Edit operations that change content length near multi-byte characters
- Empty lines adjacent to lines with multi-byte characters

## Risks and Open Questions

1. **Performance impact:** The `safe_char_boundary` helper adds a small amount of overhead, but since it typically returns immediately when `is_char_boundary()` is true, and only loops in pathological cases, the impact should be negligible.

2. **Capture offset drift:** When edits change content length, tree-sitter captures may reference stale byte offsets. The current fix clamps these to valid boundaries, which may cause visual misalignment of highlighting until the tree is re-parsed. This is acceptable (graceful degradation vs panic) but worth noting.

3. **Line offset index accuracy:** The `line_offsets` index is built by scanning for `\n` bytes, which is correct for UTF-8 since `\n` is always a single byte and cannot appear inside multi-byte sequences. This is safe.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
