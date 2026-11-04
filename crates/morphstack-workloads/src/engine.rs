//! Single-core, cooperative, discrete-event scheduler that executes
//! per-context programs and emits a valid trace.
//!
//! A context runs until it sleeps (suspends) or exits; it is never preempted.
//! Sleeping contexts wake into a FIFO run queue. A `resume` event is emitted
//! when a woken context actually starts running again, so `suspend`→`resume`
//! measures the time the activation state was parked, including queueing.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use morphstack_trace::{ActId, CtxId, Event, EventKind, FnId};

use crate::rng::Rng;

/// One step of a context's program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Push an activation.
    Call {
        /// Function.
        func: FnId,
        /// Frame size in bytes.
        frame_bytes: u32,
    },
    /// Pop the innermost activation.
    Return,
    /// Run for this many nanoseconds without calls.
    Compute(u64),
    /// Suspend for at least this many nanoseconds. `Sleep(0)` is a yield.
    Sleep(u64),
    /// Spawn a child from the factory template with this id.
    Spawn(u32),
}

/// Fixed costs of the abstract machine, in nanoseconds of simulated time.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// Time charged per call.
    pub call_ns: u64,
    /// Time charged per return.
    pub return_ns: u64,
    /// Time charged per spawn.
    pub spawn_ns: u64,
    /// Time charged per context switch.
    pub switch_ns: u64,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            call_ns: 2,
            return_ns: 1,
            spawn_ns: 40,
            switch_ns: 20,
        }
    }
}

struct Ctx {
    id: CtxId,
    ops: VecDeque<Op>,
    stack: Vec<ActId>,
    resuming: bool,
}

/// Program factory: builds a child program from a template id. Called at
/// spawn time, so memory stays proportional to live contexts.
pub type Factory<'a> = dyn FnMut(u32, &mut Rng) -> Vec<Op> + 'a;

/// Run root programs to completion, sending every event to `sink`.
///
/// # Errors
/// Propagates the first error returned by `sink`.
///
/// # Panics
/// Never in practice: only scheduled slots are dereferenced.
#[allow(clippy::too_many_lines)]
pub fn run<E>(
    roots: Vec<Vec<Op>>,
    factory: &mut Factory<'_>,
    rng: &mut Rng,
    timing: Timing,
    sink: &mut dyn FnMut(Event) -> Result<(), E>,
) -> Result<(), E> {
    let mut now = 0u64;
    let mut next_ctx = 0u64;
    let mut next_act = 0u64;
    let mut slots: Vec<Option<Ctx>> = Vec::new();
    let mut free: Vec<usize> = Vec::new();
    let mut runq: VecDeque<usize> = VecDeque::new();
    let mut timers: BinaryHeap<Reverse<(u64, u64, usize)>> = BinaryHeap::new();
    let mut timer_seq = 0u64;

    let mut new_ctx = |ops: Vec<Op>| {
        let id = CtxId(next_ctx);
        next_ctx += 1;
        Ctx {
            id,
            ops: ops.into(),
            stack: Vec::new(),
            resuming: false,
        }
    };

    for ops in roots {
        let ctx = new_ctx(ops);
        sink(Event {
            ts: now,
            kind: EventKind::Spawn {
                ctx: ctx.id,
                parent: None,
            },
        })?;
        insert(ctx, &mut slots, &mut free, &mut runq);
    }

    loop {
        // Wake every timer that is due.
        while let Some(Reverse((wake, _, slot))) = timers.peek().copied() {
            if wake > now && !runq.is_empty() {
                break;
            }
            timers.pop();
            now = now.max(wake);
            runq.push_back(slot);
        }
        let Some(slot) = runq.pop_front() else {
            if timers.is_empty() {
                return Ok(());
            }
            continue;
        };
        now += timing.switch_ns;

        let mut exited = false;
        let mut children = Vec::new();
        {
            let ctx = slots[slot].as_mut().expect("scheduled slot is live");
            if ctx.resuming {
                ctx.resuming = false;
                sink(Event {
                    ts: now,
                    kind: EventKind::Resume { ctx: ctx.id },
                })?;
            }
            loop {
                let Some(op) = ctx.ops.pop_front() else {
                    // Program finished: unwind whatever is left, then exit.
                    while let Some(act) = ctx.stack.pop() {
                        sink(Event {
                            ts: now,
                            kind: EventKind::Return { ctx: ctx.id, act },
                        })?;
                        now += timing.return_ns;
                    }
                    sink(Event {
                        ts: now,
                        kind: EventKind::Exit { ctx: ctx.id },
                    })?;
                    exited = true;
                    break;
                };
                match op {
                    Op::Call { func, frame_bytes } => {
                        let act = ActId(next_act);
                        next_act += 1;
                        ctx.stack.push(act);
                        sink(Event {
                            ts: now,
                            kind: EventKind::Call {
                                ctx: ctx.id,
                                act,
                                func,
                                frame_bytes,
                            },
                        })?;
                        now += timing.call_ns;
                    }
                    Op::Return => {
                        if let Some(act) = ctx.stack.pop() {
                            sink(Event {
                                ts: now,
                                kind: EventKind::Return { ctx: ctx.id, act },
                            })?;
                            now += timing.return_ns;
                        }
                    }
                    Op::Compute(ns) => now += ns,
                    Op::Sleep(ns) => {
                        sink(Event {
                            ts: now,
                            kind: EventKind::Suspend { ctx: ctx.id },
                        })?;
                        ctx.resuming = true;
                        timers.push(Reverse((now + ns, timer_seq, slot)));
                        timer_seq += 1;
                        break;
                    }
                    Op::Spawn(template) => {
                        // The spawn event is emitted now so it keeps its
                        // timestamp; the child is queued once the parent
                        // yields, which is before it could run anyway.
                        let child = new_ctx(factory(template, rng));
                        sink(Event {
                            ts: now,
                            kind: EventKind::Spawn {
                                ctx: child.id,
                                parent: Some(ctx.id),
                            },
                        })?;
                        children.push(child);
                        now += timing.spawn_ns;
                    }
                }
            }
        }
        if exited {
            slots[slot] = None;
            free.push(slot);
        }
        for child in children {
            insert(child, &mut slots, &mut free, &mut runq);
        }
    }
}

fn insert(
    ctx: Ctx,
    slots: &mut Vec<Option<Ctx>>,
    free: &mut Vec<usize>,
    runq: &mut VecDeque<usize>,
) {
    let slot = if let Some(i) = free.pop() {
        slots[i] = Some(ctx);
        i
    } else {
        slots.push(Some(ctx));
        slots.len() - 1
    };
    runq.push_back(slot);
}
