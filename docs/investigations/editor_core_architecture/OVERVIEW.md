---
status: ONGOING
trigger: "Need to define the minimal editor core — the exact boundaries of what lives inside the fast path before any chunks can be planned"
proposed_chunks:
  - prompt: "macOS window + Metal surface: Open a native macOS window with a CAMetalLayer-backed view and render a solid background color via Metal. Prove the Rust-to-Metal pipeline works end-to-end (device, command queue, render pass, present). No event handling beyond window close."
    chunk_directory: metal_surface
    depends_on: []
  - prompt: "Text buffer data structure: Implement a text buffer (rope or gap buffer) with cursor position. Supports insert char, delete char, cursor movement (left, right, up, down, beginning/end of line). Returns dirty line information. Fully unit-tested, no rendering or macOS dependencies."
    chunk_directory: text_buffer
    depends_on: []
  - prompt: "Monospace glyph atlas + text rendering: Load a monospace font via Core Text, rasterize glyphs into a Metal texture atlas, and render hardcoded multi-line text to the Metal surface as textured quads. Layout is trivial: x = col * glyph_width, y = row * line_height."
    chunk_directory: glyph_rendering
    depends_on: [0]
  - prompt: "Viewport + buffer-to-screen rendering: Connect the text buffer to the glyph renderer. Introduce viewport state (scroll offset, visible line range). Render visible buffer lines through the viewport. Implement DirtyRegion enum (None | Lines(from,to) | FullViewport) and only re-render dirty lines."
    chunk_directory: viewport_rendering
    depends_on: [1, 2]
  - prompt: "Main loop + input events + editable buffer: Wire up the drain-all-then-render main loop on NSRunLoop. Hook NSView key events into a buffer focus target (FocusTarget trait) that interprets keystrokes: printable chars insert, Backspace deletes, arrow keys move cursor. Drain all pending events, accumulate dirty regions, render once. Include cursor blink timer."
    chunk_directory: editable_buffer
    depends_on: [3]
created_after: []
---

## Trigger

lite-edit's entire philosophy hinges on a "small, fast core" that owns the input→render critical path. Before we can implement anything, we need to precisely define what that core contains and how its subsystems compose. The GOAL.md describes the north-star (keystroke-to-glyph under 8ms P99) and the constraint (single-threaded critical path, plugins on separate threads), but we haven't designed the core itself.

The core subsystems, refined through exploration (see H1 traces and the focus-target-as-interpreter revision below):

1. **Input capture** — receives all HID events from macOS (keystrokes, scroll, mouse) and delivers them to the active focus target. This is the entry point of the critical path. Delivery is trivial: hand the event to whatever has focus.
2. **Focus targets** — the active focus target interprets its own input. The buffer's focus target owns a chord state machine and resolves key sequences to editor commands. A plugin-provided focus target (minibuffer, completion menu, file picker) handles input with its own logic. The core defines the `FocusTarget` trait; implementations are either built-in (buffer editing) or provided by plugins.
3. **Command execution** — focus targets produce commands; execution runs them against core state (buffer, viewport, cursor) or dispatches them to plugins via lock-free channels.
4. **Render loop + surface** — the tightly-coupled subsystem that, given core state, produces the next frame on a Metal-backed surface. Must be incremental.

Plus **core state**: text buffer (rope or similar), cursor(s), viewport (scroll offset, visible line range), focus stack.

Key insight: chord dispatch is **not** a core subsystem — it's an implementation detail of the buffer's focus target. The core doesn't have a chord system; the buffer editing target does. This means plugins can define focus targets with entirely different input models without the core knowing or caring.

## Success Criteria

