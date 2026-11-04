# StateSolver SLR: Comparison Matrix

**Date:** 2026-11-04 | **Protocol:** `protocol.md` | **Verdict:** `verdict.md`

**Legend.** ✓ = has the property; ✗ = lacks it; partial = has it only in part or in a restricted form (see Notes); ? = the sources consulted do not settle it. **unverified** = the bibliographic or technical detail was not confirmed against a primary source.

**Column meanings.**
- **Native bin.** Works on unmodified native (x86/x86-64) binaries without recompilation.
- **Det. replay.** Uses deterministic record/replay (or checkpoint plus re-execution) of the original run.
- **On-demand deps.** Dependences are computed per query, with no full dynamic dependence graph or full trace stored eagerly.
- **Alloc-epoch id.** Heap objects are identified by allocation instance (address + time/epoch, key, tag, or label), not by address alone.
- **Crosses free/reuse.** Correctly separates the dependences or pointers of two objects that share an address over time.
- **Syscall src.** Treats system-call or external input as a provenance source.
- **Causal expl.** Produces a causal chain or slice explaining a value or crash, not just a detection.
- **UAF+reuse eval.** The evaluation includes use-after-free cases where the freed memory was reallocated.

## A. Time-travel, omniscient, and replay debuggers

| Work | Year/Venue | Native bin. | Det. replay | On-demand deps | Alloc-epoch id | Crosses free/reuse | Syscall src | Causal expl. | UAF+reuse eval | Link | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Omniscient Debugging (Lewis) | 2003, AADEBUG / arXiv cs/0310016 | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | partial | ✗ | https://arxiv.org/abs/cs/0310016 | Java; records every state change eagerly; backward navigation. |
| Bidirectional debugging (Boothe) | 2000, PLDI | partial | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | https://doi.org/10.1145/349299.349339 | Re-execution with logged I/O plus checkpoints. Needs event counters compiled into the program. No dependence queries. |
| rr | 2017, USENIX ATC | ✓ | ✓ | partial | ✗ | ✗ | ✗ | ✗ | ✗ | https://www.usenix.org/conference/atc17/technical-sessions/presentation/ocallahan | Substrate. Reverse watchpoints give an on-demand, *address-keyed* "last write". A human chains them by hand. |
| Pernosco | 2019–, commercial | ✓ | ✓ (rr) | ✗ | ? | partial | ✓ | partial | partial | https://pernos.co/about/dataflow/ | Builds an **eager omniscient DB** of all memory and register writes at replay time (vision page; 10s–100s GB per O'Callahan's slides). The dataflow view skips copies and follows pipes and sockets. The UAF walkthrough shows `operator new`/malloc returning the reused address as a dataflow *entry*, but finding the `delete` needed a separate manual search. No documentation of lifetime-qualified identity. |
| UndoDB / LiveRecorder (UDB) | commercial | ✓ | ✓ | partial | ✗ | ? | ✗ | ✗ | ✗ | https://docs.undo.io/UDB-quickref.pdf | `last EXPR` jumps to the previous change on demand. Vendor page says it "stops on allocations and frees affecting locations of interest" (unverified detail). |
| WinDbg Time Travel Debugging + TTD queries | 2017–, Microsoft | ✓ (Windows) | ✓ | partial | partial | ✗ | ✗ | ✗ | ✗ | https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/time-travel-debugging-object-model | `TTD.Memory`/`TTD.Calls` LINQ queries run over an index. `TTD.Data.Heap()` lists heap allocation objects (details unverified). No dependence or slice computation. |
| QIRA (Hotz) | 2014–2016, Enigma talk / OSS | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | https://qira.me | QEMU-based "timeless" full-trace debugger. Per-instruction logging is plausible but unverified. |
| REVEN / esReven (Tetrane, eShard) | 2017–, commercial | ✓ (whole-VM) | ✓ | partial | ? | partial | partial | partial | partial | https://blog.tetrane.com/2020/vulnerability-detection-use-after-free.html | Memory-history index plus forward/backward taint per query. The UAF notebook pairs alloc/free through taint. Vendor material only; whether lifetime identity survives reuse is not documented. |
| TOD — Scalable omniscient debugging (Pothier, Tanter, Piquer) | 2007, OOPSLA | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | partial | ✗ | https://pleiad.cl/papers/2007/pothierAl-oopsla2007.pdf | Java; distributed event DB; partial traces. |
| Undo AI / UDB MCP server | 2025, vendor + CppCon poster | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | partial | ✗ | https://docs.undo.io/HowUndoAIWorks.html | An LLM agent drives time-travel commands over MCP. The agent improvises the explanation; there is no slicing engine. |

