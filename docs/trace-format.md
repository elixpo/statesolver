# MorphStack trace format, version 0

**Status:** v0, draft. The format may still change before Stage 2 (#2) closes. Every change must bump `version`.

A trace records how the activation state of a program evolves over time. The simulator replays it under every representation policy, and the oracle uses it to compute the hindsight-optimal schedule. A trace says *what the program did*, never *how it was represented*, so a single trace can price every policy.

Reference implementation: [`crates/morphstack-trace`](../crates/morphstack-trace).

## Data model

A trace is one **header** followed by a stream of **events**.

### Header

| Field | Type | Required | Meaning |
|---|---|---|---|
| `format` | string | yes | Always `"morphstack-trace"`. |
| `version` | u16 | yes | `0`. Readers reject newer versions. |
| `source` | string | yes | Producer, such as `morphstack-workloads/w6`, `tokio-capture`, or `go-runtime-trace`. |
| `seed` | u64 | no | Generator seed for synthetic traces. |
| `description` | string | no | Free text. |
| `functions` | array | no | Function table: `{ "id": u32, "name": string, "can_suspend": bool }`. |

The `can_suspend` flag drives the static baseline *D* (heap if the function can suspend). When the table is non-empty, every `call` must reference an entry in it.

### Events

Every event has a `ts` (u64 nanoseconds from an arbitrary origin) and an `ev` tag.

| `ev` | Fields | Meaning |
|---|---|---|
| `spawn` | `ctx`, optional `parent` | A new execution context (coroutine, task, goroutine) becomes runnable. |
| `call` | `ctx`, `act`, `fn`, `frame_bytes` | Activation `act` of function `fn` is pushed on `ctx`. |
| `return` | `ctx`, `act` | The innermost activation of `ctx`, which must be `act`, returns. |
| `suspend` | `ctx` | `ctx` parks with its activations still live. |
| `resume` | `ctx` | A suspended `ctx` continues. |
| `exit` | `ctx` | `ctx` terminates. |
| `sample` | optional `rss_bytes`, `cycles`, `llc_misses`, `dtlb_misses` | Process-wide resource sample. Counters are deltas since the previous sample. |

`ctx` and `act` are u64. `fn` and `frame_bytes` are u32. `frame_bytes` covers locals, spills, saved registers, and the return address.

Call depth is **derived** from `call` and `return`, so it is not stored. This keeps a trace from containing a depth that contradicts its own events.

### Validity rules

`morphstack trace validate` enforces these rules (see `Rule` in `validate.rs`):

1. Timestamps are non-decreasing.
2. A context id is spawned at most once. Ids are never reused, even after `exit`.
3. A `spawn` parent, if present, is live.
4. Every other context event refers to a context that has been spawned and has not exited.
5. `call` and `return` happen only while the context is not suspended.
6. Activation ids are unique across the whole trace.
7. `call` references a known function when the header has a function table.
8. `return` names the innermost live activation of its context.
9. `suspend` requires the context to be running, and `resume` requires it to be suspended.
10. `exit` requires the context to be running with an empty activation stack.

A trace may end with contexts still live. That is how a trace taken from a window of a longer run ends.

## Encodings

Both encodings carry exactly the same data model, and `morphstack trace convert` translates between them without loss. Readers detect the encoding from the file's first bytes, not from its extension.

### JSONL (`*.jsonl`)

The first non-blank line is the header object. Each following non-blank line is one event object. Lines longer than 1 MiB are rejected.

```json
{"format":"morphstack-trace","version":0,"source":"example","functions":[{"id":1,"name":"main","can_suspend":true}]}
{"ts":0,"ev":"spawn","ctx":1}
{"ts":100,"ev":"call","ctx":1,"act":1,"fn":1,"frame_bytes":256}
{"ts":200,"ev":"suspend","ctx":1}
```

### Binary (`*.msb`)

```text
magic        8 bytes   "MSTRACE\0"
version      u16 little-endian
header_len   u32 little-endian (at most 16 MiB)
header       header_len bytes: the header as JSON
events       repeated until end of file
```

Each event is:

```text
tag     u8
dts     varint: zigzag(ts - previous_ts, wrapping), previous_ts starts at 0
payload per tag, all integers LEB128 varints:
  1 spawn    ctx, has_parent (one byte: 0 or 1), [parent]
  2 call     ctx, act, fn, frame_bytes
  3 return   ctx, act
  4 suspend  ctx
  5 resume   ctx
  6 exit     ctx
  7 sample   flags (one byte; bit 0 rss, 1 cycles, 2 llc, 3 dtlb), then each present field in bit order
```

Varints are at most 10 bytes, and `fn` and `frame_bytes` must fit in u32. An unknown tag, unknown flag bits, an overlong varint, or a file that ends partway through an event are all errors. A file that ends cleanly between two events is a valid trace.

Typical event size is 3–8 bytes, against 40–80 bytes for JSONL.

## Statistics

`morphstack trace stats` reports contexts (total, peak live, peak suspended), activations, maximum depth, peak live frame bytes, and distributions of frame size, activation lifetime, suspension duration, and depth at suspension.

Quantiles come from a log-linear histogram. They are exact below 16. Above 16, the reported value is the upper bound of its bucket, at most 12.5% above the true value. Means and maxima are exact. These statistics are for workload characterization. The simulator never uses histogram approximations.

## Changing the format

- Adding an optional field, or a new event tag that older readers reject loudly, needs a version bump and a converter.
- Every published dataset in the paper artifact records the format version it was produced with.
