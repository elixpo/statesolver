//! Trace-driven simulator, fixed-representation baselines, and the exact
//! hindsight oracle.
//!
//! The unit of decision is the **context**: before each of its steps, a
//! policy picks the representation the context's whole activation stack lives
//! in. Per-activation and per-segment granularity (RQ5) build on this in
//! Stage 3.
//!
//! Because the cost model is separable across contexts (memory is priced
//! linearly, with no shared pressure term), the per-context oracle is the
//! exact hindsight optimum for the whole trace. With a non-linear shared
//! memory-pressure term it would only be a lower bound; see
//! `docs/simulator.md`.

pub mod oracle;

use std::collections::HashMap;

use morphstack_cost::{Breakdown, CostModel, Rep, Step, StepKind};
use morphstack_trace::{CtxId, Event, EventKind, FnId, Header};
use serde::Serialize;

/// One context's steps, ready for pricing.
#[derive(Debug, Clone)]
pub struct Context {
    /// Trace identifier.
    pub id: CtxId,
    /// Whether the context's entry function can suspend (static knowledge).
    pub entry_can_suspend: bool,
    /// Step timestamps.
    pub ts: Vec<u64>,
    /// Steps.
    pub steps: Vec<Step>,
    /// Live frames before each step (for migration costs).
    pub frames_before: Vec<u32>,
}

/// A trace prepared for simulation.
#[derive(Debug, Clone)]
pub struct Workload {
    /// Contexts in spawn order.
    pub contexts: Vec<Context>,
    /// Last timestamp in the trace.
    pub end_ts: u64,
}

impl Workload {
    /// Build from a header and an event stream. Events for unknown contexts
    /// are ignored; validate traces first.
    ///
    /// # Errors
    /// Propagates errors from the event stream.
    pub fn from_events<E>(
        header: &Header,
        events: impl IntoIterator<Item = Result<Event, E>>,
    ) -> Result<Self, E> {
        let can_suspend: HashMap<FnId, bool> = header
            .functions
            .iter()
            .map(|f| (f.id, f.can_suspend))
            .collect();
        let mut index: HashMap<CtxId, usize> = HashMap::new();
        let mut contexts: Vec<Context> = Vec::new();
        // Per-context running state: live bytes, frame sizes, high-water mark.
        let mut live: Vec<(u64, Vec<u32>, u64)> = Vec::new();
        let mut end_ts = 0;

        for e in events {
            let e = e?;
            end_ts = end_ts.max(e.ts);
            let Some(ctx_id) = e.kind.ctx() else { continue };
            let i = if let EventKind::Spawn { .. } = e.kind {
                let i = contexts.len();
                index.insert(ctx_id, i);
                contexts.push(Context {
                    id: ctx_id,
                    entry_can_suspend: false,
                    ts: Vec::new(),
                    steps: Vec::new(),
                    frames_before: Vec::new(),
                });
                live.push((0, Vec::new(), 0));
                i
            } else if let Some(&i) = index.get(&ctx_id) {
                i
            } else {
                continue;
            };
            let (bytes, frames, hwm) = &mut live[i];
            let (live_before, hwm_before) = (*bytes, *hwm);
            let frames_before = u32::try_from(frames.len()).unwrap_or(u32::MAX);
            let kind = match e.kind {
                EventKind::Spawn { .. } => StepKind::Spawn,
                EventKind::Call {
                    func, frame_bytes, ..
                } => {
                    if frames.is_empty() && contexts[i].steps.len() == 1 {
                        contexts[i].entry_can_suspend =
                            can_suspend.get(&func).copied().unwrap_or(false);
                    }
                    frames.push(frame_bytes);
                    *bytes += u64::from(frame_bytes);
                    *hwm = (*hwm).max(*bytes);
                    StepKind::Call
                }
                EventKind::Return { .. } => {
                    *bytes -= frames.pop().map_or(0, u64::from);
                    StepKind::Return
                }
                EventKind::Suspend { .. } => StepKind::Suspend,
                EventKind::Resume { .. } => StepKind::Resume,
                EventKind::Exit { .. } => {
                    index.remove(&ctx_id);
                    StepKind::Exit
                }
                EventKind::Sample { .. } => continue,
            };
            let c = &mut contexts[i];
            if let Some(prev) = c.ts.last().copied()
                && let Some(last) = c.steps.last_mut()
            {
                last.dt_ns = e.ts - prev;
            }
            c.ts.push(e.ts);
            c.frames_before.push(frames_before);
            c.steps.push(Step {
                kind,
                live_before,
                live_after: *bytes,
                frames_after: u32::try_from(frames.len()).unwrap_or(u32::MAX),
                hwm_before,
                hwm_after: *hwm,
                dt_ns: 0,
            });
        }
        // Contexts still live at the end hold their state until the trace ends.
        for c in &mut contexts {
            if let (Some(&ts), Some(last)) = (c.ts.last(), c.steps.last_mut())
                && last.kind != StepKind::Exit
            {
                last.dt_ns = end_ts - ts;
            }
        }
        Ok(Self { contexts, end_ts })
    }

