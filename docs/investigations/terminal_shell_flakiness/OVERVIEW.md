---
status: ONGOING
trigger: "Shell-spawning tests flaky under parallel load; real app occasionally creates non-functional terminal tabs"
proposed_chunks:
  - prompt: "Fix spawn error swallowing and PTY fd leaks in terminal tab creation"
    chunk_directory: "terminal_spawn_reliability"
    depends_on: []
created_after: ["treesitter_editing"]
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

Two observations:

1. **Test flakiness**: Three terminal integration tests (`test_shell_prompt_appears`, `test_shell_produces_content_after_poll`, `test_poll_events_returns_processed_on_output`) and two editor tests (`test_poll_agents_dirty_after_terminal_creation`, `test_single_pane_terminal_dirty_and_content`) fail intermittently during full-suite `cargo test` runs. All pass reliably in isolation.

2. **Real-app flakiness**: The operator reports that terminal tab creation in the actual app "occasionally doesn't result in a working tab" — the terminal appears but is non-functional.

## Success Criteria

1. Identify root cause(s) of why terminal tabs occasionally fail to become functional in the real app
2. Determine whether test flakiness and app flakiness share a root cause or are separate issues
3. Propose concrete fix(es) — either as chunks or direct patches

## Testable Hypotheses

### H1: PTY spawn has a race condition — output arrives before poll loop starts

- **Rationale**: `spawn_shell` creates the PTY and the shell starts immediately. If the shell produces output (prompt) before the first `poll_events` call, the output may be buffered but the "dirty" signal could be missed, leaving the terminal appearing blank.
- **Test**: Add logging to PTY read thread to see if bytes arrive before first poll. Check if `poll_events` correctly reads all buffered data on first call.
- **Status**: UNTESTED

### H2: Resource contention during parallel shell spawning overwhelms PTY/fork

- **Rationale**: Test suite spawns ~25 shells concurrently. In isolation all tests pass; under full load they fail. Fork+exec under high load can be slow or fail silently.
- **Test**: Run the failing tests with `--test-threads=1` to serialize them. If they pass, contention is the cause (for tests at least).
- **Status**: VERIFIED — see Exploration Log 2026-02-28 (contention experiment)

### H3: PTY file descriptor or reader thread is dropped/closed prematurely

- **Rationale**: The real-app flakiness ("occasionally doesn't result in a working tab") suggests a lifecycle bug. If the PTY reader thread exits early or the fd is closed, the terminal would appear stuck.
- **Test**: Review `PtyHandle` drop/cleanup logic. Add error logging to the PTY reader thread to see if it exits unexpectedly.
- **Status**: UNTESTED

### H4: Terminal viewport size is zero at spawn time, causing shell to not render

- **Rationale**: A previous chunk (`terminal_viewport_init`) fixed a case where `visible_rows=0` caused `scroll_to_bottom` to scroll past all content. A similar issue could occur if the PTY is told the terminal size is 0×0 — some shells won't output a prompt.
- **Test**: Log the cols/rows passed to PTY spawn and check if they're ever zero.
- **Status**: UNTESTED

## Exploration Log

### 2026-02-28: Initial observations during test fixing

While fixing performance test failures (debug-build O(n²) assertions), discovered 5 additional flaky tests — all related to shell spawning.

**Key observations:**
- All 5 tests pass reliably in isolation (individual or per-crate runs)
- All 5 fail intermittently during full `cargo test` (which runs ~2500 tests in parallel)
- The terminal integration tests (3) even fail when run as a group of 37 but pass individually — suggesting the other shell-spawning tests in the same file create contention
- The editor tests (2) seem more sensitive — they failed even after increasing timeouts from 1s to 3s, but passed with 10s timeout
- Terminal integration tests failed even with 10s timeout during full suite, but pass with 10s when run per-crate

**Relevant code paths:**
- `PtyHandle::spawn` at `crates/terminal/src/terminal_buffer.rs:239`
- `EditorState::new_terminal_tab` at `crates/editor/src/editor_state.rs:4340`
- `assert_line_index_consistent` at `crates/editor/src/editor_state.rs:792` (separate issue, fixed)

**Mitigation applied:** Increased poll timeouts from 1-2s to 10s in commit `e7de4237`. This helps but doesn't address the real-app flakiness reported by operator.

### 2026-02-28: Contention experiment

