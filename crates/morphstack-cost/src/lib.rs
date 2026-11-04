//! Parametric cost model for activation-state representations.
//!
//! The model prices one *step* of one context (a trace event plus the time
//! until that context's next event) under a representation, and prices
//! migrations between representations. It is deliberately **stateless per
//! step**: everything a step's cost depends on is a function of the trace, not
//! of the representation history. This is what lets the oracle in
//! `morphstack-sim` be an exact `O(T·|R|²)` dynamic program.
//!
//! Modelling assumptions (each is a sensitivity-analysis axis):
//!
//! - **Contiguous** stacks start at `min_stack_bytes`, double on overflow
//!   (copying the live bytes), and never shrink: capacity follows the
//!   context's high-water mark since spawn. Fresh stack memory pays a
//!   first-touch (page fault and zeroing) cost when the stack first grows past
//!   its high-water mark, when growth copies into a new stack, and when a
//!   migration into contiguous pre-sizes a stack to the high-water mark.
//! - **Segmented** stacks hold `ceil(live / segment_bytes)` segments; crossing
//!   a boundary allocates or frees a segment eagerly, which reproduces the
//!   hot-split problem.
//! - **Heap** frames are allocated and freed one per activation, with a
//!   per-call locality penalty.
//! - **Memory** is priced at `ns_per_byte_second`, converting footprint into
//!   the same unit as CPU time. The default, 0.125 ns per byte-second, is the
//!   ratio of typical cloud prices for 1 GB of RAM and one vCPU (≈ 1:8).
//!
//! Default parameters are placeholders until calibrated by microbenchmarks
//! (Stage 2, elixpo/morphstack#2). Profiles are TOML files.

use serde::{Deserialize, Serialize};

/// An activation-state representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rep {
    /// One contiguous, growable stack per context.
    Contiguous,
    /// A linked list of fixed-size stack segments.
    Segmented,
    /// Individually allocated heap frames.
    Heap,
}

impl Rep {
    /// All representations, in index order.
    pub const ALL: [Self; 3] = [Self::Contiguous, Self::Segmented, Self::Heap];

    /// Dense index for tables.
    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }

    /// Short name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Contiguous => "contiguous",
            Self::Segmented => "segmented",
            Self::Heap => "heap",
        }
    }

    /// One-letter tag for timelines.
    #[must_use]
    pub fn letter(self) -> char {
        match self {
            Self::Contiguous => 'C',
            Self::Segmented => 'S',
            Self::Heap => 'H',
        }
    }
}

/// What a step does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    /// Context created.
    Spawn,
    /// Activation pushed.
    Call,
    /// Activation popped.
    Return,
    /// Context parked.
    Suspend,
    /// Context continues.
    Resume,
    /// Context destroyed.
    Exit,
}

/// Everything the cost of one step depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    /// What happened.
    pub kind: StepKind,
    /// Live frame bytes before the step.
    pub live_before: u64,
    /// Live frame bytes after the step.
    pub live_after: u64,
    /// Live frames after the step.
    pub frames_after: u32,
    /// High-water mark of live bytes before the step.
    pub hwm_before: u64,
    /// High-water mark of live bytes after the step.
    pub hwm_after: u64,
    /// Nanoseconds until this context's next step; the state after this step
    /// is held for that long.
    pub dt_ns: u64,
}

/// Contiguous-stack parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contiguous {
    /// Initial and minimum capacity.
    pub min_stack_bytes: u64,
    /// Stack-limit check per call.
    pub check_ns: f64,
    /// Allocate a stack (mmap and guard page, or pool).
    pub alloc_ns: f64,
    /// Release a stack.
    pub free_ns: f64,
    /// Fixed cost of a growth relocation.
    pub grow_fixed_ns: f64,
    /// Copy cost per live byte on growth.
    pub copy_ns_per_byte: f64,
    /// First-touch cost (page fault and zeroing) per byte of fresh stack memory.
    pub touch_ns_per_byte: f64,
}

/// Segmented-stack parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segmented {
    /// Segment size.
    pub segment_bytes: u64,
    /// Per-segment metadata.
    pub header_bytes: u64,
    /// Split check per call.
    pub check_ns: f64,
    /// Allocate a segment.
    pub alloc_ns: f64,
    /// Free a segment.
    pub free_ns: f64,
    /// Extra cost of a call or return that crosses a segment boundary.
    pub boundary_ns: f64,
}

