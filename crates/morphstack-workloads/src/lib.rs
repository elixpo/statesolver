//! Seeded, reproducible synthetic workloads for MorphStack.
//!
//! Every workload is a set of per-context programs executed by a cooperative
//! discrete-event scheduler ([`engine`]). The same `(workload, seed, scale)`
//! produces a bit-identical trace on every platform (see [`rng`]).
//!
//! | Workload | Shape | Expected winner (hypothesis) |
//! |---|---|---|
//! | `w1-nested` | one context, many shallow call trees | contiguous |
//! | `w2-deep` | one context, very deep recursion | contiguous vs segmented (growth) |
//! | `w3-shallow-massive` | 100k shallow contexts parked for seconds | heap |
//! | `w4-frequent-suspend` | 64 contexts, microsecond suspensions | contiguous |
//! | `w5-long-suspend` | 10k deep contexts parked for a minute | heap / segmented |
//! | `w6-phase-change` | long-lived workers alternating CPU and parked phases | **changes over time** |
//! | `server` | Poisson request handlers with I/O waits | mixed |
//! | `burst` | bursts of short-lived suspended tasks | mixed |
//! | `mixed` | CPU workers plus I/O tasks | mixed |
//! | `adv-alternating` | suspensions alternate short/long | adversarial for online policies |
//! | `adv-hot-split` | call/return oscillates across a segment boundary | adversarial for segmented |

pub mod engine;
pub mod rng;

use morphstack_trace::{Event, FnId, FunctionInfo, Header};

use engine::{Op, Timing};
use rng::Rng;

/// Available workloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workload {
    /// W1: normal nested calls.
    W1Nested,
    /// W2: deep recursion.
    W2Deep,
    /// W3: massive shallow concurrency.
    W3ShallowMassive,
    /// W4: frequent suspend/resume.
    W4FrequentSuspend,
    /// W5: long suspensions.
    W5LongSuspend,
    /// W6: phase-changing.
    W6PhaseChange,
    /// Synchronous-style request server.
    Server,
    /// Burst concurrency.
    Burst,
    /// Mixed CPU and I/O.
    Mixed,
    /// Adversarial: alternating short and long suspensions.
    AdvAlternating,
    /// Adversarial: oscillation across a segment boundary.
    AdvHotSplit,
}

impl Workload {
    /// Every workload, in documentation order.
    pub const ALL: [Self; 11] = [
        Self::W1Nested,
        Self::W2Deep,
        Self::W3ShallowMassive,
        Self::W4FrequentSuspend,
        Self::W5LongSuspend,
        Self::W6PhaseChange,
        Self::Server,
        Self::Burst,
        Self::Mixed,
        Self::AdvAlternating,
        Self::AdvHotSplit,
    ];

    /// Stable command-line name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::W1Nested => "w1-nested",
            Self::W2Deep => "w2-deep",
            Self::W3ShallowMassive => "w3-shallow-massive",
            Self::W4FrequentSuspend => "w4-frequent-suspend",
            Self::W5LongSuspend => "w5-long-suspend",
            Self::W6PhaseChange => "w6-phase-change",
            Self::Server => "server",
            Self::Burst => "burst",
            Self::Mixed => "mixed",
            Self::AdvAlternating => "adv-alternating",
            Self::AdvHotSplit => "adv-hot-split",
        }
    }

    /// Parse a command-line name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|w| w.name() == name)
    }
}

// Function table shared by all workloads.
const MAIN: FnId = FnId(1);
const LEAF: FnId = FnId(2);
const RECURSE: FnId = FnId(3);
const HANDLER: FnId = FnId(4);
const IO_WAIT: FnId = FnId(5);
const PARSE: FnId = FnId(6);
const WORKER: FnId = FnId(7);
const TASK: FnId = FnId(8);

