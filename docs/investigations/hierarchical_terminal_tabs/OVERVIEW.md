---
status: ONGOING
trigger: "Exploring a unified tab model where tabs are terminal-backed buffers, with hierarchical grouping for agent workspaces"
proposed_chunks:
  - prompt: "Introduce BufferView trait with StyledLine/Span/Style types in the buffer crate. Style must support terminal-grade attributes: fg/bg color, bold, italic, dim, underline (5 variants + color), strikethrough, inverse, hidden. Include CursorInfo (position, shape, blinking). Implement BufferView for TextBuffer. Migrate renderer from &[&str] to &dyn BufferView."
    chunk_directory: null
    depends_on: []
  - prompt: "Renderer enhancements for styled content: per-cell background color rendering (background rect pass before glyph pass), cursor shape rendering (block, beam, underline), underline rendering. These are prerequisites for terminal display but also benefit syntax-highlighted file editing."
    chunk_directory: null
    depends_on: [0]
  - prompt: "Add TerminalBuffer backed by alacritty_terminal implementing BufferView. Wire PTY I/O via portable-pty or raw PTY. Full terminal emulation: alternate screen, wide chars, scroll regions. Prove it can render shell output through the same pipeline as file content."
    chunk_directory: null
    depends_on: [1]
  - prompt: "Terminal input encoding: build keystroke-to-escape-sequence mapping layer. Handle mode-dependent encoding (APP_CURSOR, BRACKETED_PASTE, KITTY_KEYBOARD). Encode arrow keys, function keys, modifier combos (Ctrl-C, etc). Mouse event encoding per mouse mode (click, motion, drag, SGR). Route input through tab dispatch layer."
    chunk_directory: null
    depends_on: [2]
  - prompt: "File-backed scrollback for terminal buffers. As lines scroll off alacritty's in-memory grid, convert to StyledLine and append to a compact on-disk log. Implement paging so BufferView::styled_line() transparently serves from memory or disk. Target: unlimited scrollback with bounded memory (~6.6MB per terminal regardless of history length)."
    chunk_directory: null
    depends_on: [2]
  - prompt: "Workspace model and left rail UI. Implement Editor -> Vec<Workspace> -> Vec<Tab> data model. Render vertical left rail (~48-64px) with workspace tabs showing labels (branch name) and status indicators (running/needs-input/errored/idle). Clicking swaps content area. Keyboard: Cmd+1..9 for workspace switching. Always visible, even with one workspace."
    chunk_directory: null
    depends_on: [0]
  - prompt: "Content tab bar within workspace. Horizontal tab bar at top of content area for the active workspace's open tabs. Heterogeneous tabs (file, terminal, diff) unified through BufferView. Keyboard: Cmd+Shift+]/[ to cycle, Cmd+W to close. Unread badges on terminal tabs with new output since last viewed."
    chunk_directory: null
    depends_on: [5]
  - prompt: "Agent lifecycle management. Implement AgentHandle wrapping a TerminalBuffer with state inference (Starting, Running, NeedsInput, Stale, Exited, Errored). PTY idle detection for Running->NeedsInput transition. Wire agent state to workspace left-rail indicators. Support agent launch, restart, and graceful termination within a workspace."
    chunk_directory: null
    depends_on: [2, 5]
created_after: ["editor_core_architecture"]
---

## Trigger

Modern code editors treat terminals as second-class citizens — they live in a separate panel, a separate abstraction, with different keybindings and a different mental model from editor tabs. Meanwhile, tools like TMux treat everything as a terminal buffer, which is powerful but lacks the editing affordances developers expect. And tools like Composer (which orchestrates multiple Claude Code instances across Git worktrees) solve the multi-agent problem but live entirely outside the editor.

This investigation explores whether lite-edit can unify these concepts:

1. **Tabs as terminal-backed buffers** — Instead of tabs being "files" and terminals being "something else," every tab views a buffer, and some buffers are backed by terminal emulators. The core abstraction is the editor, but it can view buffers owned by terminals.

2. **Hierarchical tab grouping** — At the parent level, a tab represents a workspace owned by an agent (e.g., a Claude Code instance working in a Git worktree). Underneath that parent tab are child tabs representing the contents of that workspace — files being edited, terminal sessions, diffs, etc.

3. **The Composer-meets-TMux vision** — Combine the multi-agent orchestration of Composer (run many AI coding agents in parallel worktrees, get notified when they need input) with the buffer-centric philosophy of TMux (everything is a viewable buffer), but flip the primary abstraction from terminal to editor.

## Success Criteria

- Identify a concrete tab/buffer data model that supports both file-backed and terminal-backed buffers uniformly
- Determine how hierarchical grouping (workspace → contents) maps to UI and data structures
- Assess feasibility of rendering terminal emulator output in an editor-style view (scrollback, search, copy)
- Understand what "agent owns a workspace" means in terms of lifecycle, notifications, and user interaction
- Identify the key technical risks and open questions that would need to be resolved before implementation
- Produce proposed chunks if the concept is viable

## Testable Hypotheses

### H1: A unified buffer abstraction can represent both files and terminals

- **Rationale**: Both files and terminals produce text content. A terminal is essentially a buffer that receives asynchronous updates from a PTY. An editor buffer receives updates from user keystrokes. The rendering machinery (syntax highlighting aside) is similar.
- **Test**: Define a buffer interface and verify that both file I/O and PTY output can be adapted to it without lossy abstractions (e.g., terminal colors, cursor positioning)
- **Status**: VERIFIED — A unified buffer *abstraction* (trait) works, but unified buffer *storage* does not. The seam is at the view layer.

### H2: Hierarchical tabs add navigational value without excessive complexity

