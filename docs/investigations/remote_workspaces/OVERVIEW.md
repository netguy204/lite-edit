---
status: ONGOING
trigger: "Exploring how to add SSH-based remote workspace support — connecting buffers to remote files with change notification, fuzzy open, and remote terminal sessions"
proposed_chunks: []
created_after: ["concurrent_edit_sync"]
---

<!--
DO NOT DELETE THIS COMMENT until the investigation reaches a terminal status.
This documents the frontmatter schema and guides investigation workflow.

STATUS VALUES:
- ONGOING: Investigation is active; exploration and analysis in progress
- SOLVED: The investigation question has been answered. If proposed_chunks exist,
  implementation work remains—SOLVED indicates the investigation is complete, not
  that all resulting work is done.
- NOTED: Findings documented but no action required; kept for future reference
- DEFERRED: Investigation paused; may be revisited later when conditions change

TRIGGER:
- Brief description of what prompted this investigation
- Examples:
  - "Test failures in CI after dependency upgrade"
  - "User reported slow response times on dashboard"
  - "Exploring whether GraphQL would simplify our API"
- The trigger naturally captures whether this is an issue (problem to solve)
  or a concept (opportunity to explore)

PROPOSED_CHUNKS:
- Starts empty; entries are added if investigation reveals actionable work
- Each entry records a chunk prompt for work that should be done
- Format: list of {prompt, chunk_directory, depends_on} where:
  - prompt: The proposed chunk prompt text
  - chunk_directory: Populated when/if the chunk is actually created via /chunk-create
  - depends_on: Optional array of integer indices expressing implementation dependencies.

    SEMANTICS (null vs empty distinction):
    | Value           | Meaning                                 | Oracle behavior |
    |-----------------|----------------------------------------|-----------------|
    | omitted/null    | "I don't know dependencies for this"  | Consult oracle  |
    | []              | "Explicitly has no dependencies"       | Bypass oracle   |
    | [0, 2]          | "Depends on prompts at indices 0 & 2"  | Bypass oracle   |

    - Indices are zero-based and reference other prompts in this same array
    - At chunk-create time, index references are translated to chunk directory names
    - Use `[]` when you've analyzed the chunks and determined they're independent
    - Omit the field when you don't have enough context to determine dependencies
- Unlike narrative chunks (which are planned upfront), these emerge from investigation findings
-->

## Trigger

lite-edit currently assumes all workspaces are local directories. We want to explore adding SSH-based remote workspace support so the editor can connect to a remote machine and:

- Open files via a fuzzy picker backed by remote directory listing
- Edit buffers connected to remote files with change notification
- Spawn terminals that execute in the remote environment

This is a concept investigation — no existing bug or failure, but a significant architectural capability that would require touching many subsystems (file I/O, buffer management, file watching, terminal spawning, workspace model, file index).

## Success Criteria

1. **Identify all local filesystem coupling points** — catalog every location where `std::fs`, `PathBuf`, `notify`, or `portable-pty` creates a hard dependency on local I/O
2. **Design the abstraction boundary** — determine what traits/enums are needed to support both local and remote workspaces without duplicating the editor's core logic
3. **Choose SSH/SFTP approach** — evaluate Rust SSH libraries (`ssh2`, `openssh`, shelling out to `ssh`) and recommend one with rationale
4. **Determine remote file watching strategy** — since SSH doesn't provide filesystem events, define how change detection works remotely
5. **Produce a chunk decomposition** — break the work into implementable chunks with clear dependency ordering

## Testable Hypotheses

### H1: The existing `Workspace` model can be extended to represent remote contexts without restructuring

- **Rationale**: `Workspace` already has a `root_path`, its own `FileIndex`, and an independent pane tree. A remote workspace could be a workspace variant with a different storage backend.
- **Test**: Trace the `Workspace` struct's dependencies and determine if `root_path: PathBuf` can be generalized without cascading changes through the pane/tab layer.
- **Status**: UNTESTED

### H2: A thin VFS trait over file I/O can be introduced incrementally without blocking other work

- **Rationale**: File operations are concentrated in `editor_state.rs` (`save_file`, `reload_file_tab`, `merge_file_tab`) and `file_index.rs`. If these are the only call sites, a trait can be slotted in without touching rendering, buffer editing, or layout.
- **Test**: Catalog all `std::fs` call sites and verify they can be routed through a single trait.
- **Status**: UNTESTED

### H3: The `openssh` crate (async, wraps system SSH) is preferable to `ssh2` (libssh2 bindings)

- **Rationale**: `openssh` reuses the user's SSH config, agent forwarding, and ProxyJump settings for free. `ssh2` requires reimplementing authentication. However, lite-edit has no async runtime today.
- **Test**: Evaluate both crates for: auth complexity, SFTP support, PTY allocation, multiplexing, and whether async is a dealbreaker.
- **Status**: UNTESTED

### H4: Polling-based remote file watching at modest intervals (1-5s) is acceptable for the UX

- **Rationale**: Local `notify` gives sub-100ms latency on changes. Remote can't match this. But if most edits originate in the editor itself, the primary use case for watching is detecting _external_ changes (e.g., `git checkout` on the remote), where 1-5s latency is fine.
- **Test**: Identify the user-visible scenarios where file watching matters and assess acceptable latency for each.
- **Status**: UNTESTED

