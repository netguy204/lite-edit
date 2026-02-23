//! Profiling test for scroll performance bottlenecks.
//!
//! Run with: cargo test -p lite-edit-syntax --release --test profile_scroll -- --nocapture
//!
//! This measures each hypothesized bottleneck from the scroll_perf_deep investigation.

use lite_edit_syntax::{LanguageRegistry, SyntaxHighlighter, SyntaxTheme};
use std::time::Instant;

fn make_highlighter(source: &str) -> SyntaxHighlighter {
    let registry = LanguageRegistry::new();
    let config = registry.config_for_extension("rs").unwrap();
    let theme = SyntaxTheme::catppuccin_mocha();
    SyntaxHighlighter::new(config, source, theme).unwrap()
}

/// Load a real large Rust file for testing
fn load_large_file() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../editor/src/editor_state.rs"
    );
    match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => {
            // Fallback: generate a synthetic large file
            let mut source = String::new();
            for i in 0..2000 {
                source.push_str(&format!(
                    "/// Documentation for function_{}\n\
                     fn function_{}(x: i32, y: &str) -> Result<(), Box<dyn std::error::Error>> {{\n\
                     \tlet value = x * {};\n\
                     \tprintln!(\"{{}}{{i}}\", value, y);\n\
                     \tOk(())\n\
                     }}\n\n",
                    i, i, i * 42
                ));
            }
            source
        }
    }
}

