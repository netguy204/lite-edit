<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The regression manifests as: typing printable characters produces an NSBeep (system
chime) and the characters are silently discarded, while hotkeys (Cmd+Q, Cmd+P, etc.)
continue to work.

**Hypothesis**: The `unicode_ime_input` chunk introduced `NSTextInputClient` conformance
and changed the key event flow. When the view's `keyDown:` receives a printable key
without Command modifier, it now calls `interpretKeyEvents:` which routes through the
macOS text input system. That system then calls either:
- `insertText:replacementRange:` → character insertion (expected)
- `doCommandBySelector:` → action command (when text input system doesn't recognize
  the key as text, causing NSBeep)

The beep suggests `doCommandBySelector:` is being called with an unhandled selector.
The `doCommandBySelector:` implementation only handles a fixed set of known selectors
(insertNewline:, deleteBackward:, etc.). If macOS routes a character key there instead
of to `insertText:`, we get a beep and no insertion.

**Likely Root Causes** (in order of probability):

1. **Missing `NSTextInputClient` protocol conformance declaration**: The MetalView
   implements the protocol methods but may not be formally declared as conforming
   to `NSTextInputClient`. Without the declaration, macOS won't recognize the view
   as an input client and will fall back to the old `doCommandBySelector:` path.

2. **Incorrect `hasMarkedText` return**: If `hasMarkedText` returns unexpected values,
   the text input system may behave incorrectly.

3. **Missing `interpretKeyEvents:` call**: If the bypass conditions for Command keys
   are too broad, regular character keys might be bypassing the text input system.

**Investigation Strategy**:

1. Use `git bisect` to pinpoint the exact commit
2. Add logging to trace the input path (`keyDown:` → `interpretKeyEvents:` → which
   callback is invoked)
3. Verify `NSTextInputClient` protocol conformance is declared
4. Test fixes against baseline commit `7a494aaf`

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk does not touch the renderer.
  It focuses purely on input event routing.

No subsystem considerations apply to this bug fix.

## Sequence

### Step 1: Reproduce and Confirm the Regression

Run the editor from HEAD and verify that:
1. Typing printable characters (a-z, 0-9, punctuation) produces NSBeep
2. Hotkeys (Cmd+Q, Cmd+S, Cmd+P) continue to work
3. Terminal tabs (if any) may or may not have the same issue (check this)

Then checkout `7a494aaf` and verify that character input works correctly there.

Location: Manual testing / build verification

### Step 2: Git Bisect to Isolate the Culprit Commit

Run `git bisect` between `7a494aaf` (good) and HEAD (bad) to identify the exact
commit that introduced the regression. The test criterion is:
- Good: typing 'a' in a buffer inserts 'a' at cursor
- Bad: typing 'a' produces NSBeep and no character appears

Commands:
```bash
git bisect start HEAD 7a494aaf
# Test each commit: cargo run, type a character
git bisect good  # or git bisect bad
```

Expected outcome: identify the specific commit that broke character input.

Location: Git repository

### Step 3: Analyze the Breaking Commit

Once the commit is identified, examine its changes. Based on the GOAL's hypothesis,
the most likely culprits are:

**If `unicode_ime_input` (commit 7bf55449 or c96f6dfa)**:
- Check `metal_view.rs` for NSTextInputClient implementation
- Verify the protocol conformance is properly declared
- Check `__do_command_by_selector` for unhandled selectors
- Check `__insert_text` is being called (add logging)

**If `focus_stack` (commit 1c9802bb or a7e182ef)**:
- Check event dispatch path through FocusStack
- Verify BufferFocusTarget receives events correctly
- Check if KeyEvent vs InsertText routing is correct

Location: Identified breaking commit's diff

### Step 4: Add Diagnostic Logging

Add temporary logging to trace the input path:

1. In `MetalView::__key_down`: Log when called, whether it bypasses or calls
   `interpretKeyEvents:`
2. In `MetalView::__insert_text`: Log when called and with what text
3. In `MetalView::__do_command_by_selector`: Log the selector name to see what
   commands are being routed there