## B. Dynamic slicing and demand-driven dependence

| Work | Year/Venue | Native bin. | Det. replay | On-demand deps | Alloc-epoch id | Crosses free/reuse | Syscall src | Causal expl. | UAF+reuse eval | Link | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Dynamic Program Slicing (Agrawal & Horgan) | 1990, PLDI | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | https://doi.org/10.1145/93542.93576 | Defines dynamic slices and dependence graphs. Source-level. |
| SPYDER (Agrawal, DeMillo, Spafford) | 1993, SP&E 23(6) | ✗ | partial | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | https://doi.org/10.1002/spe.4380230603 | Slicing plus execution backtracking. C source level. |
| Precise dynamic slicing algorithms (Zhang, Gupta, Zhang) | 2003, ICSE | ✗ | ✗ | partial | ✗ | ✗ | ✗ | ✓ | ✗ | https://www.cs.purdue.edu/homes/xyzhang/Comp/icse03.pdf | **LP algorithm builds the relevant part of the DDG on demand** from an eagerly stored compacted address trace. This is the key prior art for "on-demand deps". |
| Cost effective dynamic program slicing (Zhang & Gupta) | 2004, PLDI | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | https://dl.acm.org/doi/pdf/10.1145/996893.996855 | A compacted DDG beats demand-driven slicing in time (20 min → 16 s). This is the eager-versus-lazy tradeoff that must be quantified. |
| Whole Execution Traces (Zhang & Gupta) | 2004 MICRO / 2005 TACO | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | partial | ✗ | https://www.cs.purdue.edu/homes/xyzhang/Comp/MICRO04.pdf | Eager, compressed full history (values, addresses, dependences). |
| Execution fast forwarding (Zhang, Tallam, Gupta) | 2006, FSE | ? | ✓ | partial | ✗ | ✗ | partial | ✓ | ✗ | https://doi.org/10.1145/1181775.1181786 | Logs events, prunes them with a meta-slice, then replays a reduced log and traces it. Close in spirit to "replay only what the query needs". |
| DrDebug (Wang, Patil, Pereira, Lueck, Gupta, Neamtiu) | 2014, CGO | ✓ (Pin) | ✓ (PinPlay) | partial | ✗ | ✗ | ✗ | ✓ | ✗ | https://www.cs.ucr.edu/~neamtiu/pubs/cgo14wang.pdf | Replay-scoped slicing. Computes the full DDG for the *region*, address-based. Execution slices can be replayed. |
| Redux (Nethercote & Mycroft) | 2003, RV/ENTCS 89(2) | ✓ (Valgrind) | ✗ | ✗ | ✗ | ✗ | ✓ | partial | ✗ | https://www.cl.cam.ac.uk/techreports/UCAM-CL-TR-606.pdf | Builds the full dynamic dataflow graph eagerly; views reachable from syscall inputs. "Wildly unscaleable" (Valgrind pubs page). |
| Dynamic Slicing by On-demand Re-execution (Postolski, Braberman, Garbervetsky, Uchitel) | 2022, arXiv 2211.04683 | ✗ | partial | ✓ | ✗ | ✗ | ✗ | ✓ | ✗ | https://arxiv.org/abs/2211.04683 | **Closest "on-demand" slicer.** Re-executes repeatedly using a frontier of dependencies and suggests R&R for nondeterminism. Java (ASM agent, IdentityHashMap object identity), so no address reuse arises. |
| RecovSlicing — LLM as an Execution Estimator (Pei et al.) | 2025, arXiv 2508.18721 | ✗ | ✗ | partial | ✗ | ✗ | ✗ | ✓ | ✗ | https://arxiv.org/abs/2508.18721 | Partial instrumentation, with an LLM inferring missing dependences. Java benchmarks (Defects4J; Slicer4J baseline). |
| A system for dynamic slicing and program visualization (Ray Wang, MIT M.Eng.) | 2019, thesis | partial | ? | ✗ | ✗ | ✗ | ? | ✓ | ✗ | https://dspace.mit.edu/handle/1721.1/121764 | Binary-level slicing lifted to LLVM IR. Use of PANDA replay is suggested by a secondary summary but **unverified**. |
| Whyline (Ko & Myers) | 2008, ICSE | ✗ | ✗ | ✗ | ✗ | ✗ | partial | ✓ | ✗ | https://faculty.washington.edu/ajko/papers/Ko2008JavaWhyline.pdf | "Why did/didn't" questions answered from a recorded trace. The UX precedent for the harness's question style. Java. |
| Kairux (inflection point) | 2019, SOSP | ✗ | ✗ | partial | ✗ | ✗ | ✗ | ✓ | ✗ | https://thenewstack.io/kairux-root-cause-debugging-with-the-inflection-point-hypothesis/ | JVM distributed systems; adaptive dynamic slicing. Venue taken from a secondary write-up. |
| Memory Indexing (Sumner & Zhang) | 2010, FSE | ? | ✗ | ✗ | partial | partial | ✗ | ✗ | ✗ | https://www.cs.purdue.edu/homes/xyzhang/Comp/fse10.pdf | Canonical, allocation-structure-based identity for memory locations *across executions*. A related idea of logical identity beyond raw addresses, applied to a different problem (cross-run alignment). |

