/// Prototype: Test line-level 3-way merge for concurrent edit scenarios.
///
/// Simulates the proposed merge behavior:
/// - Base: file content at load/save time
/// - Ours: current buffer content (user's local edits)
/// - Theirs: new disk content (external program's edits)
///
/// Tests realistic scenarios: non-overlapping edits, overlapping edits,
/// additions at different points, deletions, Claude Code style bulk changes.

use similar::TextDiff;
use std::fmt;

// ─────────────────────────────────────────────────────────────────────────────
// Three-way merge implementation (line-level)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum MergeResult {
    /// Merge succeeded with no conflicts
    Clean(String),
    /// Merge produced conflicts (output contains conflict markers)
    Conflict(String),
}

impl MergeResult {
    fn is_clean(&self) -> bool {
        matches!(self, MergeResult::Clean(_))
    }

    fn content(&self) -> &str {
        match self {
            MergeResult::Clean(s) | MergeResult::Conflict(s) => s,
        }
    }
}

impl fmt::Display for MergeResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MergeResult::Clean(s) => write!(f, "{}", s),
            MergeResult::Conflict(s) => write!(f, "{}", s),
        }
    }
}

/// Represents a region in the three-way diff.
#[derive(Debug)]
enum MergeRegion {
    /// Both sides agree (or neither changed from base)
    Unchanged(Vec<String>),
    /// Only "ours" changed
    Ours(Vec<String>),
    /// Only "theirs" changed
    Theirs(Vec<String>),
    /// Both changed differently — conflict
    Conflict {
        ours: Vec<String>,
        theirs: Vec<String>,
    },
}