/// Heap-frame parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Heap {
    /// Per-frame metadata.
    pub header_bytes: u64,
    /// Fixed per-context descriptor.
    pub context_bytes: u64,
    /// Allocate one frame.
    pub alloc_ns: f64,
    /// Free one frame.
    pub free_ns: f64,
    /// Locality penalty per call (pointer chasing, cache misses).
    pub locality_ns: f64,
}

/// Costs common to every representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Common {
    /// Call overhead.
    pub call_ns: f64,
    /// Return overhead.
    pub return_ns: f64,
    /// Context switch on suspend or resume.
    pub switch_ns: f64,
}

/// Memory pricing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    /// Cost of holding one byte for one second, in nanoseconds of CPU time.
    pub ns_per_byte_second: f64,
}

/// Copy and fix-up cost of one migration `from → to`, on top of releasing
/// the source and acquiring the destination storage.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Migration {
    /// Fixed cost (descriptor rewrite, scheduler bookkeeping).
    pub fixed_ns: f64,
    /// Copy cost per live byte.
    pub ns_per_byte: f64,
    /// Pointer fix-up and descriptor cost per live frame.
    pub ns_per_frame: f64,
}

/// A complete cost profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostModel {
    /// Profile name.
    pub name: String,
    /// Shared costs.
    pub common: Common,
    /// Contiguous parameters.
    pub contiguous: Contiguous,
    /// Segmented parameters.
    pub segmented: Segmented,
    /// Heap parameters.
    pub heap: Heap,
    /// Memory pricing.
    pub memory: Memory,
    /// Migration matrix, indexed `[from][to]` by [`Rep::index`]. The diagonal
    /// is ignored.
    pub migration: [[Migration; 3]; 3],
}

impl Default for CostModel {
    fn default() -> Self {
        let m = |fixed_ns, ns_per_byte, ns_per_frame| Migration {
            fixed_ns,
            ns_per_byte,
            ns_per_frame,
        };
        let zero = m(0.0, 0.0, 0.0);
        Self {
            name: "default-uncalibrated".to_owned(),
            common: Common {
                call_ns: 1.5,
                return_ns: 1.0,
                switch_ns: 25.0,
            },
            contiguous: Contiguous {
                min_stack_bytes: 8192,
                check_ns: 0.2,
                alloc_ns: 400.0,
                free_ns: 300.0,
                grow_fixed_ns: 250.0,
                copy_ns_per_byte: 0.08,
                touch_ns_per_byte: 0.1,
            },
            segmented: Segmented {
                segment_bytes: 4096,
                header_bytes: 32,
                check_ns: 0.4,
                alloc_ns: 60.0,
                free_ns: 40.0,
                boundary_ns: 15.0,
            },
            heap: Heap {
                header_bytes: 16,
                context_bytes: 64,
                alloc_ns: 18.0,
                free_ns: 12.0,
                locality_ns: 2.5,
            },
            memory: Memory {
                ns_per_byte_second: 0.125,
            },
            // Release and acquire costs come from the representation
            // parameters; these entries cover copying and fix-up only.
            //            to: contiguous           segmented            heap
            migration: [
                [zero, m(50.0, 0.08, 2.0), m(50.0, 0.10, 6.0)], // from contiguous
                [m(50.0, 0.08, 2.0), zero, m(50.0, 0.10, 6.0)], // from segmented
                [m(50.0, 0.10, 6.0), m(50.0, 0.10, 6.0), zero], // from heap
            ],
        }
    }
}

/// Per-category cost, in nanoseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Breakdown {
    /// Calls, returns, switches, checks, per-frame allocation.
    pub exec: f64,
    /// Contiguous growth and segment allocation/free.
    pub growth: f64,
    /// Priced memory footprint.
    pub memory: f64,
    /// Migrations.
    pub migration: f64,
}

impl Breakdown {
    /// Sum of all categories.
    #[must_use]
    pub fn total(&self) -> f64 {
        self.exec + self.growth + self.memory + self.migration
    }
}

impl std::ops::AddAssign for Breakdown {
    fn add_assign(&mut self, o: Self) {
        self.exec += o.exec;
        self.growth += o.growth;
        self.memory += o.memory;
        self.migration += o.migration;
    }
}