1. **Define the responsibility boundary** of each core subsystem with enough precision to write chunk GOALs against them.
2. **Define the data flow** between subsystems: what crosses each boundary, in what form, and who owns it.
3. **Identify the key design decisions** for each subsystem (e.g., how focus targets interpret input, how the render loop decides what's dirty, how input capture avoids blocking).
4. **Establish latency budget allocation** — how the 8ms budget is divided across input capture, focus target handling, buffer mutation, and rendering.
5. **Produce proposed chunks** that can be implemented to build the core.

## Testable Hypotheses

### H1: The original four subsystems (surface, render loop, input queue, chord dispatch) are sufficient to define the core pipeline

- **Rationale**: These cover the full path from keypress to glyph. The text buffer is the data structure they operate on but is a separate concern.
- **Test**: Walk through representative editing operations (insert char, delete word, page-down, Ctrl-A to beginning of line, Ctrl-X Ctrl-F to find file) and verify every step is handled by one of these four + the buffer.
- **Status**: REFINED — Traces revealed gaps (viewport, focus routing, command routing, non-keystroke input). Subsequent analysis produced a revised four-subsystem model: input capture → focus targets → command execution → render loop + surface. Chord dispatch moved from core subsystem to implementation detail of the buffer's focus target. See exploration log entries from 2026-02-21.

### H2: Chord dispatch can be stateless per-event with a small accumulator for multi-key sequences

- **Rationale**: Most commands are single-keystroke or two-key chords (Ctrl-X prefix). A simple state machine with a timeout should suffice — no need for complex grammar parsing.
- **Test**: Enumerate the actual chord patterns we want to support and verify whether a state machine is even needed.
- **Status**: VERIFIED (stronger than expected) — All target chords are single-step modifier+key. Chord dispatch is fully stateless: a pure function from (modifiers, key) → Option<Command>. No accumulator, no timeout, no state machine. See exploration log entry from 2026-02-21.

### H3: The render loop can skip full-frame rendering by tracking a dirty region from buffer mutations

- **Rationale**: Single-character inserts only affect one line (plus possibly line-wrap changes). Redrawing only dirty lines keeps rendering well within budget.
- **Test**: Analyze what buffer operations produce what dirty regions, including multi-glyph operations (kill line, select-all-replace).
- **Status**: VERIFIED WITH NUANCE — Dirty region tracking helps the common case (1-2 lines for typing), but multi-line and whole-buffer operations produce larger dirty regions up to full viewport. Full viewport redraws (~6K glyphs) are still under 1ms on Metal, so the worst case is fine. The dirty region model is simple: `None | Lines(from, to) | FullViewport`. Selection state changes also produce dirty regions independently of buffer mutations. See exploration log.

### H4: Input queue can be a simple single-producer single-consumer ring buffer with no locks on the critical path

- **Rationale**: macOS delivers key events on the main thread. If the render loop also runs on the main thread (as is typical for Metal), the input queue might not even need to be a queue — events could be processed synchronously in the event loop.
- **Test**: Determine whether macOS event delivery and Metal rendering can share the main run loop without introducing latency.
- **Status**: FALSIFIED IN FRAMING, ANSWERED — No custom queue is needed at all. macOS's `NSRunLoop` is the event queue. The entire critical path runs synchronously on the main thread: event arrives → focus target handles → buffer mutates → render if dirty → present. No separate render thread, no CVDisplayLink, no SPSC buffer. Rendering is on-demand, not continuous. See exploration log.

## Exploration Log

### 2026-02-21: H1 — Tracing representative operations through the pipeline

Goal: walk through concrete editing operations and verify that every step on the critical path is owned by one of the four subsystems (input queue, chord dispatch, render loop, surface) plus the text buffer. If any step falls outside these five, H1 is falsified and we need to expand the core.

**Notation**: Each trace shows the critical path as a chain of responsibilities.

---

#### Trace 1: Insert character 'a'

1. **Input queue**: macOS `NSEvent` (keyDown, keyCode='a', no modifiers) captured → enqueued as raw key event
2. **Chord dispatch**: Single key, no active chord prefix → resolves immediately to command `InsertChar('a')`
3. **Text buffer**: Insert 'a' at cursor position. Returns mutation descriptor (line N, column range)
4. **Render loop**: Mutation descriptor marks line N dirty. Recomputes glyph layout for line N. Submits draw call.
5. **Surface**: Metal presents the updated texture/vertex buffer to screen.

✅ All steps accounted for.

---

#### Trace 2: Delete word (Ctrl-Backspace or Alt-Backspace)

1. **Input queue**: `NSEvent` (keyDown, backspace + Alt modifier) → enqueued
2. **Chord dispatch**: Single modified key → resolves to `DeleteWordBackward`
3. **Text buffer**: Find word boundary behind cursor, delete from there to cursor. Returns mutation (line N, possibly spanning to line N-1 if at line start).
4. **Render loop**: Dirty lines N (and N-1 if joined). Re-layout, submit.
5. **Surface**: Present.

✅ All steps accounted for. Note: "find word boundary" is buffer responsibility — it needs to understand word characters. This is a text-buffer concern, not a new subsystem.

---

#### Trace 3: Page Down

1. **Input queue**: `NSEvent` (Page Down key) → enqueued
2. **Chord dispatch**: → `PageDown`
3. **Text buffer**: Cursor moves down by viewport-height lines. No text mutation.
4. ⚠️ **Viewport**: Who owns the viewport (scroll offset, visible line range)? The cursor moved, so the viewport must scroll. Something must compute the new viewport origin.
5. **Render loop**: Entire visible region changed → full redraw (or texture scroll + render new lines).
6. **Surface**: Present.

⚠️ **Gap identified: Viewport/scroll state.** The viewport is the mapping between buffer coordinates and screen coordinates. It's not the text buffer (which is content), not the render loop (which consumes viewport state), and not chord dispatch (which produces commands). **The viewport is a piece of core state that sits between command execution and rendering.**

---

#### Trace 4: Ctrl-A (beginning of line)

1. **Input queue**: `NSEvent` (keyDown, 'a' + Ctrl) → enqueued
2. **Chord dispatch**: → `BeginningOfLine`
3. **Text buffer**: Cursor moves to column 0 of current line. No text mutation.
4. **Render loop**: Old cursor position and new cursor position are dirty (cursor glyph moved). Minimal redraw.
5. **Surface**: Present.

✅ Accounted for, but note: **cursor position** is state. Where does it live? It's logically part of the buffer (it's a position within the text), but it could also be argued it's editor state that sits alongside the buffer. Either way, it's covered — cursor is buffer-adjacent state.

---

#### Trace 5: Ctrl-X Ctrl-F (find file — multi-key chord)

1. **Input queue**: `NSEvent` (keyDown, 'x' + Ctrl) → enqueued
2. **Chord dispatch**: Ctrl-X is a prefix key. Chord state machine enters "Ctrl-X prefix" state. **No command dispatched yet.** Optionally, the status area shows "C-x-" to indicate pending chord.
3. **Input queue**: `NSEvent` (keyDown, 'f' + Ctrl) → enqueued
4. **Chord dispatch**: In "Ctrl-X prefix" state + Ctrl-F → resolves to `FindFile`. Chord state resets.
5. ⚠️ **FindFile execution**: This isn't a buffer operation. It opens a minibuffer/dialog for the user to type a filename. Who handles this?

Per GOAL.md: "keybindings" and "file tree" and "search" are plugins. So `FindFile` is a command that the core dispatches **to a plugin**. The core's job ends at producing the command; the plugin handles the UX of file finding.

