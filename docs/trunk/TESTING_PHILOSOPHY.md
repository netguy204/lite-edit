# Testing Philosophy

<!--
This document establishes how we think about verification in this project.
It informs every chunk's testing strategy but doesn't prescribe specific tests.

The goal is to answer: "Given a piece of functionality, how should we
approach testing it?" This creates consistency across chunks and helps
agents understand what kind of tests to write.
-->

## Testing Principles

<!--
What beliefs guide your testing approach? These should be specific enough
to resolve debates about whether a given test is worth writing.

We practice test-driven development for code with meaningful behavior. The workflow is:

1. **Write failing tests first** — Before writing implementation code, write tests that express what the code should do. These tests must fail initially.
2. **Write the implementation** — Write the minimum code necessary to make the tests pass.
3. **See previously failing tests succeed** — The same tests that failed now pass, providing confidence that the implementation satisfies the requirements.

This order applies to code that validates, transforms, computes, or has side effects. Writing tests after implementation invites tests that merely describe what the code happens to do, rather than what it should do.

**When TDD doesn't apply**: Scaffolding code (struct definitions, enum variants, trait declarations, FFI bindings) often has no meaningful behavior to test. If the only failing test you can write is a trivial test (see Anti-Pattern: Trivial Tests below), skip the red phase entirely. Write the scaffolding code, then add tests only for its behavioral aspects. The goal is signal, not ritual.

**When TDD is impractical**: GPU rendering, macOS window management, and Metal pipeline setup involve visual output and platform state that can't be meaningfully asserted in a unit test. For these, write the implementation first, verify visually, then add tests for the testable components (e.g., glyph layout math, dirty region merging, viewport calculations) and integration tests where feasible.

### Goal-Driven Test Design

Tests must assert semantically meaningful properties with respect to the goal. There must always be a clear, traceable relationship between:

- The success criteria in a chunk's GOAL.md
- The tests that verify those criteria

Each test should answer: "What success criterion does this test verify?" If the answer isn't clear, the test may not be valuable.

### Semantic Assertions Over Structural Assertions

**Avoid superficial assertions.** Tests that check types, field existence, or implementation details provide false confidence. They pass when the code is wrong and break when the code is refactored correctly.

Bad:
```rust
#[test]
fn test_buffer_exists() {
    let buf = TextBuffer::new();
    assert!(std::mem::size_of_val(&buf) > 0);
}
```

Good:
```rust
#[test]
fn test_insert_char_appears_in_line() {
    let mut buf = TextBuffer::new();
    buf.insert_char('a');
    assert_eq!(buf.line_content(0), "a");
}
```

The bad test passes even if the buffer can't store text. The good test verifies the actual goal: inserted characters are retrievable.

### Test Behavior at Boundaries

The interesting bugs live at boundaries. Prioritize testing:

- Empty states (empty buffer, cursor at position 0)
- Buffer boundaries (delete at start, insert at end, cursor past last line)
- Line boundaries (backspace joining lines, newline splitting lines)
- Edge cases explicitly mentioned in success criteria

### Humble View Architecture

lite-edit follows a variant of the Elm Architecture (Model-View-Update) adapted for Rust's ownership model:

- **Model**: Mutable application state owned by the main thread — `TextBuffer`, `Viewport`, `DirtyRegion`, cursor(s), focus stack. Plain Rust structs with no platform dependencies.
- **Update**: `FocusTarget.handle_key(event, &mut EditorContext)` — takes an input event and a mutable reference to state, mutates it, and accumulates dirty regions. This is the Elm `update` function with mutable state instead of immutable return values. Mutable state is safe here because the entire critical path is single-threaded by design.
- **View**: The Metal render loop — reads the model (buffer content, viewport, dirty region) and produces pixels. This is the **humble object**: it contains no logic, makes no decisions, and is not unit-tested. It just projects state onto the screen.

This architecture makes the application testable by construction:

```rust
// The entire editing pipeline is testable without a window, GPU, or macOS:
let mut ctx = EditorContext::new(buffer, viewport);
let mut target = BufferFocusTarget::new();

target.handle_key(KeyEvent::char('H'), &mut ctx);
target.handle_key(KeyEvent::char('i'), &mut ctx);
target.handle_key(KeyEvent::backspace(), &mut ctx);
target.handle_key(KeyEvent::char('o'), &mut ctx);

assert_eq!(ctx.buffer.line_content(0), "Ho");
assert_eq!(ctx.buffer.cursor_position(), (0, 2));
assert_eq!(ctx.dirty_region, DirtyRegion::Lines { from: 0, to: 1 });
```