- **Rationale**: Grouping tabs by workspace mirrors how developers think about parallel tasks. But tree-structured UI can become unwieldy (see: VS Code's file explorer vs. flat tab bar).
- **Test**: Sketch UI mockups and interaction flows for common scenarios (switch between agent workspaces, drill into a workspace's files, handle notifications)
- **Status**: VERIFIED — The left-rail (workspaces) + top-bar (content) layout provides clear visual hierarchy. Arc Browser and Discord validate the pattern at scale. Interaction flows map cleanly. See exploration log.

### H3: A terminal tab can be a full-featured terminal emulator, not just an output viewer

- **Rationale**: The intent is not a terminal viewer for agent output — it's a real terminal emulator that could happily run Vim, Emacs, htop, or any TUI application. This means full escape sequence handling (alternate screen buffer, mouse reporting, cursor shapes, 256/truecolor, bracketed paste, kitty keyboard protocol), proper input passthrough (raw keypresses including modifier combos, function keys, mouse events), accurate grid rendering (wide characters, combining characters, box-drawing), and correct resize behavior (SIGWINCH). The question is whether this can be achieved through the same BufferView trait and rendering pipeline used for file editing, or whether TUI apps require a fundamentally different rendering path.
- **Test**: (1) Analyze what alacritty_terminal already handles vs what we'd need to build. (2) Identify where the BufferView::styled_line() abstraction breaks down for full terminal emulation (alternate screen, cursor addressing, mouse passthrough). (3) Assess whether the existing Metal glyph rendering pipeline can handle the terminal grid's requirements (cursor shapes, cell-level bg colors, underline styles, wide chars). (4) Prototype running a TUI app (e.g., vim or htop) through alacritty_terminal and rendering the grid.
- **Status**: VERIFIED — BufferView abstraction holds for full terminal emulation with Style enrichment. alacritty_terminal handles all escape sequence interpretation. Remaining work: rendering enhancements (per-cell bg, cursor shapes, underlines) and input encoding (keystroke → escape sequence by mode). See exploration log.

### H5: alacritty_terminal is performant enough for inline use

- **Rationale**: alacritty_terminal is designed for a standalone terminal emulator where it owns the render loop. Using it as a library embedded in an editor adds a layer of indirection — we interpret PTY output into alacritty's grid, then convert the grid to StyledLines for our renderer. This double-hop could introduce latency or CPU overhead, especially during high-throughput output (e.g., `cat` of a large file, compiler output floods, CI logs).
- **Test**: Benchmark alacritty_terminal's `Term::process` on realistic workloads: (1) sustained high-throughput output (100K+ lines), (2) rapid small writes (interactive shell usage), (3) escape-sequence-heavy output (colored compiler output, TUI apps). Measure time per process call and memory footprint of the grid + scrollback.
- **Status**: VERIFIED — Processing is fast (~170 MB/s), grid read is cheap (0.24% of frame at 60fps). Damage tracking needs care. See exploration log 2026-02-21 benchmark entry.

### H4: The agent workspace lifecycle maps cleanly to tab lifecycle

- **Rationale**: An agent working in a worktree has a lifecycle: start, run, need input, produce output, complete. This should map to tab states (active, needs-attention, completed). But edge cases abound — what happens when an agent crashes? When it spawns sub-processes?
- **Test**: Enumerate agent lifecycle states and map each to tab behavior
- **Status**: VERIFIED — Seven lifecycle states identified (Empty → Starting → Running → NeedsInput → Stale → Exited → Errored), each mapping to distinct left-rail indicators and content behaviors. The agent IS its terminal — state is inferred from PTY behavior. See exploration log.

## Exploration Log

### 2026-02-21: Analyzing the existing buffer abstraction

**Current architecture**: The `buffer` crate provides `TextBuffer`, backed by a `GapBuffer` with a `LineIndex`. It's designed for interactive text editing — mutations happen one character at a time at a cursor position, and each mutation returns `DirtyLines` for efficient rendering. The renderer (`crates/editor/src/renderer.rs`) currently takes `&[&str]` lines directly via `set_content()`.

**Key observation**: There is no trait abstraction between buffer and renderer today. The renderer consumes plain `&[&str]`. This is actually a good starting point — the coupling is minimal and introducing a trait won't require unwinding deep assumptions.

### 2026-02-21: Mapping the difference space between file buffers and terminal buffers

I enumerated the fundamental differences between a file-editing buffer and a terminal buffer:

| Dimension | File Buffer (TextBuffer) | Terminal Buffer |
|-----------|------------------------|-----------------|
| **Who mutates** | User keystrokes at cursor | PTY output stream (async) |
| **Mutation model** | Insert/delete at point | Escape-sequence interpreter: cursor addressing, erase, scroll regions, alternate screen |
| **Content model** | Variable-length lines of plain characters | Fixed-width grid of styled cells (char + fg/bg/attrs) |
| **Growth** | Unbounded (file can be any size) | Fixed viewport + scrollback ring buffer |
| **User input** | Goes into buffer directly | Goes to PTY stdin (not into the buffer) |
| **Cursor semantics** | Single cursor position for editing | Terminal cursor that jumps anywhere in the grid |
| **Style** | None inherent (syntax highlighting applied externally) | Inline ANSI attributes (color, bold, underline, etc.) per cell |
| **Dirty tracking** | `DirtyLines` returned from mutations | Entire grid can change in one write; need to diff or track |

**Conclusion**: These are genuinely different data structures. Trying to force terminal content through a gap buffer would be fighting the abstraction at every turn. A terminal emulator's internal state (grid of styled cells, cursor state, scroll region, alternate screen) is irreducibly different from a text editing buffer.

### 2026-02-21: Surveying prior art

**Neovim `:terminal`**: Uses a terminal emulator library (libvterm) to maintain a grid of cells. The terminal buffer is a special buffer type — it has the same buffer ID and line-access API as regular buffers, but the content is read-only from the editor's perspective and updated by the terminal emulator. Lines are accessed as styled text. This is essentially the trait approach: same interface, different backing.

**Emacs term-mode / vterm**: vterm uses libvterm and presents terminal output as an Emacs buffer with text properties (faces) for styling. The buffer content is plain text with overlays for colors. This works but has performance issues because Emacs's buffer model (gap buffer with text properties) isn't well-suited to rapid terminal updates.

**VS Code integrated terminal**: Uses xterm.js, which is a completely separate rendering pipeline from the editor. The terminal is not a "buffer" in the Monaco editor sense — it's a different widget entirely. This is the approach lite-edit wants to avoid.

**Zed**: Uses a terminal emulator (alacritty_terminal) and renders the terminal grid using the same GPU pipeline as the editor, but it's a distinct view type with its own rendering logic, not a buffer.

**Key takeaway**: Everyone who has succeeded treats the terminal emulator as a separate state machine but exposes a common **view interface** for rendering. Nobody successfully unifies the backing storage.

### 2026-02-21: Designing the seam — a `BufferView` trait

The right abstraction boundary is at the **view layer**, not the storage layer. Both file buffers and terminal buffers produce the same thing for the renderer: **a sequence of styled lines with dirty tracking**.

```rust
/// A styled character span within a line
pub struct Span {
    pub text: String,         // or &str with lifetime
    pub style: Style,         // fg, bg, bold, italic, underline, etc.
}

/// A line as the renderer sees it
pub struct StyledLine {
    pub spans: Vec<Span>,
}

/// What the renderer needs from any buffer
pub trait BufferView {
    /// Number of lines available for display
    fn line_count(&self) -> usize;

    /// Get a styled line for rendering. Returns None if out of bounds.
    fn styled_line(&self, line: usize) -> Option<StyledLine>;

    /// Drain accumulated dirty state since last call.
    /// Returns which lines need re-rendering.
    fn take_dirty(&mut self) -> DirtyLines;

    /// Whether this buffer accepts direct text input (file=yes, terminal=no)
    fn is_editable(&self) -> bool;

    /// Optional: cursor position for display (both buffer types have cursors,
    /// but they mean different things)
    fn cursor_position(&self) -> Option<Position>;
}
```

**Why this works**:
- The renderer is decoupled from storage. It calls `styled_line()` and `take_dirty()` and draws.
- `TextBuffer` implements `BufferView` trivially — `styled_line()` returns a single span with default style (syntax highlighting can be layered later as a wrapper/decorator).
- `TerminalBuffer` implements `BufferView` by querying the terminal emulator's cell grid and converting rows of styled cells into `StyledLine`.
- `DirtyLines` already exists and works for both — terminal updates produce `FromLineToEnd` or `Range` dirty regions.

**What this does NOT do**:
- It doesn't unify input handling. Keystrokes in a file buffer mutate the TextBuffer directly. Keystrokes in a terminal buffer get sent to the PTY's stdin. The tab/view layer needs to dispatch input differently based on buffer type.
- It doesn't unify the backing storage. And it shouldn't.

### 2026-02-21: The terminal emulator question

A terminal buffer needs an actual terminal emulator — something that interprets VT100/xterm escape sequences and maintains grid state. Options in Rust:

1. **alacritty_terminal** — Battle-tested, used by Alacritty and Zed. Provides `Term<T>` with a grid of styled cells. Well-maintained. Has some Alacritty-specific opinions but is usable as a library.

2. **vte** — Lower-level: a parser for escape sequences. You'd need to build the grid/state machine on top. Used by alacritty_terminal internally.

3. **portable-pty + custom** — Use `portable-pty` for PTY management and build a minimal terminal state machine. More control but significant effort.

4. **Write from scratch** — Educational but a massive undertaking for correctness. Terminal emulation is a deep rabbit hole.

For the investigation's purposes, the choice of terminal emulator library doesn't affect the `BufferView` abstraction — it's an implementation detail of `TerminalBuffer`.

### 2026-02-21: Assessing the BufferView trait for terminal scrollback

One subtlety: when viewing a terminal's scrollback, the user is essentially in a "read-only editor" mode — they can scroll through historical output, search it, copy from it. This maps naturally to `BufferView`:

- `line_count()` returns scrollback_lines + viewport_lines
- `styled_line(n)` returns from the scrollback ring buffer for n < scrollback_len, from the live viewport grid for n >= scrollback_len
- The cursor is at the terminal's cursor position within the viewport region

This also means the tab chrome can show a scroll indicator, and the same viewport/scrolling machinery used for file editing can be reused for terminal scrollback. This is the TMux-like property we want.

### 2026-02-21: Benchmarking alacritty_terminal (H5)

Wrote a benchmark prototype (`prototypes/alacritty_bench/`) testing five workloads. Results on Apple Silicon (release build):

**Processing throughput (the `Processor::advance` call):**

| Workload | Time | Throughput | Per line |
|----------|------|-----------|----------|
| 100K lines plain text | 68ms | 169 MB/s | 685 ns |
| 100K lines colored (escape-heavy) | 67ms | 171 MB/s | 671 ns |
| 3K interactive writes + damage checks | 0.7ms | 94 MB/s | 222 ns |

**Key finding: escape parsing adds zero overhead.** Colored output is actually marginally *faster* than plain text (likely due to shorter content lines with escape sequences consuming bytes). This eliminates a major concern.

**Grid read overhead (the "conversion hop"):**

| Measurement | Result |
|-------------|--------|
| Full 40-line × 120-col grid → StyledLine conversion | 39.5 µs |
| As % of 60fps frame budget (16.6ms) | 0.24% |
| Per visible line | ~1 µs |

**Key finding: the conversion hop is negligible.** Reading all 40 visible lines and converting to styled spans costs 0.24% of a frame at 60fps. This is not a bottleneck.

**Damage tracking — the one gotcha:**

The `Interactive + selective grid read` benchmark (which reads only damaged lines) was *slower* than expected at 12.9µs per write — but this was dominated by the damage system reporting 40 damaged lines per write (the full viewport). Investigation: `term.damage()` reports `Full` damage after many operations (especially when the terminal is in scroll mode, which is the common case for output). This means in practice, we'll often need to re-read the entire viewport rather than just changed lines.

**This is fine for our use case.** Since the full viewport read is only 39.5µs, even re-reading every frame costs < 0.25% of budget. Damage tracking is useful for the *renderer* (which cells need new GPU uploads), not for the StyledLine conversion layer.

**Memory footprint:**

| Scrollback | Grid memory |
|-----------|-------------|
| 1K lines | 2.9 MB |
| 10K lines | 27.6 MB |
| 100K lines | 274.8 MB |

Cell size is 24 bytes. For agent workspaces, 10K scrollback (27.6 MB per terminal) is reasonable. With 10 agent workspaces that's ~276 MB for terminal grids — acceptable for a development tool but worth noting. 100K scrollback would be excessive. We should default to 10K and make it configurable.

**Borrow checker note:** `term.damage()` borrows `&mut self`, which conflicts with `term.grid()` (`&self`). The pattern is: collect damage info first, drop the borrow, then read the grid. This is a minor ergonomic annoyance but not a real problem.

### 2026-02-21: Agent workspace lifecycle exploration (H4)

**What is an "agent" concretely?** In the near term, it's a CLI process — Claude Code, aider, copilot-cli — running in a PTY within a Git worktree. The agent reads from stdin (for user input/approval), writes to stdout (its output stream), edits files in the worktree, and eventually exits. From our perspective, the agent IS a terminal session. The workspace wraps that terminal session plus the worktree's files.

**Agent lifecycle states:**

Mapped out every state a CLI agent process can be in, and what the workspace should do:

```
                ┌─────────────┐
                │   EMPTY     │  Workspace exists, no agent attached
                │   (idle)    │  User can edit files, open terminals
                └──────┬──────┘
                       │ user launches agent
                       ▼
                ┌─────────────┐
                │  STARTING   │  PTY spawned, process initializing
                │  (brief)    │  Show spinner in left rail
                └──────┬──────┘
                       │ agent begins producing output
                       ▼
                ┌─────────────┐
         ┌─────│  RUNNING     │◄────────────────┐
         │     │  (autonomous)│                  │
         │     └──────┬──────┘                  │
         │            │ agent asks question /    │
         │            │ needs approval           │
         │            ▼                          │
         │     ┌─────────────┐                  │
         │     │  NEEDS INPUT │  user provides   │
         │     │  (waiting)   │──────────────────┘
         │     └──────┬──────┘
         │            │ (timeout / user ignores)
         │            ▼
         │     ┌─────────────┐
         │     │  STALE       │  Agent still waiting but user
         │     │  (aged wait) │  hasn't responded in a while
         │     └─────────────┘
         │
         │ agent process exits
         ▼
    ┌─────────────┐
    │  EXITED      │───┐
    │  (exit code) │   │ exit code != 0
    └──────┬──────┘   ▼
           │     ┌─────────────┐
           │     │  ERRORED     │
           │     │  (crashed)   │
           │     └─────────────┘
           │ exit code == 0
           ▼
    ┌─────────────┐
    │  COMPLETED   │  Agent finished its task successfully
    │  (done)      │  Workspace persists for review
    └─────────────┘
```

**Mapping to workspace/tab behavior:**

| Agent State | Left Rail Indicator | Content Area Behavior | Notification |
|------------|--------------------|-----------------------|--------------|
| **EMPTY** | ⚪ Gray | Normal editor — files and manual terminals | None |
| **STARTING** | ⏳ Spinner | Agent terminal tab created, shows startup output | None |
| **RUNNING** | 🟢 Green | Agent terminal shows streaming output. Files agent edits can be opened in other tabs | Badge if not viewing this workspace |
| **NEEDS_INPUT** | 🟡 Yellow (pulsing) | Agent terminal shows the prompt/question. Cursor is active in terminal for user to type response | **Strong notification** — this blocks agent progress |
| **STALE** | 🟠 Orange | Same as NEEDS_INPUT but visually escalated | Optional OS notification |
| **EXITED(0)** | ✅ Checkmark | Terminal shows final output + exit. Scrollback preserved. Files available for review | Subtle notification |
| **ERRORED** | 🔴 Red | Terminal shows error output. Scrollback preserved for debugging | Strong notification |
| **COMPLETED** | ✅ Checkmark (faded) | Same as EXITED(0) after user has reviewed | None |

**The critical transition: RUNNING → NEEDS_INPUT**

This is the most important lifecycle event. It's why Composer exists — to surface when an agent needs human attention across many parallel agents. How do we detect it?

For Claude Code specifically, the detection heuristic is:
1. **Output pattern matching**: Claude Code prints a distinctive prompt when asking for input (e.g., permission requests, clarification questions). We could watch the terminal output for patterns.
2. **PTY idle detection**: When an agent is waiting for stdin, the PTY output stops and the process is blocking on read(). We could detect "output stopped + process still alive" as a signal.
3. **Process-specific signals**: Some agents might support a sideband notification protocol (e.g., OSC escape sequences signaling "I need input").
4. **Heuristic combination**: Output stops for >N seconds + process is still alive + last output line looks like a prompt → probably needs input.

**Practical approach**: Start with PTY idle detection (option 2) as the primary signal, with output pattern matching as a refinement. This is agent-agnostic — it works for Claude Code, aider, or any interactive CLI tool. Agent-specific integrations can layer on top later.

**The EXITED → workspace-persists question:**

When an agent finishes, what happens to the workspace? Options:

- **A: Workspace stays until user closes it.** The terminal tab shows the final output (scrollback preserved). File tabs remain open. User can review the agent's work, run tests, etc. This is the right default.
- **B: Auto-close after review.** If user switches away and comes back, maybe offer to close. Or auto-close after N minutes of inactivity post-completion.
- **C: Collapse to summary.** Replace the full workspace with a compact summary in the left rail (e.g., "auth-refactor: completed ✅"). User can expand to see the terminal scrollback and files.

Leaning toward **A** with the workspace rail showing a faded ✅. The user explicitly closes when they're done reviewing. This matches how Composer works — completed sessions stick around.

**Agent restart:**

From ERRORED or COMPLETED, the user should be able to restart the agent in the same workspace. This means:
- The terminal tab gets a new PTY (old scrollback preserved above the new session, or in a separate "previous run" tab)
- The worktree is already set up
- State transitions back to STARTING

**Multiple agents in one workspace?**

Probably not. One workspace = one worktree = one agent. If you want two agents, that's two worktrees and two workspaces. This keeps the model simple and avoids conflicts.

**The "agent handle" abstraction:**

```rust
struct AgentHandle {
    /// The PTY process
    process: Child,
    /// The terminal buffer for agent I/O
    terminal: TerminalBuffer,
    /// Current lifecycle state
    state: AgentState,
    /// When the current state was entered
    state_changed_at: Instant,
    /// Configuration for this agent
    config: AgentConfig,
}

enum AgentState {
    Starting,
    Running,
    NeedsInput { prompt: Option<String> },
    Stale { waiting_since: Instant },
    Exited { code: i32 },
}

struct AgentConfig {
    /// The command to run (e.g., "claude", "aider")
    command: String,
    /// Arguments
    args: Vec<String>,
    /// Working directory (the worktree root)
    cwd: PathBuf,
    /// Idle timeout before RUNNING → STALE
    idle_timeout: Duration,
}
```

**Relationship to the Workspace data model (from H2):**

```rust
struct Workspace {
    id: WorkspaceId,
    label: String,
    root_path: PathBuf,
    agent: Option<AgentHandle>,  // ← the agent lifecycle lives here
    tabs: Vec<Tab>,
    active_tab: usize,
}
```

The workspace owns the agent. The agent's terminal is one of the workspace's tabs (typically the first/pinned tab). The agent state drives the workspace's left-rail indicator.

**Key insight: the agent IS its terminal.** There's no separate "agent protocol" — the agent is a process in a PTY, and all interaction happens through the terminal. The agent's state is inferred from the terminal's behavior (output flowing, output stopped, process exited). This means the agent handle is really a thin wrapper around a TerminalBuffer with state-inference logic.

**What about non-CLI agents?**

Future agents might communicate via LSP-like protocols, HTTP APIs, or custom IPC. The current model (agent = PTY process) doesn't preclude this — the AgentHandle could be made into a trait with different implementations. But for the initial design, PTY-based CLI agents are the target.

**Unresolved questions:**

1. **NEEDS_INPUT detection reliability.** PTY idle detection is heuristic — the agent might be doing slow computation (not waiting for input). False positives would be annoying (pulsing yellow when the agent is just thinking). Need to tune the idle timeout, and potentially combine with output pattern matching.

2. **Scrollback across agent restarts.** When restarting an agent in the same workspace, should the previous session's scrollback be preserved? If yes, where does the boundary go? A visual separator ("── Previous session ──") in the terminal scrollback? Or a separate "history" tab?

3. **Agent-initiated file opens.** Can the agent tell the editor "open this file"? Claude Code doesn't have this capability today, but a future integration could use OSC escape sequences or a sideband channel to request file opens, diff views, etc.

### 2026-02-21: Hierarchical tabs exploration (H2)

**Starting premise:** Composer is an existence proof that workspace-level tabs work well. Each Composer tab represents a Claude Code instance in a Git worktree — you see its status, switch between agents, get notified when one needs input. What Composer lacks is the ability to *edit files within those workspace tabs*. lite-edit should fuse the two: workspace tabs that contain full editing environments.

**The two-level visual model:**

The user's vision: two spatially and visually distinct tab levels.

```
┌──────┬─────────────────────────────────────────────┐
│      │  [main.rs]  [lib.rs]  [Cargo.toml]          │  ← Content tabs (top, horizontal)
│  W1  ├─────────────────────────────────────────────┤
│      │                                             │
│  W2  │    fn main() {                              │
│  ●   │        println!("hello");                   │
│  W3  │    }                                        │
│      │                                             │
│  W4  │                                             │
│      │                                             │
└──────┴─────────────────────────────────────────────┘
  ↑ Workspace tabs (left, vertical)
  ● = needs attention indicator
```

- **Left rail**: Workspace tabs. Visually heavy, identity-level — each represents an agent's entire working context (worktree, terminal sessions, open files). Think Discord servers or Slack workspaces on the left edge.
- **Top bar**: Content tabs within the selected workspace. These are the familiar editor tabs — files, terminals, diffs. They belong to and live inside the workspace.

**Prior art analysis:**

| Tool | Workspace level | Content level | How it feels |
|------|----------------|---------------|--------------|
| **Composer** | Horizontal tabs per agent | None (output only) | Good grouping, no editing depth |
| **VS Code** | None (single workspace) | Horizontal file tabs | No multi-workspace concept |
| **JetBrains** | Separate windows per project | Horizontal file tabs | Heavyweight — each project is an OS window |
| **TMux** | Sessions (detached concept) | Windows (tabs) → Panes | Good hierarchy, but terminal-only |
| **Zellij** | Sessions | Tabs → Panes | Similar to TMux, terminal-centric |
| **Discord** | Vertical left rail (servers) | Channels within server | Strong visual hierarchy, proven at scale |
| **Arc Browser** | Vertical left rail (spaces) | Tabs within space | Very close to what we want |
| **Safari** | Tab groups (dropdown) | Tabs within group | Weak visual hierarchy — groups feel hidden |

**Key observation: Arc Browser is the closest analog.** Arc puts "Spaces" on the left as a vertical rail, and tabs within each space live in a sidebar or top bar. The visual distinction between space-level and tab-level is immediately legible. Safari's tab groups attempted similar but failed because the grouping was hidden behind a dropdown — not spatially distinct.

**The left rail as workspace selector:**

Why left (vertical) for workspaces and top (horizontal) for content:

1. **Spatial separation creates cognitive separation.** The two tab levels operate at different frequencies — you switch workspaces rarely (minutes apart), you switch content tabs constantly (seconds apart). Putting them on different axes makes the interaction distinct.

2. **Vertical rail scales better for workspaces.** You might have 2-10 workspaces. A vertical rail with large, identifiable icons/labels handles this well. Horizontal tabs would crowd at 5+.

3. **Horizontal tabs are the learned pattern for files.** Every editor uses horizontal tabs for open files. Keeping this for content tabs means zero learning curve for the within-workspace experience.

4. **The left rail is always visible.** Unlike a dropdown or sidebar toggle, the workspace rail is persistent — you always see which workspaces exist and which need attention. This is critical for the agent-notification use case.

**Workspace tab anatomy:**

Each workspace tab in the left rail needs to convey:

```
┌──────┐
│  W1  │  ← Short label (branch name? agent task?)
│ auth │
│  ●   │  ← Status indicator
└──────┘
```

- **Label**: Derived from the worktree (branch name) or user-assigned. Needs to be short — the rail is narrow.
- **Status indicator**: 
  - 🟢 Agent running, no input needed
  - 🟡 Agent needs input / waiting for approval
  - 🔴 Agent errored / process exited
  - ⚪ Idle / no agent attached (just a workspace with files)
  - Unread badge for new terminal output since last viewed
- **Visual state**: Selected workspace is highlighted; the content area shows its tabs.

**Content tabs within a workspace:**

Each workspace owns a set of content tabs. These are heterogeneous — a mix of:

- **File tabs**: Editing a source file (backed by TextBuffer + BufferView)
- **Terminal tabs**: A live terminal session (backed by TerminalBuffer + BufferView)
- **Diff tabs**: Viewing changes (future — backed by a diff-aware BufferView)
- **Agent output tab**: The agent's primary terminal — special because it's always present when an agent is attached

The key insight from the BufferView exploration: **all of these are BufferView implementations**. The content tab doesn't care whether it's showing a file or a terminal. It has a `BufferView`, renders it, and dispatches input appropriately.

**The "agent output" tab as a default:**

When a workspace has an attached agent (Claude Code instance), there's always at least one tab: the agent's terminal. This is the equivalent of what Composer shows — the agent's output stream. But now you can also open file tabs alongside it, viewing the files the agent is editing. And the terminal tab IS a full terminal (H3), so you can scroll back through agent output, search it, etc.

**Data model sketch:**

```rust
/// A workspace — one per agent/worktree
struct Workspace {
    id: WorkspaceId,
    label: String,              // e.g., branch name
    root_path: PathBuf,         // worktree root
    agent: Option<AgentHandle>, // attached agent, if any
    tabs: Vec<Tab>,             // content tabs
    active_tab: usize,          // which content tab is focused
    status: WorkspaceStatus,    // for the left rail indicator
}

/// A content tab within a workspace
struct Tab {
    id: TabId,
    label: String,              // filename, "Terminal", etc.
    buffer: Box<dyn BufferView>,
    kind: TabKind,              // File, Terminal, Diff, AgentOutput
    dirty: bool,                // unsaved changes (files only)
    unread: bool,               // new content since last viewed (terminals)
}

/// The top-level editor state
struct Editor {
    workspaces: Vec<Workspace>,
    active_workspace: usize,    // which workspace is selected in the left rail
}
```

**Interaction flows:**

*Scenario 1: Switch between agent workspaces*
- Click workspace in left rail (or keyboard shortcut: Cmd+1, Cmd+2, etc.)
- Content area swaps to show that workspace's tabs and active content
- If workspace has unread indicator, it clears

*Scenario 2: Agent needs input*
- Left rail shows 🟡 on that workspace
- Could optionally flash/pulse to draw attention
- User clicks workspace, sees the agent terminal tab, types the response

*Scenario 3: Open a file the agent is editing*
- User is in workspace W2, agent just modified `src/auth.rs`
- User opens `src/auth.rs` — it appears as a new content tab (top bar)
- User can now view the file alongside the agent terminal (by switching tabs)
- Or: split view (future) to see both simultaneously

*Scenario 4: Create a new workspace without an agent*
- User just wants a plain editing workspace (no agent attached)
- Left rail shows a workspace with ⚪ status
- Content tabs are file tabs, terminals, etc. — works like a normal editor
- Agent can be attached later

**The "no workspace" degenerate case:**

When lite-edit launches with a single directory and no agents, the left rail could either:
- Show a single workspace (the current directory) — the rail is minimal but present
- Hide entirely until a second workspace is created — cleaner for the simple case

Leaning toward: **always show the rail**, even with one workspace. Consistency matters, and it teaches the user the workspace concept from the start. The rail is narrow enough (~48-64px) to not waste significant space.

**Keyboard navigation model:**

- `Cmd+1..9`: Switch workspace (left rail)
- `Cmd+Shift+]` / `Cmd+Shift+[`: Cycle content tabs (top bar)  
- `Cmd+T`: New terminal tab in current workspace
- `Cmd+O`: Open file tab in current workspace
- `Cmd+W`: Close current content tab
- `Cmd+Shift+W`: Close workspace (with confirmation if agent running)
- `Cmd+N`: New workspace

This mirrors browsers (Cmd+1..9 for tabs) but shifts that to the workspace level, with Cmd+Shift for within-workspace navigation.

**Unresolved questions:**

1. **Can a file be open in multiple workspaces?** If two agents edit the same file in different worktrees, each workspace has its own copy (they're in different worktrees). But what if two tabs in the *same* workspace point to the same file? Should they share a buffer or be independent views?

2. **Split views within a content tab.** TMux has panes within windows. Do we want splits within a workspace's content area? This is useful (terminal + file side by side) but adds a third level of hierarchy. Defer to a future investigation.

3. **Drag-and-drop between workspaces.** Can you drag a file tab from one workspace to another? Probably not meaningful (different worktrees = different file copies), but terminal tabs could potentially be moved.

4. **Workspace ordering.** Fixed order? Drag to reorder? Auto-sort by recent activity? For agent workspaces, recency probably makes most sense.

### 2026-02-21: Full terminal emulation analysis (H3)

The intent is a terminal tab that can run Vim, Emacs, htop, or any TUI application — not just an agent output viewer. This raises the bar significantly. Analyzed what alacritty_terminal provides vs. what we need to build.

**What alacritty_terminal handles (we get for free):**

All 66 methods of the vte `Handler` trait are implemented. This covers:

| Capability | Status |
|-----------|--------|
| Cursor addressing (goto, move, save/restore) | ✅ Handled |
| Alternate screen buffer (swap_alt) | ✅ Handled |
| Scroll regions | ✅ Handled |
| Character attributes (bold, italic, underline, dim, strikeout, inverse, hidden) | ✅ Handled |
| 256-color and truecolor (RGB) | ✅ Handled |
| Wide characters (CJK, emoji) | ✅ Handled (WIDE_CHAR / WIDE_CHAR_SPACER flags) |
| Combining / zero-width characters | ✅ Handled (CellExtra.zerowidth storage) |
| Mouse mode flags (click, motion, drag, SGR) | ✅ Tracked in TermMode |
| Bracketed paste mode | ✅ Tracked in TermMode |
| Kitty keyboard protocol | ✅ Tracked in TermMode |
| Hyperlinks (OSC 8) | ✅ Handled |
| Cursor shape (block, beam, underline, hidden) | ✅ Handled via RenderableCursor |
| Window title changes | ✅ Handled via Event::Title |
| Resize (SIGWINCH equivalent) | ✅ term.resize() |
| Selection / vi mode | ✅ Built in |
| Damage tracking | ✅ LineDamageBounds per line |

**What we need to build (our rendering and input layers):**

1. **Per-cell background colors.** The current Metal pipeline renders glyph quads with a single text color on a uniform background. A terminal needs per-cell (or per-span) background colors. This is the biggest rendering gap. Requires: a separate "background rect" pass before the glyph pass, or a combined shader that handles both.

2. **Cursor shape rendering.** The current renderer draws a cursor as a block highlight. A terminal uses block, beam (vertical line), and underline cursor shapes, plus blinking state. `RenderableCursor` provides shape and position — we just need to render it.

3. **Wide character layout.** The monospace layout assumes 1 cell = 1 glyph. Wide characters occupy 2 cells. The glyph buffer needs to handle WIDE_CHAR (draw glyph spanning 2 cells) and WIDE_CHAR_SPACER (skip the spacer cell). This is a layout concern, not a rendering pipeline change.

4. **Input encoding.** When a terminal tab is focused, keystrokes must be encoded per terminal mode and sent to PTY stdin. This means:
   - Arrow keys → `\x1b[A` / `\x1bOA` (depending on APP_CURSOR mode)
   - Function keys → appropriate escape sequences
   - Mouse events → encoded per mouse mode (if enabled by the TUI app)
   - Bracketed paste → wrap pasted text in `\x1b[200~` ... `\x1b[201~`
   - Kitty keyboard protocol encoding (if app requests it)
   - Modifier combos (Ctrl-C → `\x03`, etc.)

   alacritty_terminal tracks *which modes are active* but doesn't encode input — that's the frontend's job. Alacritty's main repo has this logic in its input handling code; we'd need equivalent logic.

5. **Underline styles.** The Cell supports 5 underline variants (single, double, curly, dotted, dashed) plus underline color. The current renderer has no underline support. Needs: either texture-based underline rendering or procedural line drawing in the shader.

6. **Inverse video / hidden text.** When `Flags::INVERSE` is set, fg and bg swap. When `Flags::HIDDEN` is set, the character is invisible. These are trivial to handle in the StyledLine conversion.

7. **Box-drawing / line-drawing characters.** Some terminals render box-drawing characters (U+2500–U+257F) with pixel-perfect alignment. This is a nice-to-have, not a blocker — rendering them as regular glyphs works, just with possible alignment gaps at cell boundaries.

**Reassessing the BufferView trait for full terminal emulation:**

The `BufferView::styled_line()` abstraction holds up well even for full terminal emulation. The key insight: the *renderer* doesn't need to know it's rendering a terminal. It sees styled lines with cursor information. The differences live in:

- **The input layer** (how keystrokes are dispatched)
- **The rendering details** (per-cell backgrounds, cursor shapes, underlines)

However, `styled_line()` needs slight enrichment for terminal rendering:

```rust
/// What the renderer needs to know about cursor display
pub struct CursorInfo {
    pub position: Position,
    pub shape: CursorShape,      // Block, Beam, Underline, Hidden
    pub blinking: bool,
}

/// Extended style for terminal cells
pub struct Style {
    pub fg: Color,
    pub bg: Color,               // per-cell background (new for terminal)
    pub bold: bool,
    pub italic: bool,
    pub underline: UnderlineStyle, // None, Single, Double, Curly, Dotted, Dashed
    pub underline_color: Option<Color>,
    pub strikethrough: bool,
    pub dim: bool,
    pub inverse: bool,           // renderer swaps fg/bg
    pub hidden: bool,            // renderer skips glyph
}
```

For `TextBuffer`, most of these are default/off. For `TerminalBuffer`, they're populated from Cell flags. The renderer handles them uniformly.

**The alternate screen question:** When Vim or htop is running, the terminal is in alternate screen mode. There's no scrollback — the viewport IS the content. `BufferView` handles this naturally:
- `line_count()` returns screen_lines (e.g., 40)
- Scrolling is disabled (or handled by the TUI app itself)
- The cursor is the terminal's cursor, not an editor cursor

When the TUI app exits (alternate screen deactivated), scrollback reappears. This transition is handled by alacritty_terminal's `swap_alt()` — our `BufferView` just reads from whichever grid is currently active.

**The input encoding problem is the hardest part.** It's not conceptually difficult, but it's a lot of fiddly mapping tables (key → escape sequence, varying by mode). Options:
- Port Alacritty's input handling code (it's in the alacritty crate, not alacritty_terminal)
- Use a crate like `crossterm` for reference
- Build our own mapping table (tedious but finite)

**Assessment:** Full terminal emulation is achievable. alacritty_terminal does the heavy lifting (escape sequence interpretation, grid management, mode tracking). What remains for us is:
1. Rendering enhancements (per-cell bg, cursor shapes, underlines) — moderate effort
2. Input encoding (keystroke → escape sequence by mode) — significant effort, lots of edge cases
3. Mouse event forwarding — moderate effort
4. The BufferView trait works with minor enrichment to the Style type

### 2026-02-21: File-backed scrollback analysis

The benchmark showed memory is the one scaling concern: 24 bytes/cell × 120 cols = 2.8 KB/line. At 10 agent workspaces:

| Scrollback depth | Per terminal | 10 workspaces |
|-----------------|-------------|---------------|
| 10K lines | 27.6 MB | 276 MB |
| 100K lines | 274.8 MB | 2.7 GB |
| Unlimited | Unbounded | Unbounded |

For long-lived agent sessions (Claude Code running for hours), 10K lines isn't enough — you lose context. But 100K lines per terminal is untenable at scale.

**Proposed architecture: tiered scrollback**

```
┌─────────────────────────┐
│   Viewport (40 lines)   │  alacritty_terminal owns this (always in memory)
├─────────────────────────┤
│ Recent cache (~2K lines) │  alacritty_terminal's scrollback (in memory)
├─────────────────────────┤
│  Cold scrollback (file)  │  Our code — StyledLines serialized to disk
└─────────────────────────┘
```

**Key insight**: We don't need to serialize alacritty's 24-byte Cell structs. When lines scroll off alacritty's grid, we convert them to `StyledLine` (which we're already doing for rendering) and append to a compact on-disk format. A typical 120-char line with a few style changes compresses from 2,880 bytes (cell grid) to ~150-200 bytes (UTF-8 text + style markers) — a ~15x reduction.

**Why this works cleanly with the BufferView trait**: `styled_line(n)` doesn't care where the data comes from. For lines in the viewport or recent cache, it reads from alacritty's grid. For cold scrollback, it pages from disk. The trait boundary hides this completely.

**Memory budget per terminal with file-backed scrollback**:
- Viewport grid: ~115 KB (40 × 120 × 24)
- Recent cache: ~5.5 MB (2K × 120 × 24)
- Page cache for cold scrollback: ~1 MB (configurable)
- Total: ~6.6 MB regardless of history length

At 10 workspaces: ~66 MB total. That's a 4x reduction vs. 10K in-memory scrollback, with *unlimited* history depth.

**Sequencing**: This is not blocking for the initial terminal integration. Start with alacritty's in-memory scrollback (2-5K lines), add file-backed cold storage later when the multi-workspace story materializes.

## Findings

### Verified Findings

1. **The existing TextBuffer and a terminal buffer have fundamentally different storage models.** A gap buffer with line index is wrong for terminal content, which is a fixed-width grid of styled cells with scroll regions and an alternate screen buffer. (Evidence: dimension comparison table above.)

2. **The right unification point is a view trait, not a storage trait.** Both buffer types produce the same output for the renderer: styled lines with dirty tracking. Every successful editor-terminal integration (Neovim, Zed) takes this approach. (Evidence: prior art survey.)

3. **The existing `DirtyLines` abstraction is reusable.** It already handles single-line changes, ranges, and "from line to end" patterns — all of which arise in terminal output. No modifications needed. (Evidence: `DirtyLines` variants map directly to terminal update patterns.)

4. **alacritty_terminal's processing performance is not a risk.** Escape-sequence parsing adds zero measurable overhead vs. plain text (~170 MB/s both). The grid-to-StyledLine conversion hop costs 39.5µs for a full 40×120 viewport (0.24% of a 60fps frame budget). Memory is 27.6 MB at 10K scrollback per terminal. (Evidence: benchmark prototype `prototypes/alacritty_bench/`, release build on Apple Silicon.)

5. **File-backed scrollback is feasible and well-motivated for multi-workspace scenarios.** In-memory scrollback at 10K lines per terminal costs 27.6 MB each (276 MB for 10 workspaces). A tiered approach — small in-memory cache + compact on-disk log of StyledLines — caps memory at ~6.6 MB per terminal regardless of history depth, a ~15x storage reduction over the cell grid format. The BufferView trait's `styled_line(n)` hides the paging transparently. (Evidence: Cell size analysis, serialization format comparison.)

6. **Full terminal emulation (Vim, htop, etc.) is achievable through alacritty_terminal + our rendering/input layers.** alacritty_terminal implements all 66 vte Handler methods — escape sequences, alternate screen, mouse mode tracking, cursor shapes, wide characters, hyperlinks. What remains for us: (a) rendering enhancements (per-cell background colors, cursor shapes, underline styles), (b) input encoding (keystroke → escape sequence, varying by terminal mode), (c) mouse event forwarding. The BufferView trait holds with minor Style enrichment. (Evidence: Handler trait analysis, RenderableContent/RenderableCursor API review.)

7. **Input encoding is the highest-effort remaining problem for full terminal emulation.** alacritty_terminal tracks which modes are active (APP_CURSOR, BRACKETED_PASTE, KITTY_KEYBOARD, mouse modes) but doesn't encode input — that's the frontend's responsibility. This is a large surface area of key-to-escape-sequence mappings with mode-dependent behavior. (Evidence: TermMode flags analysis, comparison with Alacritty's frontend input code.)

