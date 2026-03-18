

<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Replace the hardcoded `is_excluded()` function in `crates/editor/src/file_index.rs`
with git-aware file discovery. The current implementation blanket-excludes all
dotfile paths, `target/`, and `node_modules/`, which hides git-tracked files in
directories like `.github/`, `.config/`, and `.claude/`.

**Strategy**: Use `git ls-files` and `git check-ignore` via `std::process::Command`
to determine which files belong in the index. This avoids adding a heavy native
dependency like `git2`/libgit2 while being perfectly accurate — git itself is the
authority on what's tracked and what's ignored. The subprocess cost is paid once
during the initial walk; the watcher path uses a cached gitignore checker.

**Two-phase approach**:

1. **Initial walk** — In a git repo, replace the recursive `walk_directory` with
   `git ls-files --cached --others --exclude-standard` which returns exactly the
   files we want: tracked files plus untracked-but-not-ignored files. This is a
   single subprocess call that returns the full file list, which is faster than
   walking the tree ourselves and filtering.

2. **Watcher path** — For incremental events (new files created, renames), we
   need to decide whether a newly-appearing path should enter the cache. Use
   `git check-ignore --quiet <path>` to test individual paths, falling back to
   the default exclusion list for non-git directories.

3. **Non-git fallback** — When the root is not inside a git repository (no
   `.git/` directory found), fall back to a hardcoded exclusion list that
   excludes `.git/`, `target/`, and `node_modules/` — but does NOT blanket-
   exclude all dotfiles, preserving the current behavior for the common case
   while being more permissive for non-git projects too.

**Testing approach**: Per TESTING_PHILOSOPHY.md, write tests first for the
git-aware filtering logic. Tests will create temporary git repos with
`.github/` directories and `.gitignore` files to verify that tracked dotfiles
appear and ignored paths don't. The `is_excluded` replacement function is pure
logic that can be tested without platform dependencies.

## Sequence

### Step 1: Add git detection helper

Create a helper function `fn is_git_repo(root: &Path) -> bool` that checks
whether the root directory is inside a git repository by running
`git rev-parse --is-inside-work-tree` (or simply checking for `.git/`
directory existence ascending from root).

Also create `fn git_repo_root(root: &Path) -> Option<PathBuf>` to find the
actual git root, since the indexed directory might be a subdirectory of the
repo.

Location: `crates/editor/src/file_index.rs`

### Step 2: Implement git-based file listing

Create `fn git_ls_files(root: &Path) -> Option<Vec<PathBuf>>` that runs:
```
git ls-files --cached --others --exclude-standard -z
```
from the `root` directory. The `-z` flag uses NUL separators for reliable
parsing of paths with special characters. Returns `None` if the command fails
(not a git repo, git not installed, etc.).

The returned paths are relative to the git root; convert them to be relative
to the index `root` if they differ.

Location: `crates/editor/src/file_index.rs`

### Step 3: Implement git-based path exclusion check

Create `fn is_git_ignored(root: &Path, relative: &Path) -> bool` that runs:
```
git check-ignore --quiet <path>
```
from the `root` directory. Returns `true` if the path is ignored (exit code 0),
`false` if not ignored (exit code 1). Returns `false` on any error (git not
available, not a git repo) to err on the side of inclusion.

Also always exclude the `.git/` directory itself, since `git check-ignore`
won't report it.

Location: `crates/editor/src/file_index.rs`

### Step 4: Implement fallback exclusion for non-git directories

Create `fn is_excluded_fallback(path: &Path) -> bool` that implements sensible
defaults for non-git directories:
- Exclude `.git/` directory
- Exclude `target/` directory
- Exclude `node_modules/` directory
- Do NOT blanket-exclude all dotfiles (this is the key behavioral change
  even for non-git repos)

This replaces the old `is_excluded()` for the non-git case.

Location: `crates/editor/src/file_index.rs`

### Step 5: Refactor walk_directory to use git ls-files when available

Modify `start_internal` to detect whether the root is a git repo. If so:
- Call `git_ls_files()` to get the full file list in one shot
- Populate the cache directly from the result
- Skip the recursive `walk_directory` entirely

If not a git repo (or if `git_ls_files` fails):
- Use the existing `walk_directory` but with `is_excluded_fallback` instead
  of the old `is_excluded`

Store a flag (e.g., `is_git: bool`) on `FileIndex` so the watcher event
handler knows which exclusion strategy to use.

Location: `crates/editor/src/file_index.rs` (modify `start_internal`,
`walk_directory`)

### Step 6: Update watcher event filtering to be git-aware