## C. Analysis decoupled via replay / retroactive queries

| Work | Year/Venue | Native bin. | Det. replay | On-demand deps | Alloc-epoch id | Crosses free/reuse | Syscall src | Causal expl. | UAF+reuse eval | Link | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|
| ReVirt (Dunlap et al.) | 2002, OSDI | ✓ (whole-VM) | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | https://web.eecs.umich.edu/~pmchen/papers/dunlap02.pdf | Instruction-level VM replay for intrusion analysis. |
| BackTracker (King & Chen) | 2003, SOSP | ✓ | ✗ | ✗ | ✗ | ✗ | ✓ | ✓ | ✗ | — (primary page not fetched; **unverified** link) | Dependence graph at OS-object granularity (process/file), not memory. Located via secondary sources only. |
| Aftersight (Chow, Garfinkel, Chen) | 2008, USENIX ATC | ✓ (whole-VM) | ✓ | partial | ✗ | ✗ | ✗ | ✗ | ✗ | https://www.academia.edu/6506489/VMwareDecoupling_Dynamic_Program_Analysis_from_Execution_in_Virtual_Environments | Heavy analysis moved to replay. The analysis is chosen up front, not driven by query. |
| PANDA + taint2 (Dolan-Gavitt et al.) | 2015, PPREW | ✓ (whole-system) | ✓ | partial | ✗ | ✗ | ✓ | partial | ✗ | https://github.com/panda-re/panda | Forward taint at replay from labeled sources (file_taint), byte/address labels. Not backward or demand-driven. |
| Eidetic Systems / Arnold (Devecsery, Chow, Dou, Flinn, Chen) | 2014, OSDI | ✓ | ✓ | ✓ | ✗ | ✗ | ✓ | partial | ✗ | https://www.usenix.org/conference/osdi14/technical-sessions/presentation/devecsery | **Closest on replay plus lazy provenance.** Inter-process lineage is indexed eagerly. *Intra-process lineage is computed at query time* by replay plus Pin taint, with unique taint IDs for syscall input bytes. Identity is the byte address. Backward queries are done by forward taint from inputs, not demand-driven backward slicing. |
| JetStream (Quinn, Devecsery, Chen, Flinn) | 2016, OSDI | ✓ | ✓ | ✓ | ✗ | ✗ | ✓ | partial | ✗ | https://www.usenix.org/conference/osdi16/technical-sessions/presentation/quinn | Parallel DIFT queries over replay "epochs" (= time slices, *not* allocation epochs). |
| Sledgehammer (Quinn, Flinn, Cafarella) | 2018, OSDI | ✓ | ✓ | partial | ✗ | ✗ | ✗ | ✗ | ✗ | https://www.usenix.org/conference/osdi18/presentation/quinn | Cluster-parallel debugging queries over replayed time slices. |
| Tralfamadore / Execution Mining (Lefebvre, Cully, Head, et al.) | c. 2009–2012, venue **unverified** | ✓ (whole-system) | ✗ | ✗ | ✗ | ✗ | ? | partial | ✗ | https://www.cs.ubc.ca/~andy/papers/tralfamadore-vee-final.pdf | Stores a full CPU trace, then runs offline composable analyses and a debugger. The EuroSys 2009 venue in the brief was not confirmed (a VEE paper exists). |
| DoubleTake (Liu, Curtsinger, Berger) | 2016, ICSE (arXiv 1601.07962) | partial | ✓ | partial | ✗ | partial | ✗ | partial | partial | https://arxiv.org/abs/1601.07962 | Snapshots per *epoch* (time slice). Replays an epoch on error with watchpoints to find the culprit. Replaces the allocator for repeatable addresses and defers reuse. |
| iReplayer (Liu et al.) | 2018, PLDI (venue **unverified**; arXiv 1804.01226) | ✓ | ✓ | partial | ✗ | ✗ | ✗ | partial | ? | https://arxiv.org/abs/1804.01226 | In-situ replay; re-executes with watchpoints on overwritten canaries to find the root cause. |