#[allow(clippy::cast_precision_loss)]
fn f(v: u64) -> f64 {
    v as f64
}

impl CostModel {
    /// Load a TOML profile.
    ///
    /// # Errors
    /// Fails if the text is not a valid profile.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Serialize to TOML.
    ///
    /// # Panics
    /// Never: every field is TOML-representable.
    #[must_use]
    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("profile serializes")
    }

    /// Contiguous capacity for a high-water mark.
    #[must_use]
    pub fn contiguous_capacity(&self, hwm: u64) -> u64 {
        hwm.max(self.contiguous.min_stack_bytes)
            .checked_next_power_of_two()
            .unwrap_or(u64::MAX)
    }

    /// Segments needed for `live` bytes (at least one).
    #[must_use]
    pub fn segments(&self, live: u64) -> u64 {
        live.div_ceil(self.segmented.segment_bytes).max(1)
    }

    /// Bytes held by a context in `rep`.
    #[must_use]
    pub fn footprint(&self, rep: Rep, live: u64, frames: u32, hwm: u64) -> u64 {
        match rep {
            Rep::Contiguous => self.contiguous_capacity(hwm),
            Rep::Segmented => {
                self.segments(live) * (self.segmented.segment_bytes + self.segmented.header_bytes)
            }
            Rep::Heap => {
                live + u64::from(frames) * self.heap.header_bytes + self.heap.context_bytes
            }
        }
    }

    /// Cost of one step under `rep`. Exit steps hold no memory.
    #[must_use]
    pub fn step(&self, rep: Rep, s: &Step) -> Breakdown {
        let c = &self.common;
        let mut b = Breakdown::default();
        match (rep, s.kind) {
            (_, StepKind::Suspend | StepKind::Resume) => b.exec += c.switch_ns,

            (Rep::Contiguous, StepKind::Spawn) => b.exec += self.contiguous.alloc_ns,
            (_, StepKind::Exit) => b.exec += self.release(rep, s.live_before, s.frames_after),
            (Rep::Contiguous, StepKind::Call) => {
                b.exec += c.call_ns + self.contiguous.check_ns;
                let k = &self.contiguous;
                // Deepening past the high-water mark touches fresh pages.
                b.growth += k.touch_ns_per_byte * f(s.hwm_after - s.hwm_before);
                if self.contiguous_capacity(s.hwm_after) > self.contiguous_capacity(s.hwm_before) {
                    // Relocation copies the live bytes into fresh pages.
                    b.growth += k.grow_fixed_ns
                        + (k.copy_ns_per_byte + k.touch_ns_per_byte) * f(s.live_before);
                }
            }
            (Rep::Contiguous, StepKind::Return) => b.exec += c.return_ns,

            (Rep::Segmented, StepKind::Spawn) => b.exec += self.segmented.alloc_ns,
            (Rep::Segmented, StepKind::Call) => {
                b.exec += c.call_ns + self.segmented.check_ns;
                let (before, after) = (self.segments(s.live_before), self.segments(s.live_after));
                if after > before {
                    b.growth +=
                        f(after - before) * self.segmented.alloc_ns + self.segmented.boundary_ns;
                }
            }
            (Rep::Segmented, StepKind::Return) => {
                b.exec += c.return_ns;
                let (before, after) = (self.segments(s.live_before), self.segments(s.live_after));
                if after < before {
                    b.growth +=
                        f(before - after) * self.segmented.free_ns + self.segmented.boundary_ns;
                }
            }

            (Rep::Heap, StepKind::Spawn) => b.exec += self.heap.alloc_ns,
            (Rep::Heap, StepKind::Call) => {
                b.exec += c.call_ns + self.heap.alloc_ns + self.heap.locality_ns;
            }
            (Rep::Heap, StepKind::Return) => b.exec += c.return_ns + self.heap.free_ns,
        }
        if s.kind != StepKind::Exit {
            let bytes = self.footprint(rep, s.live_after, s.frames_after, s.hwm_after);
            b.memory = self.memory.ns_per_byte_second * f(bytes) * f(s.dt_ns) * 1e-9;
        }
        b
    }

    /// Cost of releasing a context's storage in `rep` (also paid at exit).
    #[must_use]
    pub fn release(&self, rep: Rep, live: u64, frames: u32) -> f64 {
        match rep {
            Rep::Contiguous => self.contiguous.free_ns,
            Rep::Segmented => self.segmented.free_ns * f(self.segments(live)),
            Rep::Heap => self.heap.free_ns * f(u64::from(frames) + 1),
        }
    }

    /// Cost of acquiring storage for a migrated context in `rep`. A new
    /// contiguous stack is sized to, and pre-faulted up to, the high-water mark.
    #[must_use]
    pub fn acquire(&self, rep: Rep, live: u64, frames: u32, hwm: u64) -> f64 {
        match rep {
            Rep::Contiguous => {
                self.contiguous.alloc_ns + self.contiguous.touch_ns_per_byte * f(hwm.max(live))
            }
            Rep::Segmented => self.segmented.alloc_ns * f(self.segments(live)),
            Rep::Heap => self.heap.alloc_ns * f(u64::from(frames) + 1),
        }
    }

    /// Cost of migrating a context holding `live` bytes in `frames` frames,
    /// with high-water mark `hwm`: release the source, acquire the
    /// destination, copy the live bytes, and fix up each frame.
    #[must_use]
    pub fn migrate(&self, from: Rep, to: Rep, live: u64, frames: u32, hwm: u64) -> f64 {
        if from == to {
            return 0.0;
        }
        let m = &self.migration[from.index()][to.index()];
        self.release(from, live, frames)
            + self.acquire(to, live, frames, hwm)
            + m.fixed_ns
            + m.ns_per_byte * f(live)
            + m.ns_per_frame * f(u64::from(frames))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(kind: StepKind, live_before: u64, live_after: u64, hwm_before: u64) -> Step {
        Step {
            kind,
            live_before,
            live_after,
            frames_after: 1,
            hwm_before,
            hwm_after: hwm_before.max(live_after),
            dt_ns: 0,
        }
    }

    #[test]
    fn contiguous_growth_only_when_capacity_doubles() {
        let m = CostModel::default();
        let quiet = m.step(Rep::Contiguous, &step(StepKind::Call, 100, 200, 100));
        // Only first-touch of the 100 new bytes, no relocation.
        assert!((quiet.growth - 100.0 * m.contiguous.touch_ns_per_byte).abs() < 1e-9);
        let grow = m.step(Rep::Contiguous, &step(StepKind::Call, 8100, 8300, 8100));
        assert!(grow.growth > m.contiguous.grow_fixed_ns);
    }

    #[test]
    fn segmented_pays_on_both_sides_of_a_boundary() {
        let m = CostModel::default();
        let up = m.step(Rep::Segmented, &step(StepKind::Call, 4000, 4200, 4000));
        let down = m.step(Rep::Segmented, &step(StepKind::Return, 4200, 4000, 4200));
        assert!(up.growth > 0.0 && down.growth > 0.0);
    }

    #[test]
    fn memory_is_priced_by_footprint_and_time() {
        let m = CostModel::default();
        let mut s = step(StepKind::Suspend, 1000, 1000, 1000);
        s.dt_ns = 1_000_000_000;
        let c = m.step(Rep::Contiguous, &s).memory;
        let h = m.step(Rep::Heap, &s).memory;
        assert!((c - 0.125 * 8192.0).abs() < 1e-9, "{c}");
        assert!(h < c);
    }

    #[test]
    fn migration_is_zero_on_diagonal_and_asymmetric() {
        let m = CostModel::default();
        assert!(m.migrate(Rep::Heap, Rep::Heap, 1 << 20, 100, 1 << 20).abs() < f64::EPSILON);
        assert!(
            m.migrate(Rep::Heap, Rep::Contiguous, 4096, 10, 4096)
                > m.migrate(Rep::Contiguous, Rep::Heap, 4096, 10, 4096)
        );
    }

    #[test]
    fn migrating_before_exit_never_pays() {
        let m = CostModel::default();
        let exit = |r| m.step(r, &step(StepKind::Exit, 512, 0, 4096)).total();
        for from in Rep::ALL {
            for to in Rep::ALL {
                if from != to {
                    assert!(m.migrate(from, to, 512, 2, 4096) + exit(to) > exit(from));
                }
            }
        }
    }

    #[test]
    fn toml_round_trip() {
        let m = CostModel::default();
        assert_eq!(CostModel::from_toml(&m.to_toml()).unwrap(), m);
    }
}
