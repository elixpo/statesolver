# Stage 2 — rr backend: record wrapper, replay driver, historical state, on-demand allocation epochs

**Labels:** `tracking`, `stage-2`, `replay`, `recorder`

Direction: [docs/DIRECTION.md](../docs/DIRECTION.md) §1. rr records and replays. StateSolver drives the replay and derives everything else on demand.

## Done before the lock (kept as fixtures)
- [x] Our own ptrace recorder (`statesolver-record`) with syscall output models, vDSO patching, TSC trapping, and corpus tests. It is now a **test fixture and reference** only: about 50 µs per allocation against rr's near-native speed.
- [x] Feasibility: rr 5.9 records and replays corpus bug 001 on this hybrid i5-13450HX. Reverse-continue with a watchpoint finds the stale `memcpy` writer.

## Scope
- [ ] `statesolver record`: wraps `rr record` (trace directory management, `--stdin`, environment capture, and checks for the PMU and `perf_event_paranoid`).
- [ ] Replay driver in Rust: start `rr replay -s <port>` and speak the **gdb remote serial protocol**. Supports continue and reverse-continue, step, breakpoints, hardware watchpoints, reading registers and memory, and the event and tick position (`when`).
- [ ] Positions: map rr event numbers and ticks to StateSolver event IDs, and make "go to E" deterministic.
- [ ] **On-demand allocation epochs:** in one replay pass, breakpoint the allocator (the same logic as the fixture recorder, but at replay time) and build the temporal heap from it. Cache it per trace.
- [ ] Historical state queries: `regs`, `memory`, `maps`, `stack` at an event.
- [ ] Cross-check: the epochs derived from rr replay of the corpus match the fixture recorder's `.ss` traces.

## Exit criteria
- `statesolver lifetime` and `origin` answer correctly from rr traces of bugs 001–003, tested in CI on a runner with PMU access (local or self-hosted; GitHub-hosted runners may lack a vPMU, so document it).
