# StateSolver discoveries

A dated log of what we learned, including dead ends, newest first. This is raw material for the paper.
Each entry: **date**, the finding, the evidence, and what it changes.
MorphStack's log is archived at [docs/archive/morphstack/DISCOVERIES.md](docs/archive/morphstack/DISCOVERIES.md).

---

## 2026-11-05: A real benchmark bug records under rr on the laptop; ASan is manageable

- **Finding:** SEC-bench `jq` was reproduced and recorded end to end on this laptop: pull the image, `secb build` (41 s), `secb repro`, then `rr record` and `rr pack` inside the container (26 MB). The container runs with `--cap-add=SYS_PTRACE --security-opt seccomp=unconfined` and is pinned to a P-core. Details are in [docs/study/benchmarks.md](docs/study/benchmarks.md).
- **Gotchas:**
  - rr must be installed **inside** the container (the rr 5.9 release package). Bind-mounting the host rr breaks on library mismatches, and recording on the host fails because rr rejects `madvise(102)` on this kernel.
  - `secb repro` exits 0 even when it crashes, so detect the ASan line instead.
  - ARVO images run Ubuntu 16.04, which is too old for rr 5.9, so the fuzz target and PoC are copied into an Ubuntu 22.04 container.
- **ASan:** both benchmarks build with ASan, and its quarantine stops freed chunks being reused. `-e SANITIZER=none` gives a plain build of the same commit for SEC-bench (verified on jq). `ASAN_OPTIONS=quarantine_size_mb=0` re-enables reuse without a rebuild. Both remain options for the 5-bug ASan test.
- **Ground truth:** T2L-ARVO's line-level labels are not public (the linked repository returns 404). SEC-bench includes the developer fix patch for every instance, so fix-commit ground truth (the primary measure) is available for all of them.
- **Pilot list:** 20 SEC-bench bugs across 13 projects: 12 use-after-free, 2 double free, 1 bad free, and 5 heap overflows. In 11 of the 15 lifetime bugs, the free is in a different file from the use: long causal chains.

## 2026-11-05: No big machine, but the laptop has the rare resource; the study is scaled to it

- **Finding:** the constraint was disk (13 GB free), not the machine class. This laptop has a working PMU, which most cloud VMs lack and rr needs for both recording and replay. Cleaning regenerable Rust build outputs (about 49 GB in 7 workspace copies) and unused Docker images and build cache (about 28 GB) freed about 74 GB, leaving 92 GB free.
- **Decision:** a laptop-scale study (DIRECTION.md, Changes): a one-bug-at-a-time pipeline, 40–60 real bugs in small and medium projects, fewer models, and serial overnight batches. The direction, claim, arms, and pilot gate are unchanged.
- **Threat to validity to report:** benchmark size and the bias toward projects that build in 6 GB of RAM.

## 2026-11-04: explain-crash classifies all three corpus bugs correctly with one lifetime-aware rule

- **Rule:** for the write that produced the crashing value, compare the allocation its **address pointer came from** (provenance origin) with the allocation that **owned the address** at that moment:

  | Pointer origin | Owner at write time | Verdict |
  |---|---|---|
  | Freed | Another allocation at the same address | Use-after-free with reuse |
  | Freed | None | Use-after-free |
  | Live | A different allocation | Overflow |

- **Results:**
  - 001: use-after-free with address reuse. The pointer chain crosses from inside `memcpy` back to `load_user`'s `malloc`, at epoch 1.
  - 002: double free. The pointer chain shows the aliasing store `b->buf = a->buf`.
  - 003: heap buffer overflow, 8 bytes past a 24-byte buffer into its neighbour. The value chain follows a SIMD copy back to the `read()` syscall, so the corrupt bytes are external input.