## D. Crash and UAF root-cause analysis from traces, dumps, or replays

| Work | Year/Venue | Native bin. | Det. replay | On-demand deps | Alloc-epoch id | Crosses free/reuse | Syscall src | Causal expl. | UAF+reuse eval | Link | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **FreeWill (He, Hu, Su, Cai, Liang)** | 2022, USENIX Security | ✓ (QEMU/Pin) | ✗ | partial | ✓ | ✓ | ✗ | ✓ | ✓ | https://www.usenix.org/system/files/sec22fall_he-liang.pdf | **Closest UAF work.** Records the full bug trace, then *slices backward from the crash pointer to choose the correct lifetime* of a reallocated block ("the same block has more than one lifetime"; this happened in 18 bugs). Diagnoses refcount miscounting and suggests patches. UAF-specific; full trace, not replay. |
| REPT (Cui, Ge, Kasikci, et al.) | 2018, OSDI | ✓ | ✗ | partial | ✗ | ✗ | ✗ | partial | ? | https://www.usenix.org/conference/osdi18/presentation/weidong | Intel PT plus core dump; recovers data by forward/backward reverse execution (~92% values correct). Enables reverse debugging, not slicing. |
| RETracer (Cui, Peinado, et al.) | 2016, ICSE | ✓ | ✗ | ✓ | ✗ | ✗ | ✗ | partial | ✗ | https://www.microsoft.com/en-us/research/publication/retracer-triaging-crashes-by-reverse-execution-from-partial-memory-dumps/ | Backward taint from a memory dump with no trace; blames a function. |
| POMP (Xu, Mu, Xing, Liu, Chen, Mao) | 2017, USENIX Security | ✓ (32-bit) | ✗ | partial | ✗ | ✗ | ✗ | ✓ | ? | https://www.usenix.org/conference/usenixsecurity17/technical-sessions/presentation/xu-jun | PT plus core dump, reverse execution, backward taint; pinpoints crash-relevant statements. Address aliasing is the hard part (POMP++, DeepVSA). |
| CREDAL (Xu, Mu, Chen, Xing, Wang, Liu) | 2016, CCS | partial | ✗ | ✓ | ✗ | ✗ | ✗ | partial | ? | https://doi.org/10.1145/2976749.2978340 | Core dump plus source; locates corrupted variables. |
| ARCUS (Yagemann et al.) | **2021**, USENIX Security | ✓ | ✗ | partial | ? | ? | partial | ✓ | partial | https://www.usenix.org/conference/usenixsecurity21/presentation/yagemann | PT trace plus symbolic "what-if" analysis. Root cause plus binary-level checks. Its UAF module's allocation model is **unverified**. |
| AURORA (Blazytko et al.) | 2020, USENIX Security | ✓ (Pin) | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ? | https://www.usenix.org/conference/usenixsecurity20/presentation/blazytko | Statistical predicates over many crashing and non-crashing traces. Handles cases with no data dependence between root cause and crash. |
| HOTracer (Jia et al.) | 2017, USENIX Security | ✓ | ✗ | ✗ | partial | ✗ | ✓ | partial | ✗ | https://www.usenix.org/conference/usenixsecurity17/technical-sessions/presentation/jia | Heap overflow only (spatial alloc/access consistency over offline traces). Not UAF. |
| Failure sketching / Gist (Kasikci et al.) | 2015, SOSP | partial | ✗ | partial | ✗ | ✗ | ✗ | ✓ | ✗ | https://dslab.epfl.ch/research/gist | Static slice refined by PT in production. Needs source/IR. |
| Snorlax — Lazy diagnosis (Kasikci, Cui, Ge, Niu) | 2017, SOSP | ✓ | ✗ | ✓ | ✗ | ✗ | ✗ | partial | ✗ | https://web.eecs.umich.edu/~barisk/public/snorlax.pdf | Concurrency bugs; "lazy" means diagnosis after failure from coarse PT timing. |
| Execution Reconstruction (Zuo et al.) | 2021, PLDI | partial | partial | partial | ✗ | ✗ | partial | ✗ | ✗ | https://www.cs.purdue.edu/homes/pfonseca/papers/pldi21-er.pdf | Reproduces failures via PT plus symbolic execution across recurrences. Reproduction, not explanation. |
| MSRC VulnScan | 2017, MSRC blog | ✓ (Windows) | ✓ (TTD) | ✓ | ✗ | partial | ✗ | partial | partial | https://www.microsoft.com/en-us/msrc/blog/2017/10/vulnscan-automated-triage-and-root-cause-analysis-of-memory-corruption-issues | **Closest industrial analogue.** Backward taint from the crash over a TTD trace. For UAF, a backward lookup on the invalid pointer is followed by a forward trace of frees. Logging all allocs/frees was "too costly". No lifetime identity described; outputs a bug class. |
| Intel patent US 10,198,335 (Gottschlich, Pokam, Pereira, Spear) | 2019 (filed 2016) | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | partial | https://patents.google.com/patent/US10198335 | Snapshots at each free, keyed by address in the MPX bounds table; the entry is *cleared on reuse*. Leaves root cause to the programmer. |
| Zeno (Wu, Chen, Phan) | 2019, NSDI | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ | https://www.usenix.org/conference/nsdi19/presentation/wu | Temporal provenance for performance bugs in distributed systems. Cited for the "temporal" causality framing only. |