fn functions() -> Vec<FunctionInfo> {
    [
        (MAIN, "main", true),
        (LEAF, "leaf", false),
        (RECURSE, "recurse", false),
        (HANDLER, "handler", true),
        (IO_WAIT, "io_wait", true),
        (PARSE, "parse", false),
        (WORKER, "worker_loop", true),
        (TASK, "task", true),
    ]
    .into_iter()
    .map(|(id, name, can_suspend)| FunctionInfo {
        id,
        name: name.to_owned(),
        can_suspend,
    })
    .collect()
}

/// Generation parameters.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    /// Which workload.
    pub workload: Workload,
    /// Seed.
    pub seed: u64,
    /// Multiplier for counts (contexts, iterations). `1.0` is the reference size.
    pub scale: f64,
}

impl Spec {
    fn n(&self, base: u64) -> u64 {
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let v = (base as f64 * self.scale).round() as u64;
        v.max(1)
    }
}

/// The trace header for a spec.
#[must_use]
pub fn header(spec: &Spec) -> Header {
    let mut h = Header::new(format!("morphstack-workloads/{}", spec.workload.name()));
    h.seed = Some(spec.seed);
    h.description = Some(format!("scale={}", spec.scale));
    h.functions = functions();
    h
}

/// Generate a workload, streaming events into `sink`.
///
/// # Errors
/// Propagates the first error returned by `sink`.
pub fn generate<E>(spec: &Spec, sink: &mut dyn FnMut(Event) -> Result<(), E>) -> Result<(), E> {
    let mut rng = Rng::new(spec.seed ^ 0x6d6f_7270_6873_7461); // "morphsta"
    let (roots, mut factory) = build(spec, &mut rng);
    engine::run(roots, &mut *factory, &mut rng, Timing::default(), sink)
}

/// Generate a workload into memory.
#[must_use]
pub fn generate_vec(spec: &Spec) -> Vec<Event> {
    let mut out = Vec::new();
    let _ = generate::<std::convert::Infallible>(spec, &mut |e| {
        out.push(e);
        Ok(())
    });
    out
}

fn call(func: FnId, frame_bytes: u32) -> Op {
    Op::Call { func, frame_bytes }
}

fn bytes(rng: &mut Rng, lo: u32, hi: u32) -> u32 {
    u32::try_from(rng.range(u64::from(lo), u64::from(hi))).unwrap_or(hi)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn ns(v: f64) -> u64 {
    v.max(0.0).round() as u64
}

/// Random call tree rooted at the current depth, with compute at the leaves.
fn call_tree(rng: &mut Rng, ops: &mut Vec<Op>, max_depth: u64) {
    let depth = rng.range(2, max_depth);
    for _ in 0..depth {
        ops.push(call(LEAF, bytes(rng, 32, 256)));
    }
    ops.push(Op::Compute(rng.range(50, 500)));
    for _ in 0..depth {
        ops.push(Op::Return);
    }
}

fn recursion(rng: &mut Rng, ops: &mut Vec<Op>, depth: u64) {
    for _ in 0..depth {
        ops.push(call(RECURSE, bytes(rng, 64, 160)));
        ops.push(Op::Compute(5));
    }
    ops.push(Op::Compute(1_000));
    for _ in 0..depth {
        ops.push(Op::Return);
    }
}

type Built<'a> = (Vec<Vec<Op>>, Box<engine::Factory<'a>>);

