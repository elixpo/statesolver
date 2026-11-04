# Stage 3 — Causal query engine: why, origin, history, explain-crash with evidence IDs

**Labels:** `tracking`, `stage-3`, `analysis`, `research`

Direction: [docs/DIRECTION.md](../docs/DIRECTION.md) §1 (engine) and §2 (arms A4 and A5).

## Scope
- [ ] Instruction semantics: `iced-x86` decoding and register and memory read/write sets, including partial registers, flags, and `rep movs`/SIMD copies.
- [ ] **Producer search:** for a value at an event, reverse-execute to its producer (a hardware watchpoint for memory, single-step for registers), then recurse on the producer's inputs. Stop at syscall input, constants, allocation returns, or a depth limit.
- [ ] **Temporal object graph:** resolve every memory location to `(allocation, epoch)` at that event; pointer provenance carries the epoch.
- [ ] **Address-only mode**, a switch that gives arm A5 / WATCHER-style dependence on the *same* engine.
- [ ] Commands: `why`, `origin`, `history`, `lifetime`, `explain-crash` (bug classification plus evidence chain).
- [ ] **Evidence IDs:** every answer is stored with an ID and the replay position it came from, so any cited claim can be re-checked automatically.
- [ ] Ground-truth tests: each corpus bug's `ROOT_CAUSE.md` is turned into an assertion on `explain-crash`'s output.
- [ ] Grow the corpus beyond 3, including real reuse bugs (FreeWill's reallocation cases, SEC-bench UAFs).

## Exit criteria
- `explain-crash` finds the annotated root cause for every corpus bug. Address-only mode reproduces the misleading chain on the reuse bugs (001), and the difference is a CI test.
