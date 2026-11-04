# StateSolver SLR: Search Protocol

**Review question.** Has any prior work built a *query-driven* reversible-debugging technique for *unmodified native x86-64 binaries* that reconstructs *machine-level causal slices on demand through deterministic replay* (without eagerly recording a full dynamic dependence graph), using *allocation-lifetime identity (address + epoch)* so that temporal heap dependencies are told apart across virtual-address reuse, crossing register, memory, and pointer copies, alloc/free/realloc boundaries, and system-call input, and producing a *causal explanation* of a crash? (Research direction §§3, 4, 8, 10, 11; tracking issue 01, "Literature review".)

**Date all queries were run:** 2026-11-04
**Reviewer:** automated agent pass (Claude). A human must check it before the novelty gate is closed.

## 1. Sources and how they were reached

The agent had no authenticated access to the native search interfaces of any database. Every query went through a general web-search tool (`WebSearch`, in "standard" or "extended" mode). Some queries were restricted to one domain. Candidates were then confirmed against publisher, USENIX, DBLP, researchr, arXiv, vendor, or author pages (`WebFetch`). For a few works, the full-text PDF was downloaded and read with `pdftotext`.

| Source | How it was searched | Coverage caveat |
|---|---|---|
| ACM DL / IEEE Xplore / USENIX | open web search; venue pages fetched to confirm | No native ACM or IEEE search, so hit counts are not the databases' own. |
| arXiv | open web search plus a domain-restricted query (`arxiv.org`); abstract pages fetched | Reasonable coverage for 2022–2026 LLM and slicing work. |
| Vendor and grey literature | pernos.co, docs.undo.io, learn.microsoft.com (TTD), Tetrane/eShard REVEN docs and blog, the MSRC blog (VulnScan), Valgrind and ASan documentation, patents (USPTO / Google Patents) | Included because Pernosco, TTD, Undo, and REVEN are documented only there. Vendor claims are marked as such. |
| Google Scholar | **not queried** (the tool has no Scholar access) | **Gap.** A human must re-run Q-A1…Q-F4 in Scholar and run forward-citation searches on Undangle, FreeWill, Arnold, and DrDebug (see verdict.md). |

**Hit counts.** The tool returns about 10 ranked results per query and no total, so every "Hits" value is "n/a (top ~10)". "Relevant" counts results that passed screening and entered `matrix.md`.

**Full-text checks (beyond abstract):** Undangle (ISSTA 2012; §3, pointer taint label = root address + instruction counter, and its discussion of reuse); FreeWill (USENIX Security 2022; §4.1, UAF object lifetime identification under reallocation); Arnold / Eidetic Systems (OSDI 2014; how lineage is split between an eager inter-process index and query-time intra-process taint via replay); Postolski et al. (arXiv 2211.04683; implementation language). The following were read on vendor pages: Pernosco's dataflow and UAF example pages, Pernosco's vision page (omniscient database), the MSRC VulnScan blog post, and the Intel patent US 10,198,335.

## 2. Inclusion and exclusion criteria

**Include** a work if it does at least one of the following:
- I1. Provides reverse, time-travel, or omniscient debugging, or deterministic record/replay used for debugging or analysis.
- I2. Computes dynamic slices, dynamic dependences, data-flow or taint provenance, or lineage of runtime values, whether eagerly or on demand.
- I3. Assigns per-allocation identity (keys, tags, labels, capabilities, lifetimes) to heap objects or pointers, or reasons about dangling pointers and memory reuse.
- I4. Performs crash or vulnerability root-cause analysis from traces, dumps, or replays of native programs.
- I5. (Harness stage) 2020–2026 work that attaches LLMs or agents to debuggers, record/replay, or execution traces.

**Exclude** a work if:
- E1. It is a tutorial, textbook, or generic blog with no original mechanism. Such sources were used for background only.
- E2. It concerns debugging LLM agents themselves (agent-trajectory replay such as AgentRx, AgentDebugX, DoVer, Chronicle), not program executions. These were noted and excluded.
- E3. It is distributed-systems or microservice RCA with no memory or instruction-level provenance. Zeno is kept as a must-cite only because the brief lists it.
- E4. Its bibliographic identity could not be confirmed. Such works are marked **unverified** in the matrix, and no claim rests on them.

## 3. Queries run (2026-11-04)

Mode: S = standard, X = extended. "Rel." = number of results entering the matrix.

### A. Time-travel / omniscient debuggers and Pernosco

