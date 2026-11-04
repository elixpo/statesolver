//! Exact hindsight oracle.
//!
//! For each context, a Viterbi-style dynamic program over
//! `(step, representation)` finds the schedule minimizing
//! `Σ C(r_t, x_t) + Σ M(r_{t-1}, r_t)`. It runs in `O(T·|R|²)` time and stores
//! `O(T·|R|)` back-pointers. Ties prefer staying in the current
//! representation, so the oracle never migrates without a strict gain.

use morphstack_cost::{CostModel, Rep};

use crate::{Context, Schedule, Workload};

/// The optimal schedule for one context and its cost.
#[must_use]
pub fn solve_context(model: &CostModel, c: &Context) -> (Vec<Rep>, f64) {
    let n = c.steps.len();
    if n == 0 {
        return (Vec::new(), 0.0);
    }
    let mut back: Vec<[u8; 3]> = Vec::with_capacity(n);
    let mut cost = Rep::ALL.map(|r| model.step(r, &c.steps[0]).total());
    back.push([0, 1, 2]);
    for i in 1..n {
        let s = &c.steps[i];
        let mut next = [0f64; 3];
        let mut from = [0u8; 3];
        for to in Rep::ALL {
            let t = to.index();
            // Start from "stay", so ties keep the current representation.
            let (mut best, mut arg) = (cost[t], t);
            for q in Rep::ALL {
                let candidate = cost[q.index()]
                    + model.migrate(q, to, s.live_before, c.frames_before[i], s.hwm_before);
                if candidate < best {
                    best = candidate;
                    arg = q.index();
                }
            }
            next[t] = best + model.step(to, s).total();
            from[t] = u8::try_from(arg).unwrap_or(0);
        }
        cost = next;
        back.push(from);
    }
    let (mut r, total) =
        cost.iter().enumerate().fold(
            (0, f64::INFINITY),
            |(bi, bv), (i, &v)| {
                if v < bv { (i, v) } else { (bi, bv) }
            },
        );
    let mut reps = vec![Rep::Contiguous; n];
    for i in (0..n).rev() {
        reps[i] = Rep::ALL[r];
        r = usize::from(back[i][r]);
    }
    (reps, total)
}

/// The optimal schedule for every context.
#[must_use]
pub fn solve(model: &CostModel, w: &Workload) -> Schedule {
    w.contexts
        .iter()
        .map(|c| solve_context(model, c).0)
        .collect()
}
