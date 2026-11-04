# StateSolver SLR: Novelty Verdict

**Date:** 2026-11-04 | **Inputs:** `protocol.md` (67 queries), `matrix.md` (68 works)

## Update 2026-11-04: forward-citation check (see [forward-citations.md](forward-citations.md))

**WATCHER (Liu et al., OOPSLA 2020, doi:10.1145/3428211)** already does crash-triggered, replay-based backward slicing of unmodified native binaries back to syscall input, without an eager dependence graph. The clause "first to reconstruct only the queried dependences through checkpointed replay" is **withdrawn**. WATCHER keys dependences by raw address ("last write per address"), and its §3.4 claims this handles heap reuse. None of its 24 bugs is a use-after-free or reuse bug. It is the natural **address-only baseline**.

Also cite: Ravel (AsiaCCS 2017: replay plus alloc/free lifetimes, address-based dependence), UAFSan (ISSTA 2021), BSan (ICSE-C 2025).

**The remaining technical claim:** lifetime-qualified dependence inside replay-driven on-demand slicing, plus evidence that WATCHER-style last-write tracking breaks under reuse. Project direction is being re-scrutinized in [agent-paper-scrutiny.md](agent-paper-scrutiny.md).

## Verdict: GO (narrowed, provisional)

No located work combines all five parts of the claim. Those parts are: unmodified native binaries; deterministic replay; dependences reconstructed per query rather than stored eagerly; allocation-lifetime identity across address reuse; and a backward causal slice that reaches syscall input.

**However, the brief's "allocation-epoch identity" is not new on its own.** Undangle (ISSTA 2012) already labels binary-level pointers with *allocation address + instruction counter* and catches uses after memory reuse. FreeWill (USENIX Sec 2022) already slices backward on binary traces to pick the right lifetime of a reallocated block. The novelty therefore has to rest on *lifetime-qualified dependence semantics inside on-demand, replay-driven backward slicing*, plus the measured effect on root-cause localization. It cannot rest on the identity scheme.

**Conditions before the verdict is final:** (1) a human re-runs the queries natively in Google Scholar and the ACM DL, with forward-citation searches on Undangle, FreeWill, Arnold, and DrDebug; (2) the Pernosco question is resolved: does its dataflow view distinguish lifetimes after reuse? Its documentation is silent. Ask the vendor or test it on a reuse micro-benchmark. If either check finds lifetime-aware backward dataflow over replay, **PIVOT** to an empirical claim: "the first quantitative study of how address reuse corrupts backward slices, with an on-demand replay technique as the vehicle".

## Closest works and how each falls short

1. **Undangle (Caballero et al., ISSTA 2012).** Unmodified x86 binaries under TEMU. Pointer taint labels are {root-addr, icounter, offset}, propagated through copies and arithmetic, and the tool detects uses after `t_memreuse`. *It falls short because:* it is a **forward** pass over a **fully recorded instruction trace**. There is no deterministic replay and no per-query reconstruction. It tracks *pointers only*, not general values. It does not take syscall input as a source. Its output is the dangling pointer's creation site, not a causal slice to the crash.
2. **FreeWill (He et al., USENIX Security 2022).** Binary traces from QEMU/Pin. Backward slicing from the crash pointer selects the correct lifetime when "the same block has more than one lifetime" (this mattered in 18 bugs). *It falls short because:* it records the full trace eagerly, has no replay, and is **UAF- and refcount-specific**. It explains reference miscounting, not arbitrary machine values, and no syscall-input sources are mentioned.
3. **Arnold / Eidetic Systems (OSDI 2014) and JetStream (OSDI 2016).** Deterministic replay of unmodified binaries. Intra-process lineage is computed **at query time** by replay plus Pin taint, with syscall input bytes as sources. *They fall short because:* identity is the **byte address**, so lineage silently merges objects across reuse. Queries run as forward taint from tagged inputs, not demand-driven backward slicing. Neither system addresses or evaluates temporal heap bugs.
4. **Pernosco (rr-based).** Native, replay-based backward dataflow that skips copies and follows pipes and sockets. Its UAF walkthrough shows `malloc` returning the reused address. *It falls short because:* it builds an **eager omniscient database** of every memory and register write (10s–100s GB). No public documentation shows lifetime-qualified identity; finding the `delete` needed a separate manual search. It is a commercial product with no published evaluation.
5. **DrDebug (CGO 2014).** Pin/PinPlay replay plus dynamic slicing on native code. *It falls short because:* it computes the region's DDG eagerly at replay, uses address-based memory dependence, and is not evaluated on temporal heap bugs.
6. **MSRC VulnScan (2017) / REVEN.** Backward taint from the crash over TTD or whole-VM traces, with UAF handling. *They fall short because:* allocation identity is not described (VulnScan dropped full alloc/free logging as "too costly"). They are vendor material, the output is a bug class or notebook, and there is no peer-reviewed evaluation.

The demand-driven slicing lineage (Zhang–Gupta LP, ICSE 2003; Postolski et al., arXiv 2022) shows that on-demand dependence recovery is **not** new. It was done over stored traces (LP) or in Java with managed object identity (Postolski).

## Recommended novelty sentence

