# Stage 4 — Query-driven causal reconstruction with allocation epochs, and the evaluation

**Labels:** `tracking`, `stage-4`, `analysis`, `research`

## Outcome

The research core: `why`, `origin`, `lifetime`, and `explain-crash` produce **allocation-epoch-aware causal slices**, reconstructed **on demand through replay**, and evaluated against address-only tracing.

## Scope
- [ ] Instruction semantics: decode with `iced-x86` and compute the register and memory read/write sets per instruction, including partial-register and flag semantics.
- [ ] **Query-driven reconstruction:** for a target (register or memory at an event), replay the minimal interval with instrumentation, find the producer, then recurse on the producer's inputs. Stop at constants, syscall input, allocation returns, or user-chosen boundaries.
- [ ] **Temporal object graph:** resolve every memory access to `ObjectID {range, alloc_event, free_event, epoch}`, so that pointer provenance follows `Allocation #41 / epoch 3`, not just `0x55550000`.
- [ ] Commands: `why <reg|addr> --event`, `origin <pointer>`, and `lifetime <addr>`.
- [ ] `explain-crash`: classify the failure (UAF with reuse, double free, overflow, corrupted function pointer, and so on) and print an evidence chain with event ids.
- [ ] Baseline A: **address-only** dependence on the *same* replay engine (an ablation that isolates the epoch effect).
- [ ] Baseline B: an **eager** full dynamic dependence graph or write index (Pernosco-like). Report the break-even point between storage and recording overhead and query latency as the number of queries per trace grows.
- [ ] Baseline C: **Undangle-style** forward pointer-label propagation over a full trace. Show the queries it cannot answer (stale writes into non-pointer data, input chains that cross a `realloc`).

## Evaluation
- [ ] Bug corpus (research direction §9): UAF, UAF with address reuse, stale-pointer writes, double free, heap overflow, wrong pointer propagation, corrupted function pointer, and corruption caused by external input. Include **real bugs** too: FreeWill's reallocation cases, SEC-bench, Magma, and CVE reproductions. Report the share of the corpus where reuse actually changes the slice.
- [ ] Metrics: recording overhead, trace size, per-query reconstruction time, candidate causal events, slice size, crash-to-root-cause distance, localization accuracy, and the **Root Cause Reduction Ratio**.
- [ ] Ablations: no epochs (address-only), no on-demand replay (eager DDG), and different checkpoint intervals.

- [ ] Custom allocators: state how epochs are derived for pools and arenas (a hook API or a symbol list).

## Claim (from the literature review, [docs/slr/verdict.md](../docs/slr/verdict.md))
Answer backward causal queries about arbitrary machine values by reconstructing only the queried dependences through checkpointed replay, resolving heap dependences by (address, epoch). Do not claim epoch identity, on-demand slicing, or replay-time provenance individually.

## Exit criteria
- Every number in the evaluation is produced by one command (`statesolver bench --all`).
- The hypothesis in research direction §10 is supported or refuted with data. Either outcome gets written up.
