---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/buffer_target.rs
code_references:
  - ref: crates/editor/src/buffer_target.rs#resolve_command
    implements: "Ctrl+D/N/P Emacs keybinding match arms mapping to DeleteForward, MoveDown, MoveUp commands"
  - ref: crates/editor/src/buffer_target.rs#test_ctrl_d_resolves_to_delete_forward
    implements: "Test verifying Ctrl+D maps to DeleteForward command"
  - ref: crates/editor/src/buffer_target.rs#test_ctrl_n_resolves_to_move_down
    implements: "Test verifying Ctrl+N maps to MoveDown command"
  - ref: crates/editor/src/buffer_target.rs#test_ctrl_p_resolves_to_move_up
    implements: "Test verifying Ctrl+P maps to MoveUp command"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- fallback_glyph_metrics
---

# Chunk Goal

## Minor Goal

Add the remaining standard Emacs cursor keybindings to complete the set alongside the existing Ctrl+A, Ctrl+E, Ctrl+F, Ctrl+B, and Ctrl+K bindings.

All three commands (`DeleteForward`, `MoveDown`, `MoveUp`) already exist and are bound to their respective non-Emacs keys (Delete, Down, Up). This chunk adds Ctrl+D/N/P as additional triggers for the same commands in `resolve_command()`.

## Success Criteria

- Pressing Ctrl+D in a buffer deletes the character under the cursor (same behavior as the Delete key).
- Pressing Ctrl+N moves the cursor down one line (same behavior as the Down arrow).
- Pressing Ctrl+P moves the cursor up one line (same behavior as the Up arrow).
- All existing key bindings (Delete, Down, Up arrows) continue to work unchanged.
- The bindings are added in `resolve_command()` in `crates/editor/src/buffer_target.rs` alongside the other Ctrl+letter Emacs bindings.