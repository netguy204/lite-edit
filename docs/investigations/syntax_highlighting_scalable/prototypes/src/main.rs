///! H2 Benchmark: Incremental re-highlighting after edits
///!
///! Tests tree-sitter incremental parse + highlight query performance on
///! a real ~5800-line Rust file from the lite-edit codebase.
///!
///! Scenarios:
///!   1. Initial full parse
///!   2. Single character insertion (mid-file)
///!   3. Newline insertion (line split)
///!   4. Deleting a character
///!   5. Inserting a multi-line paste (50 lines)
///!   6. Highlight query on changed range after single-char edit
///!   7. Full file highlight query (worst case)

use std::time::Instant;
use tree_sitter::{InputEdit, Parser, Point, Tree};
use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};

/// Standard highlight capture names used by tree-sitter-rust's highlights.scm
const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "escape",
    "function",
    "function.builtin",
    "function.macro",
    "keyword",
    "label",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
];

fn main() {
    // Load the benchmark file
    let source_path = std::env::args().nth(1).expect(
        "Usage: ts-highlight-bench <path-to-rust-file>"
    );
    let source = std::fs::read_to_string(&source_path).expect("Failed to read source file");
    let source_bytes = source.as_bytes();

    println!("=== Tree-sitter H2 Benchmark ===");
    println!("File: {}", source_path);
    println!("Size: {} bytes, {} lines", source.len(), source.lines().count());
    println!();

    // =========================================================================
    // Setup: Create parser with Rust language
    // =========================================================================
    let mut parser = Parser::new();
    let language = tree_sitter_rust::LANGUAGE;
    parser.set_language(&language.into()).expect("Failed to set language");

    // =========================================================================
    // Benchmark 1: Initial full parse
    // =========================================================================
    let iterations = 50;

    let mut total_ns = 0u128;
    let mut tree: Option<Tree> = None;
    for _ in 0..iterations {
        let start = Instant::now();
        tree = Some(parser.parse(source_bytes, None).expect("Parse failed"));
        let elapsed = start.elapsed();
        total_ns += elapsed.as_nanos();
    }
    let avg_initial_us = total_ns / iterations as u128 / 1000;
    println!("1. Initial full parse:        {:>8} µs avg ({} iterations)", avg_initial_us, iterations);

    let tree = tree.unwrap();

    // =========================================================================
    // Benchmark 2: Single character insertion at mid-file
    // =========================================================================
    // Insert 'x' at roughly the middle of the file
    let midpoint = source.len() / 2;
    // Find the byte offset of the midpoint in terms of (row, col)
    let (mid_row, mid_col) = byte_offset_to_point(&source, midpoint);

    let mut mutated_source = source.clone();
    mutated_source.insert(midpoint, 'x');
    let mutated_bytes = mutated_source.as_bytes();

    let mut total_ns = 0u128;
    for _ in 0..iterations {
        let mut edited_tree = tree.clone();
        edited_tree.edit(&InputEdit {
            start_byte: midpoint,
            old_end_byte: midpoint,
            new_end_byte: midpoint + 1,
            start_position: Point::new(mid_row, mid_col),
            old_end_position: Point::new(mid_row, mid_col),
            new_end_position: Point::new(mid_row, mid_col + 1),
        });
        let start = Instant::now();
        let _new_tree = parser.parse(mutated_bytes, Some(&edited_tree)).expect("Incremental parse failed");
        let elapsed = start.elapsed();
        total_ns += elapsed.as_nanos();
    }
    let avg_incr_single_us = total_ns / iterations as u128 / 1000;
    println!("2. Incremental single char:   {:>8} µs avg", avg_incr_single_us);

    // =========================================================================
    // Benchmark 3: Newline insertion (line split)
    // =========================================================================
    let mut newline_source = source.clone();
    newline_source.insert(midpoint, '\n');
    let newline_bytes = newline_source.as_bytes();

    let mut total_ns = 0u128;
    for _ in 0..iterations {
        let mut edited_tree = tree.clone();
        edited_tree.edit(&InputEdit {
            start_byte: midpoint,
            old_end_byte: midpoint,
            new_end_byte: midpoint + 1,
            start_position: Point::new(mid_row, mid_col),
            old_end_position: Point::new(mid_row, mid_col),
            new_end_position: Point::new(mid_row + 1, 0),
        });
        let start = Instant::now();
        let _new_tree = parser.parse(newline_bytes, Some(&edited_tree)).expect("Incremental parse failed");
        let elapsed = start.elapsed();
        total_ns += elapsed.as_nanos();
    }
    let avg_incr_newline_us = total_ns / iterations as u128 / 1000;
    println!("3. Incremental newline:       {:>8} µs avg", avg_incr_newline_us);

    // =========================================================================
    // Benchmark 4: Delete a character at mid-file
    // =========================================================================
    let mut deleted_source = source.clone();
    deleted_source.remove(midpoint);
    let deleted_bytes = deleted_source.as_bytes();

    let mut total_ns = 0u128;
    for _ in 0..iterations {
        let mut edited_tree = tree.clone();
        edited_tree.edit(&InputEdit {
            start_byte: midpoint,
            old_end_byte: midpoint + 1,
            new_end_byte: midpoint,
            start_position: Point::new(mid_row, mid_col),
            old_end_position: Point::new(mid_row, mid_col + 1),
            new_end_position: Point::new(mid_row, mid_col),
        });
        let start = Instant::now();
        let _new_tree = parser.parse(deleted_bytes, Some(&edited_tree)).expect("Incremental parse failed");
        let elapsed = start.elapsed();
        total_ns += elapsed.as_nanos();
    }
    let avg_incr_delete_us = total_ns / iterations as u128 / 1000;
    println!("4. Incremental delete char:   {:>8} µs avg", avg_incr_delete_us);

    // =========================================================================
    // Benchmark 5: Large paste (50 lines of code)
    // =========================================================================
    let paste_text: String = (0..50).map(|i| format!("    let var_{} = {};\n", i, i * 42)).collect();
    let paste_len = paste_text.len();
    let paste_lines = paste_text.lines().count();

    let mut pasted_source = source.clone();
    pasted_source.insert_str(midpoint, &paste_text);
    let pasted_bytes = pasted_source.as_bytes();

    let mut total_ns = 0u128;
    for _ in 0..iterations {
        let mut edited_tree = tree.clone();
        edited_tree.edit(&InputEdit {
            start_byte: midpoint,
            old_end_byte: midpoint,
            new_end_byte: midpoint + paste_len,
            start_position: Point::new(mid_row, mid_col),
            old_end_position: Point::new(mid_row, mid_col),
            new_end_position: Point::new(mid_row + paste_lines, 0),
        });
        let start = Instant::now();
        let _new_tree = parser.parse(pasted_bytes, Some(&edited_tree)).expect("Incremental parse failed");
        let elapsed = start.elapsed();
        total_ns += elapsed.as_nanos();
    }
    let avg_incr_paste_us = total_ns / iterations as u128 / 1000;
    println!("5. Incremental 50-line paste: {:>8} µs avg", avg_incr_paste_us);

    // =========================================================================
    // Benchmark 6: Highlight query on changed range after single-char edit
    // =========================================================================
    // First, do the incremental parse to get the new tree
    let mut edited_tree_for_hl = tree.clone();
    edited_tree_for_hl.edit(&InputEdit {
        start_byte: midpoint,
        old_end_byte: midpoint,
        new_end_byte: midpoint + 1,
        start_position: Point::new(mid_row, mid_col),
        old_end_position: Point::new(mid_row, mid_col),
        new_end_position: Point::new(mid_row, mid_col + 1),
    });
    let new_tree = parser.parse(mutated_bytes, Some(&edited_tree_for_hl)).expect("Parse failed");

    // Find changed ranges between old and new tree
    let changed_ranges = tree.changed_ranges(&new_tree).collect::<Vec<_>>();
    println!();
    println!("   Changed ranges after single-char edit: {}", changed_ranges.len());
    for (i, range) in changed_ranges.iter().enumerate() {
        println!("     range[{}]: bytes {}..{} (rows {}..{})",
            i, range.start_byte, range.end_byte,
            range.start_point.row, range.end_point.row);
    }

    // Benchmark: highlight just the changed lines using tree-sitter-highlight
    let mut hl_config = HighlightConfiguration::new(
        language.into(),
        "rust",
        tree_sitter_rust::HIGHLIGHTS_QUERY,
        "",  // injections
        "",  // locals
    ).expect("Failed to create highlight config");
    hl_config.configure(HIGHLIGHT_NAMES);

    let mut highlighter = Highlighter::new();

    // Highlight the full mutated source but measure time
    // (tree-sitter-highlight doesn't support range-limited highlighting directly,
    //  but the underlying tree-sitter query cursor does range restriction)
    let mut total_ns = 0u128;
    let mut total_events = 0usize;
    for _ in 0..iterations {
        let start = Instant::now();
        let highlights = highlighter.highlight(
            &hl_config,
            mutated_bytes,
            None,
            |_| None,
        ).expect("Highlight failed");

        let mut events = 0;
        for event in highlights {
            match event.expect("highlight event error") {
                HighlightEvent::Source { start: _, end: _ } => events += 1,
                HighlightEvent::HighlightStart(_) => events += 1,
                HighlightEvent::HighlightEnd => events += 1,
            }
        }
        let elapsed = start.elapsed();
        total_ns += elapsed.as_nanos();
        total_events = events;
    }
    let avg_full_highlight_us = total_ns / iterations as u128 / 1000;
    println!();
    println!("6. Full file highlight query:  {:>8} µs avg ({} events)", avg_full_highlight_us, total_events);

    // =========================================================================
    // Benchmark 7: Highlight a single visible viewport (~60 lines)
    // =========================================================================
    // Simulate highlighting just the lines visible on screen
    // Find byte range for 60 lines around the midpoint
    let viewport_start_line = mid_row.saturating_sub(30);
    let viewport_end_line = mid_row + 30;
    let vp_start_byte = line_to_byte_offset(&source, viewport_start_line);
    let vp_end_byte = line_to_byte_offset(&source, viewport_end_line).min(source.len());

    println!();
    println!("   Viewport: lines {}..{} (bytes {}..{}, {} bytes)",
        viewport_start_line, viewport_end_line,
        vp_start_byte, vp_end_byte, vp_end_byte - vp_start_byte);

    // Extract viewport text for standalone highlighting
    let viewport_text = &source[vp_start_byte..vp_end_byte];
    let viewport_bytes = viewport_text.as_bytes();

    let mut total_ns = 0u128;
    let mut vp_events = 0;
    for _ in 0..iterations {
        let start = Instant::now();
        let highlights = highlighter.highlight(
            &hl_config,
            viewport_bytes,
            None,
            |_| None,
        ).expect("Highlight failed");

        let mut events = 0;
        for event in highlights {
            match event.expect("highlight event error") {
                HighlightEvent::Source { .. } => events += 1,
                HighlightEvent::HighlightStart(_) => events += 1,
                HighlightEvent::HighlightEnd => events += 1,
            }
        }
        let elapsed = start.elapsed();
        total_ns += elapsed.as_nanos();
        vp_events = events;
    }
    let avg_vp_highlight_us = total_ns / iterations as u128 / 1000;
    println!("7. Viewport highlight (60 ln): {:>7} µs avg ({} events)", avg_vp_highlight_us, vp_events);

    // =========================================================================
    // Summary
    // =========================================================================
    println!();
    println!("=== Summary ===");
    println!("Latency budget: <8000 µs total keypress-to-glyph");
    println!();
    println!("Incremental parse (typical edit):  {} µs", avg_incr_single_us);
    println!("Incremental parse (50-line paste):  {} µs", avg_incr_paste_us);
    println!("Full file highlight:              {} µs", avg_full_highlight_us);
    println!("Viewport-only highlight (60 ln):  {} µs", avg_vp_highlight_us);
    println!();

    let sync_total = avg_incr_single_us + avg_full_highlight_us;
    let sync_vp_total = avg_incr_single_us + avg_vp_highlight_us;
    println!("Sync path (parse + full highlight):     {} µs ({:.1}% of budget)",
        sync_total, sync_total as f64 / 8000.0 * 100.0);
    println!("Sync path (parse + viewport highlight): {} µs ({:.1}% of budget)",
        sync_vp_total, sync_vp_total as f64 / 8000.0 * 100.0);

    if sync_vp_total < 1000 {
        println!();
        println!("✅ H2 VERIFIED: Incremental re-highlighting stays under 1ms");
        println!("✅ H3 LIKELY: Synchronous highlighting feasible (viewport-scoped)");
    } else if sync_vp_total < 2000 {
        println!();
        println!("⚠️  H2 MARGINAL: Incremental re-highlighting ~{} µs", sync_vp_total);
        println!("   May need viewport-scoped highlighting or async fallback");
    } else {
        println!();
        println!("❌ H2 FALSIFIED: Incremental re-highlighting too slow ({} µs)", sync_vp_total);
        println!("   Async highlighting path (H4) required");
    }
}

/// Convert a byte offset to (row, col) in the source text.
fn byte_offset_to_point(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut row = 0;
    let mut col = 0;
    for (i, ch) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += ch.len_utf8();
        }
    }
    (row, col)
}

/// Convert a line number to byte offset in the source text.
fn line_to_byte_offset(source: &str, target_line: usize) -> usize {
    let mut line = 0;
    for (i, ch) in source.char_indices() {
        if line >= target_line {
            return i;
        }
        if ch == '\n' {
            line += 1;
        }
    }
    source.len()
}
