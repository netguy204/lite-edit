<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Alt+Backspace and Alt+D do not work in the editor or terminal because macOS's
text input system intercepts Option-modified keys before they reach our key
handler. When Option+D is pressed, `interpretKeyEvents:` invokes macOS character
composition which produces `∂` (U+00F0), and that composed character is sent to
`insertText:` instead of the key event reaching `convert_key_event()`.

**Root cause analysis:**

The `__key_down` method in `metal_view.rs` decides which keys bypass the macOS
text input system (`interpretKeyEvents:`) and which flow through it:

```rust
// Current bypass condition (line 325):
if has_command || has_control || is_escape || is_function_key {
    // ... direct key handling via convert_key_event()
}
```

- **Cmd-modified keys**: Bypassed → work correctly
- **Ctrl-modified keys**: Bypassed (added by `emacs_line_nav` chunk) → work correctly
- **Option-modified keys**: NOT bypassed → flow through `interpretKeyEvents:`
  → macOS composes Unicode characters like `∂` (Opt+D), `∫` (Opt+B), etc.
  → composed character sent to `insertText:` as literal text
  → `convert_key_event()` never sees the key, so Alt+D/Alt+Backspace don't work

**The fix:**

Add Option (NSEventModifierFlags::Option) to the bypass condition, alongside
Command and Control. This routes Option-modified keys directly through
`convert_key_event()` where:

1. `charactersIgnoringModifiers()` is already used when Option is held (line 1195)
2. The Option modifier is captured correctly (line 1100)
3. The KeyEvent reaches `InputEncoder` for terminals or `resolve_command()` for buffers

The downstream handling is already correct:
- `InputEncoder::encode_special_key()` handles Alt+Backspace → `\x1b\x7f` ✓
- `resolve_command()` handles Alt+Backspace → `DeleteBackwardWord` ✓
- `resolve_command()` handles Alt+D → `DeleteForwardWord` ✓

**Why this is safe for IME:**

IME composition (CJK input methods, dead keys) uses the base key without
modifiers or with Shift only. Option-modified keys are NOT used by IME:
- Japanese Hiragana: Types base characters, spacebar for conversion
- Chinese Pinyin: Types base letters, numbers for tone selection
- Dead keys: Option+E, Option+U, etc. produce accent marks for composition

However, dead key composition WILL break with this change. For example, typing
Option+E followed by 'a' currently produces 'á'. With the bypass, Option+E will
be sent as a key event instead of starting composition.

**Tradeoff decision**: The goal prioritizes making Alt+Backspace and Alt+D work
in both editor and terminal contexts over preserving dead key composition. Dead
key users are a small minority compared to users expecting standard Alt+Backspace
word deletion. A future chunk could add a setting to toggle this behavior.

## Sequence

### Step 1: Update `__key_down` to bypass interpretKeyEvents for Option-modified keys

Modify the bypass condition in `__key_down` to include `has_option` alongside
`has_command` and `has_control`. This routes Option+key combinations directly
through `convert_key_event()` → `send_key()`.

Change the bypass condition from:
```rust
let has_command = flags.contains(NSEventModifierFlags::Command);
let has_control = flags.contains(NSEventModifierFlags::Control);

if has_command || has_control || is_escape || is_function_key {
```

To:
```rust
let has_command = flags.contains(NSEventModifierFlags::Command);
let has_control = flags.contains(NSEventModifierFlags::Control);
let has_option = flags.contains(NSEventModifierFlags::Option);

if has_command || has_control || has_option || is_escape || is_function_key {
```

Also update the comment block to mention Option-modified keys:
```rust
// Check if this is a "bypass" key that should skip the text input system.
// These include:
// - Keys with Command modifier (shortcuts like Cmd+S, Cmd+Q)
// - Keys with Control modifier (Emacs bindings like Ctrl+A, Ctrl+E)
// - Keys with Option modifier (word operations like Alt+Backspace, Alt+D)
// - Escape key (cancel operations, exit modes)
// - Function keys (F1-F12)
```

And update the comment in the bypass block:
```rust
// Bypass the text input system for command shortcuts, control shortcuts,
// option shortcuts, and function keys.
```

