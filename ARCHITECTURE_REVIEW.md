# lite-edit Architecture Review

**Date**: 2026-02-25
**Reviewers**: GUI Framework Architect (Cocoa/Tk/Qt perspective) + Performance Architect (Casey Muratori / Mike Acton perspective)

---

Initial


[lite-edit perf] Frame #694
  Keystroke-to-present:  P50=5.6ms  P95=7.4ms  P99=8.2ms
  Dirty region:          partial=690 (99.4%)  full=4 (0.6%)  skipped=0
  styled_line:           P50=27µs  P95=55µs  P99=61µs  (avg 6 lines/frame)


----

## Executive Summaries

### GUI Framework Architect

lite-edit has a solid foundation: the Elm-style drain-all-then-render loop eliminates an entire class of reentrancy bugs that plague Cocoa apps, the BufferView trait cleanly unifies text and terminal rendering, and the BSP pane layout is the right choice for a code editor. However, several architectural seams are under-specified — the focus/responder system is flat rather than hierarchical, the rendering pipeline has grown into mega-files that mix layout/style/hit-testing concerns, and the text system lacks the Unicode sophistication needed for production use. The most impactful changes would be introducing a proper view tree for event routing, breaking up the rendering monolith, and adding grapheme cluster awareness to the buffer.

### Performance Architect

lite-edit's performance posture is strong where it matters most: the gap buffer gives O(1) edits at cursor, the drain-all-then-render loop batches events before rendering, and the glyph atlas avoids per-frame rasterization. The 8ms P99 target is achievable with the current architecture. The primary risks are: (1) per-frame Vec allocations in the quad emission pipeline that could be eliminated with pre-allocated buffers, (2) `Arc<Mutex<Box<dyn BufferView>>>` triple-indirection on every tab access during render, (3) the `styled_line()` path allocating a new `StyledLine` (with its `Vec<StyledSpan>`) for every visible line every frame, and (4) full-viewport re-renders triggered too aggressively. The biggest single win would be making `StyledLine` output reusable across frames when lines haven't changed.

---

## Part I: GUI Framework Architecture Review

### 1. Component Model & Event Propagation

**Finding**: The focus system is flat — `EditorFocus` is an enum with 4 variants (Buffer, Selector, FindInFile, ConfirmDialog). There is no responder chain. Events go directly to the current focus target.

