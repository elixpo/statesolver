# MorphStack TODO

The single list of open work. Tracking issues: #1 to #5. Update this file in the same commit as the work.
Findings go in [DISCOVERIES.md](DISCOVERIES.md), not here.

## Stage 1: Foundations (#1). Done, waiting to be closed

- [x] Workspace, lint policy, CI on Linux, macOS, and Windows, governance files
- [x] Trace format v0, plus `trace validate`, `trace stats`, and `trace convert`
- [x] Literature review: 53 works, provisional GO verdict ([docs/slr/verdict.md](docs/slr/verdict.md))
- [x] Oracle patent-claims check (cleared)
- [ ] Close #1 (maintainer)

## Stage 2: Workloads, cost model, simulator, oracle (#2). In progress

- [x] Eleven seeded generators with cross-platform determinism pinned in CI
- [x] Cost model with release, acquire, copy, and fix-up migration, and first-touch cost
- [x] Simulator and baselines A to D, plus F (per-context static choice in hindsight)
- [x] Exact per-context oracle, with property tests
- [x] `morphstack sim | characterize | profile` and the first report ([results/stage2](results/stage2))
- [x] Sensitivity sweep over memory price and migration-cost scale (`morphstack sweep`, [results/stage2/sweep.md](results/stage2/sweep.md))
- [x] `morphstack calibrate`: measured first touch, copy, mmap/munmap, and segment/frame malloc/free ([results/profiles](results/profiles))
- [x] Memory price documented as an economic regime, not a measurement ([docs/simulator.md](docs/simulator.md))
- [x] Measured `segmented.boundary_ns` = 0.93 ns with safe `corosensei::on_stack` (threshold was about 100–120 ns), so the switching gain is 0% on every workload
- [ ] **Research-direction decision**: test first, then decide (chosen 2026-11-04)
  - [x] Granularity: ideal and free-migration bounds show finer granularity is worth at most 0.1%
  - [x] Headroom is blocked by acquire and release, and in the end by page faults for fresh stacks; warm pools revive switching (W2 15%, W6 8%)
  - [ ] Model a stack pool explicitly, with resident pool memory priced
  - [ ] A real trace (Python 3.12 `sys.monitoring` asyncio capture: exact per-coroutine call, return, yield, and resume)
- [ ] Per-context timeline output (`sim --timeline`) for paper graph 5
- [ ] Calibrate on aarch64 macOS as a second machine
- [ ] Real traces: Python asyncio via `sys.monitoring` first. Tokio and Go traces do not record call depth (Rust tasks are state machines; Go's trace has no stack depth).
- [ ] Go/no-go criterion 2: the oracle beats the best fixed policy by more than 10% on at least one **real** trace
- [x] `docs/simulator.md` covering model assumptions, decision granularity, the separability condition for oracle exactness, and memory regimes
- [ ] A finer static baseline D. It currently equals always-heap on every workload.
- [ ] Shared memory-pressure term (non-linear) and an oracle lower bound under coupling

## Stage 3: Online policies (#3)

- [ ] `Policy` trait over causal observations, which never see `dt_ns`
- [ ] Policies must defer migration to stack low points (DISCOVERIES, 2026-11-04)
- [ ] P1 threshold with hysteresis; P2 rent-or-buy / work function; P3 lazy experts; P4 contextual bandit; P5 optional predictor
- [ ] Report regret against the oracle **and** against F, keeping heterogeneity and switching separate
- [ ] Theory: asymmetric MTS formulation and a competitive bound for P2
- [ ] RQ5 granularity (per activation and per segment), RQ6 oscillation, ablations, adversarial suite

## Stage 4: Runtime (#4)

- [ ] Coroutine runtime with three representations and live migration (x86-64 Linux first)
- [ ] Frame descriptors, migration verifier, differential tests, fuzzing, Miri
- [ ] Benchmarks with hardware counters; validate the simulator's predictions against measured runs

## Stage 5: Release, Crumb integration, paper (#5). The paper comes last

- [ ] `morphstack-agent` built on `crumb-harness` (explain, literature watch, experiment runner)
- [ ] Crumb marketplace package
- [ ] Release pipeline: install.sh, install.ps1, Homebrew, Debian
- [ ] Publish `morphstack-trace` to crates.io once the format is stable (no placeholder crates)
- [ ] Web presence: `morphstack.elixpo.com`, or an elixpo.com / research.elixpo.com directory entry
- [ ] Native Google Scholar and ACM DL re-run of the literature review, which turns GO into final GO or PIVOT
- [ ] Paper, artifact, and arXiv submission (cs.PL; cross-list cs.OS and cs.PF), which needs an endorsement
