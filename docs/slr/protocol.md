# MorphStack SLR: Search Protocol

**Review question.** Has any prior work implemented *per-activation, bidirectional, online selection and migration among multiple activation-state representations (contiguous stack, segmented stacklets, heap frames), driven by runtime behaviour, and evaluated against a hindsight oracle with explicit switching cost*? (Research plan §3, tracking issue 01.)

**Date all queries were run:** 2026-11-04
**Reviewer:** automated agent pass (Claude), to be checked by a human before the novelty gate is closed.

## 1. Sources and how they were reached

The agent had no direct, authenticated access to the database search interfaces. Every query went through a general web-search tool (`WebSearch`). Some queries were restricted to a database's domain with `allowed_domains`. Candidate papers were then confirmed against publisher, DBLP, researchr, arXiv, or author pages (`WebFetch`, or a PDF read with `pdftotext`).

| Source | How it was searched | Coverage caveat |
|---|---|---|
| ACM Digital Library | domain-restricted web search (`dl.acm.org`), plus confirming DOIs on publisher and mirror pages | The ACM DL's native search was not used, so the hit counts are not ACM's. |
| IEEE Xplore | domain-restricted web search (`ieeexplore.ieee.org`) | Few results were returned. Needs a native re-run. |
| USENIX | domain-restricted web search (`usenix.org`) | Few results were returned. Needs a native re-run. |
| arXiv | domain-restricted web search (`arxiv.org`) and abstract pages | Reasonable coverage. |
| Springer / ScienceDirect | domain-restricted web search (`link.springer.com`, `sciencedirect.com`) | Few results were returned. Needs a native re-run. |
| Google Scholar | **not queried directly** (the tool has no Scholar access) | **Gap.** A human must re-run the core queries in Scholar, including forward-citation searches on the closest works (see verdict.md). |
| Grey literature | open web search: design documents, JEPs, mailing lists, patents (USPTO / Google Patents) | Included because Go, Loom, Rust, and Cog VM design decisions are documented only there. |

**Hit counts.** The search tool returns about 10 ranked results per query and does not report a total. Every count below is therefore "n/a (top ~10 returned)". The "relevant" column counts results that passed screening and entered the matrix or the background notes.

## 2. Inclusion and exclusion criteria

**Include** a work if it does at least one of the following:
- I1. Implements or evaluates a representation of activation records or control stacks: contiguous, growable or copied, segmented or stacklet, linked heap frames, CPS or heap continuations, or stackless state machines.
- I2. Migrates live activation state between representations or locations, such as stack to heap, heap to stack, segment splitting, OSR or deoptimization, or freeze/thaw.
- I3. Makes an online or adaptive runtime decision among alternative implementations: JIT tiering, GC selection, or collection and representation switching.
- I4. Provides the theory for online decisions with switching or movement cost, compared against an offline or hindsight optimum: MTS, smoothed OCO, or regret with switching cost.

**Exclude** a work if:
- E1. It is a textbook, lecture note, or tutorial with no original mechanism. Such sources were used only for background.
- E2. It concerns process or VM migration across machines, where the "representation" stays fixed and only the host changes. Examples are CloneCloud and heterogeneous-ISA migration.
- E3. It concerns page or tiered-memory migration of data (not activation state). These are noted as distant analogues only.
- E4. It could not be located or its bibliographic identity could not be confirmed. Nothing in this category was cited as fact. Items with partially confirmed details are marked "unverified" in the matrix.

**Screening depth.** Most works were screened from the abstract and metadata. The following were checked against full text: Farvardin & Reppy (PLDI 2020; §3.6 strategy definitions), Sivaramakrishnan et al. (PLDI 2021; §5.2 heap-allocated fibers), and Hieb, Dybvig & Bruggeman (PLDI 1990; overflow, underflow, and reinstatement mechanism).

## 3. Queries run (2026-11-04)

### 3.1 Research plan §8 terms

