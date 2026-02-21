# Testing Philosophy

This document establishes how we think about verification in lite-edit.
It informs every chunk's testing strategy but doesn't prescribe specific tests.

## Testing Principles

### Test-Driven Development

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

## Test Categories

### Unit Tests

Unit tests verify individual functions and structs in isolation. In this project:

- **Boundary**: A single function, method, or small module.
- **Dependencies**: Real implementations for everything — no mocking framework. Pure Rust code has no external dependencies to mock.
- **Location**: Same file as implementation, in `#[cfg(test)]` module.

Primary targets: text buffer operations, dirty region merging, viewport line range calculations, command resolution.

### Integration Tests

Integration tests verify interactions between components. In this project:

- **Boundary**: Multiple modules working together (e.g., buffer + viewport + dirty region → correct render input).
- **Dependencies**: Real implementations. Filesystem access via temp directories where needed.
- **Location**: `tests/` directory, one file per major feature area.

Primary targets: buffer mutation → dirty region → viewport visible lines pipeline, focus target receiving a sequence of key events and producing correct buffer state.

### Performance Tests

Performance tests verify the quantitative requirements from GOAL.md. These are critical — the north-star metric is keystroke-to-glyph latency under 8ms at P99.

- **Unit-level benchmarks**: Buffer insert/delete throughput, glyph layout computation time, dirty region merge overhead. Run via `cargo bench` using Criterion.
- **End-to-end latency**: Keystroke-to-present timing using input injection. Requires the full application running on macOS with Metal. Run manually or in a dedicated benchmark harness, not in CI.
- **Resource benchmarks**: Memory usage (RSS) and idle CPU. Measured via external tools (`/usr/bin/time`, Instruments), not in-process.

Performance tests target these GOAL.md requirements:
- Keystroke-to-glyph P99 < 8ms
- Memory < 50MB (core, no plugins)
- Idle CPU < 2%
- Startup < 100ms

### What We Don't Have (Yet)

- **Property tests**: May be valuable for text buffer operations (insert/delete sequences should be reversible, cursor position should always be valid). Add when the buffer is implemented and edge cases emerge.
- **Visual regression tests**: Screenshot comparison for rendering correctness. Potentially valuable but complex to set up. Defer until rendering is stable.

## Hard-to-Test Properties

### Rendering Correctness

Metal rendering output can't be meaningfully asserted in a unit test. Our approach:

1. **Test the inputs to rendering**: Verify that the correct glyph positions, atlas coordinates, and dirty regions are computed. If the inputs are correct, the rendering is correct (assuming Metal and the shaders work).
2. **Visual smoke tests**: Manual verification that text looks right. Document expected appearance in chunk success criteria.
3. **Benchmark as correctness proxy**: If full-viewport rendering completes in <2ms, it's evidence the pipeline isn't doing degenerate work.

### Input Latency

End-to-end latency from keystroke to photon is the north-star metric but involves the full macOS input stack, Metal presentation, and display hardware.

1. **Benchmark the controllable path**: Measure from "event received" to "Metal command buffer committed." This excludes OS and display latency but captures our code's contribution.
2. **End-to-end measurement**: Use input injection (`evhz`-style) and a light sensor or high-speed capture for ground truth. Manual, not CI.
3. **Regression detection**: Criterion benchmarks in CI catch performance regressions in the measurable path.

### Concurrency Safety

The critical path is single-threaded by design (GOAL.md constraint). Concurrency concerns arise only at the plugin boundary (background threads delivering results to the main thread via channels).

1. **Design away the problem**: Lock-free channels, `dispatch_async` to main queue. No shared mutable state between threads.
2. **Stress tests**: If plugin integration introduces concurrency, add stress tests that flood the channel while the main loop is running.

## What We Don't Test

