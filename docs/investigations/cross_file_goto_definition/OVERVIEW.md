---
status: ONGOING
trigger: "Cmd+click on symbols defined in other files does not navigate to the definition. Same-file go-to-definition works, but cross-file resolution fails silently or shows 'Definition not found'."
proposed_chunks:
  - prompt: "Fix symbol index to filter out @reference.* captures and fix method capture interleaving by switching from QueryCaptures to QueryMatches"
    chunk_directory: gotodef_index_captures
    depends_on: []
  - prompt: "Initialize symbol index on session restore so cross-file go-to-definition works on the most common startup path"
    chunk_directory: gotodef_session_restore
    depends_on: []
created_after: ["terminal_scroll_viewport"]
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

Cmd+click (and F12/Cmd+D) on symbols defined in other files does not navigate to their definitions. Same-file go-to-definition works correctly via the `LocalsResolver` (tree-sitter locals.scm queries), but the cross-file fallback via the `SymbolIndex` (tree-sitter tags.scm queries) has never worked end-to-end despite the infrastructure being in place.

The two-tier resolution flow is: same-file locals → cross-file symbol index lookup. The second tier is where the failure occurs. This is a core navigation feature — without it, users must manually find and open files containing referenced symbols.

## Success Criteria

1. **Root cause identified**: Determine exactly why cross-file go-to-definition fails — is the symbol index empty, are tags queries not matching, is the identifier extraction wrong, or is navigation itself broken?
2. **Reproducible test case**: A concrete scenario (specific symbol, specific files) that demonstrates the failure.
3. **Fix path clear**: Either a direct fix or a proposed chunk with a clear implementation plan.

## Testable Hypotheses

### H1: The tags.scm queries don't capture the expected symbols for one or more languages

- **Rationale**: The symbol index relies on tags.scm queries producing `@definition.*` and `@name` captures. If the queries are malformed, use unexpected capture names, or don't match the tree-sitter grammar version, no symbols get indexed. The `index_file` function silently skips files that fail (`Err(_e)` is swallowed in `index_workspace`).
- **Test**: Write a standalone test that calls `index_file` on a real Rust source file from this project (e.g., `crates/buffer/src/lib.rs`) and check if any symbols are returned. Print capture names to verify they match the expected `@definition.*` / `@name` pattern.
- **Status**: PARTIALLY FALSIFIED — top-level definitions work, but methods in impl blocks are missing and reference captures are being indexed as Unknown definitions

### H2: Same-file resolution succeeds first, preventing the cross-file fallback from ever being reached

- **Rationale**: The `LocalsResolver` runs first (line 1420). If it returns `Some` for identifiers that are actually defined in other files (false positive match — e.g., a parameter shadows an imported name), the cross-file path is never taken. The code jumps to the locals definition and returns early (line 1452).
- **Test**: Add debug logging or a test that calls `goto_definition` on a known cross-file symbol and check whether the same-file resolver returns `Some` or `None`.
- **Status**: UNTESTED

### H3: The symbol index is never initialized (remains `None`)

- **Rationale**: `symbol_index` starts as `None` and is only set via `start_symbol_indexing()`, which is called in `add_startup_workspace` and `new_workspace`. If the workspace is created through a different code path, the index may never be initialized.
- **Test**: Add a debug print or breakpoint when `goto_definition` reaches the cross-file path, and check if `workspace.symbol_index` is `None` vs `Some`.
- **Status**: VERIFIED — The session restoration path in `main.rs:402-408` calls `restore_into_editor()` which creates workspaces via `Editor::new_deferred()` and never calls `start_symbol_indexing()`. Since the editor typically starts via session restore (not fresh directory picker), this is likely the **most common** startup path and it leaves `symbol_index` as `None` for all restored workspaces.

### H4: The identifier extracted at cursor position doesn't match what's in the symbol index

- **Rationale**: `identifier_at_position()` uses the tree-sitter AST to extract the identifier node text at the cursor byte offset. If it extracts a different string than what the tags query indexed (e.g., qualified name vs simple name, or including/excluding a prefix), the lookup will return no matches.
- **Test**: Log both the extracted identifier and the full contents of the symbol index to see if the name exists but under a different key.
- **Status**: UNTESTED

