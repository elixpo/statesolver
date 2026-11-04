# Simulator and cost model

This document describes what `morphstack-sim` and `morphstack-cost` compute, the assumptions behind them, and when the oracle is exact. Read it before you interpret any number in `results/`.

## Decision granularity

The simulator makes one decision per **context**. Before each step of a context, a policy chooses the representation that holds the context's whole activation stack: contiguous, segmented, or heap. A switch between steps is a migration.

Finer granularity (per activation, per segment, or per group of activations) is research question RQ5 and is Stage 3 work.

## Steps

A *step* is one trace event (`spawn`, `call`, `return`, `suspend`, `resume`, or `exit`) plus the time `dt` until the same context's next event. The state after the step is held for `dt`.

The cost of a step depends only on:
- the step's kind,
- the live bytes before and after it,
- the live frames after it,
- the context's high-water mark (the most live bytes it has ever held),
- `dt`.

All of these come from the trace, never from earlier representation choices. That is what keeps the oracle exact.

## Representations

| | Footprint | Per-call cost | Growth and boundary cost |
|---|---|---|---|
| Contiguous | `next_pow2(max(hwm, min_stack))` | call + check | first touch of new depth; doubling copies the live bytes into fresh pages |
| Segmented | `ceil(live / segment) × (segment + header)` | call + check | allocate or free a segment, plus a boundary cost, every time a call or return crosses a segment boundary (the hot-split problem) |
| Heap | `live + frames × header + context descriptor` | call + allocate + locality penalty | free on return |

Contiguous stacks never shrink. This is a deliberate, conservative assumption. Real runtimes shrink stacks during garbage collection, and modelling that is TODO work.

## Migration

```text
migrate(from, to) = release(from) + acquire(to)
                  + migration_scale × (fixed + ns_per_byte × live + ns_per_frame × frames)
```

Migrating into contiguous acquires a stack sized to the high-water mark and pre-faults it. The cost of releasing storage at `exit` uses the same `release()` function, so a migration made just before exit can never be cheaper than exiting without it. A regression test enforces this.

## Memory price

Footprint is converted into CPU-equivalent time at `memory.ns_per_byte_second`. This is an **economic parameter, not a hardware measurement**. It encodes how much a byte held for one second is worth compared with one nanosecond of CPU.

| Regime | Value | Meaning |
|---|---|---|
| Cloud cost (default) | 0.125 | Typical on-demand price ratio of 1 GB RAM to one vCPU, about 1:8. Approximate; verify against current list prices before publication. |
| Memory-bound | 10–100× | A fixed RAM budget, such as millions of connections on one host, where footprint decides capacity |
| CPU-bound | 0.01–0.1× | Memory is plentiful and latency matters |

Results must always state their regime. `morphstack sweep` covers all three.

## Oracle exactness

The oracle in `oracle.rs` runs a per-context Viterbi dynamic program in `O(T·|R|²)` time. It is the **exact** hindsight optimum for the whole trace, because the total cost separates into a sum of per-context costs. Memory is priced linearly, and the model has no shared non-linear term. If a shared memory-pressure term is added later (a convex function of total footprint), the per-context oracle becomes a lower bound, and the coupled problem needs a Lagrangian or iterative method. That is listed in TODO.

## Baselines

| | Policy |
|---|---|
| A, B, C | always contiguous, always segmented, always heap |
| D | heap if the context's entry function can suspend, otherwise contiguous |
| F | **hindsight static**: each context gets the single representation that is cheapest for it over its life |
| E | the oracle |

The gain from the best of A, B, or C to F is the value of **heterogeneity**. The gain from F to E is the value of **switching over time**. Report both separately.

## Calibration

`morphstack calibrate` measures, in safe Rust:
- first-touch cost,
- copy bandwidth,
- stack `mmap` and `munmap`,
- allocator costs for segments and frames.

These need the Stage 4 runtime and are left at their defaults: call and return overhead, split checks, **segment boundary crossing**, heap locality, and migration fix-up. Each calibrated profile records which values were measured.

**Sensitivity warning.** On the calibrated x86-64 profile, whether switching over time is worth anything at all depends on `segmented.boundary_ns`. Below about 100 ns it is worth about 0%. At 150 ns or more it is worth 14–15% on W2 and W6. See `DISCOVERIES.md`. Until the runtime measures this value, no claim about switching gains is supported.