Location: `crates/editor/src/metal_view.rs#MetalView::__key_down` (lines 298-346)

### Step 2: Add backreference comment

Add a chunk backreference comment at the fix site, documenting that this
modification was made for the Option modifier bypass:

```rust
// Chunk: docs/chunks/terminal_word_delete - Route Option-modified keys through bypass path
```

This should be added alongside the existing backreference:
```rust
// Chunk: docs/chunks/emacs_line_nav - Route Ctrl-modified keys through bypass path
```

Location: `crates/editor/src/metal_view.rs#MetalView::__key_down`

### Step 3: Verify resolve_command handles Alt+D and Alt+Backspace

Confirm that `resolve_command()` in `buffer_target.rs` has the correct mappings:

- Alt+Backspace → `DeleteBackwardWord` (from `delete_backward_word` chunk)
- Alt+D → `DeleteForwardWord` (from `word_forward_delete` chunk)

These should already exist. No changes expected.

Location: `crates/editor/src/buffer_target.rs#resolve_command`

### Step 4: Verify InputEncoder handles Alt+Backspace and Alt+D

Confirm that `InputEncoder` in `crates/terminal/src/input_encoder.rs` correctly
encodes:

- Alt+Backspace → `\x1b\x7f` (ESC + DEL)
- Alt+D → `\x1b\x64` (ESC + 'd')

These should already exist from `terminal_alt_backspace` and related chunks.
No changes expected.

Location: `crates/terminal/src/input_encoder.rs#InputEncoder::encode_special_key`

### Step 5: Manual testing - Terminal path

Build and run the editor. Open a terminal tab and test:

**Alt+Backspace in terminal:**
1. Start a shell (bash or zsh)
2. Type: `echo hello world`
3. Press Alt+Backspace
4. Expected: `world` is deleted, leaving `echo hello `

**Alt+D in terminal:**
1. Type: `echo hello world`
2. Move cursor to before `hello` (Ctrl+A, then right arrow 5 times)
3. Press Alt+D
4. Expected: `hello` is deleted, leaving `echo  world`

### Step 6: Manual testing - Editor buffer path

Open a file buffer and test:

**Alt+Backspace in buffer:**
1. Type: `hello world`
2. Place cursor at end of line
3. Press Alt+Backspace
4. Expected: `world` is deleted, leaving `hello `

**Alt+D in buffer:**
1. Type: `hello world`
2. Place cursor at start of `world`
3. Press Alt+D
4. Expected: `world` is deleted, leaving `hello `

### Step 7: Non-regression testing

Verify that these behaviors still work:

- Regular typing (no modifiers) inserts characters
- Cmd+key shortcuts (Cmd+S, Cmd+Q, Cmd+P) still work
- Ctrl+key emacs bindings (Ctrl+A, Ctrl+E, Ctrl+K) still work
- Alt+Arrow navigation (Alt+Left, Alt+Right) still work for word movement
- Function keys (F1-F12) still work

### Step 8: Update code_paths in GOAL.md

Update the chunk's GOAL.md frontmatter with the file touched:

```yaml
code_paths:
  - crates/editor/src/metal_view.rs
```

Location: `docs/chunks/terminal_word_delete/GOAL.md`

## Risks and Open Questions

- **Dead key composition will break**: Users who rely on Option+E, Option+U,
  etc. to type accented characters (á, ü, etc.) will no longer be able to do
  so. This is an intentional tradeoff - the goal explicitly states this is
  acceptable in favor of making Alt+Backspace/Alt+D work.

- **Some Option+key Unicode shortcuts will break**: macOS users who type special
  characters via Option+key (e.g., Option+G → ©, Option+2 → ™) will no longer
  get those characters. This is the same tradeoff as dead keys.

- **Terminal applications expecting literal Option+key characters**: Some
  terminal applications might expect to receive the composed Unicode character
  from Option+key. With this change, they'll receive ESC+key escape sequences
  instead. This matches standard terminal emulator behavior (iTerm2, Terminal.app
  with "Use Option as Meta key" enabled).

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