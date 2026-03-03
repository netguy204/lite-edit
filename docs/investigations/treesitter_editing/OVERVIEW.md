---
status: SOLVED
trigger: "LSP is out of scope, but go-to-definition and intelligent indent are table-stakes editing features. Tree-sitter parses already produce structural info we're leaving on the table."
proposed_chunks:
  - prompt: "Wire up incremental tree-sitter parsing: switch from full-reparse update_source() to the incremental edit() path by having buffer mutations return byte-offset info and calling Tab::notify_edit() from all mutation sites in editor_state.rs."
    chunk_directory: incremental_parse
    depends_on: []
  - prompt: "Add intelligent auto-indent using Helix-style indent queries: load indents.scm alongside highlight queries, port Helix's indent computation algorithm with hybrid heuristic, integrate into Enter-key handling. Start with Rust and Python."
    chunk_directory: treesitter_indent
    depends_on: [0]
  - prompt: "Add same-file go-to-definition using locals queries: load locals.scm queries, implement scope-walking resolution algorithm, wire to click-on-symbol action. Start with Rust and Python."
    chunk_directory: treesitter_gotodef
    depends_on: [0]
  - prompt: "Add cross-file go-to-definition via workspace symbol index: build background indexer using tags.scm queries, extract top-level definitions into name-to-location map, wire as fallback for same-file go-to-def."
    chunk_directory: treesitter_symbol_index
    depends_on: [2]
created_after: ["remote_workspaces"]
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

LSP integration is explicitly out of scope for lite-edit (see GOAL.md), but two features — "click on symbol to find definition" and "intelligent indent" — are table-stakes for a code editor. lite-edit already has tree-sitter parsing for syntax highlighting across 13 languages. The parse trees contain structural information (scope boundaries, definition sites, indent-relevant nodes) that we're leaving on the table.

This investigation explores how far tree-sitter alone can take us toward these features without introducing LSP, and where the practical boundary lies between tree-sitter-only capabilities and what would require a language server.

### Current state

- Tree-sitter parses are done per-buffer in `crates/syntax` (the `SyntaxHighlighter` struct)
- Trees are retained and nominally support incremental parsing, though the editor currently calls the full-reparse path (`update_source`) rather than the incremental path (`edit`)
- Only highlight queries are loaded; `locals_query` exists on `LanguageConfig` but is `#[allow(dead_code)]` — stored but never used
- No go-to-definition, symbol indexing, auto-indentation, or other semantic features exist

## Success Criteria

1. **Go-to-definition feasibility**: Determine what tree-sitter can resolve (same-file, cross-file, language-specific limits) and where the practical boundary with LSP lies
2. **Indent strategy identified**: Determine which approach to intelligent indentation works best (indent queries like Neovim/Helix, direct AST node inspection, or hybrid) and what query files are needed per language
3. **Architecture delta documented**: Identify what changes the current `crates/syntax` architecture needs (e.g., tree retention improvements, new query types, cross-file indexing structures)
4. **Chunk proposals**: Produce concrete chunk prompts for implementation work

## Testable Hypotheses

### H1: Tree-sitter `locals.scm` queries can resolve same-file symbol definitions

- **Rationale**: Tree-sitter's `locals.scm` query pattern (used by Neovim and Helix) tracks scope/definition/reference relationships. The `LanguageConfig` already has a `locals_query` field that is populated but unused. If these queries work reliably, same-file go-to-definition is achievable without any cross-file infrastructure.
- **Test**: Load `locals.scm` queries for Rust and Python, run them against sample files, and verify that `@local.definition` captures correctly resolve when matched against `@local.reference` captures at cursor position.
- **Status**: VERIFIED (with caveats) — The approach works for local variables, parameters, and locally-defined functions. However: (1) most grammars (Rust, Python, Go) do NOT ship `locals.scm` upstream — we'd need to write or port them from nvim-treesitter; (2) the algorithm has documented bugs for hoisted function names in Rust/C (nvim-treesitter issue #499); (3) cannot resolve imports, method calls on types, trait implementations, or anything cross-file.

### H2: Cross-file go-to-definition is feasible via a workspace-wide symbol index built from tree-sitter

