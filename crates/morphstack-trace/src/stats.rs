//! Single-pass, constant-memory-per-live-object trace statistics.
//!
//! Statistics tolerate invalid traces: inconsistent events are skipped rather
//! than causing a panic. Run the validator first for trustworthy numbers.

use std::collections::HashMap;

use serde::Serialize;

use crate::event::{ActId, CtxId, Event, EventKind};

/// Log-linear histogram: 8 linear sub-buckets per power of two, so quantiles
/// are exact below 16 and within 12.5% above. Mean and max are exact.
#[derive(Debug, Clone, Default)]
pub struct Histogram {
    buckets: Vec<u64>,
    count: u64,
    sum: u128,
    max: u64,
}

const SUB_BITS: u32 = 3;
const SUB: u64 = 1 << SUB_BITS;

impl Histogram {
    /// Record one value.
    pub fn record(&mut self, v: u64) {
        let i = Self::bucket(v);
        if i >= self.buckets.len() {
            self.buckets.resize(i + 1, 0);
        }
        self.buckets[i] += 1;
        self.count += 1;
        self.sum += u128::from(v);
        self.max = self.max.max(v);
    }

    fn bucket(v: u64) -> usize {
        if v < 2 * SUB {
            return usize::try_from(v).unwrap_or(usize::MAX);
        }
        let msb = v.ilog2();
        let shift = msb - SUB_BITS;
        let sub = (v >> shift) - SUB; // 0..SUB
        usize::try_from(u64::from(shift + 1) * SUB + sub).unwrap_or(usize::MAX)
    }

    /// Upper bound of bucket `i`, clamped to the observed max.
    fn upper(&self, i: usize) -> u64 {
        let i = i as u64;
        let ub = if i < 2 * SUB {
            i
        } else {
            let shift = i / SUB - 1;
            let sub = i % SUB;
            let ub = (u128::from(SUB + sub + 1) << shift) - 1;
            u64::try_from(ub).unwrap_or(u64::MAX)
        };
        ub.min(self.max)
    }

    /// Number of recorded values.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Approximate quantile, `q` in `[0, 1]`.
    #[must_use]
    pub fn quantile(&self, q: f64) -> u64 {
        if self.count == 0 {
            return 0;
        }
        // Rank is at most `count`, so the float round trip is exact in practice.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let rank = ((q.clamp(0.0, 1.0) * self.count as f64).ceil() as u64).max(1);
        let mut seen = 0;
        for (i, n) in self.buckets.iter().enumerate() {
            seen += n;
            if seen >= rank {
                return self.upper(i);
            }
        }
        self.max
    }

    /// Summary for reports.
    #[must_use]
    pub fn summary(&self) -> Summary {
        #[allow(clippy::cast_precision_loss)]
        let mean = if self.count == 0 {
            0.0
        } else {
            self.sum as f64 / self.count as f64
        };
        Summary {
            count: self.count,
            mean,
            p50: self.quantile(0.50),
            p90: self.quantile(0.90),
            p99: self.quantile(0.99),
            max: self.max,
        }
    }
}

/// Distribution summary.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Summary {
    /// Number of samples.
    pub count: u64,
    /// Exact mean.
    pub mean: f64,
    /// Approximate median.
    pub p50: u64,
    /// Approximate 90th percentile.
    pub p90: u64,
    /// Approximate 99th percentile.
    pub p99: u64,
    /// Exact maximum.
    pub max: u64,
}

/// Aggregate statistics for a trace.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TraceStats {
    /// Total events.
    pub events: u64,
    /// Events per kind, in format order.
    pub by_kind: Vec<(&'static str, u64)>,
    /// First to last timestamp, in nanoseconds.
    pub duration_ns: u64,
    /// Contexts spawned.
    pub contexts: u64,
    /// Contexts still live at the end of the trace.
    pub contexts_live_at_end: u64,
    /// Highest number of simultaneously live contexts.
    pub peak_live_contexts: u64,
    /// Highest number of simultaneously suspended contexts.
    pub peak_suspended_contexts: u64,
    /// Activations started.
    pub activations: u64,
    /// Deepest activation stack seen on any context.
    pub max_depth: u64,
    /// Highest total frame bytes live across all contexts.
    pub peak_live_frame_bytes: u64,
    /// Frame sizes, in bytes, over all calls.
    pub frame_bytes: Summary,
    /// Activation lifetimes (call to return), in nanoseconds.
    pub activation_lifetime_ns: Summary,
    /// Suspension durations (suspend to resume), in nanoseconds.
    pub suspension_ns: Summary,
    /// Activations live while their context was suspended, per suspension.
    pub depth_at_suspend: Summary,
}

struct Ctx {
    stack: Vec<(ActId, u64, u32)>,
    suspended_at: Option<u64>,
}