| # | Query string | Source / filter | Mode | Hits | Rel. |
|---|---|---|---|---|---|
| A1 | `Pernosco dataflow view "memory" allocation malloc free how it works` | web | X | n/a | 1 (Pernosco UAF example) |
| A2 | `Pernosco "dataflow" "memory allocation" OR "malloc" docs pernos.co` | `pernos.co` | S | n/a | 3 (dataflow page, interposition blog, overview) |
| A3 | `Pernosco omniscient database precomputed index rr replay architecture blog O'Callahan "memory" writes` | web | X | n/a | 3 (vision page, rr remix post, Rust talk slides) |
| A4 | `WinDbg Time Travel Debugging data model queries TTD.Memory TTD.Calls heap "TTD.Heap" or "Heap" objects` | web | X | n/a | 2 (TTD object model, TTDQueries sample) |
| A5 | `Undo LiveRecorder UDB time travel debugging "last" command memory watch heap allocation` | web | S | n/a | 2 (UDB quickref, Undo blog) |
| A6 | `QIRA timeless debugger geohot records every instruction memory query` | web | S | n/a | 1 |
| A7 | `eShard Tetrane REVEN timeless analysis taint backward memory history allocation "use after free" detection` | web | X | n/a | 2 (REVEN UAF blog and notebook) |
| A8 | `REVEN "memory history" resource replay "taint" on-demand OR "computed" trace REVEN documentation replay resources` | web | S | n/a | 1 (What-is-REVEN doc) |
| A9 | `Boothe "Efficient algorithms for bidirectional debugging" PLDI 2000` | web | S | n/a | 1 |
| A10 | `Pothier Tanter Piquer "Scalable omniscient debugging" OOPSLA 2007 TOD; omniscient debugging partial trace re-execution on demand` | web | S | n/a | 1 |
| A11 | `"Engineering record and replay for deployability" rr USENIX ATC 2017 O'Callahan Jones Froyd` | web | S | n/a | 1 |
| A12 | `time travel debugging heap allocation history "use-after-free" reverse debugging freed memory reused record replay paper` | web | X | n/a | 0 (**no academic work on allocation-aware TTD found**) |

### B. Dynamic slicing, demand-driven / lazy dependence

| # | Query string | Source / filter | Mode | Hits | Rel. |
|---|---|---|---|---|---|
| B1 | `Zhang Gupta Gupta "Precise dynamic slicing algorithms" ICSE 2003 demand-driven LP algorithm` | web | S | n/a | 1 |
| B2 | `"WET" whole execution traces Zhang Gupta MICRO 2004 compacted representation dependences` | web | S | n/a | 1 |
| B3 | `on-demand dynamic slicing using deterministic replay re-execution checkpoint instead of storing dependence graph` | web | X | n/a | 4 (Postolski et al. 2022; DrDebug; RecovSlicing 2025; graph-less slicing, unverified) |
| B4 | `DrDebug deterministic replay based cyclic debugging dynamic slicing CGO 2014 PinPlay region slices` | web | S | n/a | 1 |
| B5 | `"Dynamic slicing long running programs through execution fast forwarding" Zhang Tallam Gupta FSE 2006` | web | S | n/a | 1 |
| B6 | `lazy dynamic dependence graph construction demand-driven dynamic slicing binary x86 replay "on demand"` | web | X | n/a | 3 (Zhang & Gupta PLDI 2004; Ray Wang thesis; Postolski) |
| B7 | `Ray Wang MIT thesis "A system for dynamic slicing and program visualization" PANDA` | web | S | n/a | 1 |
| B8 | `Redux data flow graphs Valgrind Nethercote Mycroft 2003 dynamic dataflow binary` | web | S | n/a | 1 |
| B9 | `"Debugging with dynamic slicing and backtracking" Agrawal DeMillo Spafford Software Practice and Experience 1993 SPYDER` | web | S | n/a | 1 |
| B10 | `Whyline "debugging reinvented" asking and answering why and why not questions Ko Myers ICSE 2008` | web | S | n/a | 1 |
| B11 | `dynamic slicing heap object identity allocation lifetime address reuse binary execution trace` | web | X | n/a | 2 (Wang & Roychoudhury TOPLAS 2007 background; DoubleTake). **No binary slicer disambiguating reused heap addresses was found.** |
| B12 | `"Memory indexing" canonicalizing addresses across executions Sumner Zhang FSE 2010` | web | S | n/a | 1 |
| B13 | `rr record replay dynamic slicing OR backward taint tool on top of rr replay native binaries research` | web | X | n/a | 0 (**no published rr-based slicer found**) |
| B14 | `"allocation epoch" OR "allocation generation" OR "object generation" heap reuse dynamic dependence debugging replay native` | web | X | n/a | 1 (DoubleTake). The only "allocation epoch" debugging hit was this project's own GitHub issue (elixpo/statesolver #4), which was excluded as self. |