### H5: The `StreamingIterator` capture processing logic drops symbols

- **Rationale**: The `index_file` function uses `QueryCaptures` (a `StreamingIterator`), processing matches by tracking `current_match_id` and grouping `@name` + `@definition.*` captures. If captures arrive in an unexpected order, or if a match has only a `@definition.*` without a `@name` (or vice versa), symbols get silently dropped. The "process previous match" logic at line 339 requires *both* `symbol_name` and `symbol_kind` to be `Some`.
- **Test**: Add debug logging inside the `while let Some(...)` loop to print every capture name and node text, then verify the expected grouping.
- **Status**: VERIFIED — This is the root cause for missing methods. See Exploration Log 2026-03-07 "Debug capture ordering" entry.

## Exploration Log

### 2026-03-07: Initial code review

Reviewed the complete cross-file go-to-definition pipeline. Key files:

| Component | File |
|-----------|------|
| Symbol index | `crates/syntax/src/symbol_index.rs` |
| Same-file resolver | `crates/syntax/src/gotodef.rs` |
| UI integration | `crates/editor/src/editor_state.rs:1371` (`goto_definition`) |
| Workspace integration | `crates/editor/src/workspace.rs:899` (`start_symbol_indexing`) |
| Language configs | `crates/syntax/src/registry.rs` |

**Observations:**

1. The `index_workspace` function at line 445 silently swallows all indexing errors — if tags queries are broken, we'd never know.
2. The `index_file` capture processing (lines 326-382) uses a manual state machine to group `@name` and `@definition.*` captures by match ID. This is fragile — if capture ordering differs from expectations, symbols are silently dropped.
3. Unit tests exist for `index_file` with Rust and Python, but they use synthetic files with `{{}}` escaping (writeln! format strings). Need to verify these tests actually pass.
4. The `goto_definition` method (line 1371) tries same-file first and returns early on success. This is correct behavior but could mask cross-file issues if we never reach that path during testing.

**Next steps:**
- Run existing symbol index tests to verify they pass
- Write a diagnostic test that indexes a real file from this project
- Add instrumentation to trace which path `goto_definition` takes

### 2026-03-07: Diagnostic testing against real project files

Ran the symbol index against `crates/syntax/src/` (real project files, not synthetic test content).

**Result: The index IS being populated.** Found 196 unique symbols. Top-level functions and structs are indexed correctly:

```
LocalsResolver           -> gotodef.rs:58 (Class)
identifier_at_position   -> gotodef.rs:252 (Function)
SymbolIndex              -> symbol_index.rs:139 (Class)
LanguageRegistry         -> registry.rs:71 (Class)
```

**But methods inside `impl` blocks are NOT found:**

```
new                      -> NOT FOUND
start_indexing           -> NOT FOUND
from_capture_name        -> NOT FOUND
```

This is expected — the Rust `TAGS_QUERY` uses `(declaration_list (function_item ...))` for methods, which captures them as `@definition.method`. But the `from_capture_name` function correctly parses `"definition.method"` → `Method`. So why aren't methods found?

