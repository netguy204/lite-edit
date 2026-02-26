<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The Ctrl+A/E emacs keybindings regressed after the `unicode_ime_input` and
`input_keystroke_regression` work that changed how keystrokes are routed through
macOS's text input system. The root cause is that **Ctrl-modified keys now flow
through `interpretKeyEvents:`** which translates them into Cocoa selectors and
calls `doCommandBySelector:`.

**Current behavior analysis:**

1. **Cmd-modified keys** (like Cmd+Left/Right) bypass `interpretKeyEvents:` and go
   directly through `convert_key_event()`, which preserves full key+modifiers. This
   is why Cmd+Left/Right work.

2. **Ctrl-modified keys** (like Ctrl+A/E/F/B/N/P/D/K/V) fall through to
   `interpretKeyEvents:`, which translates them to Cocoa selectors:
   - Ctrl+A → `moveToBeginningOfParagraph:` (not `moveToBeginningOfLine:` as expected)
   - Ctrl+E → `moveToEndOfParagraph:` (not `moveToEndOfLine:` as expected)
   - Ctrl+F → `moveForward:`
   - Ctrl+B → `moveBackward:`
   - Ctrl+N → `moveDown:`
   - Ctrl+P → `moveUp:`
   - Ctrl+D → `deleteForward:`
   - Ctrl+K → `deleteToEndOfParagraph:`
   - Ctrl+V → `pageDown:` (this one works because it's in the match table)

3. The `doCommandBySelector:` match table in `metal_view.rs` only handles a subset
   of these selectors. Missing selectors fall to `_ => None` and are silently dropped.

**Fix strategy:**

The cleanest fix is to **add Ctrl-modified keys to the bypass path** in `__key_down`,
routing them directly through `convert_key_event()` just like Cmd-modified keys. This:
- Restores the pre-IME behavior for emacs bindings
- Preserves IME support (which doesn't need Ctrl+letter keys)
- Keeps the existing `resolve_command()` logic in `buffer_target.rs` working
- Avoids the complexity of mapping every possible Cocoa selector variant

This approach mirrors how the existing code already handles Cmd-modified keys.

**Alternative considered**: Expand the `doCommandBySelector:` match table to handle
all emacs-related selectors. Rejected because:
- Cocoa may send different selectors than expected (paragraph vs line)
- Some selectors like `moveForward:` conflict with terminal applications
- Would duplicate command resolution logic between `doCommandBySelector:` and `resolve_command()`
- Harder to maintain parity between the two code paths

## Sequence

### Step 1: Add diagnostic logging to identify exact selector names

Before fixing, verify the exact selectors Cocoa sends for each Ctrl+key combination.
This confirms our hypothesis about which selectors to expect.

Add temporary logging in `__do_command_by_selector` that prints the selector name
for any unhandled selector. Run the editor and press Ctrl+A, Ctrl+E, etc. to see
what macOS actually sends.

Location: `crates/editor/src/metal_view.rs#MetalView::__do_command_by_selector`

### Step 2: Update `__key_down` to bypass interpretKeyEvents for Ctrl-modified keys

Modify the bypass condition in `__key_down` to include `has_control` alongside
`has_command`. This routes Ctrl+letter combinations directly through
`convert_key_event()` → `send_key()`, matching the pre-IME behavior.

The updated condition should be:
```rust
let has_command = flags.contains(NSEventModifierFlags::Command);
let has_control = flags.contains(NSEventModifierFlags::Control);

// Bypass the text input system for command shortcuts, control shortcuts, and function keys
if has_command || has_control || is_escape || is_function_key {
    // ... existing bypass logic ...
}
```

This preserves:
- IME support for regular typing (no Ctrl held)
- Cmd+key shortcuts (already bypassed)
- Function keys and Escape (already bypassed)

Location: `crates/editor/src/metal_view.rs#MetalView::__key_down`

### Step 3: Verify existing resolve_command handles all emacs bindings

Confirm that `resolve_command()` in `buffer_target.rs` already has the correct
mappings for all emacs keybindings:

- Ctrl+A → `MoveToLineStart` ✓ (line 226)
- Ctrl+E → `MoveToLineEnd` ✓ (line 229)
- Ctrl+F → `MoveRight` ✓ (line 244)
- Ctrl+B → `MoveLeft` ✓ (line 247)
- Ctrl+N → `MoveDown` ✓ (line 254)
- Ctrl+P → `MoveUp` ✓ (line 257)
- Ctrl+D → `DeleteForward` ✓ (line 251)
- Ctrl+K → `DeleteToLineEnd` ✓ (line 233)
- Ctrl+V → `PageDown` ✓ (line 241)
- Shift+Ctrl+A → `SelectToLineStart` ✓ (line 173)
- Shift+Ctrl+E → `SelectToLineEnd` ✓ (line 178)

All mappings are already present. No changes needed to `resolve_command()`.

Location: `crates/editor/src/buffer_target.rs#resolve_command`

### Step 4: Manual testing of all emacs keybindings

Build and run the editor. Test each binding with a buffer containing multiple lines:

**Movement (cursor should move, no selection):**
- [ ] Ctrl+A → cursor to beginning of line
- [ ] Ctrl+E → cursor to end of line
- [ ] Ctrl+F → cursor right one character
- [ ] Ctrl+B → cursor left one character
- [ ] Ctrl+N → cursor down one line
- [ ] Ctrl+P → cursor up one line
- [ ] Ctrl+V → page down

**Editing:**
- [ ] Ctrl+D → delete character under cursor
- [ ] Ctrl+K → delete from cursor to end of line

**Selection (with Shift):**
- [ ] Shift+Ctrl+A → select to beginning of line
- [ ] Shift+Ctrl+E → select to end of line

**Non-regression:**
- [ ] Cmd+Left → cursor to beginning of line (still works)
- [ ] Cmd+Right → cursor to end of line (still works)
- [ ] Regular typing → characters inserted (IME path still works)
- [ ] Japanese IME → composition works (if available to test)

Location: Manual testing

### Step 5: Verify no system beep on any emacs keybinding

Run the editor with audio enabled. Press each Ctrl+key combination:
- Ctrl+A, E, F, B, N, P, D, K, V

None should produce a system beep. The previous behavior (before this fix) may have
caused beeps on some of these due to unhandled selectors in `doCommandBySelector:`.

Location: Manual testing

### Step 6: Clean up diagnostic logging

Remove any temporary logging added in Step 1.

Location: `crates/editor/src/metal_view.rs`

### Step 7: Add backreference comment at the fix site

Add a chunk backreference comment at the modified code site:

```rust
// Chunk: docs/chunks/emacs_line_nav - Route Ctrl-modified keys through bypass path
```

Location: `crates/editor/src/metal_view.rs#MetalView::__key_down`

### Step 8: Update code_paths in GOAL.md

Update the chunk's GOAL.md frontmatter with the files touched:
```yaml
code_paths:
  - crates/editor/src/metal_view.rs
```

Location: `docs/chunks/emacs_line_nav/GOAL.md`

## Risks and Open Questions

- **Risk: Breaking IME for edge cases involving Ctrl**: Some IME configurations might
  use Ctrl+key combinations. This is rare on macOS (most use Option or no modifiers).
  Mitigation: Test with Japanese Hiragana IME if available.

- **Question: Should Ctrl+Shift combinations also bypass?** Currently Shift+Ctrl+A/E
  are used for selection extension. These should already work with the bypass since
  Ctrl is set. The `convert_key_event()` path correctly captures the Shift modifier.

- **Question: What about Option+Ctrl combinations?** These are not standard emacs
  bindings and can continue through the IME path. The fix only bypasses when Control
  alone (or with Shift/Command) is held.

- **Low risk: Terminal tabs using different input path**: Terminal input uses
  `InputEncoder` from KeyEvents, not the IME text input path. Terminal emacs
  bindings should work independently of this fix.

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
-->