### C. Analysis decoupled via replay

| # | Query string | Source / filter | Mode | Hits | Rel. |
|---|---|---|---|---|---|
| C1 | `Aftersight decoupling dynamic program analysis from execution in virtual environments USENIX ATC 2008` | web | S | n/a | 1 |
| C2 | `Eidetic systems OSDI 2014 Arnold retroactive queries provenance replay Devecsery` | web | S | n/a | 1 (+ PDF read) |
| C3 | `JetStream cluster-scale parallelization of information flow queries ODSI 2016 replay` | web | S | n/a | 1 |
| C4 | `Tralfamadore unifying source code and execution experience EuroSys 2009 Lefebvre trace analysis` | web | S | n/a | 1 (the "Execution Mining" paper and UBC theses; **the EuroSys 2009 venue was not confirmed**) |
| C5 | `PANDA platform for architecture-neutral dynamic analysis record replay taint2 plugin` | web | S | n/a | 1 |
| C6 | `"Repeatable reverse engineering with PANDA" Dolan-Gavitt PPREW 2015 taint tainted_branch` | web | S | n/a | 1 |
| C7 | `ReVirt enabling intrusion analysis through virtual-machine logging and replay OSDI 2002; BackTracker backtracking intrusions SOSP 2003` | web | S | n/a | 2 |
| C8 | `Sledgehammer cluster-fueled debugging OSDI 2018 Quinn Flinn replay retroactive queries` | web | S | n/a | 1 |

### D. Crash / UAF root-cause analysis from traces

| # | Query string | Source / filter | Mode | Hits | Rel. |
|---|---|---|---|---|---|
| D1 | `REPT reverse debugging of failures in deployed software OSDI 2018 Intel PT` | web | S | n/a | 1 |
| D2 | `ARCUS symbolic root cause analysis exploits production systems USENIX Security` | web | S | n/a | 1 (**venue is USENIX Security 2021, not 2022**) |
| D3 | `AURORA statistical crash analysis automated root cause diagnosis USENIX Security 2020 Blazytko` | web | S | n/a | 1 |
| D4 | `POMP postmortem program analysis hardware-enhanced reverse execution USENIX Security 2017 memory alias` | web | S | n/a | 1 (+ POMP++ noted) |
| D5 | `Kairux failure diagnosis inflection point OSDI 2019; CREDAL locate crash memory corruption core dump source` | web | S | n/a | 1 (**Kairux is SOSP 2019, not OSDI**) |
| D6 | `CREDAL towards locating memory corruption vulnerability with core dump CCS 2016 Xu Mu` | web | S | n/a | 1 |
| D7 | `HOTracer heap overflow use-after-free root cause trace analysis` | web | S | n/a | 3 (HOTracer: heap overflow only; FreeWill; Intel patent) |
| D8 | `use-after-free root cause analysis execution trace allocation free use triple tool paper` | web | X | n/a | 2 (FreeWill, iReplayer) |
| D9 | `use-after-free diagnosis root cause binary trace "reallocated" freed memory reused object "dangling pointer" creation point automatic analysis 2019..2025` | web | X | n/a | 3 (FreeWill, Intel patents, GUEB) |
| D10 | `taint analysis over Time Travel Debugging traces TTD backward taint tool binary root cause crash 2022 2023 2024` | web | X | n/a | 3 (MSRC VulnScan, RETracer, Taintgrind as background) |
| D11 | `RETracer triaging crashes reverse execution partial memory dumps ICSE 2016 Cui Peinado` | web | S | n/a | 1 |
| D12 | `"Execution reconstruction" harnessing failure reoccurrences failure reproduction PLDI 2021 Intel PT symbolic` | web | S | n/a | 1 |
| D13 | `Failure sketching Gist SOSP 2015 Kasikci; "Lazy diagnosis of in-production concurrency bugs" Snorlax SOSP 2017` | web | S | n/a | 2 |
| D14 | `Zeno diagnosing performance problems temporal provenance NSDI 2019; SPYDER Agrawal DeMillo Spafford debugging with dynamic slicing and backtracking` | web | S | n/a | 1 (Zeno) |
| — | patent full text: `patents.google.com/patent/US10198335` | WebFetch | — | — | 1 |

### E. Temporal memory safety, allocation identity, heap-history tools

