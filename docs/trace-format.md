# StateSolver trace format (`.ss`), version 0

**Status:** v0, draft. The format may change until Stage 3 (#3) closes, and every change bumps `version`. The reference implementation is [`crates/statesolver-trace`](../crates/statesolver-trace).

A trace records **facts** about one execution: the facts needed to replay it deterministically, and the allocation lifetimes needed to give heap objects an identity. Instruction-level dependencies are deliberately **not** recorded. Stage 4 reconstructs them on demand by replaying, and that choice is the research claim.

## Containers

| Container | Extension | Content |
|---|---|---|
| JSONL | `.jsonl` | One JSON object per line: the header first, then events |
| `.ss` | `.ss` | The same JSONL stream, zstd-compressed (`zstdcat t.ss \| head`) |

Readers detect the container from the zstd magic bytes. Lines are limited to 64 MiB. Byte strings are lowercase hex.

## Header

| Field | Meaning |
|---|---|
| `format` | always `"statesolver-trace"` |
| `version` | `0` |
| `arch`, `os` | `"x86_64"`, `"linux"` |
| `recorder` | producer, for example `statesolver-record 0.1.0` |
| `program` | `{path, argv, sha256?}` |
| `description` | optional notes (kernel, environment) |

## Events

Every event has an `id` (0, 1, 2, … with no gaps, written `E<id>`) and an `ev` tag.

| `ev` | Fields | Purpose |
|---|---|---|
| `regs` | `full`, `values: [[reg, u64]]` | Register snapshot (`full: true`), or the registers changed since the previous `regs` event |
| `maps` | `op` (`map` `unmap` `protect` `brk`), `regions: [{start, len, perms, path?}]` | Address-space changes |
| `syscall` | `pc`, `nr`, `args[6]`, `ret`, `writes: [{addr, data}]` | Completed syscall. `writes` holds the bytes the kernel put into the tracee: replay injects them, and analysis treats them as **external input** |
| `alloc` | `alloc`, `addr`, `size`, `epoch`, `kind`, `realloc_of?`, `caller` | A heap object begins |
| `free` | `alloc`, `addr`, `epoch`, `caller` | A heap object ends |
| `signal` | `signo`, `code`, `addr?`, `pc`, `fatal` | Signal delivery. `fatal: true` ends the trace |
| `nondet` | `pc`, `insn`, `values: [[reg, u64]]` | A non-deterministic instruction (`rdtsc`, `rdtscp`) trapped and emulated by the recorder; replay writes the same values |
| `sanitizer` | `kind`, `addr`, `size`, `pc` | AddressSanitizer caught a memory error (`load`, `store`, `double-free`, `bad-free`, `generic`) before the access happened; derived by the replay index from ASan's report hooks |
| `mark` | `label`, `pc` | Ground-truth or user marker (`ss_mark("…")` in the bug corpus) |
| `exit` | `code` | Normal exit, which ends the trace |

## Allocation identity

`ObjectID = {range, alloc_event, free_event, epoch}`. The **epoch** is a per-address generation: the n-th allocation starting at an address has epoch n. So `0x55550010 @ epoch 1` and `0x55550010 @ epoch 2` are different objects, even though their addresses are identical.

A `realloc` that moves is recorded as a `free` of the old allocation followed by an `alloc` with `kind: realloc` and `realloc_of` set. An in-place `realloc` is recorded the same way, with the same address and the next epoch.

## Validity rules (`statesolver trace validate`)

1. Event ids are 0, 1, 2, … with no gaps.
2. No `regs` delta comes before a full snapshot.
3. Allocation ids are never reused.
4. A new allocation never overlaps a live one (`malloc(0)` counts as one byte).
5. `epoch` equals the number of earlier allocations at the same start address, plus 1.
6. `realloc_of` names an allocation that was freed earlier.
7. `free` names a live allocation, with its address and epoch. A double free breaks this rule, so the recorder emits the event as it happened and the validator flags it.
8. Map regions have nonzero length and page-aligned starts (except `brk`).
9. Nothing follows `exit` or a fatal `signal`.

A **buggy program can produce a trace that breaks rule 7.** That is a finding about the program, not a corrupt trace. `validate` reports it, and the analysis treats it as evidence (see `bug-corpus/002-double-free`).

## Example

[`docs/examples/uaf-reuse.jsonl`](examples/uaf-reuse.jsonl) is the scenario from §1 of the research direction, written by hand: Allocation #41 is freed, its address is reused by #79 at epoch 2, and a stale write is followed by a crash.

```bash
statesolver lifetime 0x55550018 --trace docs/examples/uaf-reuse.jsonl --event 10
```
