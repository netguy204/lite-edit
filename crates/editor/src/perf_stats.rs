//! Performance measurement instrumentation for lite-edit.
//!
//! This module provides `PerfStats`, a lightweight stats collector gated behind
//! the `perf-instrumentation` Cargo feature. When enabled it tracks:
//!
//! 1. **Keystroke-to-present latency** — P50/P95/P99 over a rolling 1000-sample window
//! 2. **Dirty region hit rate** — partial vs full viewport vs skipped frame counts
//! 3. **styled_line() cost** — per-frame aggregate timing of the styled_line collection
//!
//! Stats are auto-printed to stderr every 1000 frames (~17s at 60 fps) and can be
//! dumped on-demand via Ctrl+Shift+P (sets `EditorState::dump_perf_stats`).

use std::time::{Duration, Instant};

use crate::dirty_region::DirtyRegion;

/// Maximum number of samples retained in each ring buffer.
const RING_CAP: usize = 1000;

/// How often (in frames) to auto-report to stderr.
const AUTO_REPORT_INTERVAL: u64 = 1000;

/// Performance statistics collector.
///
/// All fields are zero-cost when the `perf-instrumentation` feature is disabled
/// because the entire module (and all call-sites) are behind `#[cfg]` gates.
pub struct PerfStats {
    /// Timestamp set at the start of `process_pending_events`.
    frame_start: Option<Instant>,

    /// Rolling ring buffer of frame latencies (event-start → render-end).
    frame_latencies: Vec<Duration>,
    /// Write cursor into `frame_latencies`.
    frame_lat_cursor: usize,
    /// Whether the ring buffer has wrapped (i.e. we have ≥ RING_CAP samples).
    frame_lat_full: bool,

    /// Total number of rendered frames.
    frame_count: u64,
    /// Number of frames where `DirtyRegion::Lines` was taken (partial repaint).
    partial_frames: u64,
    /// Number of frames where `DirtyRegion::FullViewport` was taken.
    full_frames: u64,
    /// Number of frames where `DirtyRegion::None` (skipped render).
    skipped_frames: u64,

    /// Rolling ring buffer of `(total_time, line_count)` per frame.
    styled_line_costs: Vec<(Duration, usize)>,
    /// Write cursor into `styled_line_costs`.
    styled_cursor: usize,
    /// Whether the styled_line ring buffer has wrapped.
    styled_full: bool,
    // Chunk: docs/chunks/invalidation_separation - Layout recalc counters
    /// Number of frames where layout recalculation was skipped.
    layout_skipped: u64,
    /// Number of frames where layout recalculation was performed.
    layout_performed: u64,
}

impl PerfStats {
    /// Creates a new, empty stats collector.
    pub fn new() -> Self {
        Self {
            frame_start: None,
            frame_latencies: Vec::with_capacity(RING_CAP),
            frame_lat_cursor: 0,
            frame_lat_full: false,
            frame_count: 0,
            partial_frames: 0,
            full_frames: 0,
            skipped_frames: 0,
            styled_line_costs: Vec::with_capacity(RING_CAP),
            styled_cursor: 0,
            styled_full: false,
            // Chunk: docs/chunks/invalidation_separation - Initialize layout counters
            layout_skipped: 0,
            layout_performed: 0,
        }
    }

    /// Called at the top of `process_pending_events` to start the frame timer.
    pub fn mark_frame_start(&mut self) {
        self.frame_start = Some(Instant::now());
    }

    /// Called after the render pass completes. Records the frame latency.
    pub fn mark_frame_end(&mut self) {
        if let Some(start) = self.frame_start.take() {
            let elapsed = start.elapsed();
            ring_push(
                &mut self.frame_latencies,
                &mut self.frame_lat_cursor,
                &mut self.frame_lat_full,
                elapsed,
            );
        }
        self.frame_count += 1;
    }

    /// Records the dirty region type for the current frame.
    pub fn record_dirty_region(&mut self, dirty: &DirtyRegion) {
        match dirty {
            DirtyRegion::None => self.skipped_frames += 1,
            DirtyRegion::Lines { .. } => self.partial_frames += 1,
            DirtyRegion::FullViewport => self.full_frames += 1,
        }
    }

    /// Records the cost of collecting styled lines for a single frame.
    pub fn record_styled_line_batch(&mut self, duration: Duration, line_count: usize) {
        ring_push(
            &mut self.styled_line_costs,
            &mut self.styled_cursor,
            &mut self.styled_full,
            (duration, line_count),
        );
    }

    // Chunk: docs/chunks/invalidation_separation - Record layout recalc stats from renderer
    /// Updates layout recalc stats from the renderer's counters.
    ///
    /// This should be called after each render with the renderer's
    /// `layout_recalc_counters()` values.
    pub fn update_layout_counters(&mut self, skipped: usize, performed: usize) {
        self.layout_skipped = skipped as u64;
        self.layout_performed = performed as u64;
    }

