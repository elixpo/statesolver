# StateSolver discoveries

A dated log of what we learned, including dead ends, newest first. This is raw material for the paper.
Each entry: **date**, the finding, the evidence, and what it changes.
MorphStack's log is archived at [docs/archive/morphstack/DISCOVERIES.md](docs/archive/morphstack/DISCOVERIES.md).

---

## 2026-11-04: rr backend works; allocation epochs derived during replay match the fixture recorder exactly

- **Finding:** `statesolver record` (an rr wrapper) followed by `statesolver index` (one replay pass breaking on the allocator) reproduces the fixture recorder's results on every corpus bug:
  - 001: 3 allocations, 1 free, reuse at epochs 1→2, SIGSEGV;
  - 002: 5 allocations, 6 frees, the double free flagged, SIGABRT;
  - 003: 2 allocations, SIGSEGV.

  Indexing takes about 0.12 s per bug, and every derived event carries its rr event number, so queries can replay to it.
- **rr protocol facts learned (all handled in `statesolver-replay`):**
  1. rr does **not** step over software breakpoints on continue. The client must remove the breakpoint, single-step, and re-insert it.
  2. After a breakpoint stop, `rip` equals the breakpoint address (no +1).
  3. rr does not report library load addresses, but the replay process is rr's real child, and its `/proc/<pid>/maps` show the recorded library paths. Its `/proc/<pid>/exe` is `rr_exec_stub`, so find the executable through its mapping.
  4. `qRRCmd:when:-1` returns the hex text `Completed event: N`.
  5. After replaying a fatal signal, rr delivers its own SIGKILL, then reports an exit. The last fault signal before the end is the death.
  6. rr records with ASLR on, so heap addresses differ between recordings, though not between replays of one recording.
- **Lesson:** `pkill -f "rr replay"` matches the shell running it, and a stale rr server held the probe socket. Always kill rr by PID.

## 2026-11-04: Direction locked: rr-backed crash explainer, and an agent study as the paper

- **Scrutiny:** [docs/slr/agent-paper-scrutiny.md](docs/slr/agent-paper-scrutiny.md). Closest threats:
  - **DebugHarness** (arXiv 2604.03610): an agent with GDB, pwndbg, and rr over MCP on SEC-bench. It measures patch success only; there is no root-cause accuracy and no measured hallucination.
  - **Undo AI** and the open-source **rr MCP servers**: agent plus rr is a commodity.
  - **Veritas** and **KernelDiag**: grounding and evidence-graph framing.
  - **T2L-ARVO**: the same localization metric, without a debugger.
- **Remaining gap:** a controlled study of root-cause accuracy *and* trace-verified claim support, isolating the effect of allocation-lifetime-aware causal queries against raw replay access.
- **Decision:** [docs/DIRECTION.md](docs/DIRECTION.md).
  - The project comes first: an open crash explainer on rr, with an MCP server and the Crumb agent.
  - The paper is the agent study, with arms A0–A5, SEC-bench primary, and a pilot gate on about 20 bugs.
  - arXiv cs.SE, cross-listed to cs.AI.
- **Known risk:** a null result. The pilot exists to find that out cheaply, and a null result would be reported honestly.

## 2026-11-04: rr drives cleanly over the gdb remote protocol; the address-only answer is visibly incomplete

- **Finding:** `rr replay -s PORT` plus `gdb target extended-remote` runs to the SIGSEGV in bug 001. `watch -l *(long*)h` followed by `reverse-continue` stops at `__memcpy_avx_unaligned_erms`, called from `main.c:46`; the value changed from `greet` to `0x4141…`.
- **Implication:** the last writer by address is the stale `memcpy`, and that is all an address-only tool reports. The missing fact is that `memcpy`'s destination pointer came from `s.cached_user`, which points into the freed `User` at epoch 1, while the address now belongs to `Handler` at epoch 2. That missing fact is exactly what A4 adds over A1 and A5.

## 2026-11-04: Breakpoint-based allocation tracking costs about 50 µs per allocation, far behind rr

