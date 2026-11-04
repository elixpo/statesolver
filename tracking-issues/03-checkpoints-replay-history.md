# Stage 3 — Checkpoints, deterministic replay, and historical state queries

**Labels:** `tracking`, `stage-3`, `replay`

## Outcome

Any recorded event can be reached again exactly, and the state at that event (registers, memory, maps, stack, allocations) can be queried.

## Scope
- [ ] Periodic checkpoints, by `fork()` of the stopped tracee or by snapshotting registers, maps, and dirty pages.
- [ ] A deterministic runner: restore a checkpoint, inject recorded syscall results, and stop at an exact event. Use instruction or branch counting for precise positioning.
- [ ] `statesolver rewind <event>`, `replay <a>..<b>`, and `state --event`.
- [ ] Historical queries: `regs`, `memory <addr>`, `maps`, `stack` (unwinding through frame pointers or DWARF CFI), `allocations`, and `lifetime <addr>`.
- [ ] History queries: `history <reg>` and `history <addr>`, listing value changes with their producing events.
- [ ] Replay fidelity tests: replayed state at every checkpoint matches the recorded state, and is cross-checked against rr where it is available.

## Exit criteria
- 100% replay fidelity on the bug corpus.
- Reaching any event takes time bounded by the checkpoint interval (report the measured trade-off between trace size and rewind latency).