## E. Allocation identity, temporal memory safety, heap-history tools

| Work | Year/Venue | Native bin. | Det. replay | On-demand deps | Alloc-epoch id | Crosses free/reuse | Syscall src | Causal expl. | UAF+reuse eval | Link | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **Undangle (Caballero, Grieco, Marron, Nappa)** | 2012, ISSTA | ✓ (TEMU, unmodified OS) | ✗ | ✗ | ✓ | ✓ | ✗ | partial | ✓ | https://www.microsoft.com/en-us/research/publication/undangle-early-detection-dangling-pointers-use-free-double-free-vulnerabilities/ | **Closest identity work.** Pointer taint label = {root-type, **root-addr, icounter** (instruction count at allocation), offset}. That is address + time identity, propagated through register/memory copies and pointer arithmetic. Explicitly catches uses after `t_memreuse`. Forward pass over a full offline instruction trace. Pointers only (no general value slices); reports dangling-pointer *creation*. |
| Lock-and-key (Patil & Fischer 1997; Xu, DuVarney, Sekar 2004) | 1997 SP&E / 2004 FSE — **unverified** (secondary citation only) | ✗ | ✗ | ✗ | ✓ | ✓ | ✗ | ✗ | ✗ | https://arxiv.org/abs/2208.12900 | Unique key per allocation. Originals not fetched; cite via CETS and Zhou et al. 2022. |
| CETS (Nagarakatte, Zhao, Martin, Zdancewic) | 2010, ISMM | ✗ | ✗ | ✗ | ✓ | ✓ | ✗ | ✗ | ✓ | https://llvm.org/pubs/2010-06-ISMM-CETS.html | Compiler-inserted unique key per allocation in disjoint metadata. Detection and prevention; no explanation. |
| Fat pointers for temporal safety of C (Zhou et al.) | 2022, arXiv 2208.12900 | ✗ | ✗ | ✗ | ✓ | ✓ | ✗ | ✗ | ✓ | https://arxiv.org/abs/2208.12900 | Checked C extension; compared against CETS. |
| Memory Tagging / HWASan / MTE (Serebryany et al.) | 2018, arXiv 1802.09517 | partial | ✗ | ✗ | partial | ✓ | ✗ | ✗ | ✓ | https://arxiv.org/abs/1802.09517 | 4–8-bit tags reassigned on free; a probabilistic "epoch". MTE needs only allocator support. |
| AddressSanitizer (Serebryany et al.) | 2012, USENIX ATC | ✗ | ✗ | ✗ | ✗ | partial | ✗ | partial | partial | https://www.usenix.org/conference/atc12/technical-sessions/presentation/serebryany | Quarantine *delays* reuse. Alloc/free stacks in the report are the de facto "explanation". Misses UAF once the quarantine is flushed. |
| Valgrind Memcheck + origin tracking (Bond, Nethercote, et al.) | 2007, OOPSLA; Valgrind ≥3.4 | ✓ | ✗ | partial | ✗ | ✗ | ✗ | partial | ✗ | https://doi.org/10.1145/1297027.1297057 | Piggybacks origin (e.g., "created by a heap allocation" site) for undefined values. Memcheck misses dangling uses into reused memory (per Undangle). |
| Memory protection via dynamic tainting (Doudalis, Clause, Venkataramani, Prvulovic, Orso) | 2012, IEEE TC (ASE 2007 precursor) — authors partly **unverified** | ✓ (Pin/Dytan) | ✗ | ✗ | partial | ✓ | ✗ | ✗ | partial | https://www2.seas.gwu.edu/~guruv/tc2012.pdf | The same taint mark goes on the allocation and its pointer, checked on access. Uses a small set of *reusable* marks, so identity is probabilistic. |
| DangNULL (Lee et al.) | 2015, NDSS | ✗ | ✗ | ✗ | partial | ✓ | ✗ | ✗ | ✓ | https://www.ndss-symposium.org/ndss2015/ndss-2015-programme/preventing-use-after-free-dangling-pointers-nullification | Tracks pointer→object relations; nullifies on free. |
| FreeSentry (Younan) | 2015, NDSS — **unverified** (own page not located) | ✗ | ✗ | ✗ | partial | ✓ | ✗ | ✗ | ? | — | Known only through DangSan's related work. |
| DangSan (van der Kouwe, Nigade, Giuffrida) | 2017, EuroSys | ✗ | ✗ | ✗ | partial | ✓ | ✗ | ✗ | ✓ | https://download.vusec.net/papers/dangsan_eurosys17.pdf | Per-object log-structured pointer tracking. |
| pSweeper (Liu, Zhang, Wang) | 2018, CCS | ✗ | ✗ | ✗ | partial | ✓ | ✗ | partial | ✓ | https://www.eecis.udel.edu/~hnw/paper/ccs18.pdf | Concurrent pointer sweeping plus **object origin tracking** of how a dangling pointer was created, as a root-cause aid. |
| CRCount (Shin et al.) | 2019, NDSS | ✗ | ✗ | ✗ | partial | ✓ | ✗ | ✗ | ✓ | https://www.ndss-symposium.org/wp-content/uploads/2019/02/ndss2019_05A-4_Shin_paper.pdf | Implicit reference counting via pointer footprinting. |
| MarkUs (Ainsworth & Jones) | 2020, IEEE S&P | partial | ✗ | ✗ | ✗ | ✓ | ✗ | ✗ | ✓ | https://www.cl.cam.ac.uk/~tmj32/papers/docs/ainsworth20-sp.pdf | Drop-in allocator: quarantine plus mark-from-roots *prevents* reuse while dangling pointers exist. |
| Cornucopia (Filardo et al.) | 2020, IEEE S&P | ✗ (CHERI) | ✗ | ✗ | ✓ | ✓ | ✗ | ✗ | ✓ | https://www.microsoft.com/en-us/research/publication/cornucopia-temporal-safety-for-cheri-heaps/ | Capability provenance plus revocation sweeps before reuse. Needs a CHERI ISA and recompilation. |
| GUEB (Feist et al.) | c. 2014, OSS | ✓ | ✗ | ✗ | partial | partial | ✗ | partial | ✗ | https://www.github.com/montyly/gueb | *Static* binary UAF analysis with heap-object states. Authorship and venue **unverified**. |