This will reveal whether:
- `insertText:` is never called (protocol not recognized)
- `insertText:` is called but text is lost somewhere
- `doCommandBySelector:` is called with unexpected selectors

Location: `crates/editor/src/metal_view.rs`

### Step 5: Fix the Protocol Conformance Issue

**Most likely fix**: The `NSTextInputClient` protocol must be explicitly declared
on the MetalView class definition. In `objc2` + `define_class!`, protocols are
declared via `#[unsafe(protocol = ...)]` or similar attribute.

Check the `define_class!` macro invocation:
- Is `NSTextInputClient` listed as an implemented protocol?
- Are all required methods implemented?

The fix likely involves adding the protocol declaration to the class definition:
```rust
define_class!(
    #[unsafe(super = NSView)]
    #[unsafe(protocol = NSTextInputClient)]  // This might be missing
    ...
);
```

Location: `crates/editor/src/metal_view.rs`

### Step 6: Verify the Fix Against Baseline

After applying the fix:
1. Test that typing characters inserts them correctly
2. Test that IME input (if available) still works
3. Test that all hotkeys still function
4. Run the existing test suite: `cargo test -p editor`

Location: Manual testing + automated tests

### Step 7: Add a Regression Test

Add a test that verifies character input works. Since MetalView is a platform type
that can't be easily unit-tested, the test should verify the testable parts:

1. Verify that `TextInputEvent` flows through `drain_loop` and inserts text
2. Add an integration test in `crates/editor/tests/typing_test.rs` if one doesn't exist

Per TESTING_PHILOSOPHY.md, focus on behavior at boundaries:
- Empty buffer insertion
- Multi-character string insertion (IME commit scenario)
- Ensure no regression in KeyEvent handling

Location: `crates/editor/tests/typing_test.rs`

### Step 8: Clean Up and Document

1. Remove diagnostic logging added in Step 4
2. Add a chunk backreference comment at the fix site
3. Update `code_paths` in GOAL.md with modified files
4. Verify the fix commit message references the bisected culprit commit

Location: Modified files, GOAL.md

---

**BACKREFERENCE COMMENTS**

The fix should include a comment like:
```rust
// Chunk: docs/chunks/input_keystroke_regression - NSTextInputClient protocol fix
```

## Risks and Open Questions

- **Risk**: The bisect may reveal multiple interacting commits from `focus_stack` and
  `unicode_ime_input` that merged around the same time. Resolution: identify the
  earliest bad commit, fix that first, and verify if others need fixing.

- **Question**: Does the issue affect terminal tabs? The terminal input path uses
  `InputEncoder` directly from KeyEvents, not the IME text input path. This should
  be tested separately.

- **Question**: Is `NSTextInputClient` protocol conformance the actual issue, or is
  it something more subtle in the macOS text input routing? The diagnostic logging
  will answer this.

- **Risk**: The `objc2` / `define_class!` API for protocol conformance may have
  specific requirements. Need to check `objc2` documentation for correct syntax.

## Deviations

- **Steps 1-4 (git bisect and diagnostic logging)**: Skipped. Direct code analysis
  of `metal_view.rs` revealed the root cause immediately: the `NSTextInputClient`
  protocol was not declared in the `define_class!` macro. Without this declaration,
  macOS doesn't recognize the view as a text input client, causing all key events
  to be routed through `doCommandBySelector:` instead of `insertText:`. The git
  bisect was unnecessary because the hypothesis in the plan ("missing protocol
  conformance declaration") was confirmed by reading the code.

- **Step 5 (Fix)**: The fix was simpler than anticipated. Instead of moving
  methods between `impl` blocks, we only needed to add a single line:
  `unsafe impl NSTextInputClient for MetalView {}`. The existing methods already
  had the correct selector attributes and implementations.

- **Step 6 (Cargo.toml)**: An additional step was required: enabling the
  `NSTextInputClient` feature in `objc2-app-kit` to make the protocol trait
  available for import.

- **Step 7 (Regression test)**: No new test was added. The existing `typing_test.rs`
  already covers the buffer-level text insertion logic. The NSTextInputClient
  protocol conformance is a macOS integration layer that cannot be easily unit
  tested - it requires manual testing with the running application.