    /// Total number of steps.
    #[must_use]
    pub fn steps(&self) -> usize {
        self.contexts.iter().map(|c| c.steps.len()).sum()
    }
}

/// A representation per step, per context.
pub type Schedule = Vec<Vec<Rep>>;

/// Result of pricing one schedule.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Outcome {
    /// Cost by category, in nanoseconds.
    pub cost: Breakdown,
    /// Number of migrations.
    pub migrations: u64,
    /// Live bytes copied by migrations.
    pub bytes_migrated: u64,
    /// Peak total footprint across all contexts.
    pub peak_bytes: u64,
    /// Time-averaged total footprint.
    pub mean_bytes: f64,
    /// Fraction of step-time spent in each representation, by `Rep::index`.
    pub time_share: [f64; 3],
}

/// Price one context under a per-step schedule. `footprints`, when given,
/// receives `(ts, delta_bytes)` for the global memory timeline.
#[must_use]
pub fn price_context(
    model: &CostModel,
    c: &Context,
    reps: &[Rep],
    mut footprints: Option<&mut Vec<(u64, i64)>>,
) -> Outcome {
    let mut out = Outcome::default();
    let mut prev_bytes = 0i64;
    let mut held = [0f64; 3];
    for (i, (s, &rep)) in c.steps.iter().zip(reps).enumerate() {
        if i > 0 && reps[i - 1] != rep {
            out.cost.migration += model.migrate(
                reps[i - 1],
                rep,
                s.live_before,
                c.frames_before[i],
                s.hwm_before,
            );
            out.migrations += 1;
            out.bytes_migrated += s.live_before;
        }
        out.cost += model.step(rep, s);
        #[allow(clippy::cast_precision_loss)]
        {
            held[rep.index()] += s.dt_ns as f64;
        }
        if let Some(fp) = footprints.as_deref_mut() {
            let bytes = if s.kind == StepKind::Exit {
                0
            } else {
                i64::try_from(model.footprint(rep, s.live_after, s.frames_after, s.hwm_after))
                    .unwrap_or(i64::MAX)
            };
            fp.push((c.ts[i], bytes - prev_bytes));
            prev_bytes = bytes;
        }
    }
    out.time_share = held;
    out
}

/// Price a whole schedule.
///
/// # Panics
/// Panics if the schedule's shape does not match the workload.
#[must_use]
pub fn price(model: &CostModel, w: &Workload, schedule: &Schedule) -> Outcome {
    assert_eq!(schedule.len(), w.contexts.len(), "one schedule per context");
    let mut total = Outcome::default();
    let mut timeline = Vec::with_capacity(w.steps());
    let mut held = [0f64; 3];
    for (c, reps) in w.contexts.iter().zip(schedule) {
        assert_eq!(reps.len(), c.steps.len(), "one rep per step");
        let o = price_context(model, c, reps, Some(&mut timeline));
        total.cost += o.cost;
        total.migrations += o.migrations;
        total.bytes_migrated += o.bytes_migrated;
        for (h, t) in held.iter_mut().zip(o.time_share) {
            *h += t;
        }
    }
    let sum: f64 = held.iter().sum();
    if sum > 0.0 {
        total.time_share = held.map(|h| h / sum);
    }
    timeline.sort_unstable_by_key(|&(ts, _)| ts);
    let (mut cur, mut peak, mut area, mut last_ts) = (0i64, 0i64, 0f64, 0u64);
    let mut i = 0;
    while i < timeline.len() {
        let ts = timeline[i].0;
        #[allow(clippy::cast_precision_loss)]
        {
            area += cur as f64 * (ts - last_ts) as f64;
        }
        last_ts = ts;
        while i < timeline.len() && timeline[i].0 == ts {
            cur += timeline[i].1;
            i += 1;
        }
        peak = peak.max(cur);
    }
    total.peak_bytes = u64::try_from(peak).unwrap_or(0);
    #[allow(clippy::cast_precision_loss)]
    if w.end_ts > 0 {
        total.mean_bytes = area / w.end_ts as f64;
    }
    total
}