- **Metal shader correctness**: Shaders are simple textured-quad renderers. Verified visually, not programmatically.
- **macOS event delivery**: We trust that `NSEvent` delivers correct key codes. We test our interpretation of events, not the OS.
- **Font rasterization quality**: Core Text handles rasterization. We test that we request the right glyphs, not that they look pretty.
- **Exact visual output**: We don't screenshot-compare rendered frames. We test the data that drives rendering.

## Anti-Pattern: Trivial Tests

A **trivial test** verifies something that cannot meaningfully fail. It tests the language or compiler rather than the system's behavior. These tests add noise without adding confidence — they pass when the code is wrong and never catch real bugs.

### The Principle

**Test behavior, not language semantics.** A test is trivial if the only way it can fail is if Rust's type system, standard library, or compiler is broken.

### Identifying Trivial Tests

A test is trivial if:

1. **It asserts that a value equals what was just assigned.** Setting a field and reading it back tests Rust's struct semantics, not your code.
2. **It cannot fail unless the compiler is broken.** If the test would only fail due to a rustc bug, it provides no value.
3. **It tests no transformation, computation, side effect, or rejection.** Meaningful tests verify that something *happens*: a computation produces a result, a state change occurs, invalid input is rejected.

### Examples

**Trivial** (do not write these):
```rust
#[test]
fn test_viewport_has_offset() {
    let vp = Viewport { scroll_offset: 10, visible_lines: 50 };
    assert_eq!(vp.scroll_offset, 10);  // Tests Rust struct initialization
}

#[test]
fn test_dirty_region_is_none() {
    let dr = DirtyRegion::None;
    assert!(matches!(dr, DirtyRegion::None));  // Tests Rust enum matching
}
```

**Meaningful** (write these instead):
```rust
#[test]
fn test_dirty_regions_merge_to_superset() {
    let mut dr = DirtyRegion::Lines { from: 3, to: 5 };
    dr.merge(DirtyRegion::Lines { from: 7, to: 9 });
    assert_eq!(dr, DirtyRegion::Lines { from: 3, to: 9 });
}

#[test]
fn test_viewport_clamps_to_buffer_length() {
    let vp = Viewport::new(scroll_offset: 100, visible_lines: 50);
    let range = vp.visible_range(buffer_line_count: 110);
    assert_eq!(range, 100..110);  // Clamped, not 100..150
}
```

### Recognizing Novel Forms

Ask yourself:
- **What could make this test fail?** If only a compiler bug, it's trivial.
- **What behavior does this test verify?** If no computation, transformation, or side effect, it's trivial.
- **Would a bug in my code cause this test to fail?** If your code could be wrong and the test still passes, it's trivial.

The goal is signal, not coverage.

## Test Organization

```
src/
├── buffer.rs          # #[cfg(test)] mod tests { ... }
├── viewport.rs        # #[cfg(test)] mod tests { ... }
├── dirty_region.rs    # #[cfg(test)] mod tests { ... }
├── focus.rs           # #[cfg(test)] mod tests { ... }
└── ...

tests/
├── buffer_editing.rs       # Integration: keystroke sequences → buffer state
├── viewport_scrolling.rs   # Integration: mutations → viewport adjustment
└── ...

benches/
├── buffer_throughput.rs    # Criterion: insert/delete performance
├── layout_speed.rs         # Criterion: glyph layout computation
└── ...
```

**Organization principles:**

- **Unit tests inline**: `#[cfg(test)]` modules in the same file as the code they test. Keeps tests close to implementation, easy to maintain.
- **Integration tests in `tests/`**: Multi-module interaction tests. Named by the behavior they verify, not the module they test.
- **Benchmarks in `benches/`**: Criterion benchmarks for performance-critical paths.

## CI Requirements

All unit and integration tests must pass before code is merged:

```bash
cargo test
```

Benchmarks run on CI for regression detection but don't block merging (performance varies by CI hardware). Performance regressions are flagged for manual review.

Tests should complete in under 10 seconds. The buffer and viewport tests are pure computation — if they're slow, something is wrong.