8. **The two-level tab hierarchy (left rail for workspaces, top bar for content) is validated by prior art and maps cleanly to the editor data model.** Arc Browser and Discord prove that a vertical left rail for top-level grouping + horizontal tabs for content within a group is immediately legible and scales to 10+ groups. The spatial separation (different axes) creates cognitive separation matching usage frequency: workspace switches are infrequent (minutes), content tab switches are constant (seconds). The data model maps cleanly: `Editor → Vec<Workspace> → Vec<Tab>`, where each Tab holds a `Box<dyn BufferView>`. (Evidence: prior art survey, interaction flow analysis, data model sketch.)

9. **Agent lifecycle maps cleanly to workspace state, and the agent IS its terminal.** Seven states identified (Empty, Starting, Running, NeedsInput, Stale, Exited, Errored), each with a distinct left-rail indicator and content behavior. The critical transition is Running → NeedsInput, detectable via PTY idle heuristic (output stops, process alive). No separate "agent protocol" needed — the agent is a process in a PTY, state is inferred from terminal behavior, and the AgentHandle is a thin wrapper around a TerminalBuffer with state-inference logic. (Evidence: lifecycle state enumeration, mapping to UI behavior, data model sketch.)

10. **The current renderer coupling is minimal.** The renderer takes `&[&str]` today via `set_content()`. Migrating to a `BufferView` trait is straightforward and doesn't require reworking the Metal pipeline — just changing what produces the line data. (Evidence: code review of `renderer.rs`.)

