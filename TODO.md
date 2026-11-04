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
- [ ] **Calibrate the memory price**: this decides between the switching regime and the heterogeneity regime (see DISCOVERIES)
- [ ] Per-context timeline output (`sim --timeline`) for paper graph 5
- [ ] Calibration microbenchmarks that measure each cost parameter on x86-64 (and on aarch64 macOS)
- [ ] Real traces: a Tokio HTTP server under load, Go `runtime/trace`, and possibly Python asyncio, each with a converter
- [ ] Go/no-go criterion 2: the oracle beats the best fixed policy by more than 10% on at least one **real** trace
- [ ] `docs/simulator.md` covering model assumptions, decision granularity, and the separability condition for oracle exactness
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
