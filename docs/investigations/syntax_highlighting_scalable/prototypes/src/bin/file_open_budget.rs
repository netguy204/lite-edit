///! Benchmark: What does file-open already cost today (without highlighting)?
///! This puts the 8.5ms initial parse in context.

use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("Usage: file_open_budget <path>");
    let iterations = 100;

    // 1. File I/O: fs::read
    let mut total_ns = 0u128;
    let mut bytes = Vec::new();
    for _ in 0..iterations {
        // Drop page cache effect after first iter — but this is a microbenchmark
        let start = Instant::now();
        bytes = std::fs::read(&path).expect("read failed");
        total_ns += start.elapsed().as_nanos();
    }
    let avg_read_us = total_ns / iterations as u128 / 1000;
    println!("fs::read ({} bytes):       {:>6} µs avg", bytes.len(), avg_read_us);

    // 2. UTF-8 lossy conversion
    let mut total_ns = 0u128;
    let mut contents = String::new();
    for _ in 0..iterations {
        let start = Instant::now();
        contents = String::from_utf8_lossy(&bytes).into_owned();
        total_ns += start.elapsed().as_nanos();
    }
    let avg_utf8_us = total_ns / iterations as u128 / 1000;
    println!("UTF-8 lossy conversion:    {:>6} µs avg", avg_utf8_us);

    // 3. Gap buffer construction (simulate TextBuffer::from_str)
    // We can't import lite_edit_buffer here, so simulate the cost:
    // from_str does gap_buffer init + line_index build (scan for newlines)
    let mut total_ns = 0u128;
    for _ in 0..iterations {
        let start = Instant::now();
        // Simulate: copy into Vec (gap buffer) + scan for newlines (line index)
        let mut buf = Vec::with_capacity(contents.len() + 1024);
        buf.extend_from_slice(contents.as_bytes());
        let mut line_starts: Vec<usize> = vec![0];
        for (i, b) in buf.iter().enumerate() {
            if *b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        std::hint::black_box(&buf);
        std::hint::black_box(&line_starts);
        total_ns += start.elapsed().as_nanos();
    }
    let avg_buffer_us = total_ns / iterations as u128 / 1000;
    println!("Buffer construction:       {:>6} µs avg ({} lines)", avg_buffer_us, contents.lines().count());

    // 4. Tree-sitter initial parse (from main benchmark: ~8549 µs)
    let ts_parse_us: u128 = 8549; // from H2 benchmark

    let total_without_ts = avg_read_us + avg_utf8_us + avg_buffer_us;
    let total_with_ts = total_without_ts + ts_parse_us;

    println!();
    println!("=== File Open Budget ===");
    println!("Without highlighting:  {:>6} µs", total_without_ts);
    println!("Tree-sitter parse:     {:>6} µs", ts_parse_us);
    println!("With highlighting:     {:>6} µs", total_with_ts);
    println!("Overhead:              {:>5.1}x", total_with_ts as f64 / total_without_ts as f64);
    println!();
    
    if total_without_ts < 1000 {
        println!("⚠️  File open is currently very fast ({} µs).", total_without_ts);
        println!("   Adding 8.5ms tree-sitter parse is a {:.0}x slowdown.", 
            total_with_ts as f64 / total_without_ts as f64);
        println!("   User may perceive delay on file open for large files.");
        println!();
        println!("   Options:");
        println!("   A) Parse synchronously — 8.5ms is still one frame at 120Hz");
        println!("   B) Render first frame unhighlighted, parse async, redraw");
        println!("   C) Parse synchronously but show file immediately (parse before first render)");
    }
}