    /// Returns `true` every `AUTO_REPORT_INTERVAL` frames.
    pub fn should_auto_report(&self) -> bool {
        self.frame_count > 0 && self.frame_count % AUTO_REPORT_INTERVAL == 0
    }

    /// Formats the current stats into a human-readable report string.
    pub fn report(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("[lite-edit perf] Frame #{}\n", self.frame_count));

        // --- Keystroke-to-present latency ---
        let latencies = ring_snapshot(&self.frame_latencies, self.frame_lat_cursor, self.frame_lat_full);
        if latencies.is_empty() {
            out.push_str("  Keystroke-to-present:  (no data)\n");
        } else {
            let mut sorted: Vec<Duration> = latencies;
            sorted.sort();
            let p50 = percentile(&sorted, 50);
            let p95 = percentile(&sorted, 95);
            let p99 = percentile(&sorted, 99);
            out.push_str(&format!(
                "  Keystroke-to-present:  P50={}  P95={}  P99={}\n",
                fmt_duration(p50),
                fmt_duration(p95),
                fmt_duration(p99),
            ));
        }

        // --- Dirty region hit rate ---
        let total_dirty = self.partial_frames + self.full_frames;
        if total_dirty == 0 {
            out.push_str("  Dirty region:          (no rendered frames)\n");
        } else {
            let partial_pct = (self.partial_frames as f64 / total_dirty as f64) * 100.0;
            let full_pct = (self.full_frames as f64 / total_dirty as f64) * 100.0;
            out.push_str(&format!(
                "  Dirty region:          partial={} ({:.1}%)  full={} ({:.1}%)  skipped={}\n",
                self.partial_frames, partial_pct,
                self.full_frames, full_pct,
                self.skipped_frames,
            ));
        }

        // --- styled_line() cost ---
        let costs = ring_snapshot(&self.styled_line_costs, self.styled_cursor, self.styled_full);
        if costs.is_empty() {
            out.push_str("  styled_line:           (no data)\n");
        } else {
            let mut durations: Vec<Duration> = costs.iter().map(|(d, _)| *d).collect();
            durations.sort();
            let p50 = percentile(&durations, 50);
            let p95 = percentile(&durations, 95);
            let p99 = percentile(&durations, 99);
            let total_lines: usize = costs.iter().map(|(_, n)| *n).sum();
            let avg_lines = total_lines as f64 / costs.len() as f64;
            out.push_str(&format!(
                "  styled_line:           P50={}  P95={}  P99={}  (avg {:.0} lines/frame)\n",
                fmt_duration(p50),
                fmt_duration(p95),
                fmt_duration(p99),
                avg_lines,
            ));
        }

        // Chunk: docs/chunks/invalidation_separation - Layout skip rate
        // --- Layout recalculation ---
        let total_layout = self.layout_skipped + self.layout_performed;
        if total_layout == 0 {
            out.push_str("  Layout recalc:         (no data)\n");
        } else {
            let skip_rate = (self.layout_skipped as f64 / total_layout as f64) * 100.0;
            out.push_str(&format!(
                "  Layout recalc:         skipped={} performed={} skip_rate={:.1}%\n",
                self.layout_skipped,
                self.layout_performed,
                skip_rate,
            ));
        }

        out
    }
}

// =============================================================================
// Ring buffer helpers
// =============================================================================

/// Pushes a value into a fixed-capacity ring buffer.
fn ring_push<T>(buf: &mut Vec<T>, cursor: &mut usize, full: &mut bool, value: T) {
    if buf.len() < RING_CAP {
        buf.push(value);
    } else {
        buf[*cursor] = value;
        *full = true;
    }
    *cursor = (*cursor + 1) % RING_CAP;
}

/// Returns a snapshot (clone) of the ring buffer contents in insertion order.
fn ring_snapshot<T: Clone>(buf: &[T], cursor: usize, full: bool) -> Vec<T> {
    if !full {
        // Buffer hasn't wrapped yet — just clone the filled portion.
        buf.to_vec()
    } else {
        // Wrapped: [cursor..] is oldest, [..cursor] is newest.
        let mut out = Vec::with_capacity(RING_CAP);
        out.extend_from_slice(&buf[cursor..]);
        out.extend_from_slice(&buf[..cursor]);
        out
    }
}

// =============================================================================
// Percentile & formatting helpers
// =============================================================================

