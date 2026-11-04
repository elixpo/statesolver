# Stage 3 — Online policies, regret analysis, ablations, and adversarial evaluation

**Labels:** `tracking`, `stage-3`, `research`, `policy`

## Outcome

Build online policies that cannot see the future, measure how far they fall behind the oracle, and give a theoretical result to go with the empirical one. This stage produces the paper's core claim.

## Scope

### Policies (`morphstack-policy`)
All policies implement a single `Policy` trait with `observe(event)` and `decide(ctx) -> Option<Migration>`. Each decision must be O(1) amortized, because the same code will run inside the Stage 4 runtime.

- [ ] **P1 Threshold + hysteresis** (plan section 12): switch only when expected savings exceed the migration cost times a safety factor.
- [ ] **P2 Rent-or-buy / work-function style**: the classical MTS approach. Stay in the current representation until the cost already paid there exceeds the cost of moving. This is the policy with a known competitive ratio, so the theory section can rely on it.
- [ ] **P3 Lazy experts with switching cost**: Hedge or Follow-the-Lazy-Leader over the three representations, using the simulator's counterfactual cost for each representation as expert losses.
- [ ] **P4 Contextual bandit** (plan section 13): LinUCB or Thompson sampling over the runtime features in plan section 7 (RQ4), with migration cost added to the loss.
- [ ] **P5 (optional) Lifetime predictor** (plan section 14): logistic regression or a small decision tree. It must be trained offline and add zero allocations on the hot path.

### Theory
- [ ] Formalize the problem as an **asymmetric metrical task system** over the representation set R, with per-activation instances.
- [ ] Prove or adapt a competitive-ratio bound for P2. Separately, state a dynamic-regret bound with a switching-cost term for P3, citing the online-learning-with-switching-cost literature in plan section 4.
- [ ] Answer **RQ5 (granularity)** formally: what changes when decisions are made per context, per segment, or per activation group.

### Evaluation (generated from `morphstack-sim`, never edited by hand)
- [ ] Headline metric: normalized regret `(C_alg − C_oracle) / C_oracle` for each workload and policy. Plan target: roughly 5–15% on mixed workloads. Treat that as a target, not a promise.
- [ ] Worst-case comparison: for each policy, its regret on the workload where it does worst, compared with each fixed policy's regret on the workload where *that* policy does worst.
- [ ] **RQ3**: a sweep that scales migration cost from 0.1× to 100× and marks where adapting stops paying off.
- [ ] **RQ6 stability**: count oscillations, and plot migration frequency against threshold (paper graph 4).
- [ ] **Ablations** (plan section 22): remove one feature at a time and turn hysteresis off.
- [ ] **Adversarial suite** (plan section 23): report how far the empirical worst case is from the theoretical bound.
- [ ] Paper graphs 1, 3, and 5 (throughput by workload, adaptive cost divided by oracle cost, and representation over time) produced by `morphstack report`.

## Exit criteria
- `results/stage3/` is reproducible with one command: `morphstack report --all`.
- The theory write-up exists in `paper/sections/formulation.tex`.
- A policy is chosen to implement in the real runtime in Stage 4, and the choice is justified with data.
