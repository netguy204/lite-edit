/// Prototype: Test `notify` crate behavior for content modifications on macOS.
///
/// This script watches a temporary directory, performs various write patterns,
/// and logs exactly what events notify delivers and how quickly.
///
/// Run with: cargo run --example notify_modify_test
/// (or compile standalone: rustc --edition 2021 -L ... notify_modify_test.rs)
///
/// We're testing:
/// 1. Does notify deliver Modify(Data) events for content writes?
/// 2. What's the latency from write to event delivery?
/// 3. What happens with rapid successive writes?
/// 4. What happens when a file is truncated-then-written (common for editors)?
/// 5. What happens with atomic write (write-to-temp, rename)?

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn main() {
    let tmp_dir = std::env::temp_dir().join("notify_modify_test");
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir).expect("create tmp dir");

    let test_file = tmp_dir.join("test.txt");
    fs::write(&test_file, "initial content\n").expect("write initial");

    let (tx, rx) = mpsc::channel::<Event>();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        Config::default(),
    )
    .expect("create watcher");

    watcher
        .watch(&tmp_dir, RecursiveMode::Recursive)
        .expect("start watching");

    println!("=== notify modify event test (macOS FSEvents) ===\n");
    println!("Watcher backend: {:?}", "RecommendedWatcher (FSEvents on macOS)");
    println!("notify version: 6.x");
    println!("Test file: {}\n", test_file.display());

    // Give FSEvents time to register
    std::thread::sleep(Duration::from_millis(500));
    drain_events(&rx, "drain initial");

    // Test 1: Simple content write via fs::write
    println!("--- Test 1: fs::write (overwrite) ---");
    let start = Instant::now();
    fs::write(&test_file, "modified content\n").expect("write");
    collect_events(&rx, start, Duration::from_secs(3));

    // Test 2: Append via File::options().append()
    println!("\n--- Test 2: append write ---");
    let start = Instant::now();
    {
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&test_file)
            .expect("open append");
        writeln!(f, "appended line").expect("append");
    }
    collect_events(&rx, start, Duration::from_secs(3));

    // Test 3: Rapid successive writes (simulating Claude Code writing a file)
    println!("\n--- Test 3: rapid successive writes (5 writes, 10ms apart) ---");
    let start = Instant::now();
    for i in 0..5 {
        fs::write(&test_file, format!("rapid write {}\n", i)).expect("rapid write");
        std::thread::sleep(Duration::from_millis(10));
    }
    collect_events(&rx, start, Duration::from_secs(3));

    // Test 4: Truncate-then-write (what many editors do)
    println!("\n--- Test 4: truncate + write (editor-style save) ---");
    let start = Instant::now();
    {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&test_file)
            .expect("open truncate");
        write!(f, "truncated and rewritten\n").expect("write after truncate");
    }
    collect_events(&rx, start, Duration::from_secs(3));

    // Test 5: Atomic write (write to temp file, rename)
    println!("\n--- Test 5: atomic write (temp file + rename) ---");
    let temp_file = tmp_dir.join("test.txt.tmp");
    let start = Instant::now();
    fs::write(&temp_file, "atomically written\n").expect("write temp");
    fs::rename(&temp_file, &test_file).expect("rename");
    collect_events(&rx, start, Duration::from_secs(3));

    // Test 6: Write to a DIFFERENT file in the same directory
    println!("\n--- Test 6: write to different file (should also be detected) ---");
    let other_file = tmp_dir.join("other.txt");
    let start = Instant::now();
    fs::write(&other_file, "other file content\n").expect("write other");
    collect_events(&rx, start, Duration::from_secs(3));

    // Cleanup
    let _ = fs::remove_dir_all(&tmp_dir);

    println!("\n=== done ===");
}

fn drain_events(rx: &mpsc::Receiver<Event>, label: &str) {
    let mut count = 0;
    while rx.try_recv().is_ok() {
        count += 1;
    }
    if count > 0 {
        println!("  ({}: drained {} stale events)", label, count);
    }
}

fn collect_events(rx: &mpsc::Receiver<Event>, write_time: Instant, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    let mut events = Vec::new();

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }

        match rx.recv_timeout(remaining.min(Duration::from_millis(500))) {
            Ok(event) => {
                let latency = write_time.elapsed();
                events.push((latency, event));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // If we already have some events and haven't received more
                // in 500ms, that's likely all of them
                if !events.is_empty() {
                    break;
                }
                // Otherwise keep waiting up to the deadline
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    if events.is_empty() {
        println!("  NO EVENTS received within {:?}", timeout);
    } else {
        println!("  {} event(s) received:", events.len());
        for (latency, event) in &events {
            let paths: Vec<_> = event
                .paths
                .iter()
                .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
                .collect();
            println!(
                "    {:>8.1}ms  {:?}  paths={:?}",
                latency.as_secs_f64() * 1000.0,
                event.kind,
                paths
            );
        }
    }
}