### Hypotheses/Opinions

- **`alacritty_terminal` is confirmed as a viable terminal emulator crate for embedding.** Processing throughput (~170 MB/s) and grid read overhead (0.24% of frame) are both well within budget. The API is usable with minor borrow-checker ergonomics. Memory at 10K scrollback (27.6 MB per terminal) is acceptable for a development tool with ~10 agent workspaces.

- **Input dispatch should be handled at the tab level, not the buffer level.** A tab knows whether its buffer is editable or terminal-backed and can route keystrokes accordingly. The `is_editable()` method on `BufferView` supports this, but the actual input routing architecture hasn't been designed.

- **Terminal scrollback reuse of the editor viewport is a strong UX win** but may have performance implications — terminal scrollback buffers can be very large (10K+ lines), and the viewport/rendering machinery needs to handle this efficiently.

## Proposed Chunks

1. **BufferView trait and Style types** (index 0): Define the `BufferView` trait, `StyledLine`, `Span`, `Style`, and `CursorInfo` types. Style supports terminal-grade attributes from day one: fg/bg color, bold, italic, dim, 5 underline variants + underline color, strikethrough, inverse, hidden. Implement `BufferView` for `TextBuffer` (defaults). Migrate the renderer from `&[&str]` to `&dyn BufferView`.
   - Priority: High
   - Dependencies: None
   - Notes: Design the Style type for terminal-grade richness upfront so it doesn't need breaking changes later. The trait should be object-safe for dynamic dispatch.

