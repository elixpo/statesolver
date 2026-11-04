# Stage 2 — Workloads, cost model, simulator, and the hindsight oracle

**Labels:** `tracking`, `stage-2`, `research`, `simulator`

## Outcome

Build a deterministic, trace-driven simulator that prices any representation policy on any trace, and an **exact offline oracle**. Together they answer the second research gate: *does the best representation actually change within realistic workloads?*

## Scope

### Workload generators (`morphstack-workloads`)
- [ ] Seeded, reproducible generators for W1–W6 from plan section 19:
  - W1 nested calls
  - W2 deep recursion
  - W3 100k–1M shallow suspended contexts
  - W4 frequent suspend/resume
  - W5 long suspensions
  - W6 phase-changing A→B→C
- [ ] Adversarial generators from plan section 23: alternating short and long suspensions, and behaviour that flips immediately after a migration.
- [ ] Workload classes from plan section 9 that are not yet covered: synchronous server, burst concurrency, and mixed CPU + I/O.
- [ ] `morphstack gen <workload> --seed N --out trace.msb`

### Real traces (needed for the paper; synthetic traces alone will not convince reviewers)
- [ ] Capture at least two real sources. Candidates:
  - Instrumented Tokio tasks, for example from a small HTTP server under `wrk` load.
  - Go runtime traces (`runtime/trace` gives goroutine create, block, and unblock events).
  - Python asyncio via `sys.monitoring`.
- [ ] Converters from each source to trace format v0.

### Cost model (`morphstack-cost`)
- [ ] A parametric model for each representation covering allocation, call/return, growth or copy, pointer fixup, cache/locality penalty, resume cost, and bytes per live activation.
- [ ] An asymmetric migration-cost matrix `M(r, r')`.
- [ ] **Calibration microbenchmarks** that measure the parameters on real hardware (x86-64, plus aarch64 on a Mac), so the model is not made of guessed numbers. Store each machine's calibration as a TOML profile.
- [ ] A sensitivity analysis showing which conclusions stay true when each parameter changes by ±2×.

### Simulator and baselines (`morphstack-sim`)
- [ ] Fixed baselines A–D from plan section 11: always contiguous, always segmented, always heap, and the static `can_suspend` heuristic.
- [ ] Output for each run: total cost, cost broken down by component, peak memory, migration count and bytes, and a representation-over-time series.
- [ ] `morphstack sim --policy <p> --cost-profile <machine.toml> trace.msb`

### Oracle
- [ ] Exact oracle for each context: a Viterbi-style dynamic program over (time, representation) with transition cost `M`, running in O(T·|R|²) time.
- [ ] Document the **coupling problem**. Memory pressure and cache effects are shared across contexts, so solving each context independently is only exact when costs are separable. Report the per-context oracle as a **lower bound** and add a Lagrangian or iterative refinement for the coupled case.
- [ ] Property tests:
  - The oracle's cost is ≤ every fixed policy's cost on every trace.
  - The oracle with `M = ∞` equals the best fixed policy.

## Exit criteria
- The simulator and oracle are deterministic: the same seed and profile always produce identical output, and CI checks this.
- `results/stage2/` contains the workload-characterization report: the winning fixed policy for each workload, how many oracle switches occur per trace, and the gap between the oracle and the best fixed policy.

## Go/no-go gate (plan section 24)
Stage 3 is worth doing only if:
1. no fixed policy wins every workload, and
2. the gap between the oracle and the best fixed policy is meaningful (a working threshold is >10%) on at least one **real** trace.

If that gap is near zero, write up the negative result described in plan section 25. That is still a paper.