> To our knowledge, StateSolver is the first technique that answers backward causal queries about arbitrary machine-state values in unmodified native binaries by **reconstructing only the queried dependences through checkpointed deterministic replay**, while resolving every heap memory dependence against **allocation-lifetime identity (address, epoch)** rather than raw address. As a result, slices cross register, memory, and pointer copies, alloc/free/realloc boundaries, and system-call input without merging distinct objects that reuse the same address. We show that address-only backward slicing produces incorrect or inflated causal chains in executions with heap reuse, and that lifetime-aware on-demand reconstruction removes them without a recording-time dependence graph.

## Do NOT claim

- Allocation identity, address+time labels, or per-allocation keys/tags as new (CETS, lock-and-key, Undangle, MTE, Cornucopia).
- "First to distinguish reused heap addresses" or "first UAF root-cause tool" (Undangle, FreeWill, pSweeper's origin tracking).
- On-demand, demand-driven, or lazy dynamic slicing per se (Zhang–Gupta LP, Postolski et al.).
- Replay-decoupled or retroactive provenance queries per se (Aftersight, Arnold, JetStream, Sledgehammer, PANDA).
- Syscall/IO-sourced provenance (Redux, Arnold, Pernosco).
- Reverse or time-travel debugging, slicing, or UAF detection (rr, Boothe, SPYDER, ASan, and others).
- An LLM or agent driving a time-travel debugger (Undo AI/MCP, DebugHarness, ChatDBG). The harness may claim only that it is *grounded in slice evidence*.
- That Pernosco or TTD *cannot* handle reuse. Say only that they do not document it.

## Must-cite list

- **Debugging and replay:** Lewis 2003; Boothe PLDI 2000; rr (ATC 2017); Pernosco; Undo; WinDbg TTD; ReVirt (OSDI 2002); Aftersight (ATC 2008); PANDA (PPREW 2015); Arnold (OSDI 2014); JetStream (OSDI 2016); Sledgehammer (OSDI 2018); DoubleTake (ICSE 2016).
- **Slicing:** Agrawal & Horgan (PLDI 1990); SPYDER (SP&E 1993); Zhang, Gupta & Zhang (ICSE 2003); Zhang & Gupta (PLDI 2004); WET (MICRO 2004); Zhang, Tallam & Gupta (FSE 2006); DrDebug (CGO 2014); Redux (RV 2003); Postolski et al. (arXiv 2022); Whyline (ICSE 2008); Memory Indexing (FSE 2010).
- **Identity and temporal safety:** Undangle (ISSTA 2012); CETS (ISMM 2010); memory tagging (arXiv 2018); ASan (ATC 2012); Bond et al. (OOPSLA 2007); DangNULL (NDSS 2015); DangSan (EuroSys 2017); pSweeper (CCS 2018); MarkUs (S&P 2020); Cornucopia (S&P 2020).
- **Crash and UAF RCA:** FreeWill (Sec 2022); REPT (OSDI 2018); RETracer (ICSE 2016); POMP (Sec 2017); CREDAL (CCS 2016); ARCUS (Sec **2021**); AURORA (Sec 2020); VulnScan (MSRC 2017).
- **Harness:** ChatDBG (FSE 2025); DebugHarness (arXiv 2026); Undo AI/MCP; Zeno (NSDI 2019) for the temporal-provenance framing.

## Strongest reviewer objection and how to pre-empt it

**Objection:** "This is Undangle's (address, icounter) labels plugged into Arnold/JetStream-style replay-time analysis, with Pernosco's dataflow UX on top. It is a composition, and an eager index such as Pernosco answers queries faster anyway."

**Pre-emption in the evaluation:**
1. **Ablation on identical infrastructure.** Run the same replay engine with (a) address-only and (b) lifetime-qualified dependence. Report wrong-edge rate, slice size, distance to the true root cause, and localization accuracy *only on executions with reuse*.
2. **An Undangle-style baseline.** Forward pointer-label propagation over a full trace. Show the queries it cannot answer: stale-pointer writes that corrupt **non-pointer** data, and chains from crash to syscall input that cross a `realloc`. Also report its trace cost.
3. **An eager-index baseline.** Pernosco-like: full write log or DDG. Report the storage and recording-overhead versus query-latency break-even point as the number of queries per trace grows, echoing the LP-versus-FP tradeoff of Zhang & Gupta.
4. **Real bugs, not only the synthetic corpus.** Include CVEs whose freed block was reallocated, such as cases from FreeWill's 18 reallocation bugs, SEC-bench, or Magma, and report the share of the corpus where reuse actually changes the slice.
5. **Custom allocators.** State how epochs are derived when programs use pools or arenas. Allocation identity from `malloc` interposition alone will miss these, and reviewers will ask.

## Open uncertainties

- Pernosco's and REVEN's internal handling of allocation lifetimes is **undocumented**. This is the largest residual risk.
- Google Scholar, ACM DL, and IEEE were not queried natively. The originals of FreeSentry, Patil & Fischer, and Xu et al. 2004 were not located directly. The details of ARCUS's UAF model and the venues of Tralfamadore and iReplayer are unverified.
- DebugHarness (April 2026) and Undo AI show the harness space is moving quickly. Re-check arXiv before submission.
- If slices must be evaluated on fully lifetime-correct semantics, `mmap`/`munmap` regions, stack frame reuse, and allocator-internal metadata writes may need their own epoch model. The current claim covers the heap only.