But wait — the plugin needs to present UI (a minibuffer, a file picker). That UI needs to render on the surface. So there's an interaction:
- Plugin sends "show minibuffer with prompt 'Find file:'" to the core
- Core's render loop must know how to render a minibuffer
- User types in the minibuffer — input queue captures those keystrokes
- Chord dispatch must know that input is now directed at the minibuffer, not the buffer

⚠️ **Gap identified: Focus/input routing.** When a minibuffer or overlay is active, the input queue's keystrokes need to be routed to the right target. This is a form of **input context** or **focus** management. It's small (a stack or enum of "where does input go right now"), but it's core — you can't defer it to a plugin because it's on the critical path.

⚠️ **Gap identified: Plugin command interface.** The core must have a way to receive commands from chord dispatch and route them. Some commands are core-internal (InsertChar, cursor movement). Others are addressed to plugins (FindFile, LSP operations). The core needs a **command routing** layer that distinguishes internal vs. plugin commands and dispatches accordingly.

---

#### Trace 6: Cursor blink (idle)

1. No input events.
2. **Render loop**: A timer fires (every ~500ms). Toggle cursor visibility. Mark cursor cell dirty.
3. **Surface**: Present.

✅ Accounted for, but note: **the timer** is something. The render loop needs a way to schedule timed redraws that isn't driven by input. On macOS this is `CVDisplayLink` or a `CADisplayLink` or a simple timer in the run loop. This is a render-loop concern, not a new subsystem.

---

#### Trace 7: Scrolling with trackpad

1. **Input queue**: `NSEvent` (scrollWheel, deltaY) → enqueued. Note: this is **not** a key event — the input queue must also handle scroll events (and possibly mouse clicks for cursor placement).
2. **Chord dispatch**: Scroll events bypass chord dispatch entirely — they're not key chords. They route directly to **viewport** adjustment.
3. **Viewport**: Scroll offset updated by deltaY.
4. **Render loop**: Viewport changed → redraw visible region.
5. **Surface**: Present.

⚠️ **Confirms viewport gap.** Also shows that the input queue handles more than keystrokes — it handles all HID events (keys, scroll, mouse). And scroll events bypass chord dispatch, meaning there's routing logic even before chord dispatch.

---

### Summary of gaps found

| # | Gap | Nature | Core or Plugin? |
|---|-----|--------|-----------------|
| 1 | **Viewport state** (scroll offset, visible line range) | State that maps buffer coordinates to screen coordinates. Consumed by render loop, mutated by commands and scroll events. | Core — on the critical path between command execution and rendering. |
| 2 | **Input focus / context routing** | Determines whether keystrokes go to the main buffer, a minibuffer, a completion menu, etc. | Core — on the critical path, before chord dispatch. |
| 3 | **Command routing** (internal vs. plugin) | After chord dispatch produces a command, something must decide whether it's handled internally or sent to a plugin. | Core — on the critical path for internal commands; plugin dispatch can be async/non-blocking for plugin commands. |
| 4 | **Non-keystroke input** (scroll, mouse) | Input queue must handle scroll and mouse events. These bypass chord dispatch and route directly to viewport or cursor placement. | Core — different input types take different paths through the pipeline. |

### 2026-02-21: Revised architecture — focus target as input interpreter

The H1 exploration identified input routing/focus as a gap. The initial fix was to add an "input dispatch" subsystem that classifies events and routes them, with chord dispatch as a sibling subsystem. But this creates a problem: the dispatch function becomes a god function that must understand every focus context, and plugins can't define their own focus targets without modifying core dispatch logic.

**Key insight: the focus target should interpret its own input.** This inverts the control:

- **Old model**: dispatch interprets events based on focus context → produces commands
- **New model**: dispatch delivers events to the active focus target → the target interprets them

This means:

1. **Input delivery becomes trivial.** It's `focus_target.handle(event)`. No match on focus × event type × keymap. Can never become a god function.
2. **Plugins can define focus targets.** A completion menu plugin provides a focus target that handles arrow keys, Enter, Escape. A file finder plugin provides one that handles typing + filtering. The core doesn't know about any of these.
3. **Keymaps are per-target, not global.** The buffer's focus target owns the chord state machine with the editing keymap. A minibuffer's focus target owns a simpler one. No global keymap merger needed.
4. **Chord dispatch is demoted from core subsystem to implementation detail.** The core doesn't have a chord system. The buffer editing target does. Other focus targets might not use chords at all.

The core's contract with focus targets is a small trait:

```rust
trait FocusTarget {
    fn handle_key(&mut self, event: KeyEvent, ctx: &mut EditorContext) -> Handled;
    fn handle_scroll(&mut self, delta: ScrollDelta, ctx: &mut EditorContext);
    fn handle_mouse(&mut self, event: MouseEvent, ctx: &mut EditorContext);
}
```

Where `EditorContext` provides access to buffer, viewport, cursor, and the ability to push/pop focus targets and request redraws.

**Latency concern**: a plugin-provided focus target runs on the critical path. Mitigation: focus targets are Rust (no interpreter overhead), and the core's built-in buffer target is trusted. Plugin targets are simple UI handlers (completion menus, minibuffers) — not complex enough to blow the latency budget. If needed, a deadline could be enforced, but this is likely premature.

**Revised core subsystems:**

1. **Input capture** — HID events from macOS
2. **Focus targets** — each interprets its own input (buffer target has chord dispatch built in; plugins provide their own)
3. **Command execution** — focus targets produce commands, execution runs them
4. **Render loop + surface** — draw the frame