- **Ablation preview:** the same engine in address-only mode (arm A5) stops at "`mov [rdi],rsi` wrote 0x…", with no stale-pointer conclusion. Tests assert both outcomes.
- **Engine facts:** rr reports kernel writes during a syscall as a watchpoint hit on the `syscall` instruction, which makes syscall input a natural chain terminal. SIMD copies need one extra hop: from the vector store to the vector load to the source bytes at the same offset.
- **Caveat:** three synthetic bugs. The pilot (#4) is what tests whether this generalises to real CVEs.

## 2026-11-04: First end-to-end lifetime-aware provenance chain on a real replay

- **Finding:** from the stale `name[0] = 'A'` store in bug 001, `provenance::trace` follows the pointer back through 6 hops of real x86-64 code: `mov rax,[rbp-20h]` (load `s.cached_user`) ← `mov [rbp-20h],rax` (`s.cached_user = u`) ← `mov rax,[rbp-18h]` ← `mov [rbp-18h],rax` (`u = load_user()`) ← `mov rax,[rbp-8]` (`return u`) ← `mov [rbp-8],rax` (`u = malloc()`). It ends at **allocation #0, epoch 1**. The address being written belongs to **#1, epoch 2** at that time. This is the mechanism the paper's arm A4 exposes and A5 (address-only) cannot.
- **Bugs found on the way:**
  1. iced's `virtual_address` takes the *memory operand's* index, not 0.
  2. Its register callback is also asked for the segment base (DS), which must be 0 in 64-bit mode; returning `None` voids the address.

  Both are fixed and covered by the test.

## 2026-11-04: Reverse watchpoints stop on the writer before it executes; bug 001 has two stale writes

- **Finding:** in rr 5.9, reverse-continue with a write watchpoint stops **on** the writing instruction, before it executes. The location still holds the old value, the writer's address and source registers are readable, and one forward step shows the new value. Reverse-continuing from a crash first re-reports the pending signal at the same position, and the engine skips it.
- **On bug 001** the primitives recover the full mechanism from the crash:
  1. The last writer of `Handler.run` is `memcpy`'s `mov [rdi],rsi`, changing `greet+0x18` to `0x4141…`.
  2. Before it, `mov byte [rax],0x41` (`s.cached_user->name[0] = 'A'`) changed `greet` to `greet+0x18`.
  3. That `rax` came from `mov rax,[rbp-20h]`, a load of `s.cached_user`, and equals the reused address.

  An address-only tool stops at step 1. The epoch question is step 3: the pointer *value* was produced while allocation #0 (epoch 1) owned the address, but it is dereferenced while #1 (epoch 2) owns it.

## 2026-11-04: On hybrid CPUs rr must be pinned to a P-core; positions need the stack pointer

- **Finding 1:** `rr record` binds to a random CPU. On this hybrid i5-13450HX (P-cores 0–11, E-cores 12–15), landing on an E-core gives `FATAL check_working_counters: Got 0 branch events`, and rr leaves an `incomplete` trace. Earlier runs passed by luck, which made the replay tests look flaky. Fix: `rr record --bind-to-cpu=<first P-core>` from `/sys/devices/cpu_core/cpus` (`statesolver_replay::rr_record_args`). Replay uses the CPU stored in the trace, so it follows. 10 of 10 pinned recordings were clean. `statesolver record` also rejects traces that carry rr's `incomplete` marker.
- **Finding 2:** `(rr event, ticks, pc)` does not identify an instruction instance. Two calls to `ss_mark` from `main` and from `logout` had the same event, the same ticks (no conditional branch between them), and the same `pc`, and `goto` landed on the wrong one. Adding the stack pointer makes `RrPos {event, ticks, pc, sp}` unique: revisiting the same `pc` with the same ticks and `sp` would need an unconditional backward loop. A test now visits every alloc and mark position in reverse order and checks `rax` and the marker label.
- **Lesson:** good diagnostics paid off immediately. Reporting rr's own stderr on early exit, instead of a connection timeout, turned a "flaky test" into two concrete root causes in minutes.

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