- **Rationale**: Editors like Zed use tree-sitter to build a lightweight symbol index across the workspace (function names, struct/class definitions, etc.) using `tags.scm` or custom queries. This could provide cross-file jump-to-definition for top-level symbols without LSP. The question is whether the accuracy and performance are acceptable.
- **Test**: Prototype a background symbol indexer that walks project files, parses them with tree-sitter, extracts top-level definitions, and measures: (a) index build time for a medium project (~1000 files), (b) accuracy of definition resolution vs what an LSP would return.
- **Status**: UNTESTED — Research confirms `tags.scm` exists upstream for Rust, Python, and Go. GitHub's stack-graphs is a more sophisticated option but requires language-specific `.tsg` files. A simpler ctags-style approach (parse all files, extract top-level definitions into a name→location map) is the likely starting point. This hypothesis is deferred to a later phase — same-file go-to-def is the higher-value, lower-cost starting point.

### H3: Tree-sitter `indents.scm` queries can drive intelligent auto-indent

- **Rationale**: Both Neovim and Helix use `indents.scm` query files for tree-sitter-based indentation. They use incompatible capture naming conventions, but the underlying mechanism is the same: run indent queries against the parse tree, walk ancestors collecting `@indent`/`@outdent` captures, compute an indent level.
- **Test**: Port Helix's `indents.scm` for Rust and Python, run them against the current tree after a newline insertion, and compare computed indent level against what a human would expect for ~20 representative code patterns.
- **Status**: VERIFIED (approach is sound) — Helix's implementation (`helix-core/src/indent.rs`) is the reference. Key findings: (1) adopt Helix's capture convention (`@indent`/`@outdent`/`@extend`) since it's Rust-native and well-documented; (2) the hybrid heuristic (compute delta vs reference line, not absolute indent) is critical for resilience to incomplete expressions; (3) no grammars ship `indents.scm` upstream — port from Helix's `runtime/queries/{lang}/indents.scm`; (4) main limitation is ERROR nodes during mid-expression typing.

### H4: The current full-reparse-per-keystroke path must be fixed before these features are viable

- **Rationale**: The editor currently calls `update_source()` (full reparse) after every edit instead of the incremental `edit()` path. Go-to-definition and indent computation need the tree to be both up-to-date and fast. Full reparses on large files (~5000+ lines) may exceed the 8ms latency budget when combined with query execution for these new features.
- **Test**: Benchmark `update_source()` vs `edit()` on a 5000-line Rust file after a single character insertion, then add `locals.scm` query execution on top. Measure total time for each path.
- **Status**: VERIFIED (confirmed dead code) — Code analysis confirms `Tab::notify_edit()` (the incremental path) is defined but never called. All five mutation call sites in `editor_state.rs` use `sync_active_tab_highlighter()` → `update_source()` (full reparse). The incremental infrastructure (`EditEvent`, `edit()`, `notify_edit()`) is complete but unwired. The cleanest fix is to have buffer mutations return byte-offset information alongside `DirtyLines`, or add a `pending_edit: Option<EditEvent>` accumulator to `EditorContext`. The IME paths (`handle_insert_text`, etc.) are easy to fix; the `handle_key_buffer` path requires capturing pre-edit cursor position before `handle_key()` dispatches.

## Exploration Log

### 2026-02-28: Initial research — locals.scm, indents.scm, incremental parsing

Conducted three parallel research tracks:

**locals.scm (go-to-definition)**:
- `locals.scm` is a naming convention, not a core tree-sitter API. The `tree-sitter` Rust crate (v0.24) has no special locals support — it's just `Query` + `QueryCursor` with user-implemented scope-walking logic.
- Core captures: `@local.scope` (scope boundary), `@local.definition` (definition site), `@local.reference` (reference site). Resolution algorithm: find reference at cursor → walk scopes upward → match by text equality.
- Grammar coverage is sparse: only JavaScript and TypeScript ship `locals.scm` upstream. Rust, Python, Go do not — nvim-treesitter and Helix maintain their own.
- Helix uses locals only for reference highlighting, not go-to-def jump. nvim-treesitter has a full `find_definition()` Lua implementation but has documented bugs for Rust/C (issue #499).
- The hard boundary with LSP is imports and type resolution — anything cross-file or type-dependent is out of reach.
- For cross-file: `tags.scm` exists in Rust/Python/Go grammars for ctags-style indexing. GitHub's stack-graphs is a more sophisticated option but requires `.tsg` authoring per language.

**indents.scm (intelligent indent)**:
- Two incompatible ecosystems: Helix uses `@indent`/`@outdent`/`@extend`; nvim-treesitter uses `@indent.begin`/`@indent.branch`/`@indent.dedent`.
- No grammars ship `indents.scm` upstream — both editors maintain their own query files.
- Helix's implementation (`helix-core/src/indent.rs`) is the most relevant reference (Rust, well-documented, MIT-licensed). Key algorithm: find deepest node at cursor → walk ancestors → collect indent/outdent captures → apply scope rules (tail vs all) → compute indent level.
- The **hybrid heuristic** (Helix's default) is critical: instead of computing absolute indent, it computes a delta relative to a reference line. This makes errors resilient — if the query is wrong for one case, it doesn't cascade.
- `@extend` captures handle Python-style whitespace-sensitive languages by expanding a node's range to encompass subsequent indented lines.
- Main limitation: incomplete expressions produce ERROR nodes that don't match indent rules. Helix has explicit `(ERROR ...)` patterns in some language queries.

**Incremental parsing gap**:
- Confirmed: `Tab::notify_edit()` (incremental path) is defined in `workspace.rs` but never called anywhere in the codebase. All five mutation sites in `editor_state.rs` call `sync_active_tab_highlighter()` → `tab.sync_highlighter()` → `hl.update_source()` (full reparse, passes `None` as old tree).
- The seam: `handle_key_buffer()` calls `handle_key()` which mutates the buffer, then calls `sync_active_tab_highlighter()` — but the `EditEvent` needs pre-edit cursor position, which is no longer available at that point.
- Cleanest fix options: (A) have buffer mutations return byte-offset info alongside `DirtyLines`, or (B) add `pending_edit: Option<EditEvent>` to `EditorContext`.
- The IME paths are easy to fix — cursor position is in scope before the mutation. The `handle_key_buffer` path is harder because the mutation happens inside `execute_command()` in `buffer_target.rs`.

## Findings

### Verified Findings

**Go-to-definition:**

- **Same-file resolution is feasible via `locals.scm`**, but requires writing/porting query files for most languages. The tree-sitter Rust crate provides `Query` + `QueryCursor`; the scope-walking resolution logic (~100 lines) must be implemented by us. (Evidence: nvim-treesitter's `locals.lua`, tree-sitter API docs)
- **The `LanguageConfig.locals_query` field already exists** in `crates/syntax/src/registry.rs` (populated for TypeScript, `#[allow(dead_code)]`). This is a ready-made attachment point. (Evidence: codebase analysis)
- **Cross-file go-to-def requires a workspace symbol index.** `tags.scm` files exist upstream for Rust, Python, and Go — these define captures for top-level definitions (functions, structs, classes, methods). A background indexer could parse files and build a name→location map. (Evidence: tree-sitter code navigation docs, grammar repos)
- **The hard boundary with LSP is type resolution.** Method calls on typed values, trait implementations, generic instantiations, and macro-expanded names cannot be resolved by tree-sitter alone. (Evidence: tree-sitter docs, nvim-treesitter limitations)

**Intelligent indent:**

- **Helix's approach is the best fit for lite-edit.** Both are Rust editors using the same `tree-sitter` crate. Helix's `@indent`/`@outdent`/`@extend` convention is well-documented and Helix ships indent queries for all 13 languages lite-edit supports. (Evidence: Helix docs, source code)
- **The hybrid heuristic is essential**, not optional. Pure tree-sitter indentation breaks on incomplete expressions (ERROR nodes). The hybrid computes a delta relative to a reference line's actual indentation, making it resilient to query inaccuracies. (Evidence: Helix docs, issue #1440)
- **Indent queries are compiled once and reused.** Like highlight queries, they're `Query` objects stored on the syntax struct and executed with `QueryCursor` per-line. No per-keystroke query compilation overhead. (Evidence: Helix source)

**Architecture:**

- **The incremental parsing gap is real but fixable.** The infrastructure exists end-to-end (`EditEvent` → `edit()` → `notify_edit()`); only the wiring in `editor_state.rs` is missing. The cleanest fix involves buffer mutations returning byte-offset info. (Evidence: codebase analysis, see Exploration Log)
- **Both features need the same infrastructure delta**: query loading (indent + locals queries alongside highlights), tree access from the editor (currently only `SyntaxHighlighter` holds the tree, and it's behind a highlighting-specific API), and incremental parsing (for responsiveness on large files).

### Hypotheses/Opinions

- **Starting with indent is higher-value than go-to-def.** Every keypress on every file benefits from smart indent; go-to-def is used less frequently and only by users who know it exists. Indent also has a simpler interaction model (automatic on Enter) vs go-to-def (requires click handling, cursor repositioning, possibly a jump stack).
- **Same-file go-to-def is worth building even though it can't resolve imports.** Local variable and function navigation covers the most common "where is this thing defined?" question. Cross-file can be layered on later.
- **We should adopt Helix's indent query convention verbatim** (not nvim-treesitter's) to minimize porting effort and leverage the most complete Rust reference implementation.
- **Incremental parsing should be fixed before adding indent/go-to-def**, since both features add query execution to the edit cycle. Full reparse + indent query on a large file may exceed the 8ms budget.

## Proposed Chunks

0. **Wire up incremental tree-sitter parsing**: Switch `editor_state.rs` from calling `sync_active_tab_highlighter()` (full reparse via `update_source()`) to the incremental `edit()` path. Have buffer mutations in `TextBuffer` return byte-offset information alongside `DirtyLines` so `EditEvent` can be constructed. Wire `handle_key_buffer()`, `handle_insert_text()`, and the IME paths to call `Tab::notify_edit()` instead of `Tab::sync_highlighter()`.
   - Priority: High (prerequisite for chunks 1-2)
   - Dependencies: None
   - Notes: The infrastructure already exists (`EditEvent`, `edit()`, `notify_edit()`). The gap is at the `editor_state.rs` → `workspace.rs` seam. See H4 analysis and Exploration Log for fix options.

1. **Add intelligent auto-indent using Helix-style indent queries**: Implement tree-sitter-based intelligent indentation. Load `indents.scm` query files alongside highlight queries in `LanguageConfig`. Port Helix's indent computation algorithm (ancestor walk, scope-aware capture collection, hybrid heuristic). Integrate into the editor's Enter-key handling to compute correct indent for new lines. Start with Rust and Python indent queries (ported from Helix's `runtime/queries/`), then extend to remaining languages.
   - Priority: High
   - Dependencies: Chunk 0 (incremental parsing)
   - Notes: Reference implementation in `helix-core/src/indent.rs`. Use Helix's `@indent`/`@outdent`/`@extend` convention. The hybrid heuristic (delta vs reference line) is essential for resilience to incomplete expressions.

2. **Add same-file go-to-definition using locals queries**: Implement tree-sitter-based same-file symbol resolution. Load `locals.scm` query files (the `locals_query` field on `LanguageConfig` already exists but is dead code). Implement scope-walking resolution: collect `@local.scope`/`@local.definition`/`@local.reference` captures, build scope tree, resolve reference at cursor to nearest enclosing definition. Wire to a click-on-symbol or keyboard shortcut action that jumps cursor to the definition. Start with Rust and Python (port `locals.scm` from nvim-treesitter).
   - Priority: Medium
   - Dependencies: Chunk 0 (incremental parsing)
   - Notes: The `LanguageConfig.locals_query` field already stores the compiled query for TypeScript. Most grammars need `locals.scm` ported from nvim-treesitter. The resolution algorithm is ~100 lines of scope-walking logic. Cannot resolve imports or cross-file symbols — document this limitation in the UI (e.g., "definition not found in this file").

3. **Add cross-file go-to-definition via workspace symbol index**: Build a background symbol indexer using `tags.scm` queries. On project open, walk all source files, parse with tree-sitter, extract top-level definitions (functions, structs, classes, modules) into a name→(file, line, col) map. Incrementally update on file save. Wire to go-to-def as a fallback when `locals.scm` resolution finds no same-file match.
   - Priority: Low (future phase)
   - Dependencies: Chunk 2 (same-file go-to-def)
   - Notes: `tags.scm` exists upstream for Rust, Python, Go. The index should be built on a background thread. Consider memory and startup-time implications for large projects. This is the furthest we can go without LSP.

## Resolution Rationale

Both features are feasible with tree-sitter alone, within well-understood boundaries:

- **Intelligent indent**: Adopt Helix's proven approach (indent queries + hybrid heuristic). The algorithm, query convention, and query files for all 13 supported languages are available from Helix's MIT-licensed source. Main risk is ERROR node handling during mid-expression typing, mitigated by the hybrid heuristic.

- **Go-to-definition**: Same-file resolution via `locals.scm` scope-walking works for local variables, parameters, and locally-defined functions. Cross-file resolution via `tags.scm` workspace indexing covers top-level definitions. The hard boundary with LSP (type resolution, trait dispatch, macro expansion) is clear and acceptable — these features provide value without crossing it.

- **Prerequisite**: The incremental parsing gap (full reparse per keystroke instead of incremental `edit()`) must be fixed first. The infrastructure exists; only the wiring is missing.

Four chunks proposed in dependency order: incremental parsing → indent + same-file go-to-def (parallel) → cross-file go-to-def.