Plus core state: text buffer, cursor(s), viewport, focus stack.

This mirrors macOS's own `NSResponder` pattern — events go to the first responder, which handles or passes them up.

#### Re-tracing key operations under the revised model

**Insert 'a':**
1. Input capture → KeyEvent('a')
2. Deliver to active focus target (buffer target)
3. Buffer target's chord resolver: no prefix, resolves to `InsertChar('a')`
4. Buffer target calls `ctx.buffer.insert('a')`, gets dirty region
5. Buffer target calls `ctx.request_redraw(dirty_region)`
6. Render loop redraws dirty region → surface presents

✅ Clean. Chord dispatch is internal to buffer target.

**Trackpad scroll:**
1. Input capture → ScrollEvent(deltaY)
2. Deliver to active focus target (buffer target)
3. Buffer target's `handle_scroll`: adjusts `ctx.viewport.scroll_offset`
4. Buffer target calls `ctx.request_redraw(full_viewport)` 
5. Render loop redraws → surface presents

✅ Clean. No chord dispatch involved. Focus target handles scroll directly.

**Ctrl-X Ctrl-F (find file, plugin command):**
1. Input capture → KeyEvent('x', Ctrl)
2. Deliver to buffer target → chord resolver enters Ctrl-X prefix state → Handled (no command yet)
3. Input capture → KeyEvent('f', Ctrl)
4. Deliver to buffer target → chord resolver completes: `FindFile` command
5. Buffer target dispatches `FindFile` via `ctx.dispatch_plugin_command(FindFile)`
6. Plugin receives command, calls `ctx.push_focus(MinibufferTarget::new("Find file: ", callback))`
7. Subsequent keystrokes go to MinibufferTarget, which handles its own input
8. On completion, MinibufferTarget pops itself, focus returns to buffer target

✅ Clean. Plugin defines its own focus target. Core never knows what "find file" UI looks like.

**Completion menu (plugin focus target):**
1. Plugin pushes `CompletionMenuTarget` onto focus stack
2. Input capture → KeyEvent(ArrowDown)
3. Deliver to CompletionMenuTarget → moves selection down → requests redraw
4. Input capture → KeyEvent(Enter)
5. CompletionMenuTarget → applies selection, pops itself from focus stack
6. Focus returns to buffer target

✅ Clean. Core has no knowledge of completion menu keybindings.

### 2026-02-21: H4 — Input capture architecture on macOS

H4 asks whether the input queue can be a simple SPSC ring buffer. The deeper question: do we even need a queue?

#### How macOS delivers events

macOS delivers all input events (key, mouse, scroll) on the **main thread** via `NSRunLoop`. The standard flow:

1. User presses key → IOKit HID → WindowServer → `NSEvent` delivered to app's main run loop
2. App receives event in `sendEvent:` or via `nextEventMatchingMask:untilDate:inMode:dequeue:`
3. Event handler runs synchronously on the main thread

This is non-negotiable — Cocoa requires UI event handling on the main thread.

#### How Metal rendering works on macOS

Metal can render from any thread, but:
- `CAMetalLayer` (the standard way to get a Metal surface in a view) is main-thread-friendly
- `nextDrawable` on `CAMetalLayer` gives you a drawable to render into
- Presentation (`present()`) schedules the drawable for display at the next vsync
- `CVDisplayLink` fires a callback on a **separate thread** at approximately display refresh rate — but Tristan Hume's analysis shows it's just a timer, not actually synced to vsync (though later corrected for single-monitor setups via shared memory)
- Zed switched from `CADisplayLink` to `CVDisplayLink` to support ProMotion displays (variable refresh rate)

#### Key insight: a text editor is not a game

Games need a continuous render loop (CVDisplayLink/CADisplayLink) because every frame may differ — physics, animation, particles. A text editor is **event-driven**: the screen only changes when:

1. User types, scrolls, or clicks (input events)
2. Cursor blinks (timer, ~2Hz)
3. A plugin delivers async results (LSP completions, syntax highlights)

Between these events, the screen is static. There is **no reason to render at 60/120fps continuously**. The GOAL.md's 2% idle CPU constraint actually demands we NOT spin a render loop when nothing is happening.

This means the optimal architecture is **drain-all-then-render**: on each run loop iteration, consume every pending event before rendering once. The main loop looks like:

```
loop {
    // Phase 1: DRAIN — consume all pending events, accumulating dirty regions
    while let Some(event) = next_pending_event() {
        focus_target.handle(event, &mut ctx);  // mutates buffer/viewport, accumulates dirty
    }
    // Also drain plugin results (LSP, highlighting) from async channels
    while let Some(result) = plugin_channel.try_recv() {
        apply_plugin_result(result, &mut ctx);  // may also dirty regions
    }

    // Phase 2: RENDER — one
    if ctx.dirty_region != DirtyRegion::None {
        render(ctx.dirty_region);
        present();
        ctx.dirty_region = DirtyRegion::None;
    }

    // Phase 3: SLEEP — wait for next event (NSRunLoop handles this)
    wait_for_events_or_timers();
}
```

**No queue. No separate render thread. No CVDisplayLink.**

The drain-all-then-render pattern is critical for latency fairness. Consider a user typing "abc" fast enough that all three keystrokes arrive before we can process the first:

**Naive (render per event):**
```
event 'a' → handle → render → present   (latency: handle + render)
event 'b' → handle → render → present   (latency: waited for a's render + handle + render)
event 'c' → handle → render → present   (latency: waited for a+b's renders + handle + render)
```
Event 'c' pays for two unnecessary intermediate renders. Those renders were wasted — nobody saw their output.