| # | Query string | Source / filter | Mode | Hits | Rel. |
|---|---|---|---|---|---|
| E1 | `Undangle early detection of dangling pointers use-after-free ISSTA 2012 Caballero` | web | S | n/a | 1 (+ PDF read) |
| E2 | `CETS compiler enforced temporal safety for C ISMM 2010 Nagarakatte lock and key identifier` | web | S | n/a | 1 |
| E3 | `safe C execution "lock-and-key" capability temporal errors Xu DuVarney Sekar 2004 OR "Patil Fischer" "lock and key"` | web | S | n/a | 1 (Zhou et al. 2022 fat pointers). **Patil & Fischer 1997 and Xu et al. 2004 were located only via secondary citation.** |
| E4 | `MarkUs drop-in use-after-free prevention low overhead allocators IEEE S&P 2020` | web | S | n/a | 1 |
| E5 | `DangSan scalable use-after-free detection EuroSys 2017; DangNULL preventing use-after-free NDSS 2015; FreeSentry NDSS 2015` | web | S | n/a | 2 (DangSan, DangNULL). **FreeSentry's own page was not located.** |
| E6 | `Cornucopia temporal safety CHERI heaps IEEE S&P 2020 revocation capability provenance` | web | S | n/a | 1 |
| E7 | `DangZero OR pSweeper OR CRCount OR Oscar dangling pointer use-after-free prevention page aliasing CCS NDSS USENIX` | web | S | n/a | 2 (pSweeper, CRCount) |
| E8 | `Memory tagging HWASan ARM MTE tag per allocation retag on free temporal safety Serebryany arXiv 2018 "Memory Tagging and how it improves C/C++ memory safety"` | web | S | n/a | 1 |
| E9 | `AddressSanitizer "freed by thread" "previously allocated by thread" quarantine use-after-free report Serebryany USENIX ATC 2012` | web | S | n/a | 1 |
| E10 | `Valgrind memcheck --track-origins=yes origin of uninitialised value heap block allocated manual` | web | S | n/a | 1 |
| E11 | `Bond Nethercote "Tracking bad apples: reporting the origin of null and undefined value errors" OOPSLA 2007 memcheck origin tracking` | web | S | n/a | 1 |
| E12 | `dynamic taint tracking object identity per-allocation label heap "allocation" taint source free reuse provenance binary Pin` | web | X | n/a | 1 (Doudalis/Clause et al., memory protection via dynamic tainting) |

### F. LLM / agentic debugging over executions (harness stage)

| # | Query string | Source / filter | Mode | Hits | Rel. |
|---|---|---|---|---|---|
| F1 | `LLM agent debugging record replay rr time-travel debugger tool use arXiv 2024 2025` | web | X | n/a | 1 (Undo agentic-debugging poster); the rest were E2 exclusions |
| F2 | `ChatDBG AI-powered debugging assistant arXiv lldb gdb agent` | web | S | n/a | 1 |
| F3 | `debug-gym text-based environment interactive debugging LLM agents pdb Microsoft arXiv 2025` | web | S | n/a | 1 |
| F4 | `LLM crash root cause analysis execution trace memory corruption use-after-free agent arXiv` | web | X | n/a | 1 (arXiv 2605.04251, background only); the rest were E2 |
| F5 | `Undo "agentic debugging" explain UDB MCP time travel debugging AI agent 2025` | web | S | n/a | 1 (Undo AI / MCP docs) |
| F6 | `CrashFixer LLM agent Linux kernel crash resolution arXiv 2025 OR "kGym" kernel crash LLM benchmark` | web | S | n/a | 2 |
| F7 | `LLM agent "time-travel debugging" OR "record and replay" OR "reverse debugging" C/C++ root cause arXiv 2025 2026 paper` | `arxiv.org` | X | n/a | 2 (DebugHarness 2604.03610; RecovSlicing 2508.18721) |

**Total:** 67 search queries, about 20 page or PDF fetches.

## 4. Screening notes

- **"Epoch" collision.** JetStream and DoubleTake use "epoch" to mean a *time slice of execution*, not an allocation lifetime. Keep that distinction in mind when re-running searches.
- **Self-hit.** One "allocation epoch" hit was this project's own tracking issue (github.com/elixpo/statesolver/issues/4). It was excluded.
- **Venue corrections against the brief.** ARCUS appeared at USENIX Security **2021**. Kairux appeared at **SOSP** 2019. JetStream appeared at OSDI 2016.
- **Vendor material.** The capabilities of Pernosco, Undo, TTD, REVEN, and VulnScan come from vendor documentation or blogs and were not tested independently.
