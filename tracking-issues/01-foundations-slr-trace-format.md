# Stage 1 — Foundations, literature review and novelty gate, `.ss` trace format v0

**Labels:** `tracking`, `stage-1`, `research`, `infra`

## Outcome

A StateSolver workspace that meets Crumb's quality bar, a **systematic literature review** that decides whether the narrow claim in [docs/research-direction.md](../docs/research-direction.md) §11 holds, and trace format v0, which every later stage writes or reads.

## Scope

### Repository
- [x] Pivot from MorphStack. The old work is preserved under the tag `morphstack-final` and the branch `archive/morphstack`. The workspace, lint policy, CI, and governance files carry over.
- [ ] Crate skeleton: `trace`, `record`, `replay`, `analysis`, `harness`, and `cli`. Only `record` (and later `replay`) may relax `unsafe_code`, with `// SAFETY:` on every block.
- [ ] `bug-corpus/` layout: one directory per bug, holding the source, a build script, the expected root cause, and ground-truth event markers.

### Literature review (the gate)
The brief already rules out the obvious prior art: omniscient debugging, rr, dynamic slicing, SPYDER, DrDebug, and Pernosco. These areas are **close to the claim** and must also be covered:
- **Allocation-identity temporal safety:** CETS and lock-and-key identifiers (unique key per allocation), MarkUs, DangSan, pointer provenance in CHERI. "Address + lifetime as identity" exists here; our novelty can only be its use in *causal explanation*.
- **Dangling-pointer provenance tracking:** Undangle (taint-propagating pointers to their allocation), HOTracer, and UAF root-cause tools.
- **Analysis decoupled from recording through replay:** Aftersight, PANDA (record, then run heavy analyses at replay), Arnold and eidetic systems (retroactive provenance queries via replay), JetStream, and Tralfamadore.
- **Crash root-cause analysis from traces:** REPT (Intel PT plus reverse execution), ARCUS, AURORA, POMP.
- **Query-driven and demand-driven slicing:** demand-driven dataflow, lazy or partial dynamic dependence graphs, and slicing over checkpoints.
- **Pernosco's dataflow view:** exactly what it reconstructs, and whether it knows about allocations.

Deliverables, using MorphStack's protocol format:
- [ ] `docs/slr/protocol.md` with queries, databases, and dates
- [ ] `docs/slr/matrix.md` with columns: Native binaries | Deterministic replay | On-demand dependencies | Allocation-epoch identity | Crosses free/reuse | Syscall input as source | Causal explanation output | Evaluated on UAF with reuse
- [ ] `docs/slr/verdict.md`: GO, PIVOT, or STOP, with the exact novelty sentence

### Trace format `.ss` v0
- [ ] Spec in `docs/trace-format.md`. It covers:
  - a global event ordinal;
  - instruction-count / branch-count position;
  - register deltas;
  - memory-map changes;
  - syscalls with result payloads (for replay);
  - signals;
  - allocation events with `{id, address, size, epoch, alloc_event, free_event}`;
  - a checkpoint index.
- [ ] A JSONL encoding for tests and a compact binary encoding for scale, using MorphStack's streaming-reader, validator, and property-test pattern.
- [ ] Validator rules: event ordinals strictly increase, every free matches a live allocation, epochs increase monotonically per address, and no two live allocations overlap.

## Exit criteria
- CI is green on Linux, macOS, and Windows. Analysis and trace crates build everywhere; the recorder is Linux-only.
- The literature-review verdict and the trace-format spec are merged, and the validator has tests.

## Go/no-go gate
If prior work already performs on-demand, allocation-epoch-aware causal slicing over replay, narrow the claim before Stage 4.
