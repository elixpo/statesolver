# MorphStack (archived 2026-11-04)

This repository started as **MorphStack**: online adaptive representation of activation state, choosing between contiguous stacks, segmented stacks, and heap frames, and evaluated against a hindsight oracle. On 2026-11-04 the project pivoted to **StateSolver** (see [../../research-direction.md](../../research-direction.md)).

- **Full code and results:** tag [`morphstack-final`](https://github.com/elixpo/statesolver/tree/morphstack-final) and branch `archive/morphstack`
- **Research log:** [DISCOVERIES.md](DISCOVERIES.md)
- **Open work at archive time:** [TODO.md](TODO.md)

## What was built

- Trace format v0, with JSONL and varint-binary encodings, a streaming reader, a 10-rule validator, and statistics
- Eleven seeded workload generators, deterministic across platforms (libm-free RNG, digests pinned in CI)
- A cost model, simulator, an exact per-context Viterbi oracle, and baselines A–F
- `calibrate`, with measured page-fault, copy, mmap, allocator, and segment-switch costs
- Real-trace capture from CPython 3.12+ via `sys.monitoring`
- A literature review: 53 works, provisional GO

## Main findings

1. Each fixed representation wins some workload.
2. On synthetic workloads with calibrated costs, **switching over time was worth 0%**. A segment switch costs 0.93 ns, and segments are cheap. Per-context heterogeneity was worth 5–29%.
3. Finer granularity (per activation) could add at most 0.1% (ideal-bound analysis).
4. On a **real aiohttp trace**, switching was worth **9% at cloud memory prices and 42% when memory-bound**: run on segmented stacks and compact parked keep-alive connections into heap frames.
5. Model lesson: the oracle exploits every free lunch in a cost model. Unexpected switching should be treated as a model bug first.

The real-trace result (4) is a credible standalone short paper or workshop paper if it is ever revisited.

## Carried into StateSolver

- Workspace layout, lint policy (`unsafe_code` forbidden except in named crates), and CI on three operating systems
- The trace-format pattern: versioned, a JSONL format for tests, compact binary for scale, streaming readers, a semantic validator, and property tests that check truncated and garbage input never cause a panic
- The habit of keeping `TODO.md` and `DISCOVERIES.md` up to date
- The literature-review protocol and verdict format
- The `sys.monitoring` capture experience, and the principle of decoupling capture from analysis
- The Crumb integration plan: an optional `crumb-harness` agent crate that is never needed for results