| # | Query string | Source / filter | Hits | Relevant |
|---|---|---|---|---|
| Q1 | `adaptive stack representation runtime activation record migration heap stack` | web (extended) | n/a | 0 (textbook material and one process-migration patent) |
| Q2 | `"activation record" heapification continuation frame migration stack heap hybrid` | web (extended) | n/a | 3 (Hieb et al. 1990, Appel & Shao 1996, funarg background) |
| Q3 | `adaptive coroutine stacks dynamic switching stackful stackless runtime` | web (extended) | n/a | 2 (stackless vs. stackful RDMA study 2025; Photon blog as background) |
| Q4 | `runtime adaptively chooses per-frame stack or heap allocation based on profiling continuation capture frequency` | web (extended) | n/a | 1 (Oracle patent US 11,604,729) |
| Q5 | `online algorithm choose stack implementation segmented contiguous heap switching cost competitive analysis coroutines` | web (extended) | n/a | 2 (Farvardin & Reppy 2020; stackless vs. stackful 2025). **No work combining the two areas was found.** |
| Q6 | `"stack" "heap" frames "migrate back" OR "heap-to-stack" continuation resume copy frames onto stack lazily underflow handler` | web (extended) | n/a | 3 (Hieb et al.; coroutine-resumption patent US 9,003,377 as background; OCaml 5) |
| Q7 | `"adaptive" continuation representation runtime switch between stack-based and heap-based continuations dynamically` | web (extended) | n/a | 3 (Clinger et al.; Flatt's continuation-marks paper as background; Hieb et al.). **No adaptive scheme was found.** |
| Q8 | `adaptive stack representation activation records migration online policy` | `dl.acm.org` (extended) | n/a | 1 (deoptimization patents US 6,240,547 and US 6,601,235, which describe "migrating frames") |
| Q9 | `stack heap hybrid runtime frames adaptive stacklets coroutine` | `arxiv.org` (extended) | n/a | 3 (Libfork 2402.18480; libseff 2512.03083; compiler patent US 10,747,511) |
| Q10 | `stack frame relocation runtime segmented stacks threads adaptive` | `usenix.org` (extended) | n/a | 1 (Oracle lazy-copy patent US 11,726,786) |
| Q11 | `dynamic execution-state representation stack heap migration runtime` | `ieeexplore.ieee.org`, `link.springer.com`, `sciencedirect.com` (extended) | n/a | 0 (all E2) |
| Q12 | `"online activation placement" OR "adaptive continuations" OR "dynamic stack representation" runtime` | web (extended) | n/a | 0. **None of these exact phrases appears in the literature.** |
| Q13 | `runtime stacklets frame relocation stack cache continuation implementation` | web (extended) | n/a | 3 (Ma & Zhong 2023; Clinger et al.; Libfork) |

Section-8 terms covered by the combined queries above: adaptive stack representation, dynamic stack representation, runtime activation record migration, activation record heapification, continuation frame migration, stack heap hybrid runtime, adaptive continuations, runtime stacklets, online activation placement, dynamic execution-state representation, adaptive coroutine stacks, stack cache continuation, and frame relocation runtime.

### 3.2 Additional areas from tracking issue 01

| # | Query string | Source | Hits | Relevant |
|---|---|---|---|---|
| Q14 | `Borodin Linial Saks "An optimal on-line algorithm for metrical task system" JACM 1992` | web | n/a | 1 |
| Q15 | `metrical task systems asymmetric distances quasi-metric competitive ratio online algorithm` | web (extended) | n/a | 2 (Fiat & Mendel unfair MTS; Bartal et al. polylog MTS as background). **No dedicated asymmetric-MTS paper was found beyond BLS's O(\|S\|²) bound.** |
| Q16 | `Bubeck Cohen Lee Lee "Metrical task systems on trees via mirror descent and unfair gluing"` | web | n/a | 1 |
| Q17 | `Antoniadis Coester Elias Polak Simon "Online metric algorithms with untrusted predictions"` | web | n/a | 1 |
| Q18 | `Chen Goel Wierman "Smoothed online convex optimization in high dimensions via online balanced descent" COLT 2018` | web | n/a | 1 |
| Q19 | `Lin Wierman Andrew Thereska "Dynamic right-sizing for power-proportional data centers" switching cost offline optimal` | web | n/a | 1 |
| Q20 | `Zhao Wang Zhang Zhou "Non-stationary online learning with memory and non-stochastic control" AISTATS 2022` | web | n/a | 1 (authors corrected to Zhao, Wang, Zhou) |
| Q21 | `Dekel Ding Koren Peres "Bandits with switching costs: T^{2/3} regret" STOC 2014` | web | n/a | 1 |
| Q22 | `Karlin Manasse Rudolph Sleator "Competitive snoopy caching" Algorithmica 1988` | web | n/a | 1 |
| Q23 | `metrical task system runtime systems application representation switching program optimization offline optimal dynamic programming` | web (extended) | n/a | 2 (OReO, ICDE 2024; work-function DP lecture notes as background) |
| Q24 | `competitive analysis just-in-time compilation online algorithm offline optimal ski rental recompilation decision` | web (extended) | n/a | 0. **No work found that frames JIT decisions as ski rental or MTS.** |
| Q25 | `Ertl "Stack caching for interpreters" PLDI 1995 dynamic stack caching` | web | n/a | 1 |
| Q26 | `Wall "Register windows vs. register allocation" PLDI 1988` | web | n/a | 1 |
| Q27 | `Hieb Dybvig Bruggeman "Representing control in the presence of first-class continuations" PLDI 1990` | web, full-text PDF | n/a | 1 |
| Q28 | `Clinger Hartheimer Ost "Implementation strategies for first-class continuations" Higher-Order and Symbolic Computation` | web | n/a | 1 |
| Q29 | `Bruggeman Waddell Dybvig "Representing control in the presence of one-shot continuations" PLDI 1996` | web | n/a | 1 |
| Q30 | `Kumar Bruggeman Dybvig "Threads yield continuations" Lisp and Symbolic Computation` | web | n/a | 1 (metadata only) |
| Q31 | `Danvy "Formalizing implementation strategies for first-class continuations" ESOP 2000` | web | n/a | 1 |
| Q32 | `Appel Shao "Empirical and analytic study of stack versus heap cost for languages with closures" JFP 1996` | web | n/a | 1 |
| Q33 | `Appel "Compiling with Continuations" Cambridge University Press 1992 heap-allocated activation records SML/NJ` | web | n/a | 1 |
| Q34 | `Shao Appel "Space-efficient closure representations" LFP 1994 SML/NJ heap-allocated frames` | web | n/a | 1 |
| Q35 | `Farvardin Reppy "From folklore to fact: comparing implementations of stacks and continuations" PLDI 2020` | web, full-text PDF | n/a | 1 |
| Q36 | `Go contiguous stacks design document hot split Keith Randall` | web | n/a | 1 (authorship of the design document is unverified) |
| Q37 | `JEP 444 virtual threads stack chunks heap thaw freeze lazy copy` | web, plus a fetch of openjdk.org/jeps/444 | n/a | 1 |
| Q38 | `"Lazy copying of runtime-managed stack frames" patent 11726786` | web | n/a | 1 |
| Q39 | `"Efficient continuation stack storage in languages with a garbage collector" patent 11604729 Oracle` | web | n/a | 1 |
| Q40 | `Retrofitting effect handlers onto OCaml PLDI 2021 Sivaramakrishnan fibers stack segments` | web, full-text PDF | n/a | 1 |
| Q41 | `WasmFX "Continuing WebAssembly with effect handlers" OOPSLA 2023 Phipps-Costin` | web | n/a | 1 |
| Q42 | `Optimistic Stack Allocation and Dynamic Heapification for Managed Runtimes PLDI 2024` | web | n/a | 1 |
| Q43 | `Kotlin coroutines KEEP design continuation passing style state machine suspend functions` | web | n/a | 1 (Kotlin spec) |
| Q44 | `Bierman Russo Mainland Meijer Torgersen "Pause 'n' play: formalizing asynchronous C#" ECOOP 2012` | web | n/a | 1 |
| Q45 | `Rust RFC 2394 async await generators state machine lowering` | web | n/a | 1 (RFC 2394 and RFC 2033) |
| Q46 | `Rust abandon segmented stacks 2013 mailing list Brian Anderson "Abandoning segmented stacks in Rust"` | web | n/a | 1 (rust-dev message; authorship unverified) |
| Q47 | `Deutsch Schiffman 1984 "Efficient implementation of the Smalltalk-80 system" context stack frames converted heap objects` | web | n/a | 1 |
| Q48 | `Cog VM context-to-stack mapping Eliot Miranda married contexts divorce stack pages` | web | n/a | 1 (mailing lists only) |
| Q49 | `Stadler Wimmer Würthinger Mössenböck Rose "Lazy continuations for Java virtual machines" PPPJ 2009` | web | n/a | 1 |
| Q50 | `Pettyjohn Clements Marshall Krishnamurthi Felleisen "Continuations from generalized stack inspection" ICFP 2005` | web | n/a | 1 |
| Q51 | `Lazy Threads stacklets Goldstein Schauser Culler 1996 implementing fast parallel call` | web | n/a | 1 |
| Q52 | `Capriccio scalable threads internet services SOSP 2003 linked stacks` | web | n/a | 1 |
| Q53 | `Lee Boyd-Wickizer Huang Leiserson "Using memory mapping to support cactus stacks in work-stealing runtime systems" PACT 2010` | web | n/a | 1 |
| Q54 | `Leijen "Implementing algebraic effects in C" APLAS 2017 libhandler stack copying` | web | n/a | 1 |
| Q55 | `Ma Zhong "Bringing Segmented Stacks to Embedded Systems" HotMobile` | web | n/a | 1 |
| Q56 | `"On-stack replacement, distilled" D'Elia Demetrescu PLDI 2018` | web | n/a | 1 |
| Q57 | `Hölzle Chambers Ungar "Debugging optimized code with dynamic deoptimization" PLDI 1992` | web | n/a | 1 |
| Q58 | `Arnold Fink Grove Hind Sweeney "Adaptive optimization in the Jalapeño JVM" OOPSLA 2000 cost-benefit` | web | n/a | 1 |
| Q59 | `Soman Krintz Bacon "Dynamic selection of application-specific garbage collectors" ISMM 2004` | web | n/a | 1 |
| Q60 | `Singer Brown Watson Cavazos "Intelligent selection of application-specific garbage collectors" ISMM 2007` | web | n/a | 1 |
| Q61 | `Chameleon adaptive selection of collections PLDI 2009 Shacham Vechev Yahav` | web | n/a | 1 |
| Q62 | `CoCo "sound and adaptive replacement of Java collections" ECOOP 2013 Xu online switching` | web | n/a | 1 |
| Q63 | `Costa Andrzejak "CollectionSwitch: a framework for efficient and dynamic collection selection" CGO 2018` | web | n/a | 1 |
| Q64 | `Bolz Diekmann Tratt "Storage strategies for collections in dynamically typed languages" OOPSLA 2013` | web | n/a | 1 |

## 4. Known gaps (must close before the gate)

1. **Google Scholar.** Re-run Q1–Q13 natively. Run forward-citation searches ("cited by") on Hieb et al. 1990, Farvardin & Reppy 2020, Anand et al. 2024, and JEP 444/Loom. These are the most likely places for an unseen adaptive scheme to appear.
2. **Native ACM DL, IEEE Xplore, and Springer searches.** Record real hit counts.
3. **Patents.** Oracle's Loom patents mention an "adaptive" number of frames to thaw. Read the full claims of US 10,908,910 and US 11,726,786 to confirm that the adaptation is a heuristic over a single representation and not a cost-driven choice among representations.
4. **Asymmetric MTS.** No dedicated paper was located in this pass, beyond the BLS O(|S|²) bound for non-symmetric distances and the unfair-MTS line of work. Search specifically for "directed" or "quasi-metric" MTS before claiming anything about asymmetric bounds.
5. **Not searched in this pass.** GHC stack chunks, GCC `-fsplit-stack`, Lua and Stackless Python, C++20 coroutine heap elision (HALO). These are expected to be fixed-representation designs. Add them to the matrix for completeness.