/// Returns the value at the given percentile from a sorted slice.
///
/// Uses nearest-rank method: index = ceil(percentile/100 * N) - 1.
fn percentile(sorted: &[Duration], pct: usize) -> Duration {
    assert!(!sorted.is_empty());
    let idx = ((pct as f64 / 100.0) * sorted.len() as f64).ceil() as usize;
    let idx = idx.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

/// Formats a Duration as a human-friendly string (µs or ms).
fn fmt_duration(d: Duration) -> String {
    let micros = d.as_micros();
    if micros < 1000 {
        format!("{}µs", micros)
    } else {
        let ms = d.as_secs_f64() * 1000.0;
        if ms < 10.0 {
            format!("{:.1}ms", ms)
        } else {
            format!("{:.0}ms", ms)
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_single_element() {
        let data = vec![Duration::from_micros(100)];
        assert_eq!(percentile(&data, 50), Duration::from_micros(100));
        assert_eq!(percentile(&data, 99), Duration::from_micros(100));
    }

    #[test]
    fn percentile_multiple_elements() {
        // 10 elements: [100, 200, 300, ..., 1000]
        let data: Vec<Duration> = (1..=10).map(|i| Duration::from_micros(i * 100)).collect();
        // P50: ceil(0.5 * 10) - 1 = 4 → 500µs
        assert_eq!(percentile(&data, 50), Duration::from_micros(500));
        // P95: ceil(0.95 * 10) - 1 = 9 → 1000µs
        assert_eq!(percentile(&data, 95), Duration::from_micros(1000));
        // P99: ceil(0.99 * 10) - 1 = 9 → 1000µs
        assert_eq!(percentile(&data, 99), Duration::from_micros(1000));
    }

    #[test]
    fn percentile_100_elements() {
        let data: Vec<Duration> = (1..=100).map(|i| Duration::from_micros(i * 10)).collect();
        // P50: ceil(0.5 * 100) - 1 = 49 → 500µs
        assert_eq!(percentile(&data, 50), Duration::from_micros(500));
        // P95: ceil(0.95 * 100) - 1 = 94 → 950µs
        assert_eq!(percentile(&data, 95), Duration::from_micros(950));
        // P99: ceil(0.99 * 100) - 1 = 98 → 990µs
        assert_eq!(percentile(&data, 99), Duration::from_micros(990));
    }

    #[test]
    fn ring_buffer_push_and_snapshot() {
        let mut buf = Vec::new();
        let mut cursor = 0;
        let mut full = false;

        // Push 3 items into a RING_CAP-sized buffer (won't wrap)
        ring_push(&mut buf, &mut cursor, &mut full, 10u32);
        ring_push(&mut buf, &mut cursor, &mut full, 20u32);
        ring_push(&mut buf, &mut cursor, &mut full, 30u32);

        let snap = ring_snapshot(&buf, cursor, full);
        assert_eq!(snap, vec![10, 20, 30]);
        assert!(!full);
    }

    #[test]
    fn auto_report_interval() {
        let mut stats = PerfStats::new();
        assert!(!stats.should_auto_report()); // frame_count == 0

        // Simulate 999 frames
        for _ in 0..999 {
            stats.mark_frame_start();
            stats.mark_frame_end();
        }
        assert!(!stats.should_auto_report()); // frame_count == 999

        // 1000th frame
        stats.mark_frame_start();
        stats.mark_frame_end();
        assert!(stats.should_auto_report()); // frame_count == 1000
    }

    #[test]
    fn dirty_region_tracking() {
        let mut stats = PerfStats::new();
        stats.record_dirty_region(&DirtyRegion::Lines { from: 0, to: 5 });
        stats.record_dirty_region(&DirtyRegion::FullViewport);
        stats.record_dirty_region(&DirtyRegion::None);
        stats.record_dirty_region(&DirtyRegion::Lines { from: 2, to: 8 });

        assert_eq!(stats.partial_frames, 2);
        assert_eq!(stats.full_frames, 1);
        assert_eq!(stats.skipped_frames, 1);
    }

    #[test]
    fn report_with_data() {
        let mut stats = PerfStats::new();
        for i in 0..10 {
            stats.mark_frame_start();
            // Simulate some work
            stats.record_dirty_region(&DirtyRegion::Lines { from: 0, to: 5 });
            stats.record_styled_line_batch(Duration::from_micros(100 + i * 10), 40);
            stats.mark_frame_end();
        }

        let report = stats.report();
        assert!(report.contains("[lite-edit perf] Frame #10"));
        assert!(report.contains("Keystroke-to-present:"));
        assert!(report.contains("Dirty region:"));
        assert!(report.contains("styled_line:"));
    }

    #[test]
    fn fmt_duration_micros() {
        assert_eq!(fmt_duration(Duration::from_micros(320)), "320µs");
    }

    #[test]
    fn fmt_duration_millis() {
        assert_eq!(fmt_duration(Duration::from_micros(1200)), "1.2ms");
    }

    #[test]
    fn fmt_duration_large_millis() {
        assert_eq!(fmt_duration(Duration::from_millis(25)), "25ms");
    }
}
