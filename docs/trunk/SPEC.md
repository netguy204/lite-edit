# Specification

<!--
This document is the contract. It defines WHAT the system does with enough
precision that you could write a conformance test suite against it.

The spec can evolve, but changes should be deliberate. When you modify this
document, consider what downstream artifacts (chunks, implementations, tests)
need to be updated.

Mark sections as DRAFT if they're not yet solidified.
-->

## Overview

<!--
A brief summary of what this specification defines. One or two paragraphs
that orient the reader before diving into details.
-->

## Terminology

<!--
Define terms that have specific meanings in this project. This prevents
ambiguity and ensures everyone (including agents) uses language consistently.

Example:
- **Message**: A single unit of data written to the queue
- **Segment**: A file containing a sequence of messages
- **Acknowledgment**: Confirmation that a message has been processed
-->

## Data Format

<!--
If the system has a persistent representation (file format, wire protocol,
database schema), define it precisely here.

Include:
- Layout and structure
- Field definitions with types and valid ranges
- Encoding details (endianness, string encoding, etc.)
- Versioning scheme
- Checksums or integrity mechanisms

Be precise enough that someone could implement a parser from this description.
-->

## API Surface

<!--
Define the operations the system supports. For each operation:
- Name and signature
- Preconditions (what must be true before calling)
- Postconditions (what will be true after calling)
- Error conditions and how they're signaled
- Concurrency semantics (thread-safe? blocking? async?)

Example:
### write(message: bytes) -> MessageId
Appends a message to the queue.
- Preconditions: message.length > 0, message.length <= MAX_MESSAGE_SIZE
- Postconditions: message is durably stored, MessageId is unique
- Errors: QueueFull, MessageTooLarge, IOError
- Concurrency: Thread-safe, may block during fsync
-->

## Guarantees

<!--
What properties does the system guarantee? Be precise about conditions.

Examples:
- Durability: "A message is durable once write() returns. Durable means
  the message will survive process crash and OS crash, assuming no
  storage hardware failure."
- Ordering: "Messages are read in the order they were written within
  a single segment. Cross-segment ordering requires..."
- Delivery: "Each message will be delivered at least once. Exactly-once
  requires external deduplication."

Also specify what is NOT guaranteed if it might be assumed.
-->

## Performance Requirements

<!--
Quantitative requirements that implementations must meet.

Examples:
- Throughput: >= 50,000 messages/second for 1KB messages
- Latency: P99 write latency <= 10ms
- Space: Overhead per message <= 32 bytes
- Recovery: Queue must be readable within 5 seconds of process start

Specify measurement conditions (hardware class, message size, queue depth).
-->

## Limits

<!--
Hard limits that define the boundaries of correct operation.

Examples:
- Maximum message size: 16 MB
- Maximum messages per segment: 1,000,000
- Maximum queue depth: Limited by available disk space
- Maximum concurrent readers: 64

Specify what happens when limits are exceeded (error, undefined behavior, etc.)
-->

## Versioning and Compatibility

<!--
How does the spec evolve over time?
- How are versions identified?
- What compatibility guarantees exist between versions?
- How should implementations handle unknown versions?
-->

## Terminal Rendering

### Cursor Visibility

Terminal cursor visibility has two independent mechanisms that must both be
respected:

1. **DECTCEM mode** (`ESC[?25l` / `ESC[?25h`): Controls whether the terminal
   cursor is visible. Tracked by `TermMode::SHOW_CURSOR`. When off, the cursor
   must not be drawn regardless of the cursor style shape.

2. **Cursor style shape**: The configured cursor appearance (Block, Beam,
   Underline, Hidden). Set by `DECSCUSR` escape sequences.

`cursor_info()` mirrors alacritty's `RenderableCursor::new()`: when
`SHOW_CURSOR` is off, it forces `CursorShape::Hidden` regardless of the
configured style. Both rendering paths (non-wrap and wrap-aware) skip cursor
drawing when the shape is Hidden.

Programs like Claude Code (via Ink) permanently hide the terminal cursor and
render a simulated cursor using reverse-video text (SGR 7). The terminal
emulator must not draw its own cursor on top of this.

### Background Quad Emission

Each text span may produce a background quad (a colored rectangle behind the
text). Background quads are emitted when any of these conditions hold:

- The span's `bg` color is not `Color::Default` (explicit background)
- The span's `inverse` flag is set (SGR 7, reverse video)

The second condition is critical: inverse video swaps foreground and background
colors at resolve time (`resolve_style_colors`), so a span with default colors
plus `inverse: true` resolves to a light background on dark text. Without
emitting the background quad, inverse-video text would be invisible against
the terminal's default background.

This affects programs that simulate cursors with reverse-video text
(e.g., Ink's `ESC[7m T ESC[27m` renders a highlighted `T` as a fake cursor).

### Grid/Viewport Synchronization

When the window resizes, pane splits/unsplits, or any layout change alters
the content area, `sync_pane_viewports()` resizes the alacritty grid and PTY
to match:

- Terminal rows = `floor(pane_content_height / line_height)`
- Terminal cols = `floor(pane_width / advance_width)`
- The PTY receives the updated `TIOCGWINSZ` so programs see correct
  `$COLUMNS`/`$LINES`

The grid dimensions must match the viewport's `visible_lines` at all times.
A mismatch causes cursor misalignment: programs position their cursor based
on grid geometry, but the viewport renders based on its own dimensions.

## DRAFT Sections

<!--
List any sections above that are not yet finalized. Remove this section
once all parts of the spec are solidified.

- [ ] Section name: reason it's still draft
-->