#[allow(clippy::too_many_lines)]
fn build(spec: &Spec, rng: &mut Rng) -> Built<'static> {
    const MS: f64 = 1e6;
    const S: f64 = 1e9;
    let none: Box<engine::Factory<'static>> = Box::new(|_, _| Vec::new());
    match spec.workload {
        Workload::W1Nested => {
            let mut ops = vec![call(MAIN, 256)];
            for _ in 0..spec.n(20_000) {
                call_tree(rng, &mut ops, 12);
            }
            (vec![ops], none)
        }
        Workload::W2Deep => {
            let mut ops = vec![call(MAIN, 256)];
            for _ in 0..5 {
                recursion(rng, &mut ops, spec.n(20_000));
            }
            (vec![ops], none)
        }
        Workload::W3ShallowMassive => {
            let mut ops = vec![call(MAIN, 256)];
            for _ in 0..spec.n(100_000) {
                ops.push(Op::Spawn(0));
                ops.push(Op::Compute(50));
            }
            let factory = Box::new(|_: u32, rng: &mut Rng| {
                vec![
                    call(TASK, bytes(rng, 128, 384)),
                    call(IO_WAIT, 128),
                    Op::Sleep(ns(rng.lognormal(2.0 * S, 1.0))),
                    Op::Return,
                    Op::Compute(rng.range(200, 2_000)),
                    Op::Return,
                ]
            });
            (vec![ops], factory)
        }
        Workload::W4FrequentSuspend => {
            let iters = spec.n(2_000);
            let mut ops = vec![call(MAIN, 256)];
            for _ in 0..spec.n(64) {
                ops.push(Op::Spawn(0));
            }
            let factory = Box::new(move |_: u32, rng: &mut Rng| {
                let mut ops = vec![call(WORKER, 512)];
                for _ in 0..iters {
                    ops.push(call(HANDLER, bytes(rng, 256, 768)));
                    ops.push(call(IO_WAIT, 128));
                    ops.push(Op::Sleep(ns(rng.exp(20_000.0))));
                    ops.push(Op::Return);
                    ops.push(Op::Compute(1_000));
                    ops.push(Op::Return);
                }
                ops
            });
            (vec![ops], factory)
        }
        Workload::W5LongSuspend => {
            let mut ops = vec![call(MAIN, 256)];
            for _ in 0..spec.n(10_000) {
                ops.push(Op::Spawn(0));
                ops.push(Op::Compute(100));
            }
            let factory = Box::new(|_: u32, rng: &mut Rng| {
                let depth = rng.range(16, 32);
                let mut ops = vec![call(TASK, 512)];
                for _ in 0..depth {
                    ops.push(call(PARSE, bytes(rng, 64, 512)));
                }
                ops.push(call(IO_WAIT, 128));
                ops.push(Op::Sleep(ns(rng.lognormal(60.0 * S, 0.5))));
                ops.push(Op::Compute(5_000));
                ops
            });
            (vec![ops], factory)
        }
        Workload::W6PhaseChange => {
            // Long-lived workers alternate a CPU phase (deep recursion, no
            // suspension: contiguous should win) with a parked phase holding
            // a moderate stack for a long time (compact representations
            // should win). Meanwhile main releases a burst of parked tasks in
            // each parked phase.
            let cycles = 4;
            let workers = spec.n(32);
            let burst = spec.n(5_000);
            let mut main = vec![call(MAIN, 256)];
            for _ in 0..workers {
                main.push(Op::Spawn(0));
            }
            for _ in 0..cycles {
                main.push(Op::Sleep(ns(20.0 * MS)));
                for _ in 0..burst {
                    main.push(Op::Spawn(1));
                }
                main.push(Op::Sleep(ns(400.0 * MS)));
            }
            let factory = Box::new(move |template: u32, rng: &mut Rng| {
                if template == 1 {
                    return vec![
                        call(TASK, bytes(rng, 128, 384)),
                        call(IO_WAIT, 128),
                        Op::Sleep(ns(rng.lognormal(300.0 * MS, 0.5))),
                    ];
                }
                let mut ops = vec![call(WORKER, 1024)];
                for _ in 0..cycles {
                    for _ in 0..rng.range(3, 6) {
                        let d = rng.range(500, 3_000);
                        recursion(rng, &mut ops, d);
                    }
                    for _ in 0..20 {
                        ops.push(call(PARSE, bytes(rng, 128, 512)));
                    }
                    ops.push(call(IO_WAIT, 256));
                    ops.push(Op::Sleep(ns(rng.lognormal(400.0 * MS, 0.3))));
                    for _ in 0..21 {
                        ops.push(Op::Return);
                    }
                }
                ops
            });
            (vec![main], factory)
        }
        Workload::Server => {
            let mut main = vec![call(MAIN, 256)];
            for _ in 0..spec.n(20_000) {
                main.push(Op::Sleep(ns(rng.exp(50_000.0))));
                main.push(Op::Spawn(0));
            }
            let factory = Box::new(|_: u32, rng: &mut Rng| {
                let mut ops = vec![call(HANDLER, bytes(rng, 384, 1024))];
                for _ in 0..rng.range(1, 3) {
                    let d = rng.range(4, 8);
                    for _ in 0..d {
                        ops.push(call(PARSE, bytes(rng, 64, 256)));
                    }
                    ops.push(Op::Compute(rng.range(500, 5_000)));
                    for _ in 0..d {
                        ops.push(Op::Return);
                    }
                    ops.push(call(IO_WAIT, 128));
                    ops.push(Op::Sleep(ns(rng.lognormal(1.0 * MS, 1.0))));
                    ops.push(Op::Return);
                }
                ops
            });
            (vec![main], factory)
        }
        Workload::Burst => {
            let mut main = vec![call(MAIN, 256)];
            for _ in 0..10 {
                for _ in 0..spec.n(5_000) {
                    main.push(Op::Spawn(0));
                }
                main.push(Op::Sleep(ns(200.0 * MS)));
            }
            let factory = Box::new(|_: u32, rng: &mut Rng| {
                vec![
                    call(TASK, bytes(rng, 128, 512)),
                    call(IO_WAIT, 128),
                    Op::Sleep(ns(rng.lognormal(50.0 * MS, 0.7))),
                    Op::Return,
                    Op::Compute(rng.range(1_000, 10_000)),
                ]
            });
            (vec![main], factory)
        }
        Workload::Mixed => {
            let mut main = vec![call(MAIN, 256)];
            for _ in 0..8 {
                main.push(Op::Spawn(0));
            }
            for _ in 0..spec.n(10_000) {
                main.push(Op::Sleep(ns(rng.exp(100_000.0))));
                main.push(Op::Spawn(1));
            }
            let slices = spec.n(500);
            let factory = Box::new(move |template: u32, rng: &mut Rng| {
                if template == 1 {
                    return vec![
                        call(HANDLER, bytes(rng, 256, 768)),
                        call(IO_WAIT, 128),
                        Op::Sleep(ns(rng.lognormal(5.0 * MS, 1.0))),
                    ];
                }
                let mut ops = vec![call(WORKER, 1024)];
                for _ in 0..slices {
                    for _ in 0..40 {
                        call_tree(rng, &mut ops, 16);
                    }
                    ops.push(Op::Sleep(0)); // cooperative yield
                }
                ops
            });
            (vec![main], factory)
        }
        Workload::AdvAlternating => {
            let mut main = vec![call(MAIN, 256)];
            for _ in 0..spec.n(256) {
                main.push(Op::Spawn(0));
            }
            let rounds = spec.n(200);
            let factory = Box::new(move |_: u32, rng: &mut Rng| {
                let mut ops = vec![call(WORKER, 512)];
                for _ in 0..32 {
                    ops.push(call(PARSE, bytes(rng, 128, 256)));
                }
                for i in 0..rounds {
                    let d = if i % 2 == 0 { 5_000.0 } else { 200.0 * MS };
                    ops.push(Op::Sleep(ns(d)));
                    ops.push(Op::Compute(2_000));
                }
                ops
            });
            (vec![main], factory)
        }
        Workload::AdvHotSplit => {
            // Hold ~4 KiB minus a little, then repeatedly call a 256-byte
            // frame that crosses a 4 KiB segment boundary.
            let mut ops = vec![call(MAIN, 256)];
            for _ in 0..15 {
                ops.push(call(RECURSE, 250));
            }
            for _ in 0..spec.n(100_000) {
                ops.push(call(LEAF, 256));
                ops.push(Op::Compute(20));
                ops.push(Op::Return);
            }
            (vec![ops], none)
        }
    }
}