2. **Renderer enhancements for styled content** (index 1): Per-cell background color rendering (background rect pass before glyph pass), cursor shape rendering (block, beam, underline), underline rendering. These are needed for terminal display but also benefit syntax-highlighted file editing.
   - Priority: High
   - Dependencies: BufferView trait chunk
   - Notes: The Metal pipeline currently has a single clear color and single text color. This chunk adds per-cell variation. Consider a two-pass approach: background rects first, then glyph quads on top.

3. **Terminal emulator integration** (index 2): Add `TerminalBuffer` backed by `alacritty_terminal`, implementing `BufferView`. Wire PTY I/O. Full terminal emulation: alternate screen buffer, wide characters, scroll regions, all Cell flags mapped to Style. Prove shell output renders through the same pipeline as file content.
   - Priority: High
   - Dependencies: Renderer enhancements chunk
   - Notes: This is a full terminal, not an output viewer. Must handle alternate screen (Vim/htop), wide chars (CJK), combining characters. Start with shell output, then verify with a TUI app.

4. **Terminal input encoding** (index 3): Keystroke-to-escape-sequence mapping layer. Mode-dependent encoding (APP_CURSOR, BRACKETED_PASTE, KITTY_KEYBOARD). Arrow keys, function keys, modifier combos (Ctrl-C → 0x03). Mouse event encoding per mode (click, motion, drag, SGR). Route through tab dispatch.
   - Priority: High
   - Dependencies: Terminal emulator integration chunk
   - Notes: This is the highest-effort remaining problem. Large surface area of mappings with mode-dependent behavior. Consider porting from Alacritty's frontend input code or building mapping tables from xterm documentation.

