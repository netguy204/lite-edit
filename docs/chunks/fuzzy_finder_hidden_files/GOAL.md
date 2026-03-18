---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/file_index.rs
- crates/editor/Cargo.toml
code_references:
- ref: crates/editor/src/file_index.rs#FileIndex::is_git
  implements: "Flag to determine git-aware vs fallback exclusion strategy"
- ref: crates/editor/src/file_index.rs#FileIndex::should_exclude
  implements: "Routes exclusion checks to git-ignored or fallback logic based on is_git flag"
- ref: crates/editor/src/file_index.rs#FileIndex::start_internal
  implements: "Git repo detection and git ls-files based initial walk instead of recursive directory walk"
- ref: crates/editor/src/file_index.rs#is_git_repo
  implements: "Detects whether root directory is inside a git repository"
- ref: crates/editor/src/file_index.rs#git_ls_files
  implements: "Runs git ls-files to get tracked and untracked-but-not-ignored files"
- ref: crates/editor/src/file_index.rs#is_git_ignored
  implements: "Runs git check-ignore to test individual paths for watcher events"
- ref: crates/editor/src/file_index.rs#has_git_dir_component
  implements: "Helper to always exclude .git/ directory paths"
- ref: crates/editor/src/file_index.rs#is_excluded_fallback
  implements: "Fallback exclusion for non-git directories (excludes .git, target, node_modules but not all dotfiles)"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- wrap_scroll_to_cursor
---

# Chunk Goal

## Minor Goal

Change the fuzzy finder's file discovery to use git-awareness instead of
hardcoded exclusion rules. Currently `is_excluded()` in
`crates/editor/src/file_index.rs` blanket-excludes all paths with components
starting with `.`, plus `target/` and `node_modules/`. This means files in
directories like `.github/`, `.config/`, or `.claude/` are invisible to the
fuzzy finder even though they are tracked by git.

Replace the hardcoded exclusion logic with git-based filtering: include all
files tracked by git (or untracked but not ignored), and exclude only files
that are git-ignored. In non-git directories, fall back to sensible defaults.

## Success Criteria

- Files in hidden directories (e.g. `.github/workflows/ci.yml`,
  `.claude/commands/foo.md`) appear in fuzzy finder results when they are
  tracked by git
- Git-ignored files and directories (e.g. `target/`, `node_modules/`,
  `.git/`) are still excluded
- In non-git directories, a reasonable fallback exclusion list applies
  (at minimum `.git/`, `target/`, `node_modules/`)
- No regression in indexing performance for typical project sizes