**Drain-then-render:**
```
event 'a' → handle → dirty line 5
event 'b' → handle → dirty line 5 (merged)
event 'c' → handle → dirty line 5 (merged)
no more pending → render once → present
```
All three events experience roughly equal latency: the cost of handling all events (microseconds) plus one render. No intermediate frames that nobody will see. Later events don't pay for earlier events' renders.

This is the lowest-latency architecture possible because:

1. **Zero queuing delay.** Events are handled the instant they're drained. No scheduling hops.
2. **Zero thread synchronization.** Everything is on the main thread. No locks, no atomics on the critical path.
3. **Zero wasted frames.** We only render the final state after all pending mutations are applied. Intermediate states that nobody would see are never rendered.
4. **Latency fairness.** Every event in a batch experiences similar latency. No event is penalized for arriving after others in the same batch.

The "queue" is `NSRunLoop` itself — the native macOS event queue. We don't build our own.

#### What about plugin async results?

Plugins (LSP, syntax highlighting) run on background threads and produce results asynchronously. These results need to reach the main thread to update state and trigger rendering.

The standard macOS pattern: use `CFRunLoopSource` or `dispatch_async(dispatch_get_main_queue(), ...)` to wake the main run loop and deliver results. This is exactly how Cocoa apps handle background work.

From the main thread's perspective, plugin results arrive as just another "event" in the run loop — handled synchronously like any other input. No special queue needed for the critical path.

#### What about smooth scrolling?

Trackpad scroll events arrive at high frequency (~120Hz on ProMotion) as a stream of small deltas. Each scroll event:

1. Arrives on main thread via NSRunLoop
2. Focus target adjusts viewport
3. Render dirty region (full viewport for scroll)
4. Present

At 120Hz, each frame has ~8.3ms budget. Given that a full viewport redraw is <1ms of work (per H3 analysis), this is comfortable. The scroll events themselves drive the "render loop" — no CVDisplayLink needed.

For momentum scrolling (finger lifted, scroll continues), macOS continues delivering synthetic scroll events with decaying deltas. Same mechanism, no special handling needed.

#### What about cursor blink?

The cursor needs to toggle visibility every ~500ms. This is a simple `NSTimer` (or `CFRunLoopTimer`) on the main run loop:

1. Timer fires on main thread
2. Toggle cursor visibility
3. Dirty region: 1 line (cursor line)
4. Render and present

2Hz rendering of 1 line. Trivial. The timer is paused while typing (cursor stays solid) and resumed after a brief idle period.

#### What about ProMotion (variable refresh rate)?

ProMotion displays (MacBook Pro) support 24-120Hz variable refresh rate. The display can match the app's presentation rate. Since we only present when something changes:

- Typing at 120 WPM ≈ 10 chars/sec → 10 presents/sec → display runs at ~10Hz
- Fast scrolling → presents per scroll event → display runs at up to 120Hz
- Idle → cursor blink at 2Hz → display runs at very low rate

This is ideal for ProMotion — the display adapts to our actual update rate. No CVDisplayLink needed to "match" the refresh rate because we're not trying to hit a fixed framerate.

#### Conclusion

H4 is **falsified in framing but answered**: We don't need a SPSC ring buffer or any custom queue. The input "queue" is `NSRunLoop` itself. The architecture is:

- **Main thread only** for the entire critical path (input → focus target → command → buffer → render → present)
- **NSRunLoop drives everything**: input events, timers (cursor blink), and plugin result delivery (via CFRunLoopSource / dispatch_async)
- **Render-on-demand**: only render when dirty, never spin a continuous loop
- **Batch input**: drain all pending events per run loop iteration, render once

This is simpler than H4 hypothesized. There is no "input capture" subsystem to build — we just hook into macOS's existing event delivery. The first subsystem in our architecture (input capture) shrinks to: "register NSView/NSWindow event handlers that forward to the active focus target."

### 2026-02-21: H3 — Dirty region analysis for incremental rendering

H3 claims the render loop can skip full-frame rendering by tracking dirty regions. The test: analyze what buffer operations produce what dirty regions, including multi-glyph operations like kill-line and select-all-then-replace.

#### Defining "dirty region"

A dirty region is the set of screen lines that must be re-rendered after a buffer mutation. The render loop's job is: given the previous frame and a dirty region, produce the next frame by only re-laying-out and re-drawing the dirty lines. If the dirty region is the entire viewport, we fall back to a full redraw — which must still be fast enough.

The unit of dirtiness is a **screen line** (a row of glyphs on the surface), not a buffer line (which may wrap across multiple screen lines).

#### Operation-by-operation dirty region analysis

**Category 1: Single-line mutations (bounded dirty region)**

| Operation | Dirty region | Notes |
|-----------|-------------|-------|
| Insert char | 1 screen line (current line) | Unless it causes line wrap to change, then current + next |
| Delete char (Backspace) | 1 screen line | Unless it un-wraps, then current + previous |
| Cursor move (arrows, Home, End) | 2 screen lines (old + new cursor position) | Just cursor glyph change, no text re-layout needed |
| Cursor blink | 1 screen line | Toggle cursor visibility |

These are the fast path. 1-2 lines dirty. Trivially within budget.

**Category 2: Multi-line mutations (bounded but larger)**

