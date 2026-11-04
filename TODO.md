# StateSolver TODO

The single list of open work. Tracking issues: #1 to #5. Update this file in the same commit as the work.
Findings go in [DISCOVERIES.md](DISCOVERIES.md).

## Stage 1: Foundations, literature review, trace format (#1). In progress

- [x] Pivot from MorphStack: archived under the tag `morphstack-final`, the branch `archive/morphstack`, and [docs/archive/morphstack](docs/archive/morphstack/README.md)
- [x] Research direction locked: [docs/research-direction.md](docs/research-direction.md)
- [x] Crate skeleton and CI carried over
- [ ] Literature review. Must cover allocation-identity temporal safety (CETS/lock-and-key), Undangle, Aftersight/PANDA/Arnold (analysis via replay), REPT/ARCUS (crash RCA), demand-driven slicing, and Pernosco's dataflow
- [ ] `.ss` trace format v0: spec, JSONL and binary encodings, validator, property tests
- [ ] `bug-corpus/` layout and the first 3 bugs (UAF with address reuse, double free, heap overflow)

## Stage 2: Recording backend (#2)

- [ ] Decide early between our own recorder and rr as the substrate. Recommended: our own single-threaded recorder, with rr as a correctness oracle
- [ ] Environment for rr cross-checks: `apt install rr`, `kernel.perf_event_paranoid ≤ 1` (currently 4), P-core pinning (hybrid i5-13450HX)
- [ ] ptrace process control, register deltas, maps, syscall results, allocation epochs, timeline

## Stage 3: Checkpoints and replay (#3)

- [ ] Checkpoints, deterministic runner, rewind, and historical state and history queries; replay-fidelity tests

## Stage 4: Causal reconstruction and evaluation (#4)

- [ ] Instruction semantics (`iced-x86`), on-demand producer search, temporal object graph, `why`/`origin`/`lifetime`/`explain-crash`
- [ ] Address-only and eager-DDG baselines; bug corpus including real bugs; metrics; ablations

## Stage 5: Harness, UI, release, paper (#5)

- [ ] Tool API, sessions, FACT/INFERENCE/HYPOTHESIS evidence model, JSON-RPC, then MCP
- [ ] Crumb agent via `crumb-harness` (feature-gated), and a Crumb marketplace package
- [ ] Visual UI over the analysis API
- [ ] Release pipeline; elixpo.com and research.elixpo.com listings
- [ ] Paper last: Scholar and ACM DL re-run, artifact, arXiv (needs an endorsement)