5. **File-backed scrollback** (index 4): As lines scroll off alacritty's in-memory grid, convert to `StyledLine` and append to a compact on-disk log. Implement paging so `BufferView::styled_line()` transparently serves from memory or disk. Target: unlimited scrollback with bounded memory (~6.6 MB per terminal regardless of history length).
   - Priority: Medium
   - Dependencies: Terminal emulator integration chunk
   - Notes: Not blocking for initial terminal integration. Becomes important when multi-workspace (10+ agents) scenario materializes. Compact format achieves ~15x reduction over cell grid. Consider mmap for cold region.

6. **Workspace model and left rail** (index 5): Implement the `Workspace` and `Editor` data model (`Editor → Vec<Workspace> → Vec<Tab>`). Render a left-side vertical rail showing workspace tabs with labels and status indicators. Clicking a workspace swaps the content area to that workspace's tabs. Keyboard navigation: Cmd+1..9 for workspace switching.
   - Priority: High
   - Dependencies: BufferView trait chunk
   - Notes: The left rail is always visible (~48-64px wide). Even with one workspace, show the rail for consistency. Workspace labels derived from worktree branch name or user-assigned. Status indicators: running (green), needs input (yellow), errored (red), idle (gray).

7. **Content tab bar** (index 6): Horizontal tab bar at the top of the content area showing open tabs within the active workspace. Tabs are heterogeneous (file, terminal, diff). Support Cmd+Shift+]/[ for cycling, Cmd+W for close, unread badges on terminal tabs with new output.
   - Priority: High
   - Dependencies: Workspace model chunk

8. **Agent lifecycle management** (index 7): Implement `AgentHandle` wrapping a `TerminalBuffer` with state inference (Starting, Running, NeedsInput, Stale, Exited, Errored). PTY idle detection for the Running → NeedsInput transition. Wire agent state to workspace left-rail indicators. Support agent launch, restart, and graceful termination within a workspace.
   - Priority: Medium
   - Dependencies: Terminal emulator integration chunk, Workspace model chunk
   - Notes: The agent IS its terminal — AgentHandle is a thin wrapper around TerminalBuffer with state-inference logic. Start with PTY idle detection as the primary NeedsInput signal, refine with output pattern matching. One agent per workspace. Workspace persists after agent exits for review.

## Resolution Rationale

_Investigation is ONGOING._