**Why it matters**: Every mature GUI framework (Cocoa's NSResponder chain, Qt's event propagation, Tk's bindtags) learned that event routing needs to be hierarchical. Without it:
- Adding new focusable widgets requires modifying the `EditorFocus` enum (closed set)
- Keyboard shortcuts that should work regardless of focus (Cmd+Q, Cmd+W) must be duplicated in every focus target
- Nested focus (e.g., find bar inside a pane vs. global find) requires special-casing
- Accessibility frameworks expect a view hierarchy for VoiceOver navigation

**Recommendation (P1)**: Introduce a focus stack rather than a focus enum.

```rust
struct FocusStack {
    stack: Vec<Box<dyn FocusTarget>>,
}

impl FocusStack {
    fn handle_key(&mut self, event: KeyEvent, ctx: &mut EditorContext) -> Handled {
        // Top of stack gets first crack; if unhandled, propagate down
        for target in self.stack.iter_mut().rev() {
            if target.handle_key(event, ctx) == Handled::Yes {
                return Handled::Yes;
            }
        }
        Handled::No
    }
}
```

This makes the focus set open (new targets don't require enum changes), enables global shortcuts at the bottom of the stack, and supports overlay stacking (find bar on top of buffer on top of global handlers).

### 2. Layout System

**Finding**: The BSP layout in `pane_layout.rs` is well-implemented. The `PaneLayoutNode` enum (`Leaf`/`Split`) with recursive rect computation is the right approach for a code editor. The `directional_navigate` function correctly walks the tree for focus movement.

**What's missing**: No minimum size constraints. When splits get deeply nested, panes can shrink to zero or near-zero width. Mature frameworks solve this with constraint propagation.

**Recommendation (P2)**: Add a `min_size` to pane nodes and enforce it during split ratio adjustment:

```rust
const MIN_PANE_WIDTH: f32 = 120.0;  // ~15 characters
const MIN_PANE_HEIGHT: f32 = 60.0;  // ~3 lines

fn compute_rects(&self, available: Rect) -> Vec<(PaneId, Rect)> {
    match self {
        Split { direction, ratio, first, second } => {
            let clamped_ratio = clamp_ratio(*ratio, *direction, available);
            // ... use clamped_ratio
        }
    }
}
```

### 3. Rendering Invalidation

**Finding**: The `DirtyLines` → `DirtyRegion` pipeline is a good design. The merge semantics (`Single + Single = Range`, `FromLineToEnd` absorbs all) are correct.

**Gap**: There's no distinction between "content dirty" and "structure dirty." A pane resize, a tab bar change, or an overlay appearing all go through the same dirty path. In Cocoa, `setNeedsDisplay:` vs `setNeedsLayout:` are separate because layout is more expensive than redraw and they happen at different frequencies.

**Recommendation (P1)**: Separate layout invalidation from content invalidation:

```rust
enum InvalidationKind {
    Content(DirtyRegion),    // Glyph changes within existing layout
    Layout,                   // Pane resize, split change, tab bar change
    Overlay,                  // Find bar, selector, dialog appeared/changed
}
```

Content-only invalidation can skip layout recalculation entirely. This matters because the renderer currently recomputes all pane rects every frame.

### 4. State Management (Elm Architecture)

**Finding**: The drain-all-then-render approach in `drain_loop.rs` is excellent. The `EventDrainLoop` owns `EditorState` directly — no `Rc<RefCell<>>` anywhere. This is genuinely better than what most Cocoa apps achieve, where reentrancy bugs from modal dialogs and delegates are endemic.

**Concern**: The `EditorState` struct has grown to be a god object. It holds workspaces, focus state, metal view reference, font metrics, cursor blink state, quit flag, session state, and dirty region all in one struct. Every event handler gets `&mut EditorState`, which means any handler can mutate anything.

**Recommendation (P2)**: Factor `EditorState` into purpose-specific sub-states:

```rust
struct EditorState {
    content: ContentState,       // workspaces, tabs, buffers
    ui: UIState,                 // focus, cursor blink, dirty region
    platform: PlatformState,     // metal view, font metrics
    session: SessionState,       // persistence
}
```

Pass only the relevant sub-state to each handler. This makes mutation boundaries explicit and reduces cognitive load when reading handlers.

### 5. Text System

**Finding**: The gap buffer in `gap_buffer.rs` operates on `char` (Rust's 32-bit Unicode scalar). The `TextBuffer` in `text_buffer.rs` provides line-based access via `LineIndex`. Cursor positions are (line, col) where col is a character offset.

**Critical gap**: There is no grapheme cluster awareness. A user pressing backspace should delete one grapheme cluster (which may be multiple chars for emoji, combining characters, etc.), but the current `delete_char_before_cursor` deletes exactly one `char`. This means:
- Emoji like `👨‍👩‍👧‍👦` (7 chars: 4 codepoints + 3 ZWJ) would require 7 backspaces
- Combining characters like `é` (e + combining acute) would leave orphaned combiners
- Regional indicators `🇺🇸` (2 chars) would show a broken half-flag

**Recommendation (P0)**: Add grapheme cluster boundary detection. The `unicode-segmentation` crate provides this:

```rust
use unicode_segmentation::UnicodeSegmentation;

fn delete_grapheme_before_cursor(&mut self) {
    let line = self.line_content(self.cursor.line);
    let before_cursor = &line[..byte_offset_of(self.cursor.col)];
    if let Some(last_grapheme) = before_cursor.graphemes(true).next_back() {
        let chars_to_delete = last_grapheme.chars().count();
        for _ in 0..chars_to_delete {
            self.delete_char_before_cursor();
        }
    }
}
```

This is P0 because it's a correctness issue that affects every non-ASCII user.

### 6. Input Model

**Finding**: The `KeyEvent` / `MouseEvent` / `ScrollDelta` types in `lite-edit-input` are clean but minimal. They handle the common cases well.

**Missing pieces**:
- **IME (Input Method Editor)**: No `insertText:replacementRange:`, `setMarkedText:selectedRange:replacementRange:`, or `unmarkText`. This means Chinese, Japanese, Korean input doesn't work.
- **Accessibility**: No `NSAccessibility` protocol implementation. VoiceOver users can't use the editor.
- **Text input events vs. key events**: The current model conflates "the user pressed a physical key" with "the user wants to insert text." These need to be separate for IME to work.

**Recommendation (P0 for IME, P1 for accessibility)**: Split text input from key actions:

```rust
enum InputEvent {
    // Physical key events (for shortcuts, navigation)
    Key(KeyEvent),
    // Text insertion (from keyboard, IME, paste, dictation)
    InsertText { text: String, replacement_range: Option<Range<usize>> },
    // IME composition
    SetMarkedText { text: String, selected_range: Range<usize> },
    UnmarkText,
}
```

The `MetalView` NSView subclass needs to implement `NSTextInputClient` protocol to receive these from Cocoa.

### 7. Tab / Workspace Model

**Finding**: The two-level hierarchy (Workspace → Pane → Tab) is well-structured. Workspaces map to project directories, panes support split views, tabs hold buffers. This is more sophisticated than most editors.

**Observation**: Tabs hold `Arc<Mutex<Box<dyn BufferView>>>`. This triple indirection exists to allow the PTY reader thread to push data into terminal buffers while the main thread renders. This is architecturally sound — shared ownership with mutual exclusion — but the `Box<dyn>` inside the `Arc<Mutex<>>` is unnecessary. `Arc<Mutex<dyn BufferView>>` (trait object directly in the Arc) would eliminate one heap allocation and one pointer dereference.

**Recommendation (P2)**: Flatten to `Arc<Mutex<dyn BufferView>>` or, better, use the event channel that already exists — have PTY data go through the event channel (which it already does for wakeup), and let the drain loop push bytes into the buffer. Then terminals don't need Arc<Mutex> at all, just Box<dyn BufferView>.

### 8. BufferView Trait Design

**Finding**: The `BufferView` trait is a good abstraction boundary. It provides exactly what the renderer needs: line count, styled lines, cursor info, editability flag, and dirty tracking.

**Design tension**: `take_dirty(&mut self) -> DirtyLines` has destructive read semantics — once you take dirty, it's gone. This means only one consumer can observe changes. Currently that's fine (only the renderer reads it), but if you ever want a second consumer (e.g., accessibility notifications, or a minimap), you'll need to fan out.

**Recommendation (P2)**: Consider changing to `fn dirty_since(&self, generation: u64) -> DirtyLines` where each buffer tracks a generation counter. Consumers track their own "last seen" generation. This is how Cocoa's change counting (`NSDocument.changeCount`) works.

---

## Part II: Performance Architecture Review

### 1. Hot Path Analysis: Keystroke → Pixel

Walking the critical path:

```
NSView keyDown:       ~0µs  (Cocoa callback)
 → encode KeyEvent    ~0.1µs
 → channel.send()     ~0.3µs (crossbeam mpsc, lock-free)
 → CFRunLoopSource    ~1-5µs (run loop wakeup via mach port)
 → drain_loop         ~0.1µs
 → buffer.insert()    ~0.2µs (gap buffer at cursor, amortized)
 → line_index update  ~0.1µs
 → syntax.edit()      ~120µs (tree-sitter incremental parse)  ← DOMINANT
 → dirty_mark         ~0.01µs
 → render():
   → styled_line() × N  ~3-5µs per line × ~40 lines = 120-200µs  ← SIGNIFICANT
   → glyph layout       ~1-2µs per line × 40 = 40-80µs
   → quad emission       ~0.5-1µs per line × 40 = 20-40µs
   → Metal encode        ~50-100µs (command buffer setup + draw calls)
   → GPU present         ~0-16ms (vsync dependent)
```

**Total CPU budget**: ~400-600µs typical, well under 8ms.
**Tree-sitter dominates** at ~120µs, but this is inherent and already incremental.

**Key insight**: The 8ms target is achievable. The risks are in the tail — P99 means the 1-in-100 worst case. What causes spikes?

### 2. Per-Frame Allocation: The StyledLine Problem

**Finding** (`glyph_buffer.rs`, `buffer_view.rs`): Every call to `styled_line(line_idx)` allocates a new `StyledLine` containing a `Vec<StyledSpan>`. For a 40-line viewport, that's 40 `Vec` allocations per frame — even for lines that haven't changed.

**Impact**: Each `Vec<StyledSpan>` allocation is ~50-100ns (allocator + potential fragmentation). 40 lines × 100ns = 4µs per frame just for styled line allocation. Under memory pressure or with fragmented heap, this balloons to 10-50µs in P99.

**Recommendation (P1)**: Cache styled lines and invalidate per dirty line:

```rust
struct StyledLineCache {
    lines: Vec<Option<StyledLine>>,  // indexed by buffer line
    generation: u64,
}

impl StyledLineCache {
    fn get_or_compute(&mut self, line: usize, buffer: &dyn BufferView) -> &StyledLine {
        if self.lines[line].is_none() {
            self.lines[line] = Some(buffer.styled_line(line));
        }
        self.lines[line].as_ref().unwrap()
    }

    fn invalidate(&mut self, dirty: &DirtyLines) {
        match dirty {
            DirtyLines::Single(line) => self.lines[*line] = None,
            DirtyLines::Range { from, to } => {
                for line in *from..=*to { self.lines[line] = None; }
            }
            DirtyLines::FromLineToEnd(line) => {
                self.lines.truncate(*line);
            }
            DirtyLines::None => {}
        }
    }
}
```

**Expected impact**: Eliminates ~90% of styled_line allocations during typical editing (only the current line changes). Saves 3-4µs average, 10-40µs P99.

### 3. Quad Buffer Pre-allocation

**Finding** (`glyph_buffer.rs`): The quad buffer (`Vec<Quad>`) is rebuilt every frame. Each `push_quad()` call may trigger Vec reallocation. A typical frame with 40 lines × 80 chars = 3,200 quads, each ~64 bytes = ~200KB of quad data.

**Impact**: Vec growth follows doubling strategy, so after the first frame it stabilizes. But every full-viewport dirty (resize, overlay toggle) starts fresh.

**Recommendation (P1)**: Pre-allocate the quad buffer and `clear()` instead of creating new:

```rust
struct GlyphBuffer {
    quads: Vec<Quad>,       // reused across frames
    bg_quads: Vec<Quad>,    // reused across frames
}

impl GlyphBuffer {
    fn begin_frame(&mut self) {
        self.quads.clear();      // keeps capacity
        self.bg_quads.clear();
    }
}
```

If already doing this: verify. If the Vec is created fresh each frame in a local variable inside the render function, it allocates every time. Move it to persistent state.

**Expected impact**: Eliminates ~200KB allocation per full-viewport render. Saves 5-20µs on full redraws.

### 4. Triple Indirection on Tab Access

**Finding** (`workspace.rs`): Every tab holds `Arc<Mutex<Box<dyn BufferView>>>`. Accessing the buffer during render requires:
1. `Arc` dereference → follow pointer to heap allocation
2. `Mutex::lock()` → atomic CAS + potential kernel syscall
3. `Box` dereference → follow pointer to trait object
4. `dyn BufferView` vtable → indirect function call

This happens for every `styled_line()` call on every visible line.

**Impact**: The mutex lock/unlock is the expensive part: ~20-30ns uncontended on Apple Silicon. For 40 lines, that's 40 lock/unlock cycles = ~1µs. Under contention (PTY reader writing to terminal buffer while renderer reads), this can spike to 5-50µs due to thread scheduling.

**Recommendation (P1)**: Lock once per tab per frame, not once per line:

```rust
// Current (implicit): lock per styled_line call
for line in visible_range {
    let guard = tab.buffer.lock();  // LOCK
    let styled = guard.styled_line(line);
    drop(guard);                     // UNLOCK
    emit_quads(styled);
}

// Better: lock once, extract all needed data
let guard = tab.buffer.lock();       // LOCK once
let lines: Vec<StyledLine> = visible_range
    .map(|line| guard.styled_line(line))
    .collect();
let cursor = guard.cursor_info();
drop(guard);                          // UNLOCK once
for styled in &lines {
    emit_quads(styled);
}
```

**Expected impact**: Reduces 40 lock/unlock cycles to 1. Saves ~1µs average, ~10-40µs P99 under contention.

### 5. Renderer Monolith & Code Size

**Finding** (`renderer.rs`): 116K LOC in a single file. This is not just a readability problem — it's a performance problem:
- The compiler cannot optimize what it cannot reason about. Enormous functions inhibit inlining decisions.
- Instruction cache pressure: if the render function + all its helpers exceed L1 I-cache (typically 128-192KB on Apple Silicon), you get I-cache misses.
- 116K LOC of Rust compiles to significant machine code. Even with LTO, the function call graph through this file will have poor locality.

**Recommendation (P1)**: Split renderer into focused modules:

```
renderer/
├── mod.rs              // Top-level render() orchestration (~500 LOC)
├── layout.rs           // Pane rect computation
├── content.rs          // Text line rendering
├── tab_bar_render.rs   // Tab bar rendering
├── overlay.rs          // Find bar, selector, dialog rendering
├── cursor.rs           // Cursor and selection rendering
└── metal_pass.rs       // Metal command encoding
```

Each module is a focused rendering phase. The top-level `render()` calls them in sequence. This improves I-cache locality because only the current phase's code needs to be hot.

**Expected impact**: Difficult to quantify without profiling, but I-cache improvements on hot paths typically yield 5-15% render time reduction. For a 400µs render, that's 20-60µs.

### 6. Dirty Region ROI Analysis

**Finding** (`dirty_region.rs`): The dirty region tracking converts buffer-space `DirtyLines` to screen-space `DirtyRegion`, then uses Metal scissor rects to skip clean regions.

**Assessment**: For typical editing (typing on one line), this skips rendering 39 of 40 visible lines — a 97.5% reduction. The bookkeeping overhead is ~0.1µs per event. This is a clear win.

**However**: The implementation seems to fall back to `FullViewport` frequently — scroll events, overlay toggles, pane resizes, focus changes all trigger full redraws. For the most common P99-busting operations (rapid scrolling, terminal output), dirty tracking provides zero benefit because the entire viewport is dirty.

**Recommendation (P2)**: Profile what percentage of frames are actually partial vs. full viewport. If >50% of frames are full viewport, the dirty tracking is paying overhead on every frame but only saving work on half. Consider a simpler approach: always render the full viewport but use the cached styled lines (recommendation #2) to avoid recomputing unchanged content.

### 7. Gap Buffer vs. Rope

**Finding** (`gap_buffer.rs`): The gap buffer stores `Vec<char>` with a movable gap. This is O(1) for edits at cursor but O(n) for gap movement where n = characters between old and new cursor position.

**Assessment**: For a code editor, gap buffer is the right choice if files are <100K lines. The gap movement cost is bounded by file size, and the simplicity wins over rope complexity. The `char`-based storage (4 bytes per character) is wasteful for ASCII-heavy code but simplifies indexing.

**Concern**: For very large files (>1M characters), gap movement on a cursor jump (e.g., Cmd+End) copies 4MB of data. On Apple Silicon with ~60GB/s memory bandwidth, that's ~67µs — well under 8ms, but it's a fixed cost that scales linearly.

**Recommendation (P2)**: This is fine for now. If you ever need to support 10M+ character files, a piece table or rope would be better. But don't switch prematurely — the gap buffer's simplicity is a genuine advantage for correctness and maintainability.

### 8. Font Rasterization Path

**Finding** (`font.rs`, `glyph_atlas.rs`): Glyphs are rasterized via Core Text on first encounter and stored in a texture atlas. ASCII printable range (0x20-0x7E) is pre-populated at startup.

**Assessment**: This is the right approach. Core Text rasterization is ~10-50µs per glyph, so the atlas amortizes this to zero for steady-state rendering. The pre-population of ASCII means the common case never hits Core Text during editing.

**Concern**: Atlas growth when encountering new glyphs (e.g., opening a file with CJK characters for the first time) could cause a frame spike. Each new glyph requires: rasterize → upload to texture → potentially grow texture.

**Recommendation (P2)**: Consider rasterizing newly-encountered glyphs asynchronously. Render a placeholder (empty quad) for the first frame, then swap in the real glyph on the next frame. This caps the per-frame rasterization cost regardless of how many new glyphs appear. However, this is only relevant for the first-open case, not steady-state editing.

### 9. Syntax Highlighting Cost

**Finding**: Tree-sitter incremental parse at ~120µs per character edit is the single largest CPU cost in the keystroke path.

**Assessment**: This is inherent to tree-sitter and already optimized (incremental parsing). The alternative — regex-based highlighting — would be faster per-edit but produce worse results and miss structural information.

**Recommendation (P2)**: If the 120µs becomes a problem for P99 (e.g., during rapid typing), consider deferring tree-sitter parse by one frame: apply the edit immediately, render with stale highlighting, and parse on the next idle tick. The user won't notice a 16ms delay in syntax color updates. This effectively removes tree-sitter from the keystroke→pixel critical path.

### 10. Event Channel & Wakeup Latency

**Finding** (`drain_loop.rs`, `event_channel.rs`): Events go through crossbeam-channel (lock-free MPSC) and trigger a CFRunLoopSource for wakeup.

**Assessment**: crossbeam-channel send is ~50-100ns (excellent). The CFRunLoopSource wakeup adds ~1-5µs (mach port signaling). This is already very efficient.

**One concern**: If the run loop is in its "waiting for events" state (CFRunLoopRunInMode), the mach port wakeup has to cross the kernel boundary. This adds ~2-5µs of kernel-to-user transition time. This is irreducible on macOS.

**Recommendation**: No change needed. This is already well-optimized.

---

## Consolidated Top 10 Recommendations

Ranked by impact, combining both reviews:

### 1. Add Grapheme Cluster Awareness (P0 — Correctness)
**Source**: GUI review, text system
**Impact**: Broken behavior for emoji, combining characters, CJK
**Effort**: Small — add `unicode-segmentation` crate, modify `delete_char_before_cursor` and cursor movement
**Why first**: This is a correctness bug, not an optimization. Every non-ASCII user hits it.

### 2. Implement IME Support via NSTextInputClient (P0 — Correctness)
**Source**: GUI review, input model
**Impact**: Chinese, Japanese, Korean users cannot type
**Effort**: Medium — implement `NSTextInputClient` protocol on `MetalView`, add `InsertText`/`SetMarkedText` events
**Why second**: This blocks entire user populations from using the editor.

### 3. Cache StyledLine Output Across Frames (P1 — Performance)
**Source**: Performance review, per-frame allocation
**Impact**: Eliminates ~90% of styled_line allocations. 10-40µs P99 improvement
**Effort**: Medium — add StyledLineCache per tab, invalidate on DirtyLines

### 4. Lock Tab Buffer Once Per Frame, Not Per Line (P1 — Performance)
**Source**: Performance review, lock contention
**Impact**: Reduces 40 lock cycles to 1. 10-40µs P99 improvement under contention
**Effort**: Small — restructure the render loop to extract all data under one lock

### 5. Introduce Focus Stack (P1 — Architecture)
**Source**: GUI review, component model
**Impact**: Eliminates shortcut duplication, enables open widget set, unblocks accessibility
**Effort**: Medium — replace `EditorFocus` enum with `FocusStack`, migrate handlers

### 6. Separate Layout vs. Content Invalidation (P1 — Architecture/Performance)
**Source**: GUI review, rendering invalidation
**Impact**: Skips layout recalculation on content-only changes (the common case)
**Effort**: Medium — split `DirtyRegion` into `InvalidationKind` variants

### 7. Split renderer.rs Into Focused Modules (P1 — Maintainability/Performance)
**Source**: Performance review, renderer monolith
**Impact**: Better I-cache locality, easier to reason about and profile. 5-15% render improvement
**Effort**: Large (116K LOC to reorganize) but mechanical — no logic changes

### 8. Pre-allocate Quad Buffers (P1 — Performance)
**Source**: Performance review, quad buffer allocation
**Impact**: Eliminates ~200KB allocation per full-viewport render. 5-20µs savings
**Effort**: Small — move quad Vec to persistent state, `clear()` per frame

### 9. Defer Tree-sitter Parse for Rapid Typing (P2 — Performance)
**Source**: Performance review, syntax highlighting
**Impact**: Removes 120µs from keystroke critical path during rapid typing
**Effort**: Small — schedule parse on next idle tick, render with stale highlighting

### 10. Add Pane Minimum Size Constraints (P2 — UX)
**Source**: GUI review, layout system
**Impact**: Prevents zero-width panes from deep nesting
**Effort**: Small — add min_size constants and clamp split ratios

---

## What's Already Good

Both reviewers independently noted these strengths:

- **Drain-all-then-render loop**: Eliminates reentrancy bugs and provides natural event batching. Better than what most Cocoa apps achieve.
- **Gap buffer choice**: Right data structure for code editor workloads. Simple, fast, correct.
- **BufferView trait**: Clean polymorphism boundary. Terminal and text buffers render through the same path without special-casing.
- **Glyph atlas with ASCII pre-population**: Steady-state rendering never hits Core Text. The right approach.
- **DirtyLines merge semantics**: Correct, composable, and cheap. Good dirty tracking design.
- **BSP pane layout**: Right choice for a code editor. Simpler than constraint-based layout, sufficient for the use case.
- **Byte-budgeted terminal processing**: Prevents terminal flood from starving the UI. Good engineering.
- **crossbeam-channel for event queue**: Lock-free, low-latency, correct choice.
- **No Rc/RefCell anywhere in the main thread**: Impressive discipline. This prevents an entire category of Cocoa-app bugs.

---

## Measurement Plan

Before implementing any performance recommendations, establish baselines:

1. **Keystroke-to-present latency**: Instrument `drain_loop` from event receive to `MTLCommandBuffer.present()`. Log P50/P95/P99 over 1000 keystrokes.
2. **Frame allocation count**: Use Instruments Allocations to count heap allocations per frame during steady-state typing.
3. **Lock contention**: Use Instruments System Trace to measure mutex wait time on `BufferView` locks during terminal output + rendering.
4. **Dirty region hit rate**: Log how many frames are partial vs. full viewport over a typical editing session.
5. **styled_line() cost**: Time the full `styled_line()` path including tree-sitter highlight queries per line.

These measurements will validate (or refute) the estimated impacts above and help prioritize the work.
