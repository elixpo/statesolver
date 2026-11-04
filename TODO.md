# StateSolver TODO

The single list of open work. Tracking issues: #1 to #5. Update this file in the same commit as the work.
Findings go in [DISCOVERIES.md](DISCOVERIES.md).

## Stage 1: Foundations, literature review, trace format (#1). In progress

- [x] Pivot from MorphStack: archived under the tag `morphstack-final`, the branch `archive/morphstack`, and [docs/archive/morphstack](docs/archive/morphstack/README.md)
- [x] Research direction locked: [docs/research-direction.md](docs/research-direction.md)
- [x] Crate skeleton and CI carried over
- [ ] Literature review scope: must cover allocation-identity temporal safety (CETS/lock-and-key), Undangle, Aftersight/PANDA/Arnold (analysis via replay), REPT/ARCUS (crash RCA), demand-driven slicing, and Pernosco's dataflow
- [x] `.ss` trace format v0: [spec](docs/trace-format.md), JSONL and zstd containers, 9-rule validator, property tests
- [x] Temporal heap (`statesolver-analysis::heap`), plus `statesolver allocations` and `statesolver lifetime` working on traces
- [x] `bug-corpus/`: shared `ss_mark` markers and build script; 001 UAF with reuse (confirmed address reuse), 002 double free, 003 heap overflow driven by input. Each has a written ground truth.
- [x] Literature review: narrowed GO ([docs/slr/verdict.md](docs/slr/verdict.md)); claim wording locked in DISCOVERIES
- [ ] Human re-run of the review in Google Scholar and ACM DL, with forward citations of Undangle, FreeWill, Arnold, DrDebug (moved to #5, paper stage)
- [ ] Pernosco reuse check: run corpus bug 001 under Pernosco if access is possible

## Stage 2: rr backend (#2). In progress

- [x] Reference ptrace recorder (now a fixture): syscall models, vDSO patching, TSC trapping, refusal of threads and forks, corpus tests
- [x] Overhead measured: about 50 µs per allocation against rr at 0.04 s per 100k, so rr becomes the backend
- [x] rr feasibility: bug 001 records and replays; reverse-continue with a watchpoint finds the stale writer
- [x] `statesolver record` as an rr wrapper (`-o` trace directory, `--stdin`, `perf_event_paranoid` warning); the old recorder is reachable as `--fixture`
- [x] Replay driver (`statesolver-replay`): RSP client in no-ack mode; continue, reverse-continue, step, reverse-step; software breakpoints and hardware write watchpoints; registers, memory, `when`; replay-process maps
- [ ] Map rr positions to event IDs
- [x] On-demand allocation epochs: `statesolver index` replays once (about 0.12 s per corpus bug), caches `statesolver-index.ss` in the trace directory, and every event carries `rr_event`. Matches the fixture recorder on all 3 bugs; rr corpus tests are in the suite
- [ ] Index cost on allocation-heavy programs (each allocator call is a replay breakpoint stop)
- [ ] Historical state: regs, memory, maps, stack at an event
- [ ] CI runner with PMU access (GitHub-hosted runners may lack a vPMU)

## Stage 3: Causal query engine (#3)

- [ ] Instruction semantics (`iced-x86`); producer search through reverse execution, watchpoints, and stepping
- [ ] Temporal object graph; address-only switch (arm A5)
- [ ] `why`, `origin`, `history`, `lifetime`, `explain-crash`; evidence IDs stored with replay positions
- [ ] Ground-truth assertions from `ROOT_CAUSE.md`; grow the corpus with real reuse bugs

## Stage 4: Study instrument and pilot (#4)

- [ ] `statesolver mcp`; FACT/INFERENCE/HYPOTHESIS model; claim-citation protocol in every arm
- [ ] Arms A0–A5 with matched budgets; Crumb runner plus one other harness; 3 or more models
- [ ] SEC-bench, ARVO/T2L-ARVO, and VulnLoc loaders; rr recording of each; annotation tooling with κ
- [ ] Arm-blind claim checker
- [ ] **Pilot**: about 20 SEC-bench bugs, A1 against A4 (plus A5), with a go/adapt decision

## Stage 5: Full study, release, paper (#5)

- [ ] Full runs, strata, cost and Pareto analysis
- [ ] Release: Debian package, install script, Crumb marketplace package; elixpo.com and research.elixpo.com listings
- [ ] Optional visual UI
- [ ] Paper and artifact; arXiv cs.SE (cross-listed cs.AI, cs.CR); the maintainer's Scholar check (papers citing WATCHER, Ravel, DebugHarness)
