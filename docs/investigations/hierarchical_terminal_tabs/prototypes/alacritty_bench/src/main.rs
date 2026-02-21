//! Benchmark: alacritty_terminal performance as an embedded terminal emulator.
//!
//! Tests three workloads:
//! 1. Bulk throughput - large plain text output (simulates `cat huge_file.txt`)
//! 2. Interactive latency - small writes with grid reads between each
//! 3. Escape-heavy output - colored/styled text (simulates compiler output)
//!
//! Also measures the cost of reading styled lines from the grid (the "conversion hop"
//! from alacritty's grid to our hypothetical StyledLine representation).

use std::time::{Duration, Instant};

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::{Config, Term, TermDamage};
use alacritty_terminal::vte::ansi;

// =============================================================================
// Setup helpers
// =============================================================================

struct TermSize {
    cols: usize,
    lines: usize,
}

impl Dimensions for TermSize {
    fn columns(&self) -> usize {
        self.cols
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn total_lines(&self) -> usize {
        self.lines
    }
}

fn make_term(cols: usize, lines: usize, scrollback: usize) -> (Term<VoidListener>, ansi::Processor) {
    let size = TermSize { cols, lines };
    let config = Config {
        scrolling_history: scrollback,
        ..Default::default()
    };
    let term = Term::new(config, &size, VoidListener);
    let processor = ansi::Processor::new();
    (term, processor)
}

// =============================================================================
// Data generators
// =============================================================================

/// Generate plain text lines (simulates `cat` output).
fn generate_plain_text(num_lines: usize, cols: usize) -> Vec<u8> {
    let mut buf = Vec::with_capacity(num_lines * (cols + 1));
    for i in 0..num_lines {
        // Fill line with printable ASCII, varying content
        for j in 0..cols {
            buf.push(b'A' + ((i + j) % 26) as u8);
        }
        buf.push(b'\n');
    }
    buf
}

/// Generate escape-heavy output (simulates colored compiler output).
fn generate_colored_output(num_lines: usize, cols: usize) -> Vec<u8> {
    let mut buf = Vec::with_capacity(num_lines * (cols + 50));
    let colors = [
        "\x1b[31m",  // red
        "\x1b[32m",  // green
        "\x1b[33m",  // yellow
        "\x1b[34m",  // blue
        "\x1b[1;31m", // bold red
        "\x1b[0m",   // reset
    ];

    for i in 0..num_lines {
        // Simulate: "error[E0308]: mismatched types" style output
        let color = colors[i % colors.len()];
        buf.extend_from_slice(color.as_bytes());

        // Content portion
        let content_len = cols.saturating_sub(10); // leave room for escapes
        for j in 0..content_len {
            buf.push(b'a' + ((i + j) % 26) as u8);
        }

        buf.extend_from_slice(b"\x1b[0m"); // reset
        buf.push(b'\n');
    }
    buf
}

/// Generate small interactive writes (shell prompt + short commands).
fn generate_interactive_writes() -> Vec<Vec<u8>> {
    let mut writes = Vec::new();

    // Simulate: prompt appears, user types, output comes back
    for i in 0..1000 {
        // Shell prompt with color
        writes.push(format!("\x1b[32muser@host\x1b[0m:\x1b[34m~/project\x1b[0m$ ").into_bytes());
        // User command echoed
        writes.push(format!("echo hello {i}\r\n").into_bytes());
        // Command output
        writes.push(format!("hello {i}\r\n").into_bytes());
    }

    writes
}

// =============================================================================
// Grid reading (simulates BufferView::styled_line conversion)
// =============================================================================

/// Simulated StyledLine - what our BufferView trait would produce.
struct StyledSpan {
    text: String,
    fg: alacritty_terminal::vte::ansi::Color,
    bg: alacritty_terminal::vte::ansi::Color,
    bold: bool,
    italic: bool,
    underline: bool,
}

struct StyledLine {
    spans: Vec<StyledSpan>,
}

/// Convert a terminal grid row to a StyledLine.
/// This is the "conversion hop" we need to measure.
fn row_to_styled_line(row: &[Cell], num_cols: usize) -> StyledLine {
    let mut spans: Vec<StyledSpan> = Vec::new();

    for col_idx in 0..num_cols {
        let cell = &row[col_idx];
        let bold = cell.flags.contains(alacritty_terminal::term::cell::Flags::BOLD);
        let italic = cell.flags.contains(alacritty_terminal::term::cell::Flags::ITALIC);
        let underline = cell.flags.contains(alacritty_terminal::term::cell::Flags::UNDERLINE);

        // Try to coalesce with previous span if same style
        if let Some(last) = spans.last_mut() {
            if last.fg == cell.fg
                && last.bg == cell.bg
                && last.bold == bold
                && last.italic == italic
                && last.underline == underline
            {
                last.text.push(cell.c);
                continue;
            }
        }

        spans.push(StyledSpan {
            text: cell.c.to_string(),
            fg: cell.fg,
            bg: cell.bg,
            bold,
            italic,
            underline,
        });
    }

    StyledLine { spans }
}

/// Read all visible lines from the terminal grid as StyledLines.
fn read_all_visible_lines(term: &Term<VoidListener>, cols: usize, lines: usize) -> Vec<StyledLine> {
    let grid = term.grid();
    let mut result = Vec::with_capacity(lines);
    for line_idx in 0..lines {
        let row = &grid[alacritty_terminal::index::Line(line_idx as i32)];
        row_to_styled_line(&row[..], cols)  ;
        result.push(row_to_styled_line(&row[..], cols));
    }
    result
}

// =============================================================================
// Benchmark runners
// =============================================================================

struct BenchResult {
    name: String,
    duration: Duration,
    bytes_processed: usize,
    lines_processed: usize,
}

impl BenchResult {
    fn print(&self) {
        let mb = self.bytes_processed as f64 / (1024.0 * 1024.0);
        let secs = self.duration.as_secs_f64();
        let throughput_mb = if secs > 0.0 { mb / secs } else { 0.0 };
        let throughput_lines = if secs > 0.0 {
            self.lines_processed as f64 / secs
        } else {
            0.0
        };
        let per_line_ns = if self.lines_processed > 0 {
            self.duration.as_nanos() as f64 / self.lines_processed as f64
        } else {
            0.0
        };

        println!("  {}", self.name);
        println!("    Time:       {:.3}ms", secs * 1000.0);
        println!("    Data:       {:.2} MB ({} lines)", mb, self.lines_processed);
        println!(
            "    Throughput: {:.1} MB/s | {:.0} lines/s",
            throughput_mb, throughput_lines
        );
        println!("    Per line:   {:.0} ns", per_line_ns);
        println!();
    }
}

fn bench_bulk_throughput() -> BenchResult {
    let cols = 120;
    let lines = 40;
    let scrollback = 10_000;
    let num_lines = 100_000;

    let data = generate_plain_text(num_lines, cols);
    let (mut term, mut proc) = make_term(cols, lines, scrollback);

    let start = Instant::now();
    // Feed in chunks (simulates read() from PTY fd)
    let chunk_size = 4096;
    for chunk in data.chunks(chunk_size) {
        proc.advance(&mut term, chunk);
    }
    let duration = start.elapsed();

    BenchResult {
        name: "Bulk throughput (100K lines plain text)".into(),
        duration,
        bytes_processed: data.len(),
        lines_processed: num_lines,
    }
}

fn bench_bulk_with_grid_read() -> BenchResult {
    let cols = 120;
    let lines = 40;
    let scrollback = 10_000;
    let num_lines = 100_000;

    let data = generate_plain_text(num_lines, cols);
    let (mut term, mut proc) = make_term(cols, lines, scrollback);

    let start = Instant::now();
    let chunk_size = 4096;
    for chunk in data.chunks(chunk_size) {
        proc.advance(&mut term, chunk);
    }
    // Now read all visible lines (the conversion hop)
    let _styled = read_all_visible_lines(&term, cols, lines);
    let duration = start.elapsed();

    BenchResult {
        name: "Bulk throughput + grid read (100K lines + read visible)".into(),
        duration,
        bytes_processed: data.len(),
        lines_processed: num_lines,
    }
}

fn bench_colored_output() -> BenchResult {
    let cols = 120;
    let lines = 40;
    let scrollback = 10_000;
    let num_lines = 100_000;

    let data = generate_colored_output(num_lines, cols);
    let (mut term, mut proc) = make_term(cols, lines, scrollback);

    let start = Instant::now();
    let chunk_size = 4096;
    for chunk in data.chunks(chunk_size) {
        proc.advance(&mut term, chunk);
    }
    let duration = start.elapsed();

    BenchResult {
        name: "Escape-heavy output (100K colored lines)".into(),
        duration,
        bytes_processed: data.len(),
        lines_processed: num_lines,
    }
}

fn bench_interactive() -> BenchResult {
    let cols = 120;
    let lines = 40;
    let scrollback = 10_000;

    let writes = generate_interactive_writes();
    let total_bytes: usize = writes.iter().map(|w| w.len()).sum();
    let num_writes = writes.len();

    let (mut term, mut proc) = make_term(cols, lines, scrollback);

    let start = Instant::now();
    for write in &writes {
        proc.advance(&mut term, write);
        // After each small write, read damage (simulates render check)
        let _damage = term.damage();
        term.reset_damage();
    }
    let duration = start.elapsed();

    BenchResult {
        name: "Interactive (3K small writes with damage checks)".into(),
        duration,
        bytes_processed: total_bytes,
        lines_processed: num_writes,
    }
}

fn bench_interactive_with_grid_read() -> BenchResult {
    let cols = 120;
    let lines = 40;
    let scrollback = 10_000;

    let writes = generate_interactive_writes();
    let total_bytes: usize = writes.iter().map(|w| w.len()).sum();
    let num_writes = writes.len();

    let (mut term, mut proc) = make_term(cols, lines, scrollback);

    let start = Instant::now();
    for write in &writes {
        proc.advance(&mut term, write);
        // Collect damaged line indices first to avoid borrow conflict
        let damaged_lines: Vec<usize> = {
            let damage = term.damage();
            match damage {
                TermDamage::Full => (0..lines).collect(),
                TermDamage::Partial(iter) => iter.map(|d| d.line).collect(),
            }
        };
        // Now read the damaged lines
        let grid = term.grid();
        for line_idx in damaged_lines {
            let row = &grid[alacritty_terminal::index::Line(line_idx as i32)];
            let _styled = row_to_styled_line(&row[..], cols);
        }
        term.reset_damage();
    }
    let duration = start.elapsed();

    BenchResult {
        name: "Interactive + selective grid read (3K writes)".into(),
        duration,
        bytes_processed: total_bytes,
        lines_processed: num_writes,
    }
}

fn bench_grid_read_only() -> BenchResult {
    let cols = 120;
    let lines = 40;
    let scrollback = 10_000;

    // Fill the terminal first
    let data = generate_colored_output(1000, cols);
    let (mut term, mut proc) = make_term(cols, lines, scrollback);
    proc.advance(&mut term, &data);

    // Now benchmark just the grid read, repeated many times
    let iterations = 10_000;
    let start = Instant::now();
    for _ in 0..iterations {
        let _styled = read_all_visible_lines(&term, cols, lines);
    }
    let duration = start.elapsed();

    let per_read_ns = duration.as_nanos() as f64 / iterations as f64;
    println!("  Grid read overhead (40 lines × 120 cols)");
    println!("    {} iterations in {:.3}ms", iterations, duration.as_secs_f64() * 1000.0);
    println!("    Per read: {:.0} ns ({:.3} µs)", per_read_ns, per_read_ns / 1000.0);
    println!("    At 60fps budget (16.6ms): {:.2}% of frame", (per_read_ns / 16_600_000.0) * 100.0);
    println!();

    BenchResult {
        name: "Grid read only (isolated)".into(),
        duration,
        bytes_processed: 0,
        lines_processed: iterations * lines,
    }
}

fn bench_damage_tracking() -> BenchResult {
    let cols = 120;
    let lines = 40;
    let scrollback = 10_000;

    let (mut term, mut proc) = make_term(cols, lines, scrollback);

    // Feed a bunch of content
    let data = generate_colored_output(10_000, cols);
    proc.advance(&mut term, &data);
    term.reset_damage();

    // Now do small writes and track damage
    let iterations = 10_000;
    let mut total_damaged_lines = 0usize;

    let start = Instant::now();
    for i in 0..iterations {
        // Small write: one line of output
        let line = format!("output line {i}\r\n");
        proc.advance(&mut term, line.as_bytes());

        let damage = term.damage();
        match damage {
            TermDamage::Full => total_damaged_lines += lines,
            TermDamage::Partial(iter) => total_damaged_lines += iter.count(),
        }
        term.reset_damage();
    }
    let duration = start.elapsed();

    println!("  Damage tracking (10K small writes)");
    println!("    Time: {:.3}ms", duration.as_secs_f64() * 1000.0);
    println!("    Total damaged lines reported: {}", total_damaged_lines);
    println!(
        "    Avg damaged lines per write: {:.1}",
        total_damaged_lines as f64 / iterations as f64
    );
    println!();

    BenchResult {
        name: "Damage tracking".into(),
        duration,
        bytes_processed: 0,
        lines_processed: iterations,
    }
}

// =============================================================================
// Memory measurement
// =============================================================================

fn measure_memory() {
    println!("  Memory footprint estimates:");
    println!("    Cell size: {} bytes", std::mem::size_of::<Cell>());

    let cols = 120;
    let lines = 40;
    for scrollback in [1_000, 10_000, 100_000] {
        let total_cells = cols * (lines + scrollback);
        let cell_mem = total_cells * std::mem::size_of::<Cell>();
        println!(
            "    {}x{} + {} scrollback: {:.1} MB ({} cells)",
            cols,
            lines,
            scrollback,
            cell_mem as f64 / (1024.0 * 1024.0),
            total_cells
        );
    }
    println!();
}

// =============================================================================
// Main
// =============================================================================

fn main() {
    println!("=== alacritty_terminal benchmark ===\n");

    // Memory
    println!("--- Memory ---");
    measure_memory();

    // Run each benchmark 3 times, report best
    println!("--- Processing benchmarks (best of 3) ---\n");

    let benchmarks: Vec<fn() -> BenchResult> = vec![
        bench_bulk_throughput,
        bench_bulk_with_grid_read,
        bench_colored_output,
        bench_interactive,
        bench_interactive_with_grid_read,
    ];

    for bench_fn in benchmarks {
        let mut best: Option<BenchResult> = None;
        for _ in 0..3 {
            let result = bench_fn();
            if best.as_ref().is_none_or(|b| result.duration < b.duration) {
                best = Some(result);
            }
        }
        best.unwrap().print();
    }

    println!("--- Overhead benchmarks ---\n");
    // These print their own output
    let _ = bench_grid_read_only();
    let _ = bench_damage_tracking();
}