| Operation | Dirty region | Notes |
|-----------|-------------|-------|
| **Ctrl-K (kill line)** | Current line + all lines below in viewport | Deleting a line shifts everything below it up by one line. Every line from the kill point to the bottom of the viewport must be re-drawn at its new position. |
| Delete word (across line boundary) | 2+ lines if join occurs | Joining lines dirties current line + shifts everything below |
| Enter (insert newline) | Current line + all lines below in viewport | Splits a line, pushing everything below down. Symmetric with kill-line. |
| Paste multi-line text | All lines from paste point to bottom of viewport | Similar to multiple newline insertions |

These are **viewport-bounded**: the dirty region is at most "cursor line to bottom of viewport." On a typical 50-line viewport, that's 50 lines max. Still not a full redraw — lines above the mutation are untouched.

**Category 3: Whole-buffer mutations (full viewport dirty)**

| Operation | Dirty region | Notes |
|-----------|-------------|-------|
| **Cmd-A then type char (select all + replace)** | Entire viewport | Buffer content is completely replaced. Every visible line changes. |
| **Cmd-A then Backspace (select all + delete)** | Entire viewport | Buffer is now empty. Every line must clear. |
| Page Down / Page Up | Entire viewport | Scroll by a full page. Every visible line is new content. |
| Trackpad scroll (large delta) | Entire viewport | Same as page down if delta exceeds viewport height. |
| Find-and-replace all | Entire viewport (potentially) | If replacements span visible lines. |

These are **full redraws**. The dirty region equals the viewport.

#### Key question: Can full redraws fit in the 8ms budget?

Full redraws are unavoidable for Category 3 operations. The question isn't "can we avoid them" but "are they fast enough?"

Let's estimate. A full viewport redraw means:
- ~50 visible lines × ~120 columns = ~6,000 glyphs
- Each glyph is a textured quad (2 triangles, 6 vertices or 4 vertices + index buffer)
- Total: ~6,000 quads = ~24,000 vertices

For Metal on any modern Mac GPU, rendering 24K textured vertices is trivially fast — well under 1ms. The bottleneck is not GPU draw time but **glyph layout** (computing which glyph goes where, handling font metrics, tabs, etc.) and **glyph atlas management** (ensuring glyphs are rasterized and in the texture atlas).

Glyph layout for 6,000 characters with a monospace font is simple: `x = column * glyph_width`, `y = row * line_height`. No complex shaping for a monospace code editor. This is a tight loop that should complete in well under 1ms for 6K glyphs.

So: **full redraws are fast enough**. The incremental dirty region tracking is an optimization that makes common operations (typing) nearly free, but the worst case (full viewport) is still well within budget.

#### Dirty region strategy

The analysis suggests a **three-tier approach**:

1. **Cursor-only dirty** (cursor blink, cursor move): Redraw 1-2 lines. Cheapest path.
2. **Line-range dirty** (insert/delete char, kill line, insert newline): Redraw from mutation point to bottom of viewport. Middle path.
3. **Full viewport dirty** (select-all-replace, page down, large scroll): Redraw everything visible. Most expensive but still fast.

The render loop doesn't need complex dirty region geometry (rectangles, unions, etc.). A simple representation works:

```rust
enum DirtyRegion {
    None,                          // No redraw needed (idle)
    Lines { from: usize, to: usize }, // Redraw screen lines [from, to)
    FullViewport,                   // Redraw everything visible
}
```

Where `Lines` can express both "just line 5" (`from: 5, to: 6`) and "line 5 to bottom" (`from: 5, to: viewport_height`).

#### Deep dive: Ctrl-K (kill line)

Let's trace this carefully since it's the most interesting Category 2 case:

1. Cursor is on line 20 (screen line 20 of 50 visible lines, showing buffer lines 100-149).
2. Ctrl-K kills buffer line 120 (the line at the cursor).
3. Buffer lines 121-149 shift up to positions 120-148.
4. The viewport now shows buffer lines 100-148 (one fewer line of content, unless the buffer has more lines below, in which case line 149 scrolls in).
5. **Dirty region**: screen lines 20-49 (the kill point to the bottom). Lines 0-19 are unchanged.

Can we optimize this? A GPU-side texture scroll (copy lines 21-49 up by one line height, then render only the new bottom line) would reduce it to 1 line of re-rendering. But this adds complexity. Given that rendering 30 lines takes well under 1ms anyway, **the simple approach (re-render lines 20-49) is fine and we should avoid premature optimization**.

#### Deep dive: Cmd-A + type char (select all + replace)

1. Cmd-A selects all text. **No dirty region yet** — selection highlighting might dirty the viewport, but the selection itself is just state (a range). Actually: if we render selection as a background color behind selected text, then Cmd-A dirties the entire viewport (every line gets a selection background). This is a full viewport redraw.
2. User types 'x'. The entire selection is replaced with 'x'. Buffer is now a single character.
3. **Dirty region**: Full viewport. Every line that had content must be cleared, and line 0 must show 'x'.

This is two consecutive full viewport redraws (one for selection highlight, one for replacement). Both are fine — each is under 1ms of work.

Note: **selection rendering** is itself a source of dirty regions that doesn't involve buffer mutation. The dirty region system must handle "visual state changed" (selection, cursor) not just "buffer content changed."

#### Conclusion

H3 is **verified with nuance**:
- Dirty region tracking is valuable for the common case (typing) where 1-2 lines are dirty.
- Multi-line operations (kill line, insert newline) dirty a range from mutation to viewport bottom — still cheap.
- Whole-buffer operations (select-all-replace, page scroll) require full viewport redraws — these are unavoidable but still fast (~6K glyphs, under 1ms on Metal).
- The dirty region model is simple: `None | Lines(from, to) | FullViewport`. No complex geometry needed.
- Selection state changes also produce dirty regions independently of buffer mutations.