## Exploration Log

### 2026-02-26: Initial architecture audit

Cataloged all local filesystem coupling points across the codebase:

**File I/O (read/write)** — `crates/editor/src/editor_state.rs`
- `save_file()` / `save_buffer_to_path()` — uses `std::fs::write()` directly
- `reload_file_tab()` / `merge_file_tab()` — uses `std::fs::read()` directly
- Session restore — reads file content to populate buffers on startup

**File discovery** — `crates/editor/src/file_index.rs`
- `FileIndex` walks `root_path` recursively via `fs::read_dir()`
- Caches relative `PathBuf` entries in a `Vec<PathBuf>`
- Fuzzy matching operates on this cached list

**File watching** — two layers:
- `file_index.rs`: watches workspace root recursively via `notify` crate (`RecursiveMode::Recursive`)
- `buffer_file_watcher.rs`: per-file watchers for files opened outside the workspace root, reference-counted per parent directory

**Terminal** — `crates/terminal/src/pty.rs`
- `PtyHandle::spawn()` uses `portable-pty`'s `native_pty_system()`
- Spawns user's login shell (`/bin/zsh`, `/bin/bash`)
- Reads PTY output on background thread via channel

**Session persistence** — `crates/editor/src/session.rs`
- Stores workspace root paths and tab file paths as `PathBuf`
- Uses `~/Library/Application Support/lite-edit/` (macOS-specific)

**What's already abstract (doesn't need to change):**
- `BufferView` trait — rendering works on trait objects, not raw file handles
- `TextBuffer` — buffer editing is purely in-memory, no file knowledge
- Viewport/scrolling — independent of storage
- UI layout / pane tree — `Workspace` structure is already multi-context
- Tab management — tabs don't care where content originates

**Key observation:** The `Workspace` struct already owns its own `FileIndex` and pane tree independently. Multiple workspaces coexist in the same editor. This makes "remote workspace" a natural variant — a workspace whose `FileIndex` and file I/O route through SSH instead of local fs.

## Findings

### Verified Findings

- **File I/O is concentrated, not scattered.** All file read/write operations go through a small number of methods in `editor_state.rs`. This makes introducing a VFS trait tractable — the surface area is small. (Evidence: code audit, 2026-02-26)

- **The Workspace model already supports multiple independent contexts.** Each `Workspace` owns its own `root_path`, `FileIndex`, and pane tree. The editor supports multiple workspaces simultaneously. This means a remote workspace can be a workspace variant, not a fundamentally new concept. (Evidence: `workspace.rs` structure)

- **BufferView trait decouples rendering from storage.** Since the rendering pipeline works through `BufferView`, remote-backed buffers wouldn't require changes to the rendering code. (Evidence: `buffer_view.rs` trait definition)

- **Terminal spawning is isolated in PtyHandle.** The `portable-pty` integration is contained in `pty.rs`. Replacing this with SSH-based PTY allocation for remote workspaces would not leak into terminal rendering (`TerminalBuffer` implements `BufferView`). (Evidence: `pty.rs` structure)

- **No async runtime exists today.** The editor uses a synchronous event loop. Any SSH library choice needs to account for this — either use blocking operations on background threads or introduce an async runtime. (Evidence: absence of tokio/async-std in dependencies)

### Hypotheses/Opinions

- **Shelling out to `ssh`/`sftp` via the system binary may be simpler than an SSH library.** The user's `~/.ssh/config`, agent, ProxyJump, and key management all work for free. The `openssh` crate wraps this pattern. Downside: less control, dependency on system SSH.

- **The biggest architectural risk is path representation.** `PathBuf` is used everywhere to identify files. A remote file needs a richer identifier (host + path, or a URI). This change will ripple through session persistence, tab association, recent file tracking, and the file picker. Getting this abstraction right early is critical.

- **Polling for remote file changes at 2-5s intervals is likely acceptable.** The main use case for watching is detecting external modifications (git operations, builds). Sub-second latency isn't needed for that. The editor already tracks its own writes, so it only needs to detect _other_ changes.

## Proposed Chunks

_To be populated as hypotheses are verified and the design solidifies. Early candidates:_

1. **Workspace path abstraction** — Replace `PathBuf` with a `WorkspacePath` type that can represent both local paths and remote `host:path` identifiers. Update session persistence, tab association, and file picker to use the new type.

2. **File I/O trait** — Introduce a `FileBackend` trait abstracting `read`, `write`, `list_dir`, `watch`. Implement `LocalFileBackend` wrapping current `std::fs` calls. Route all file operations through the trait.

3. **SSH connection management** — SSH session pool with multiplexing, authentication handling, and connection lifecycle. Provides SFTP and exec channels to other subsystems.

4. **Remote file backend** — Implement `FileBackend` over SFTP. Includes remote directory listing for the file picker and polling-based change detection.

5. **Remote terminal** — SSH-based PTY allocation as an alternative to `portable-pty` for remote workspaces. Wire into existing `TerminalBuffer` rendering.

6. **Remote workspace UX** — Connection UI (host picker / recent connections), workspace creation flow, connection status indicator, reconnection handling.

## Resolution Rationale

_Investigation is ONGOING. To be populated when resolved._