## F. LLM and agentic debugging (harness stage)

| Work | Year/Venue | Native bin. | Det. replay | On-demand deps | Alloc-epoch id | Crosses free/reuse | Syscall src | Causal expl. | UAF+reuse eval | Link | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|
| ChatDBG (Levin, van Kempen, Berger, Freund) | 2025, FSE (arXiv 2403.16354) | ✓ (lldb/gdb) | ✗ | ✗ | ✗ | ✗ | ✗ | partial | ✗ | https://arxiv.org/abs/2403.16354 | The LLM "takes the wheel" of a debugger. No time travel. |
| DebugHarness (Sun, Yang, Liu, Zhou, Xu) | 2026, arXiv 2604.03610 | ✓ | ✓ (rr) | ✗ | ✗ | ✗ | ✗ | partial | partial | https://arxiv.org/abs/2604.03610 | **Closest harness work.** An LLM agent with rr reverse commands plus pwndbg for C/C++ memory-safety repair (SEC-bench). The agent reasons about provenance; there is no slicing or allocation-identity engine (abstract only). |
| debug-gym (Microsoft Research) | 2025, arXiv 2503.21557 | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | partial | ✗ | https://arxiv.org/abs/2503.21557 | Python/pdb tool environment for agents. |
| CrashFixer (Mathai et al.) / kGym | 2025, arXiv 2504.20412 / 2024, NeurIPS D&B (arXiv 2407.02680) | ✓ (kernel) | ✗ | ✗ | ✗ | ✗ | ✗ | partial | ? | https://arxiv.org/abs/2504.20412 | Hypothesis-then-patch agent over kernel crash traces (syzkaller). |
| RecovSlicing | 2025, arXiv | — | — | — | — | — | — | — | — | (see §B) | Listed in §B. |

**Row count:** 68 distinct works (excluding the cross-reference row).

## Gap summary

| Required property | Works that have it | Works that combine it with **all** of: native + replay + on-demand + epoch identity |
|---|---|---|
| Allocation-instance identity crossing reuse on native binaries | Undangle, FreeWill (also CETS, MTE, and others, which need recompilation or hardware) | **none** |
| On-demand provenance over deterministic replay of native binaries | Arnold, JetStream, (VulnScan, REVEN taint, partial) | **none** (all address/byte-keyed) |
| Demand-driven slice construction | Zhang–Gupta LP, Postolski et al. | **none** (source/Java, eager trace or managed heap) |
| Syscall input as slice source | Arnold, JetStream, PANDA, Redux, Pernosco | **none** with lifetime identity |