### 2026-02-21: H2 — Testing chord dispatch complexity against actual chord set

H2 hypothesized that chord dispatch needs "a small accumulator for multi-key sequences" (e.g., Emacs-style Ctrl-X Ctrl-F prefixes). Let's test this against the actual target chord set.

#### Target chords

| Chord | Command | Modifiers | Key |
|-------|---------|-----------|-----|
| Cmd-P | File picker | Command | P |
| Cmd-S | Save | Command | S |
| Cmd-W | Close tab | Command | W |
| Cmd-A | Select all | Command | A |
| Cmd-Shift-P | Command palette | Command+Shift | P |

Plus implicit basic editing:
| Input | Command | Modifiers | Key |
|-------|---------|-----------|-----|
| Any printable char (no Cmd) | InsertChar | None (or Shift) | * |
| Backspace | DeleteBackward | None | Backspace |
| Arrow keys | CursorMove | None/Shift/Alt/Cmd | Arrow |
| Enter | InsertNewline | None | Return |
| Tab | InsertTab / Indent | None | Tab |

#### Analysis

**Every chord is a single keystroke with modifiers.** There are zero multi-key sequences. No prefixes, no accumulators, no timeouts.

This means chord dispatch in the buffer's focus target is a **pure function**:

```rust
fn resolve(modifiers: Modifiers, key: Key) -> Option<Command>
```

No state. No accumulator. No timeout. A single `match` expression.

#### Ambiguity check

The only potential ambiguity is between chords that share a key but differ by modifiers:

- **Cmd-P** (file picker) vs **Cmd-Shift-P** (command palette): Distinguished by Shift modifier. No ambiguity — modifiers are part of the key event, not sequenced.
- **P with no Command** (insert 'p'/'P'): Distinguished by absence of Command modifier. No ambiguity.

No two chords map to the same (modifiers, key) pair. The mapping is a simple injective function.

#### What about future extensibility?

The earlier Ctrl-X Ctrl-F trace assumed Emacs-style prefix chords. The actual chord set doesn't use these. But should the architecture support multi-key sequences for plugins?

Under the focus-target-as-interpreter model, this is already handled naturally:
- The buffer's built-in focus target uses a stateless resolver (it doesn't need prefixes).
- If a **plugin** wanted Vim-style `gg` or Emacs-style `C-x C-f`, it would provide its own focus target with its own stateful resolver. The core doesn't need to support this — the plugin does.

This reinforces the focus-target architecture: the core's chord resolution is trivially simple because the core only handles macOS-native modifier chords. Exotic input models are pushed to plugin focus targets.

#### Impact on earlier traces

The H1 Ctrl-X Ctrl-F trace (find file via two-key chord) was based on an assumed Emacs model. Under the actual chord set, find-file is **Cmd-P** (single keystroke):

1. Input capture → KeyEvent('p', Command)
2. Deliver to buffer focus target
3. Buffer target resolves: Cmd-P → `FindFile`
4. Buffer target dispatches `FindFile` via `ctx.dispatch_plugin_command(FindFile)`
5. Plugin pushes file picker focus target

Simpler than the original trace. No prefix state, no intermediate "chord pending" step.

#### Conclusion

H2 is verified **more strongly than hypothesized**. The hypothesis asked whether a "simple state machine with a small accumulator" would suffice. The answer is that **no state machine is needed at all** — chord dispatch is a pure stateless function for the target chord set. Multi-key sequences, if ever needed, would be a plugin concern via custom focus targets.

## Findings

### Verified Findings

- **The core pipeline is four subsystems plus core state.** Two rounds of analysis (H1 traces → focus-target revision) converged on this model:
  1. **Input capture** — receives all HID events (keys, scroll, mouse) from macOS
  2. **Focus targets** — the active target interprets its own input; the buffer target owns chord dispatch internally; plugins provide their own targets
  3. **Command execution** — runs commands against core state or dispatches to plugins
  4. **Render loop + surface** — incremental rendering to a Metal-backed surface

  Plus **core state**: text buffer, cursor(s), viewport, focus stack.

- **Chord dispatch is not a core subsystem.** It's an implementation detail of the buffer's focus target. The core defines the `FocusTarget` trait but has no built-in notion of key chords. This is the critical architectural insight — it keeps the core small and lets plugins define focus targets with entirely different input models.

- **Chord dispatch is stateless.** All target chords (Cmd-P, Cmd-S, Cmd-W, Cmd-A, Cmd-Shift-P) are single-step modifier+key combinations. The buffer focus target's chord resolver is a pure function `(modifiers, key) → Option<Command>` — no state machine, no accumulator, no timeout. Multi-key sequences (Emacs/Vim-style) are not needed for the target chord set and would be a plugin concern if ever wanted.

- **Input delivery is trivial by design.** Because focus targets interpret their own input, the delivery layer is just `focus_target.handle(event)`. There is no dispatch god function, no global keymap, no event × focus matrix.

- **The original "input queue" was too narrow.** It's really "input capture" — all HID events, not just keystrokes.

- **The original "surface" and "render loop" are one subsystem.** The surface is the output target of the render loop. No meaningful boundary between them on the critical path.

- **Focus is a stack, not a single value.** Plugins push focus targets (minibuffer, completion menu) and pop them on completion. This handles nested interactions (e.g., completion menu inside a minibuffer) naturally.