No mocking, no platform dependencies, no test harness. Just construct state, call the update function, and assert on the result.

**The architectural rule**: if you find yourself unable to test a behavior without spinning up a window or a Metal device, the logic is in the wrong place. Extract it into pure state manipulation and push the platform interaction to the edges.

### Separate Testable Logic from Platform Code

Apply the humble view principle concretely: keep testable logic in pure Rust modules, and make platform code a thin shell.

**Testable (pure Rust, no platform dependencies):**

- **Text buffer**: Insert, delete, cursor movement, line access, dirty tracking.
- **Dirty region computation**: Merge, promotion (`Lines` → `FullViewport`).
- **Viewport calculations**: Visible range, scroll clamping, cursor-follows-viewport.
- **Command resolution**: Pure function `(modifiers, key) → Option<Command>`.
- **Glyph layout math**: `col * glyph_width`, `row * line_height`.
- **Focus target logic**: Keystroke → buffer mutation → dirty region.

**Humble (platform shell, not unit-tested):**

- NSView/NSWindow setup and event forwarding.
- Metal device, command queue, pipeline state creation.
- Glyph rasterization via Core Text (we test that we request the right glyphs, not that Core Text renders them correctly).
- `CAMetalLayer` drawable acquisition and presentation.
- `NSRunLoop` drain loop (the loop itself is trivial; what it calls is tested).

### Plugins Get Copies, Not References

When plugins need buffer state (for syntax highlighting, LSP `didChange`, search), the main thread copies the affected content into notifications sent via channels. Plugins work on their own copies. This preserves:

- **Single-owner mutability**: The main thread is the sole owner of mutable state. No `Arc<RwLock<Buffer>>`, no shared mutable access.
- **Testability**: Plugin logic can be tested by feeding it copied content directly, without simulating the channel or the main loop.
- **Safety**: A misbehaving plugin cannot block the main thread by holding a lock.
-->

## Test Categories

<!--
Define the categories of tests you use and what each is responsible for.
This creates shared vocabulary for chunk TESTS.md documents.
-->

### Unit Tests

<!--
What do unit tests cover in this project?
What's the boundary of a "unit"?
How do you handle dependencies—mocking, faking, or real implementations?

Example:
Unit tests verify individual functions and structs in isolation.
Dependencies on I/O are injected as traits and faked in tests.
Unit tests must run without filesystem or network access.
-->

### Integration Tests

<!--
What do integration tests cover?
What components are allowed to be "real" vs simulated?

Example:
Integration tests verify interactions between components.
Filesystem access is real (using temp directories).
Time may be simulated to test expiration logic.
-->

### System Tests

<!--
End-to-end tests that exercise the system as a user would.
What environment do they require?
How are they isolated from each other?
-->

### Property Tests

<!--
If you use property-based testing (fuzzing, QuickCheck-style):
What properties are amenable to this approach?
How do you balance coverage vs execution time?
-->

### Performance Tests

<!--
How do you verify performance requirements from SPEC.md?
Are these run in CI or manually?
What hardware/environment assumptions do they make?
-->

## Hard-to-Test Properties

<!--
Some properties are difficult to test automatically. Document your approach
for each. Common examples:

- Durability/crash safety: How do you test that data survives crashes?
- Concurrency: How do you test for race conditions?
- Resource limits: How do you test behavior at boundaries (disk full, etc.)?
- Performance degradation: How do you catch performance regressions?
-->

## What We Don't Test

<!--
Explicitly list what's out of scope for automated testing and why.

Example:
- Actual hardware failure (disk corruption): Tested manually during
  initial development, not practical to automate
- Multi-machine scenarios: Out of scope for this single-node implementation
-->

## Test Organization

<!--
Where do tests live? How are they named? How do they map to source files?

Example:
- Unit tests: Same file as implementation, in #[cfg(test)] module
- Integration tests: tests/ directory, one file per major feature
- Property tests: tests/proptests/, require --features proptest to run
-->

## CI Requirements

<!--
What must pass before code is merged?
How long should the test suite take?
Are there tests that run nightly vs on every PR?
-->