/// Perform a line-level three-way merge.
///
/// Uses the "diff3" algorithm:
/// 1. Compute diff(base, ours) and diff(base, theirs)
/// 2. Walk both diffs simultaneously
/// 3. Non-overlapping changes apply cleanly
/// 4. Overlapping changes with different content produce conflicts
fn three_way_merge(base: &str, ours: &str, theirs: &str) -> MergeResult {
    // Compute line-level diffs from base to each side
    let diff_ours = TextDiff::from_lines(base, ours);
    let diff_theirs = TextDiff::from_lines(base, theirs);
    let base_lines: Vec<&str> = base.lines().collect();

    // Build edit maps: for each base line, what happened?
    let ours_ops = build_edit_map(&diff_ours, base_lines.len());
    let theirs_ops = build_edit_map(&diff_theirs, base_lines.len());

    // Walk through and merge
    let mut output = Vec::new();
    let mut has_conflict = false;
    let mut i = 0; // base line index

    while i <= base_lines.len() {
        // Check for insertions before this base line
        let ours_insert = ours_ops.insertions_before(i);
        let theirs_insert = theirs_ops.insertions_before(i);

        if !ours_insert.is_empty() && !theirs_insert.is_empty() {
            if ours_insert == theirs_insert {
                // Both inserted the same thing
                output.extend(ours_insert.iter().cloned());
            } else {
                // Both inserted different things — conflict
                has_conflict = true;
                output.push("<<<<<<< buffer".to_string());
                output.extend(ours_insert.iter().cloned());
                output.push("=======".to_string());
                output.extend(theirs_insert.iter().cloned());
                output.push(">>>>>>> disk".to_string());
            }
        } else if !ours_insert.is_empty() {
            output.extend(ours_insert.iter().cloned());
        } else if !theirs_insert.is_empty() {
            output.extend(theirs_insert.iter().cloned());
        }

        if i >= base_lines.len() {
            break;
        }

        // Check what happened to base line i
        let ours_action = ours_ops.action_at(i);
        let theirs_action = theirs_ops.action_at(i);

        match (ours_action, theirs_action) {
            (Action::Keep, Action::Keep) => {
                output.push(base_lines[i].to_string());
            }
            (Action::Keep, Action::Delete) => {
                // Theirs deleted, we kept — take theirs (delete)
            }
            (Action::Delete, Action::Keep) => {
                // We deleted, theirs kept — take ours (delete)
            }
            (Action::Delete, Action::Delete) => {
                // Both deleted — agree
            }
            (Action::Keep, Action::Replace(ref new)) => {
                // Theirs changed, we kept — take theirs
                output.extend(new.iter().cloned());
            }
            (Action::Replace(ref new), Action::Keep) => {
                // We changed, theirs kept — take ours
                output.extend(new.iter().cloned());
            }
            (Action::Replace(ref ours_new), Action::Replace(ref theirs_new)) => {
                if ours_new == theirs_new {
                    // Both changed to the same thing
                    output.extend(ours_new.iter().cloned());
                } else {
                    // Both changed differently — conflict
                    has_conflict = true;
                    output.push("<<<<<<< buffer".to_string());
                    output.extend(ours_new.iter().cloned());
                    output.push("=======".to_string());
                    output.extend(theirs_new.iter().cloned());
                    output.push(">>>>>>> disk".to_string());
                }
            }
            (Action::Replace(ref ours_new), Action::Delete) => {
                has_conflict = true;
                let (ours_content, theirs_content) = (ours_new.clone(), vec![]);
                output.push("<<<<<<< buffer".to_string());
                output.extend(ours_content);
                output.push("=======".to_string());
                output.extend(theirs_content);
                output.push(">>>>>>> disk".to_string());
            }
            (Action::Delete, Action::Replace(ref theirs_new)) => {
                has_conflict = true;
                let (ours_content, theirs_content) = (vec![], theirs_new.clone());
                output.push("<<<<<<< buffer".to_string());
                output.extend(ours_content);
                output.push("=======".to_string());
                output.extend(theirs_content);
                output.push(">>>>>>> disk".to_string());
            }
        }

        i += 1;
    }

    let result = output.join("\n");
    // Preserve trailing newline if any input had one
    let result = if (ours.ends_with('\n') || theirs.ends_with('\n')) && !result.ends_with('\n') {
        result + "\n"
    } else {
        result
    };

    if has_conflict {
        MergeResult::Conflict(result)
    } else {
        MergeResult::Clean(result)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Edit map: tracks what happened to each base line
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Action {
    Keep,
    Delete,
    Replace(Vec<String>),
}

struct EditMap {
    /// Action for each base line index
    actions: Vec<Action>,
    /// Lines inserted before each base line index (key = base index)
    /// Index base_lines.len() means insertions at the end
    insertions: Vec<Vec<String>>,
}

impl EditMap {
    fn action_at(&self, base_idx: usize) -> Action {
        self.actions.get(base_idx).cloned().unwrap_or(Action::Keep)
    }

    fn insertions_before(&self, base_idx: usize) -> Vec<String> {
        self.insertions
            .get(base_idx)
            .cloned()
            .unwrap_or_default()
    }
}

fn build_edit_map(diff: &TextDiff<'_, '_, '_, str>, base_len: usize) -> EditMap {
    let mut actions = vec![Action::Keep; base_len];
    let mut insertions = vec![Vec::new(); base_len + 1];

    // Collect new lines for index-based access
    let new_lines_all: Vec<&str> = diff.new_slices().iter()
        .flat_map(|s| s.lines())
        .collect();

    for op in diff.ops() {
        use similar::DiffOp;
        match *op {
            DiffOp::Equal { .. } => {
                // Lines unchanged — default is Keep
            }
            DiffOp::Delete {
                old_index,
                old_len,
                ..
            } => {
                for j in old_index..old_index + old_len {
                    actions[j] = Action::Delete;
                }
            }
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => {
                let new_lines: Vec<String> = (new_index..new_index + new_len)
                    .map(|ni| new_lines_all.get(ni).unwrap_or(&"").to_string())
                    .collect();
                insertions[old_index].extend(new_lines);
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                // Mark old lines as deleted, first old line gets replacement
                for j in old_index..old_index + old_len {
                    actions[j] = Action::Delete;
                }
                let new_lines: Vec<String> = (new_index..new_index + new_len)
                    .map(|ni| new_lines_all.get(ni).unwrap_or(&"").to_string())
                    .collect();
                // Attach replacement to the first deleted line as a Replace action
                actions[old_index] = Action::Replace(new_lines);
            }
        }
    }

    EditMap {
        actions,
        insertions,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test scenarios
// ─────────────────────────────────────────────────────────────────────────────

struct Scenario {
    name: &'static str,
    description: &'static str,
    base: &'static str,
    ours: &'static str,
    theirs: &'static str,
    expect_clean: bool,
}

fn scenarios() -> Vec<Scenario> {
    vec![
        // ── Non-overlapping edits ──────────────────────────────────────
        Scenario {
            name: "Non-overlapping: edits at different locations",
            description: "User edits line 2, Claude Code edits line 5",
            base: "fn main() {\n    let x = 1;\n    let y = 2;\n    let z = 3;\n    println!(\"hello\");\n}\n",
            ours: "fn main() {\n    let x = 42;\n    let y = 2;\n    let z = 3;\n    println!(\"hello\");\n}\n",
            theirs: "fn main() {\n    let x = 1;\n    let y = 2;\n    let z = 3;\n    println!(\"goodbye\");\n}\n",
            expect_clean: true,
        },
        Scenario {
            name: "Non-overlapping: user adds above, Claude adds below",
            description: "User inserts a line near the top, Claude appends at bottom",
            base: "use std::io;\n\nfn main() {\n    println!(\"hello\");\n}\n",
            ours: "use std::io;\nuse std::fs;\n\nfn main() {\n    println!(\"hello\");\n}\n",
            theirs: "use std::io;\n\nfn main() {\n    println!(\"hello\");\n}\n\nfn helper() {\n    // added by claude\n}\n",
            expect_clean: true,
        },
        Scenario {
            name: "Non-overlapping: user deletes a function, Claude adds a different one",
            description: "User removes old_func, Claude adds new_func at end",
            base: "fn main() {}\n\nfn old_func() {\n    // legacy\n}\n\nfn keep_func() {}\n",
            ours: "fn main() {}\n\nfn keep_func() {}\n",
            theirs: "fn main() {}\n\nfn old_func() {\n    // legacy\n}\n\nfn keep_func() {}\n\nfn new_func() {\n    // added by claude\n}\n",
            expect_clean: true,
        },

        // ── Same edits (convergent) ────────────────────────────────────
        Scenario {
            name: "Convergent: both make the same change",
            description: "Both fix the same typo",
            base: "fn main() {\n    pritnln!(\"hello\");\n}\n",
            ours: "fn main() {\n    println!(\"hello\");\n}\n",
            theirs: "fn main() {\n    println!(\"hello\");\n}\n",
            expect_clean: true,
        },

        // ── Overlapping edits (conflict) ───────────────────────────────
        Scenario {
            name: "Conflict: both edit the same line differently",
            description: "User changes line 2 to one thing, Claude to another",
            base: "fn main() {\n    let x = 1;\n}\n",
            ours: "fn main() {\n    let x = 42;\n}\n",
            theirs: "fn main() {\n    let x = 99;\n}\n",
            expect_clean: false,
        },
        Scenario {
            name: "Conflict: user deletes, Claude modifies same line",
            description: "User deletes a line, Claude modifies it",
            base: "line1\nline2\nline3\n",
            ours: "line1\nline3\n",
            theirs: "line1\nline2_modified\nline3\n",
            expect_clean: false,
        },

        // ── Claude Code realistic patterns ─────────────────────────────
        Scenario {
            name: "Claude adds new function while user edits existing one",
            description: "Realistic: Claude adds error handling fn, user tweaks main",
            base: concat!(
                "use std::io;\n",
                "\n",
                "fn main() {\n",
                "    let data = read_file();\n",
                "    process(data);\n",
                "}\n",
                "\n",
                "fn read_file() -> String {\n",
                "    std::fs::read_to_string(\"input.txt\").unwrap()\n",
                "}\n",
            ),
            ours: concat!(
                "use std::io;\n",
                "\n",
                "fn main() {\n",
                "    let data = read_file();\n",
                "    process(data);\n",
                "    println!(\"done!\");\n",
                "}\n",
                "\n",
                "fn read_file() -> String {\n",
                "    std::fs::read_to_string(\"input.txt\").unwrap()\n",
                "}\n",
            ),
            theirs: concat!(
                "use std::io;\n",
                "\n",
                "fn main() {\n",
                "    let data = read_file();\n",
                "    process(data);\n",
                "}\n",
                "\n",
                "fn read_file() -> String {\n",
                "    std::fs::read_to_string(\"input.txt\").unwrap()\n",
                "}\n",
                "\n",
                "fn handle_error(e: std::io::Error) {\n",
                "    eprintln!(\"Error: {}\", e);\n",
                "    std::process::exit(1);\n",
                "}\n",
            ),
            expect_clean: true,
        },
        Scenario {
            name: "Claude refactors function body while user adds import",
            description: "Realistic: Claude rewrites read_file internals, user adds import at top",
            base: concat!(
                "use std::io;\n",
                "\n",
                "fn main() {\n",
                "    let data = read_file();\n",
                "    println!(\"{}\", data);\n",
                "}\n",
                "\n",
                "fn read_file() -> String {\n",
                "    std::fs::read_to_string(\"input.txt\").unwrap()\n",
                "}\n",
            ),
            ours: concat!(
                "use std::io;\n",
                "use std::path::Path;\n",
                "\n",
                "fn main() {\n",
                "    let data = read_file();\n",
                "    println!(\"{}\", data);\n",
                "}\n",
                "\n",
                "fn read_file() -> String {\n",
                "    std::fs::read_to_string(\"input.txt\").unwrap()\n",
                "}\n",
            ),
            theirs: concat!(
                "use std::io;\n",
                "\n",
                "fn main() {\n",
                "    let data = read_file();\n",
                "    println!(\"{}\", data);\n",
                "}\n",
                "\n",
                "fn read_file() -> String {\n",
                "    match std::fs::read_to_string(\"input.txt\") {\n",
                "        Ok(content) => content,\n",
                "        Err(e) => {\n",
                "            eprintln!(\"Failed to read: {}\", e);\n",
                "            String::new()\n",
                "        }\n",
                "    }\n",
                "}\n",
            ),
            expect_clean: true,
        },
        Scenario {
            name: "Claude reformats while user edits content",
            description: "Realistic conflict: Claude reformats a function the user is actively editing",
            base: concat!(
                "fn calculate(a: i32, b: i32) -> i32 {\n",
                "    let result = a + b;\n",
                "    result\n",
                "}\n",
            ),
            ours: concat!(
                "fn calculate(a: i32, b: i32) -> i32 {\n",
                "    let result = a * b + 1;\n",
                "    result\n",
                "}\n",
            ),
            theirs: concat!(
                "fn calculate(\n",
                "    a: i32,\n",
                "    b: i32,\n",
                ") -> i32 {\n",
                "    let result = a + b;\n",
                "    result\n",
                "}\n",
            ),
            expect_clean: false, // first line changed in both
        },
        Scenario {
            name: "Adjacent edits: user edits line N, Claude edits line N+1",
            description: "Edits on immediately adjacent lines — should merge cleanly",
            base: "fn main() {\n    let x = 1;\n    let y = 2;\n    let z = 3;\n}\n",
            ours: "fn main() {\n    let x = 10;\n    let y = 2;\n    let z = 3;\n}\n",
            theirs: "fn main() {\n    let x = 1;\n    let y = 20;\n    let z = 3;\n}\n",
            expect_clean: true,
        },
        Scenario {
            name: "Empty base (new file created externally)",
            description: "Buffer is empty, external program writes full file",
            base: "",
            ours: "",
            theirs: "fn main() {\n    println!(\"hello\");\n}\n",
            expect_clean: true,
        },
        Scenario {
            name: "User typing at end while Claude prepends",
            description: "User appends to file, Claude adds header comment at top",
            base: "fn main() {\n    todo!();\n}\n",
            ours: "fn main() {\n    todo!();\n}\n\n// user's note\n",
            theirs: "// Generated by Claude\n\nfn main() {\n    todo!();\n}\n",
            expect_clean: true,
        },
    ]
}

fn main() {
    let scenarios = scenarios();
    let mut passed = 0;
    let mut failed = 0;
    let mut clean_count = 0;
    let mut conflict_count = 0;

    println!("=== Three-Way Merge Prototype ===\n");

    for (i, scenario) in scenarios.iter().enumerate() {
        println!("--- Scenario {}: {} ---", i + 1, scenario.name);
        println!("    {}", scenario.description);

        let result = three_way_merge(scenario.base, scenario.ours, scenario.theirs);
        let got_clean = result.is_clean();

        if got_clean {
            clean_count += 1;
        } else {
            conflict_count += 1;
        }

        let status = if got_clean == scenario.expect_clean {
            passed += 1;
            "PASS"
        } else {
            failed += 1;
            "FAIL"
        };

        println!(
            "    Result: {} (expected {}, got {})",
            status,
            if scenario.expect_clean {
                "clean"
            } else {
                "conflict"
            },
            if got_clean { "clean" } else { "conflict" }
        );

        // Show the merged output for interesting cases
        if !got_clean || !scenario.expect_clean {
            println!("    Output:");
            for line in result.content().lines() {
                println!("      | {}", line);
            }
        }
        println!();
    }

    println!("=== Summary ===");
    println!(
        "  {}/{} scenarios matched expectations ({} passed, {} failed)",
        passed,
        scenarios.len(),
        passed,
        failed
    );
    println!(
        "  {} clean merges, {} conflicts out of {} total",
        clean_count,
        conflict_count,
        scenarios.len()
    );
    println!(
        "  Clean merge rate: {:.0}%",
        (clean_count as f64 / scenarios.len() as f64) * 100.0
    );
}