- **Finding:** `benchmarks/alloc-churn.c` with 100k malloc/free pairs runs natively in under 0.01 s, takes **4.98 s** under `statesolver record`, and takes **0.04 s** under `rr record`. Compute-bound code (`benchmarks/compute.c`) records at native speed (0.09 s either way), so the cost is the 4–5 ptrace stops per allocator call, not syscalls.
- **Implication:** our own ptrace recorder cannot be the product backend. Either use an in-process allocation log (an `LD_PRELOAD` shim writing to a shared ring buffer, drained at syscall stops, as rr's syscallbuf does) or, better, **use rr as the recording and replay substrate** and derive allocation epochs on demand during replay. The second option also fits the "reconstruct only what the query needs" principle.

## 2026-11-04: WATCHER (OOPSLA 2020) takes the "on-demand replay slicing" novelty

- **Finding:** the forward-citation check ([docs/slr/forward-citations.md](docs/slr/forward-citations.md)) found WATCHER: crash-triggered backward slicing of native binaries over replayed re-executions, back to syscall input, with no eager dependence graph. It keys dependences by raw address and claims that handles reuse. None of its bugs involves reuse.
- **Implication:** the purely technical claim narrows to "lifetime-aware dependences inside replay-driven slicing, plus evidence that WATCHER-style tracking breaks under reuse". That is an incremental claim. It prompted a direction re-scrutiny: rich project first, with the paper asking whether evidence-grounded causal queries make LLM debugging agents more accurate (under review).

## 2026-11-04: Literature review gives a narrowed GO. Epoch identity is prior art; lifetime-aware on-demand slicing is the claim

- **Finding:** 67 queries, 68 works ([docs/slr/](docs/slr/verdict.md)). No work combines all five: unmodified native binaries, deterministic replay, per-query dependence reconstruction, allocation-lifetime identity across reuse, and backward slices that reach syscall input.
  - **Undangle** (ISSTA 2012) already labels binary pointers with allocation address plus instruction counter.
  - **FreeWill** (USENIX Security 2022) already slices backward to pick the right lifetime of a reallocated block.
  - **Arnold and JetStream** compute replay-time provenance with syscall sources, but key on byte addresses.
  - **Pernosco** builds an eager write database and does not document lifetimes.
- **Claim (locked wording):** answer backward causal queries about arbitrary machine values by reconstructing only the queried dependences through checkpointed replay, resolving heap dependences by (address, epoch). Show that address-only slicing gives wrong or inflated chains under reuse.
- **Do not claim:** epoch identity, on-demand slicing, replay-decoupled provenance, I/O-sourced provenance, or an LLM driving a debugger, each on its own.
- **Evaluation consequences (added to #4):**
  - an ablation on the same engine, comparing address-only with lifetime-qualified dependence;
  - an Undangle-style forward pointer-label baseline;
  - an eager-index (Pernosco-like) baseline, with the break-even point on number of queries per trace;
  - real reuse bugs (FreeWill's reallocation cases, SEC-bench, Magma);
  - a stated story for custom allocators (pools and arenas).
- **Biggest residual risk:** whether Pernosco or REVEN handle lifetimes internally is undocumented. Test Pernosco on corpus bug 001 if access is possible.

## 2026-11-04: The recorder works, and glibc's free re-enters its own entry point

- **Finding:** `statesolver record` captures all three corpus bugs correctly: 001 (address reuse, epochs 1→2, SIGSEGV), 002 (6 frees, the double free flagged as rule-7 evidence, SIGABRT), and 003 (stdin bytes captured as external input: length byte `0x30`, 48-byte payload).
- **Subtle case 1:** glibc 2.42's `free` jumps back to its own entry (same `rsp`, pointer, and return address) on its slow path. A naive entry breakpoint counts one free twice. Fix: track frees in flight until their return breakpoint, and ignore matching re-entries. A real double free in a loop is still caught, because the first call returns before the second enters.
- **Subtle case 2:** removing a return breakpoint inside its own handler left `rip` one byte into an instruction. That silently scrambled control flow, and showed up as missing frees in 002. Fix: only step-over removes breakpoints. Both cases are covered by `crates/statesolver-record/tests/corpus.rs`.
- **Implication for the evaluation:** recording correctness must itself be tested against ground truth. "Recorded trace validates" is not enough.

## 2026-11-04: A double free produces an "invalid" trace, and that must stay evidence

- **Finding:** validator rule 7 (`free` must name a live allocation) is broken by exactly the bugs StateSolver exists to explain (bug-corpus 002). The recorder must still emit the event as it happened.
- **Implication:** the spec separates **trace corruption** (rules 1–6, 8, 9) from **program misbehaviour** (rule 7). Analysis must consume rule-7 violations as evidence, never reject them. The CLI should eventually report them as "program anomalies", not "invalid trace".

## 2026-11-04: Binary trace container = zstd-compressed JSONL

- **Decision:** postcard (and other compact serde formats) cannot encode internally tagged enums or optional fields that are skipped. Supporting both would mean two parallel encodings of one data model. So `.ss` is zstd-compressed JSONL: one data model, streamable, inspectable with `zstdcat`.
- **Revisit if:** trace size or decode speed becomes a measured bottleneck in the Stage 4 evaluation. Trace size is one of the reported metrics, so measure it before optimizing.

## 2026-11-04: Pivot to StateSolver, and what carries over

- **Decision:** the maintainer locked [docs/research-direction.md](docs/research-direction.md) as the direction. MorphStack is archived (tag `morphstack-final`).
- **Carried-over lessons:**
  - **Decouple capture from analysis.** MorphStack's trace-first design (record once, then price any policy offline) maps directly onto "record once, then query any causal question".
  - **Treat the evaluator as an adversary.** MorphStack's oracle exploited every free lunch in the cost model. StateSolver's equivalent risk is a causal slice that looks right on toy bugs because the corpus is too easy. **Real bugs are mandatory**, not just synthetic ones.
  - **Report each effect separately.** MorphStack split its gains into heterogeneity and switching. StateSolver should likewise split its improvement into what comes from *epochs* and what comes from *on-demand replay*, using two separate baselines.
- **Environment notes for Stage 2:** rr is not installed; `kernel.perf_event_paranoid = 4` (rr needs ≤ 1); the CPU is a hybrid Raptor Lake i5-13450HX (P-cores and E-cores), so rr needs P-core pinning.

## 2026-11-04: The novelty risk is in temporal-safety and replay-analysis literature, not in debuggers

- **Finding (from prior knowledge; to be verified in the literature review):** per-allocation unique identifiers (address plus lifetime) are standard in temporal memory safety: CETS / lock-and-key, and CHERI provenance. Dangling-pointer provenance through taint exists in Undangle. Heavy analysis deferred to replay exists in Aftersight, PANDA, and Arnold (retroactive provenance queries).
- **Implication:** neither "epochs as identity" nor "analysis on replay" alone is novel. The claim must be their **combination for on-demand causal explanation**, measured against address-only and eager-DDG baselines. Stage 1's review targets exactly these areas.
