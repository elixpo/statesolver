# Stage 2 — Recording backend: process control, registers, maps, syscalls, allocations

**Labels:** `tracking`, `stage-2`, `recorder`

## Outcome

`statesolver record ./program` runs an unmodified single-threaded x86-64 ELF program and writes a `.ss` trace containing everything needed to replay it deterministically and to identify allocation lifetimes.

## Scope
- [ ] Process control: launch and attach with `ptrace`, `waitpid` and the signal loop, and `/proc/<pid>/maps` and `mem`.
- [ ] Register capture: full snapshot at start, deltas afterwards. Instruction stepping for targeted regions.
- [ ] ELF and symbol loading. DWARF (via `gimli`) is optional at this stage.
- [ ] Memory-map tracking for `mmap`, `munmap`, `mprotect`, and `brk`.
- [ ] **Syscall recording**, with result payloads for every non-deterministic syscall (`read`, `recv`, `clock_gettime`, `getrandom`, and others) so that replay can inject them. Instructions such as `rdtsc` and `rdrand` need trapping or masking.
- [ ] **Allocation lifetime tracking** for malloc, calloc, realloc, free, and mmap-backed allocations. Evaluate two approaches: an `LD_PRELOAD` shim, or uprobe-style breakpoints on the allocator. Each allocation gets an id and a **per-address epoch**.
- [ ] A global event timeline, written in `.ss` v0.

## Decision to make early: our own recorder, or rr as the substrate
- **Own recorder** (single-threaded, ptrace syscall interception plus result injection). We control the format and the allocation hooks, and it is tractable for single-threaded programs. Risk: subtle non-determinism.
- **rr as the substrate.** rr is mature, MIT-licensed, and handles signals and threads. Costs: it needs `perf_event_paranoid ≤ 1` (this machine is at 4) and P-core pinning on this hybrid CPU, and our query layer would drive rr's replay.
- **Recommendation:** build our own minimal recorder for the single-threaded MVP, and use **rr as a correctness oracle** (compare replayed state at checkpoints). Revisit before multithreading.

## Exit criteria
- Records every bug-corpus program, with less than 20× slowdown target on the corpus.
- The trace validates, and every allocation and free has an id and an epoch.