Built a purpose-built contention test that spawns N shells simultaneously using a barrier, then measures time-to-first-output and records spawn failures.

**Results:**

| N shells | Failures | Time range | Failure mode |
|----------|----------|------------|-------------|
| 1 | 0/1 | 524ms | — |
| 5 | 0/5 | 700ms–1.1s | — |
| 10 | 0/10 | 840ms–1.5s | — |
| 25 | 5/25 | 416ms–2.6s | `openpty: ENXIO` |
| 50 | 20/50 | 415ms–3.9s | `openpty: ENXIO` |

**Key finding**: Failures are not timeouts — `openpty` itself returns error code -6 (`ENXIO`, "Device not configured") immediately. The system runs out of PTY devices when too many are opened concurrently. `kern.tty.ptmx_max` is 511 but only ~23 `/dev/ttys*` devices exist at baseline with 61 fds already in use by other processes.

**Conclusion for test flakiness**: The test suite spawns ~25+ shells across parallel test threads. When enough tests happen to call `openpty` simultaneously, the system can't allocate PTY devices fast enough and returns ENXIO. The `spawn_shell().unwrap()` in the tests turns this into a panic. The increased timeouts help with the timing-sensitive tests but cannot help when `openpty` itself fails.

**Conclusion for real-app flakiness**: This is a **separate issue**. The real app only spawns one terminal at a time, so PTY exhaustion doesn't apply. The app flakiness needs separate investigation — H1, H3, and H4 remain untested and are more likely candidates.

**Full test suite serialized** (`--test-threads=1`): All 2500+ tests pass with zero failures, confirming this is purely a parallel-resource-exhaustion problem for tests.

## Findings

### Verified Findings

- **Test flakiness root cause**: `openpty` returns `ENXIO` when too many PTYs are opened concurrently (threshold: between 10 and 25 on this macOS system). The test suite spawns ~25+ shells across parallel threads, hitting this limit. (Evidence: contention experiment, 2026-02-28)
- **Serialization fixes test flakiness completely**: `cargo test -- --test-threads=1` passes all 2500+ tests with zero failures.
- **Test and app flakiness are separate issues**: The app only spawns one terminal at a time, so PTY exhaustion cannot be the cause of real-app flakiness.

### Hypotheses/Opinions

- The real-app flakiness likely involves H1 (race between shell output and first poll), H3 (premature PTY cleanup), or H4 (zero viewport size at spawn). These remain untested.
- For tests, `PtyHandle::Drop` detaching reader threads without joining may contribute to PTY fd leakage across sequential tests, but the contention experiment showed the issue is simultaneous opens, not lingering fds.

## Proposed Chunks

1. **Serialize shell-spawning tests**: Add `serial_test` dependency or use a mutex to prevent concurrent `openpty` calls in tests. Alternatively, mark shell-spawning tests as `#[ignore]` and run them separately.
   - Priority: Medium
   - Dependencies: None
   - Notes: `--test-threads=1` is a workaround but slows all tests. A targeted serialization of just the shell tests would be better.

2. **Investigate real-app terminal flakiness**: Separate investigation needed — add logging/telemetry to `PtyHandle::spawn`, the reader thread, and `EditorState::new_terminal_tab` to capture what happens when the app creates a non-functional terminal tab.
   - Priority: High
   - Dependencies: None
   - Notes: H1, H3, H4 from this investigation remain untested and are candidates for the app-level issue.

## Resolution Rationale

<!--
GUIDANCE:

When marking this investigation as SOLVED, NOTED, or DEFERRED, explain why.
This captures the decision-making for future reference.

Questions to answer:
- What evidence supports this resolution?
- If SOLVED: What was the answer or solution?
- If NOTED: Why is no action warranted? What would change this assessment?
- If DEFERRED: What conditions would trigger revisiting? What's the cost of delay?

Example (SOLVED):
Root cause was identified (unbounded ImageCache) and fix is straightforward (LRU eviction).
Chunk created to implement the fix. Investigation complete.

Example (NOTED):
GraphQL migration would require significant investment (estimated 3-4 weeks) with
marginal benefits for our use case. Our REST API adequately serves current needs.
Would revisit if: (1) we add mobile clients needing flexible queries, or
(2) API versioning becomes unmanageable.

Example (DEFERRED):
Investigation blocked pending vendor response on their API rate limits. Cannot
determine feasibility of proposed integration without this information.
Expected response by 2024-02-01; will revisit then.
-->