/// Incremental statistics collector.
#[derive(Default)]
pub struct StatsCollector {
    events: u64,
    by_kind: [u64; 7],
    first_ts: Option<u64>,
    last_ts: u64,
    contexts: u64,
    live: HashMap<CtxId, Ctx>,
    suspended: u64,
    peak_live: u64,
    peak_suspended: u64,
    activations: u64,
    max_depth: u64,
    live_frame_bytes: u64,
    peak_live_frame_bytes: u64,
    frame_bytes: Histogram,
    lifetime: Histogram,
    suspension: Histogram,
    depth_at_suspend: Histogram,
}

const KIND_NAMES: [&str; 7] = [
    "spawn", "call", "return", "suspend", "resume", "exit", "sample",
];

impl StatsCollector {
    /// Empty collector.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Account for one event.
    pub fn observe(&mut self, e: &Event) {
        self.events += 1;
        self.first_ts.get_or_insert(e.ts);
        self.last_ts = self.last_ts.max(e.ts);
        let k = KIND_NAMES
            .iter()
            .position(|n| *n == e.kind.name())
            .unwrap_or(0);
        self.by_kind[k] += 1;

        match &e.kind {
            EventKind::Spawn { ctx, .. } => {
                self.contexts += 1;
                self.live.insert(
                    *ctx,
                    Ctx {
                        stack: Vec::new(),
                        suspended_at: None,
                    },
                );
                self.peak_live = self.peak_live.max(self.live.len() as u64);
            }
            EventKind::Call {
                ctx,
                act,
                frame_bytes,
                ..
            } => {
                self.activations += 1;
                self.frame_bytes.record(u64::from(*frame_bytes));
                if let Some(c) = self.live.get_mut(ctx) {
                    c.stack.push((*act, e.ts, *frame_bytes));
                    self.max_depth = self.max_depth.max(c.stack.len() as u64);
                    self.live_frame_bytes += u64::from(*frame_bytes);
                    self.peak_live_frame_bytes =
                        self.peak_live_frame_bytes.max(self.live_frame_bytes);
                }
            }
            EventKind::Return { ctx, .. } => {
                if let Some((_, start, bytes)) = self.live.get_mut(ctx).and_then(|c| c.stack.pop())
                {
                    self.lifetime.record(e.ts.saturating_sub(start));
                    self.live_frame_bytes = self.live_frame_bytes.saturating_sub(u64::from(bytes));
                }
            }
            EventKind::Suspend { ctx } => {
                if let Some(c) = self.live.get_mut(ctx)
                    && c.suspended_at.is_none()
                {
                    c.suspended_at = Some(e.ts);
                    self.depth_at_suspend.record(c.stack.len() as u64);
                    self.suspended += 1;
                    self.peak_suspended = self.peak_suspended.max(self.suspended);
                }
            }
            EventKind::Resume { ctx } => {
                if let Some(at) = self.live.get_mut(ctx).and_then(|c| c.suspended_at.take()) {
                    self.suspension.record(e.ts.saturating_sub(at));
                    self.suspended = self.suspended.saturating_sub(1);
                }
            }
            EventKind::Exit { ctx } => {
                if let Some(c) = self.live.remove(ctx) {
                    if c.suspended_at.is_some() {
                        self.suspended = self.suspended.saturating_sub(1);
                    }
                    let bytes: u64 = c.stack.iter().map(|(_, _, b)| u64::from(*b)).sum();
                    self.live_frame_bytes = self.live_frame_bytes.saturating_sub(bytes);
                }
            }
            EventKind::Sample { .. } => {}
        }
    }

    /// Final statistics.
    #[must_use]
    pub fn finish(&self) -> TraceStats {
        TraceStats {
            events: self.events,
            by_kind: KIND_NAMES.iter().copied().zip(self.by_kind).collect(),
            duration_ns: self.last_ts - self.first_ts.unwrap_or(self.last_ts),
            contexts: self.contexts,
            contexts_live_at_end: self.live.len() as u64,
            peak_live_contexts: self.peak_live,
            peak_suspended_contexts: self.peak_suspended,
            activations: self.activations,
            max_depth: self.max_depth,
            peak_live_frame_bytes: self.peak_live_frame_bytes,
            frame_bytes: self.frame_bytes.summary(),
            activation_lifetime_ns: self.lifetime.summary(),
            suspension_ns: self.suspension.summary(),
            depth_at_suspend: self.depth_at_suspend.summary(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Histogram;

    #[test]
    fn small_values_are_exact() {
        let mut h = Histogram::default();
        for v in 0..16 {
            h.record(v);
        }
        assert_eq!(h.quantile(0.5), 7);
        assert_eq!(h.quantile(1.0), 15);
        assert_eq!(h.quantile(0.0), 0);
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn large_values_within_relative_error() {
        for v in [17u64, 100, 1_000, 123_456, 9_999_999_999, u64::MAX] {
            let mut h = Histogram::default();
            h.record(1);
            h.record(v);
            let q = h.quantile(1.0);
            assert_eq!(q, v, "max bucket clamps to exact max");
            let mut h = Histogram::default();
            h.record(v);
            h.record(u64::MAX);
            let p50 = h.quantile(0.5);
            assert!(
                p50 >= v && (p50 - v) as f64 <= v as f64 / 8.0 + 1.0,
                "{v} -> {p50}"
            );
        }
    }
}