Modify `handle_fs_event` and the rename handling to use the appropriate
exclusion check:
- If git repo: use `is_git_ignored(root, &relative)` plus always exclude
  `.git/` paths
- If not git repo: use `is_excluded_fallback(&relative)`

Pass the `is_git` flag through to `process_watcher_events` and
`handle_fs_event`. Add it as a parameter or store it in a shared context.

Also update `query_empty` which currently calls `is_excluded` on recency
entries — replace with the appropriate git-aware or fallback check.

Location: `crates/editor/src/file_index.rs` (modify `process_watcher_events`,
`handle_fs_event`, `query_empty`)

### Step 7: Remove old is_excluded function

Delete the old `is_excluded()` function and all call sites, replacing them
with the new git-aware or fallback paths from steps 4-6. This is a cleanup
step after all callers have been migrated.

Location: `crates/editor/src/file_index.rs`

### Step 8: Write tests for git-aware file discovery

Write failing tests first, then verify they pass after implementation:

1. **test_git_repo_includes_tracked_dotfiles** — Create a temp git repo with
   `git init`, add a `.github/workflows/ci.yml` file, `git add` it. Start a
   `FileIndex`, wait for indexing, query for `ci.yml`. Assert it appears.

2. **test_git_repo_excludes_gitignored** — Create a temp git repo, add
   `target/` to `.gitignore`, create `target/debug/foo`. Start a `FileIndex`.
   Assert `target/debug/foo` does NOT appear.

3. **test_git_repo_excludes_git_dir** — Verify `.git/HEAD` and other `.git/`
   contents never appear in results.

4. **test_git_repo_includes_untracked_non_ignored** — Create a new file that
   hasn't been `git add`ed but isn't in `.gitignore`. Assert it appears.

5. **test_non_git_fallback_excludes_target** — Create a non-git temp dir with
   `target/` and `src/`. Assert `target/` contents excluded, `src/` included.

6. **test_non_git_fallback_allows_dotfiles** — Create a non-git temp dir with
   `.config/settings.toml`. Assert it appears (new behavior vs old).

7. **test_fallback_excludes_git_dir** — Non-git directory with `.git/` still
   excludes it.

8. **test_is_excluded_fallback_unit** — Unit tests for `is_excluded_fallback`
   with various path patterns.

Location: `crates/editor/src/file_index.rs` (in `#[cfg(test)] mod tests`)

### Step 9: Update existing tests

Several existing tests depend on the old `is_excluded` behavior (e.g.,
`test_is_excluded_gitignore`, `test_is_excluded_git_config`,
`test_is_excluded_hidden_in_path`). Update or remove these tests to reflect
the new git-aware behavior:

- `test_is_excluded_gitignore` → Remove (`.gitignore` is a tracked file, should
  NOT be excluded in git repos)
- `test_is_excluded_git_config` → Rename to test that `.git/` is always excluded
- `test_is_excluded_hidden_in_path` → Remove or change to test fallback behavior
- `test_is_excluded_target` → Update to test fallback behavior
- `test_is_excluded_node_modules` → Update to test fallback behavior
- Integration test `test_file_index_excludes_hidden_files` → Update to verify
  hidden files ARE included when tracked by git

Location: `crates/editor/src/file_index.rs` (in `#[cfg(test)] mod tests`)

## Dependencies

- **External tool**: `git` must be available on PATH. This is a reasonable
  assumption for a developer-targeted code editor on macOS (ships with Xcode
  Command Line Tools). No new Rust crate dependencies are needed.

## Risks and Open Questions

- **git subprocess latency**: `git ls-files` on very large repos could be slow.
  For typical project sizes this is sub-100ms and faster than a manual recursive
  walk. If profiling reveals an issue, we can cache the result and update
  incrementally, or switch to the `ignore` crate (BurntSushi's gitignore parser
  from ripgrep) for native performance.
- **git check-ignore latency on watcher path**: Each new file event triggers a
  subprocess call. For burst events (e.g., `npm install` creating many files),
  this could be slow. Mitigation: batch multiple paths into a single
  `git check-ignore` call, or cache recent results. Start simple and optimize
  if profiling shows a problem.
- **Submodules and worktrees**: `git ls-files` handles submodules correctly by
  default (lists their checked-out files). Git worktrees also work since
  `git ls-files` operates on the current working tree. No special handling needed.
- **Race between walk and watcher**: The current architecture already handles
  this — the watcher starts before the walk completes, and duplicates are
  filtered. With `git ls-files` the walk is a single atomic snapshot, which
  actually improves the race window.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->