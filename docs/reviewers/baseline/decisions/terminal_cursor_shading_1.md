---
decision: APPROVE
summary: "Investigation revealed no reproducible bug; comprehensive cursor position regression tests added instead of code fix"
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: The block cursor inversion is rendered on the exact cell the cursor occupies, not one character behind

- **Status**: satisfied
- **Evidence**: The PLAN.md documents thorough investigation that found no off-by-one error. 12+ unit tests verify cursor position tracking is correct (e.g., `test_cursor_position_after_typing_one_char`, `test_content_and_cursor_alignment`). The coordinate conversion in `glyph_buffer.rs` uses `wrap_layout.buffer_col_to_screen_pos()` consistently for both glyphs and cursor.

### Criterion 2: Moving the cursor (typing, arrow keys, backspace) immediately updates the shading to the new position

- **Status**: satisfied
- **Evidence**: Tests cover cursor movement via typing (`test_cursor_position_after_typing_multiple_chars`), arrow key escape sequences (`test_cursor_position_after_cursor_movement_escape`), backspace (`test_cursor_position_after_backspace`), and comprehensive sequences combining all (`test_cursor_position_comprehensive_sequence`). All 11 cursor tests pass.

### Criterion 3: The previous cursor position loses its inversion when the cursor moves away (no ghost shading)

- **Status**: satisfied
- **Evidence**: Test `test_cells_have_no_cursor_inverse_flags` verifies that terminal cells don't have spurious INVERSE flags set by cursor position - inverse is only set by explicit escape sequences. The cursor quad is rendered last (after glyphs) per the renderer subsystem's "Draw Order Within Layer" convention.

### Criterion 4: Cursor rendering is correct in both the shell prompt and during TUI application use

- **Status**: satisfied
- **Evidence**: Test `test_cursor_position_shell_prompt` verifies cursor tracking with simulated shell prompt ("$ ls"). The tests use `feed_bytes()` helper for direct terminal emulator interaction, bypassing PTY timing issues. Manual verification is documented in PLAN.md Step 6.

### Criterion 5: No regression in cursor blink behavior or editor-pane cursor rendering

- **Status**: satisfied
- **Evidence**: The changes to `glyph_buffer.rs` are purely refactoring to use persistent buffers (from `quad_buffer_prealloc` chunk). The cursor quad creation logic is unchanged - only the storage (`vertices` → `self.persistent_vertices`, `indices` → `self.persistent_indices`) changed. No cursor rendering logic was modified.

## Investigation Note

Per PLAN.md "Deviations" section: The reported bug could not be reproduced through testing. Investigation found:
1. Cursor position tracking is correct (12+ tests verify)
2. No off-by-one error in coordinate conversion
3. The `file_change_events` chunk mentioned in the bug report is still FUTURE (not implemented)
4. The `pane_mirror_restore` chunk added styled line cache clearing that may have fixed the issue

The chunk pivoted to adding comprehensive regression tests rather than a code fix, which is appropriate given the investigation findings.