#[test]
fn profile_all_bottlenecks() {
    let source = load_large_file();
    let line_count = source.chars().filter(|&c| c == '\n').count() + 1;
    eprintln!("\n======================================================================");
    eprintln!("SCROLL PERFORMANCE PROFILING");
    eprintln!("File: {} lines, {} bytes", line_count, source.len());
    eprintln!("======================================================================\n");

    // =========================================================================
    // H1: highlight_viewport cost scales with scroll position
    // =========================================================================
    eprintln!("--- H1: highlight_viewport() cost at different scroll positions ---");
    eprintln!("  (Each creates a fresh highlighter to force cache miss)\n");

    for &start_line in &[0, 100, 500, 1000, 2000, 4000] {
        if start_line >= line_count {
            continue;
        }
        let end_line = (start_line + 80).min(line_count);
        let hl_fresh = make_highlighter(&source);

        let start = Instant::now();
        hl_fresh.highlight_viewport(start_line, end_line);
        let elapsed = start.elapsed();

        eprintln!(
            "  viewport({:>5}..{:>5}): {:>8}µs  ({:.1}% of 8ms budget)",
            start_line,
            end_line,
            elapsed.as_micros(),
            elapsed.as_micros() as f64 / 80.0,  // 8000µs = 8ms, /80 = percent
        );
    }

    eprintln!();

    // =========================================================================
    // H1 detail: single-line highlight at different positions (cache miss)
    // =========================================================================
    eprintln!("--- H1 detail: highlight_line() cold (cache miss) at different positions ---\n");

    for &line in &[0, 100, 500, 1000, 2000, 4000] {
        if line >= line_count {
            continue;
        }
        let hl_fresh = make_highlighter(&source);

        let start = Instant::now();
        let _ = hl_fresh.highlight_line(line);
        let elapsed = start.elapsed();

        eprintln!(
            "  highlight_line({:>5}) cold: {:>8}µs",
            line,
            elapsed.as_micros(),
        );
    }

    eprintln!();

    // =========================================================================
    // H1b: line_count() cost
    // =========================================================================
    eprintln!("--- H1b: line_count() scanning cost ---\n");
    {
        let iterations = 100;

        let start = Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(source.chars().filter(|&c| c == '\n').count() + 1);
        }
        let elapsed = start.elapsed();
        eprintln!(
            "  chars().filter() x{}: {:>8}µs total, {:>6.1}µs each",
            iterations,
            elapsed.as_micros(),
            elapsed.as_micros() as f64 / iterations as f64,
        );

        let start = Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(source.as_bytes().iter().filter(|&&b| b == b'\n').count() + 1);
        }
        let elapsed = start.elapsed();
        eprintln!(
            "  bytes().filter()  x{}: {:>8}µs total, {:>6.1}µs each",
            iterations,
            elapsed.as_micros(),
            elapsed.as_micros() as f64 / iterations as f64,
        );
    }

    eprintln!();

    // =========================================================================
    // H2: styled_line clone cost (cache hit path)
    // =========================================================================
    eprintln!("--- H2: StyledLine clone cost (cache hit path) ---\n");
    {
        let mid = line_count / 2;
        let vp_end = (mid + 80).min(line_count);
        let visible_lines = 60.min(vp_end - mid);
        let iterations = 100;

        let hl_cached = make_highlighter(&source);
        hl_cached.highlight_viewport(mid, vp_end);

        // 60 calls (what we'd need with single-call optimization)
        let start = Instant::now();
        for _ in 0..iterations {
            for line in mid..mid + visible_lines {
                std::hint::black_box(hl_cached.highlight_line(line));
            }
        }
        let elapsed = start.elapsed();
        let per_frame_60 = elapsed.as_micros() as f64 / iterations as f64;
        eprintln!(
            "   60 × highlight_line (1× per line) x{}: {:>6.1}µs per frame",
            iterations,
            per_frame_60,
        );

        // 180 calls (current 3× per line pattern)
        let start = Instant::now();
        for _ in 0..iterations {
            for line in mid..mid + visible_lines {
                std::hint::black_box(hl_cached.highlight_line(line));
                std::hint::black_box(hl_cached.highlight_line(line));
                std::hint::black_box(hl_cached.highlight_line(line));
            }
        }
        let elapsed = start.elapsed();
        let per_frame_180 = elapsed.as_micros() as f64 / iterations as f64;
        eprintln!(
            "  180 × highlight_line (3× per line) x{}: {:>6.1}µs per frame",
            iterations,
            per_frame_180,
        );
        eprintln!(
            "  => Saving from 1× optimization: {:>6.1}µs/frame ({:.0}% reduction)",
            per_frame_180 - per_frame_60,
            (per_frame_180 - per_frame_60) / per_frame_180 * 100.0,
        );
    }

    eprintln!();

    // =========================================================================
    // Full frame simulation
    // =========================================================================
    eprintln!("--- Full frame simulation (viewport highlight + 180 line clones) ---\n");

    for &start_line in &[0, 500, 1000, 2000, 4000] {
        if start_line >= line_count {
            continue;
        }
        let end_line = (start_line + 80).min(line_count);
        let visible_end = (start_line + 60).min(end_line);
        let hl_fresh = make_highlighter(&source);

        let t0 = Instant::now();
        hl_fresh.highlight_viewport(start_line, end_line);
        let t1 = Instant::now();
        for line in start_line..visible_end {
            std::hint::black_box(hl_fresh.highlight_line(line));
            std::hint::black_box(hl_fresh.highlight_line(line));
            std::hint::black_box(hl_fresh.highlight_line(line));
        }
        let t2 = Instant::now();

        let viewport_us = (t1 - t0).as_micros();
        let lines_us = (t2 - t1).as_micros();
        let total_us = (t2 - t0).as_micros();

        eprintln!(
            "  pos {:>5}: viewport={:>6}µs  clones={:>4}µs  total={:>6}µs  ({:.1}% of 8ms)",
            start_line,
            viewport_us,
            lines_us,
            total_us,
            total_us as f64 / 80.0,
        );
    }

    eprintln!();

    // =========================================================================
    // Proposed fix: precomputed line offset index cost
    // =========================================================================
    eprintln!("--- Proposed fix: precomputed line offset index ---\n");
    {
        let start = Instant::now();
        let mut offsets: Vec<usize> = vec![0];
        for (i, b) in source.as_bytes().iter().enumerate() {
            if *b == b'\n' {
                offsets.push(i + 1);
            }
        }
        let build_time = start.elapsed();
        eprintln!(
            "  Build index ({} lines): {:>6}µs (one-time cost per parse)",
            offsets.len(),
            build_time.as_micros(),
        );

        // O(1) lookups
        let iterations = 10000;
        let start = Instant::now();
        for _ in 0..iterations {
            let base = 4000.min(offsets.len().saturating_sub(80));
            for line in base..base + 62 {
                if line < offsets.len() {
                    let line_start = offsets[line];
                    let line_end = if line + 1 < offsets.len() {
                        offsets[line + 1] - 1
                    } else {
                        source.len()
                    };
                    std::hint::black_box((line_start, line_end));
                }
            }
        }
        let elapsed = start.elapsed();
        eprintln!(
            "  62 × O(1) lookup at line 4000 x{}: {:>6}µs total, {:>6.2}µs/batch",
            iterations,
            elapsed.as_micros(),
            elapsed.as_micros() as f64 / iterations as f64,
        );

        // Current O(n) approach
        let iterations_slow = 10;
        let start = Instant::now();
        for _ in 0..iterations_slow {
            let base = 4000.min(line_count.saturating_sub(80));
            for line_idx in base..base + 62 {
                let mut current_line = 0;
                let mut found_start = 0;
                for (idx, ch) in source.char_indices() {
                    if current_line == line_idx {
                        found_start = idx;
                        break;
                    }
                    if ch == '\n' {
                        current_line += 1;
                    }
                }
                std::hint::black_box(found_start);
            }
        }
        let elapsed = start.elapsed();
        eprintln!(
            "  62 × O(n) lookup at line 4000 x{}: {:>6}µs total, {:>6.1}µs/batch",
            iterations_slow,
            elapsed.as_micros(),
            elapsed.as_micros() as f64 / iterations_slow as f64,
        );
    }

    eprintln!("\n======================================================================");
    eprintln!("PROFILING COMPLETE");
    eprintln!("======================================================================\n");
}