/// Policies that need no online learning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Baseline {
    /// Always one representation (baselines A, B, C).
    Fixed(Rep),
    /// Baseline D: heap if the entry function can suspend, else contiguous.
    Static,
}

impl Baseline {
    /// All baselines.
    pub const ALL: [Self; 4] = [
        Self::Fixed(Rep::Contiguous),
        Self::Fixed(Rep::Segmented),
        Self::Fixed(Rep::Heap),
        Self::Static,
    ];

    /// Display name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Fixed(Rep::Contiguous) => "A always-contiguous",
            Self::Fixed(Rep::Segmented) => "B always-segmented",
            Self::Fixed(Rep::Heap) => "C always-heap",
            Self::Static => "D static-can-suspend",
        }
    }

    /// The schedule this baseline produces.
    #[must_use]
    pub fn schedule(self, w: &Workload) -> Schedule {
        w.contexts
            .iter()
            .map(|c| {
                let rep = match self {
                    Self::Fixed(r) => r,
                    Self::Static if c.entry_can_suspend => Rep::Heap,
                    Self::Static => Rep::Contiguous,
                };
                vec![rep; c.steps.len()]
            })
            .collect()
    }
}

/// Hindsight per-context static schedule: each context gets the single
/// representation that is cheapest for it over its whole life. The gap from
/// the best global fixed policy to this measures the value of
/// *heterogeneity*; the gap from this to the oracle measures the value of
/// *switching over time*.
#[must_use]
pub fn hindsight_static(model: &CostModel, w: &Workload) -> Schedule {
    w.contexts
        .iter()
        .map(|c| {
            let best = Rep::ALL
                .into_iter()
                .min_by(|&a, &b| {
                    let cost = |r| {
                        price_context(model, c, &vec![r; c.steps.len()], None)
                            .cost
                            .total()
                    };
                    cost(a).total_cmp(&cost(b))
                })
                .unwrap_or(Rep::Contiguous);
            vec![best; c.steps.len()]
        })
        .collect()
}

/// Upper bound on what *any* representation scheme can achieve under this
/// cost model, at any granularity (per context, per activation, per
/// segment), with free and instantaneous migration: every step pays the
/// cheapest execution and growth cost of any representation **plus** the
/// smallest memory cost of any representation, even when they come from
/// different representations.
///
/// The gap between [`hindsight_static`] and this bound is the headroom left
/// for switching over time and for finer-grained mixing.
#[must_use]
pub fn ideal_bound(model: &CostModel, w: &Workload) -> Breakdown {
    let mut total = Breakdown::default();
    for c in &w.contexts {
        for s in &c.steps {
            let costs = Rep::ALL.map(|r| model.step(r, s));
            total.exec += costs
                .iter()
                .map(|b| b.exec + b.growth)
                .fold(f64::INFINITY, f64::min);
            total.memory += costs.iter().map(|b| b.memory).fold(f64::INFINITY, f64::min);
        }
    }
    total
}

/// Free-migration bound: each step uses the single cheapest representation
/// for that step, and migrations cost nothing (no release, acquire, or
/// copy). The gap from [`hindsight_static`] to this bound is what migration
/// costs prevent; the gap from this bound to [`ideal_bound`] is what only
/// mixing representations *within* a step (finer granularity) could win.
#[must_use]
pub fn free_migration_bound(model: &CostModel, w: &Workload) -> f64 {
    w.contexts
        .iter()
        .flat_map(|c| &c.steps)
        .map(|s| {
            Rep::ALL
                .iter()
                .map(|&r| model.step(r, s).total())
                .fold(f64::INFINITY, f64::min)
        })
        .sum()
}