- **Dirty region tracking is simple and sufficient.** Three tiers: cursor-only (1-2 lines), line-range (mutation point to viewport bottom), and full viewport. The model is `None | Lines(from, to) | FullViewport` — no complex rectangle geometry. Full viewport redraws (~6K monospace glyphs) are under 1ms on Metal, so the worst case (select-all-replace, page scroll) is well within the 8ms budget. Incremental rendering is an optimization for the common case, not a correctness requirement.

- **Dirty regions come from two sources**: buffer mutations (insert, delete, kill line) and visual state changes (selection highlight, cursor blink). The render loop must track both.

- **There is no input queue to build.** macOS's `NSRunLoop` is the event queue. The entire critical path runs synchronously on the main thread. No separate render thread, no CVDisplayLink, no custom SPSC buffer.

- **The main loop is drain-all-then-render.** Each run loop iteration: (1) drain all pending events, forwarding each to the active focus target which mutates state and accumulates dirty regions, (2) drain plugin async results, (3) render once if dirty, (4) sleep until next event or timer. This ensures latency fairness — no event is penalized by intermediate renders of events ahead of it in the same batch. It also eliminates wasted frames for intermediate states nobody would see.

- **Rendering is on-demand, not continuous.** The editor only renders when something changes (input, cursor blink timer, plugin result delivery). This naturally satisfies the 2% idle CPU constraint and works well with ProMotion variable refresh rate displays.

- **"Input capture" shrinks to near-nothing.** The first subsystem in the architecture is just "register NSView/NSWindow event handlers that forward to the active focus target." There's no meaningful code to build — it's Cocoa boilerplate.

### Hypotheses/Opinions

- Viewport state is small enough (scroll offset, visible line range, a few derived values) that it doesn't need its own subsystem — it's a struct inside `EditorContext` that focus targets mutate and the render loop reads.

- Plugin-provided focus targets on the critical path are acceptable because they're compiled Rust with no interpreter overhead. The buffer's built-in focus target handles the latency-sensitive case (typing). Plugin targets handle transient UI (menus, dialogs) where a few extra microseconds are invisible.

- Command routing might be as simple as: if the command is in a known set of internal commands, execute it inline; otherwise, push it to a lock-free channel that plugins poll.

## Proposed Chunks

The goal is a working editable buffer: open the app, see text, type characters, delete characters, move the cursor with arrow keys, and see results rendered in real-time via Metal. The chunks below build toward that milestone in dependency order.

### 1. macOS window + Metal surface

Get a native macOS window open with a `CAMetalLayer`-backed view. Render a solid color to prove the Metal pipeline works end-to-end (device, command queue, render pass, present). This is the foundation — nothing else can be visually verified without it.

- Deliverable: A Rust binary that opens a macOS window and clears it to a background color using Metal.
- Key decisions: How to bridge Rust ↔ Cocoa/Metal (raw `objc` crate, `metal-rs`, `cocoa` crate, or direct FFI).
- No event handling yet beyond window close.

### 2. Text buffer data structure

The rope (or gap buffer) that holds text content, plus cursor position. Supports: insert character at cursor, delete character at cursor, move cursor (left, right, up, down, beginning/end of line). Returns dirty information (which lines changed). Fully independent of rendering — tested via unit tests.

- Deliverable: A `TextBuffer` type with methods for insert, delete, cursor movement, and line content access.
- Key decisions: Rope vs gap buffer (gap buffer is simpler for v1, rope needed for large files).
- No rendering, no macOS dependencies.

### 3. Monospace glyph atlas + text rendering

Render a static string of text to the Metal surface from chunk 1. This requires: loading a monospace font via Core Text, rasterizing glyphs into a texture atlas, building a vertex buffer of textured quads (one per glyph), and a Metal shader that draws them. Layout is trivial: `x = col * glyph_width`, `y = row * line_height`.

- Deliverable: The window from chunk 1 now displays hardcoded multi-line text rendered in a monospace font.
- Key decisions: Atlas sizing and growth strategy, font selection API (hardcoded font for now is fine).
- Depends on: chunk 1 (Metal surface).

### 4. Viewport + buffer-to-screen rendering

Connect the text buffer (chunk 2) to the glyph renderer (chunk 3). Introduce viewport state (scroll offset, visible line range). Given buffer content + viewport, render the visible lines to the surface. Implement the `DirtyRegion` enum (`None | Lines(from, to) | FullViewport`) and only re-render dirty lines.

- Deliverable: A window that displays the contents of a `TextBuffer` through a viewport. Programmatic buffer mutations (in test code) cause the display to update correctly.
- Key decisions: How viewport state is stored and passed to the renderer.
- Depends on: chunks 2 and 3.

### 5. Main loop + input events + editable buffer

Wire up the drain-all-then-render main loop. Hook NSView key events into a buffer focus target that interprets keystrokes: printable characters → `InsertChar`, Backspace → `DeleteBackward`, arrow keys → cursor movement. Each handler mutates the buffer and accumulates dirty regions. After draining all pending events, render once if dirty.

- Deliverable: A fully interactive editable buffer. User can type text, delete text, and move the cursor, with all changes rendered in real-time via Metal. The drain-all-then-render loop is in place. Cursor blink timer works.
- Key decisions: FocusTarget trait shape, EditorContext contents, how cursor blink timer integrates with NSRunLoop.
- Depends on: chunk 4 (viewport + rendering connected to buffer).

This is the milestone: **an editable buffer rendered via Metal with the core architectural patterns (focus targets, drain-then-render, dirty regions) in place.** Everything after this — plugins, LSP, syntax highlighting, tabs, file management — builds on top of this foundation.

## Resolution Rationale

<!-- To be written when investigation reaches a terminal status. -->