**Root cause identified:** The `TAGS_QUERY` also includes `@reference.call` and `@reference.implementation` patterns. The `SymbolKind::from_capture_name` function returns `None` for `"name"` captures, but returns `Some(Unknown)` for `"reference.call"` and `"reference.implementation"` because they hit the `_ => SymbolKind::Unknown` fallback after stripping the `"definition."` prefix (which they don't have).

Wait — actually the function checks for `name.starts_with("definition.")` first. Let me re-examine...

The `from_capture_name` function:
1. If starts with `"definition."` → strips prefix, maps to known kind
2. If equals `"name"` → returns `None`
3. Otherwise → uses the raw name as kind_str, hits `_ => Unknown`

So `"reference.call"` doesn't start with `"definition."`, isn't `"name"`, so it falls through to the else branch where `kind_str = name = "reference.call"`, which hits `_ => Unknown`.

**This means reference/call sites are being indexed as `SymbolKind::Unknown`!** The `index_file` function pairs captures by match ID. For a `@reference.call` match, it has both `@name` and `@reference.call` captures — the `@name` provides the symbol text and `@reference.call` provides `SymbolKind::Unknown`. Both are `Some`, so the symbol gets inserted into the index.

**This is a significant issue**: every function call site in every file is being added to the symbol index. This pollutes the index with thousands of non-definition locations.

### 2026-03-07: Printed Rust TAGS_QUERY content

The Rust `TAGS_QUERY` from tree-sitter-rust includes:

**Definitions** (what we want):
- `@definition.class` — structs, enums, unions, type aliases
- `@definition.method` — functions inside `impl` blocks
- `@definition.function` — top-level functions
- `@definition.interface` — traits
- `@definition.module` — modules
- `@definition.macro` — macro definitions

**References** (what we don't want):
- `@reference.call` — function/method call sites, macro invocations
- `@reference.implementation` — impl blocks

The `index_file` function doesn't filter out `@reference.*` captures. It treats them as valid definitions with `SymbolKind::Unknown`. This means:
1. The index is polluted with call sites
2. When you look up a symbol, you get both its definition AND every place it's called
3. This could cause the disambiguation selector to show call sites instead of/alongside definitions
4. Methods in `impl` blocks *should* be captured as `@definition.method`, but might be getting grouped incorrectly with reference captures

### 2026-03-07: Session restore path — H3 VERIFIED

Traced all workspace creation paths in `main.rs`:

| Path | Calls `start_symbol_indexing`? |
|------|-------------------------------|
| `add_startup_workspace()` (fresh start, dir picker) | YES (line 640) |
| `new_workspace()` (Cmd+N) | YES (line 851) |
| Session restore (`restore_into_editor`) | **NO** |

The session restore path (`main.rs:402-408`) does:
```rust
let mut state = EditorState::new_deferred(font_metrics);
state.editor = editor;  // restored editor, symbol_index is None
// No call to start_symbol_indexing!
```

`restore_into_editor` in `session.rs` creates workspaces but never touches `symbol_index`. Since session restore is the default startup path (used whenever a previous session exists), **most users will never have a symbol index initialized**.

This means the user would see "Symbol index not initialized" in the status bar when trying cross-file goto-definition. The fix is to iterate all restored workspaces and call `start_symbol_indexing()` after session restore.

### 2026-03-07: Confirmed methods ARE missing from index

The diagnostic showed `new`, `start_indexing`, `from_capture_name` all returning NOT FOUND. The `TAGS_QUERY` pattern for methods is:

```scheme
(declaration_list
    (function_item
        name: (identifier) @name) @definition.method)
```

This should match. The fact that `new()` isn't found across ANY of the indexed files (where there are many `impl` blocks with `new()` methods) suggests the method capture pattern may not be working with this tree-sitter version, OR the capture grouping logic is mishandling nested captures.

### 2026-03-07: Debug capture ordering — H5 VERIFIED

Wrote a diagnostic that prints every capture from the `QueryCaptures` streaming iterator. For a simple file with `pub fn top_level()`, `struct Foo`, and `impl Foo { fn new(), fn method_a() }`:

**Critical finding**: Captures arrive **interleaved across matches**, not grouped by match ID.

For `new()` inside an `impl` block:
```
Match 4 (id=3): @definition.method   text="pub fn new()..."    ← kind arrives
Match 5 (id=4): @definition.function text="pub fn new()..."    ← DIFFERENT match starts
Match 6 (id=3): @name                text="new"                ← name for match 3 arrives LATE
Match 7 (id=4): @name                text="new"                ← name for match 4 arrives LATE
```

The `index_file` state machine assumes captures for match N all arrive before match N+1 starts. When it sees match 4 (id=4) begin, it tries to finalize match 3 — but match 3 only has `symbol_kind` set (no `symbol_name` yet), so the `if let (Some(name), Some(kind), Some(start_byte))` destructuring fails and the symbol is **silently dropped**.

Methods match TWO query patterns simultaneously — `@definition.method` (inside `declaration_list`) AND `@definition.function` (general `function_item`). This causes the interleaving. Top-level functions only match ONE pattern, so their captures arrive in order.

**This is the root cause.** The fix must either:
1. Use `QueryMatches` instead of `QueryCaptures` (groups all captures per match), or
2. Buffer captures by match ID and process each match only when complete, or
3. Collect all captures first, then process by match ID

## Findings

### Verified Findings

1. **The symbol index IS being built and populated.** H1 (tags queries broken) is PARTIALLY FALSIFIED — top-level definitions are found. H3 (index never initialized) needs runtime verification but the infrastructure is correct.

2. **Reference captures are being indexed as definitions.** The `TAGS_QUERY` includes `@reference.call` and `@reference.implementation` patterns. The `from_capture_name` function doesn't filter these out — it maps them to `SymbolKind::Unknown` and they get inserted into the index. (Evidence: diagnostic run showed `index_file` entries with `(Unknown)` kind at call sites, not definition sites.)

3. **Methods inside `impl` blocks are NOT being indexed.** Symbols like `new`, `start_indexing`, `from_capture_name` were NOT FOUND despite existing in multiple files. The `TAGS_QUERY` has a pattern for methods (`@definition.method` inside `declaration_list`), but it's not producing results. (Evidence: diagnostic run against crates/syntax/src/.)

3. **H5 VERIFIED: `QueryCaptures` delivers captures interleaved across matches.** Methods in `impl` blocks match two query patterns simultaneously (`@definition.method` and `@definition.function`). The streaming iterator interleaves their captures, breaking the state machine that assumes sequential delivery per match. This causes methods to be silently dropped. (Evidence: debug capture output showing match 3's `@name` arriving after match 4's `@definition.function`.)

4. **H3 VERIFIED: Session restore never initializes the symbol index.** The session restoration path in `main.rs:402-408` replaces the editor with a restored one but never calls `start_symbol_indexing()` on any workspace. Since session restore is the default startup path, most users will have `symbol_index: None` and see "Symbol index not initialized". (Evidence: code trace through `main.rs` and `session.rs` — no mention of `symbol_index` in session restore.)

### Hypotheses/Opinions

- The reference pollution issue is a clear bug — `from_capture_name` should return `None` for `@reference.*` captures, just like it does for `@name`. This is a simple fix.
- The overall cross-file goto-definition failure is a combination of three verified bugs: (a) H3: symbol index never initialized on session restore (the most common startup path), (b) H5: methods silently dropped due to capture interleaving, and (c) reference captures polluting the index with call sites.

## Proposed Chunks

1. **Fix symbol index to filter references and capture methods**: Two bugs in `index_file` (`crates/syntax/src/symbol_index.rs`):
   - **Filter out `@reference.*` captures**: `from_capture_name` should return `None` for capture names starting with `"reference."`, not fall through to `Unknown`. This prevents call sites from polluting the index.
   - **Fix capture interleaving**: Replace `QueryCaptures` (streaming iterator) with `QueryMatches`, or buffer captures by match ID before processing. `QueryCaptures` interleaves captures across matches when a node matches multiple patterns (e.g., methods match both `@definition.method` and `@definition.function`), causing the sequential state machine to silently drop symbols.
   - Add a test that verifies methods like `new()` inside `impl` blocks are indexed.
   - Priority: High
   - Dependencies: None
   - Notes: See Exploration Log "Debug capture ordering" entry for exact interleaving pattern. The reference filter fix is one line. The capture fix requires switching from `QueryCaptures` to `QueryMatches` (which groups all captures per match) or collecting into a HashMap keyed by match ID before processing.

2. **Initialize symbol index on session restore**: After `restore_into_editor` in `main.rs:402-408`, iterate all restored workspaces and call `start_symbol_indexing()` on each. Without this, the most common startup path leaves `symbol_index: None`.
   - Priority: High
   - Dependencies: None
   - Notes: Simple fix — add a loop after `state.editor = editor` that calls `ws.start_symbol_indexing(Arc::clone(&state.language_registry))` for each workspace.

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