# StateSolver SLR: Forward-Citation Check

**Date:** 2026-11-04 | **Companion to:** `verdict.md`, `matrix.md`, `protocol.md`

**Target gap.** Find any paper that cites a closest work *and* supplies the missing piece. The missing piece is on-demand, replay-driven backward slicing of native binaries whose memory dependences are allocation-lifetime (epoch) aware across address reuse.

## Method

- **Sources.** OpenAlex `filter=cites:{id}` was used for every seed, with all pages pulled through the cursor. Semantic Scholar Graph `/paper/{id}/citations` was used where OpenAlex lacked the seed (FreeWill) or to cross-check coverage (Undangle, DrDebug). The Semantic Scholar API rate-limited heavily (HTTP 429) and was retried with back-off. OpenCitations was spot-checked and turned out to be a subset of OpenAlex.
- **Screening.** Every citing title was screened. Abstracts were read for anything touching binaries, replay, slicing, root-cause analysis, temporal memory errors, or object identity. Full text was read for the two works that survived screening (WATCHER and Ravel).
- **Second-level sweep.** When a citing paper looked close, its own citers were also pulled. This was done for WATCHER, Ravel, iReplayer, and Postolski et al.
- **Limits.** Google Scholar and the ACM DL "cited by" lists were not queried, so the counts below undercount. For example, OpenAlex lists Undangle at about 165 citations, and Scholar is usually higher. Only works that were actually retrieved are listed.

## Per-seed results

### 1. Undangle (Caballero et al., ISSTA 2012)

- **IDs:** OpenAlex `W2059278087`; S2 `DOI:10.1145/2338965.2336769`.
- **Screened:** 163 citers from OpenAlex and 199 from S2, about 230 unique after the union.
- **Finding:** Almost all citers are temporal-safety *detectors or mitigations*: DangNULL, FreeSentry, DangSan, Oscar, CRCount, pSweeper, MarkUs, HeapExpo, ViK, PACMem, CAMP, BUDAlloc, and others. The rest are static UAF finders and allocator-hardening work. None does backward slicing or replay. The closest are listed below.

| Work | Year / venue | Link | Relevance / why not overlapping |
|---|---|---|---|
| UAFSan: an object-identifier-based dynamic approach for detecting UAF | 2021, ISSTA | https://doi.org/10.1145/3460319.3464835 | Gives each allocation and its pointers a unique ID, explicitly to survive reallocation. This confirms that per-allocation identity is prior art. It is a source-level sanitizer, a forward check, with no slicing and no replay. |
| BSan: identifier-based memory error detector for COTS binaries | 2025, ICSE-Companion | https://doi.org/10.1109/icse55347.2025.00020 | Propagates identifier-based (per-allocation) identity on **binaries**. It is a detector, not a slicer, and has no replay or backward query. It adds to the "identity is not new" list. |
| EarlyDLDetect: early root-cause locator of dangling pointers and leaks | 2024, IEEE Access | https://doi.org/10.1109/access.2024.3515133 | Reports where a dangling pointer originates, much like Undangle. It has no slicing, no replay, and no general-value dependences. |
| Who allocated my memory? (MemBrush) | 2013, WCRE | https://doi.org/10.1109/wcre.2013.6671277 | Detects custom allocators in stripped binaries. It does not overlap, but it is **useful** for the custom-allocator epoch problem flagged in `verdict.md` §Objection-5. |
| Finding the needle in the heap | 2016, SSPREW | https://doi.org/10.1145/3015135.3015137 | Uses static analysis plus DSE to *trigger* UAFs in binaries. It has no slicing. |
| ViK / PTAuth / PACMem / CAMP / MTSan | 2020–2024 | (via OpenAlex/S2) | Object-ID or tag-based temporal safety. These are runtime enforcement, not causal queries. |

### 2. FreeWill (He et al., USENIX Security 2022)

- **IDs:** Not indexed in OpenAlex; title searches returned nothing. S2 paperId `12906c0c3a3a5b7c2b7e55051f8a5cd5af022755`.
- **Screened:** 19 citers, the complete S2 list.
- **Finding:** No citer extends FreeWill's lifetime-aware backward slicing to replay or to general values.

| Work | Year / venue | Link | Relevance / why not overlapping |
|---|---|---|---|
| DebugHarness | 2026, arXiv 2604.03610 | https://arxiv.org/abs/2604.03610 | Already in `verdict.md` as the harness competitor. It is an LLM driving dynamic debugging, with no lifetime-aware slicing. |
| OptRCA | 2025, TOSEM | https://doi.org/10.1145/3736718 | An improvement on AURORA: statistical RCA over fuzzing crash sets. It has no slicing and no lifetime model. |
| Crash Path Analysis Based on Fused Dynamic Control and Data Flows | 2025, AEMCSE | https://doi.org/10.1109/AEMCSE65292.2025.11042809 | Builds a crash path graph from an eagerly recorded trace plus taint. It has no replay and no allocation identity. |
| Racing on the Negative Force (RL on counterexamples) | 2024, USENIX Sec | (S2) | Statistical RCA, not slicing. |
| NeuDep | 2022, FSE | https://doi.org/10.1145/3540250.3549147 | Neural, static prediction of binary memory dependences. It is not dynamic and not lifetime-aware. |
| CountDown, PET, the cross-entry UAF work, Petri-net UAF, Fast Pointer Nullification, MTE intra-object, RemOTA, PatchAgent | 2023–2026 | (S2) | Kernel fuzzing, static UAF detection, mitigation, or repair. None overlaps. |

### 3. Arnold / Eidetic Systems (Devecsery et al., OSDI 2014)

- **IDs:** OpenAlex `W2912412735`.
- **Screened:** 85 citers.
- **Finding:** Citers fall into record/replay systems (Light, Samsara, Castor, iReplayer, Sparse R&R), provenance and attack investigation (RAIN, MPI, Mnemosyne, C²SR, HARDLOG), and the Flinn group's follow-ups (JetStream, Sledgehammer, Optimistic Hybrid Analysis, Iodine, Knockoff). None adds allocation-lifetime identity to replay-time lineage.

| Work | Year / venue | Link | Relevance / why not overlapping |
|---|---|---|---|
| **Pinpointing Vulnerabilities (Ravel)**, Chen, Khandaker & Wang | 2017, AsiaCCS | https://doi.org/10.1145/3052973.3053033 | **Partly overlapping; not in `matrix.md`.** It records production execution, and on attack it *replays* with instrumentation. It then detects def-use anomalies against a pre-computed DFG and "instruments the memory allocation/free functions to keep track of the memory life-time" to classify UAF and double-free. Full text read. *Why it falls short:* (a) during replay it logs **all** memory accesses eagerly; (b) its def-use is **address-based**, and lifetime information only *labels* the bug class; (c) its output is a def/use candidate pair, not a backward slice; (d) it is FreeBSD-only. Cite it as prior art for "replay + allocation-lifetime tracking for vulnerability localization". |
| Sledgehammer | 2018, OSDI | (in matrix) | Already covered. |
| Iodine / Optimistic Hybrid Analysis | 2019 S&P / 2018 ASPLOS | https://doi.org/10.1109/sp.2019.00043 | Forward DIFT over replay, made faster. It has no lifetime identity. |
| C²SR | 2021, NDSS | https://doi.org/10.14722/ndss.2021.23128 | Browser crime-scene reconstruction. It does not work at the native memory level. |
| Scaling Causality Analysis for Production Systems (Ph.D. thesis, Michigan) | 2016 | https://openalex.org/W2604751446 | Arnold/JetStream-lineage taint at scale. It adds no lifetime identity. |

### 4. JetStream (Quinn et al., OSDI 2016)

- **IDs:** OpenAlex `W2579121572`.
- **Screened:** 18 citers.
- **Finding:** These are Sledgehammer, Iodine, ShortCut, Knockoff, RAIN, the refinable cross-host tagging work (USENIX Sec 2018), C²SR, Sdft, podft, FSAFlow, HARDLOG, and EZEE. All are taint or provenance acceleration. **None is relevant** to lifetime-aware slicing.

### 5. DrDebug (Wang et al., CGO 2014)

- **IDs:** OpenAlex `W4242346613` and `W1973842602` (duplicate records); S2 `DOI:10.1145/2544137.2544152`.
- **Screened:** 36 citers from OpenAlex and 47 from S2, about 55 unique.

| Work | Year / venue | Link | Relevance / why not overlapping |
|---|---|---|---|
| **WATCHER: In-Situ Failure Diagnosis**, Liu, Silvestro, Zhang, Huang & Liu | 2020, PACMPL (OOPSLA) | https://doi.org/10.1145/3428211 (PDF: https://par.nsf.gov/servlets/purl/10289644) | **The most threatening find, and not in `matrix.md`.** Full text read. On a crash, it uses iReplayer's *identical re-executions* of **unmodified binaries** (no recompilation). It combines XED binary analysis with **hardware watchpoints** installed per re-execution to walk the failing value backwards level by level. It stops at "an immediate value, an input parameter … or the result of a system call", and reports "the backward dynamic data dependence slices". No dependence graph is recorded. This is on-demand, replay-driven backward slicing of native binaries back to syscall input. **On reuse it is explicitly address-only:** §3.4 says "tracking the last write naturally fixes the address re-utilization issue in which a heap object will be re-utilized after its deallocation". *Why it does not close the gap:* (a) memory identity is the raw address with last-writer-wins, so there is no allocation epoch; (b) its 24-bug evaluation covers stack and heap overflows, NULL dereferences, divide-by-zero, assertions, and aborts, and contains **no UAF or reuse bug**; (c) it diagnoses only the last iReplayer epoch, and the number of hardware debug registers bounds how many addresses it watches per run; (d) it targets the crash value only, not arbitrary queries. |
| iReplayer | 2018, PLDI | (in matrix) | WATCHER's substrate. Its own citers (22) were swept: AggrePlay, RIPT, Sparse R&R, ER, AITIA, Reverb, and others are all R&R or reproduction. None does slicing with identity. |
| Dynamic Slicing by On-demand Re-execution (Postolski et al.) | 2022, arXiv 2211.04683 | (in matrix) | Already covered; its single OpenAlex citer is irrelevant. |
| Focused Dynamic Slicing using an Abstract Memory-Model (DynAbs) | 2022, arXiv 2211.04560 | https://arxiv.org/abs/2211.04560 | Replaces memory *addresses* with symbolic terms for dependences, which is an identity change. It is C#/managed and trace-based, with no reuse issue and no replay. It is related in spirit but does not overlap. |
| Execution Reconstruction; Failure sketching; Lazy Diagnosis; Memento; DeepErr | 2015–2025 | (OpenAlex/S2) | Reproduction or statistical diagnosis. None does lifetime-aware slicing. |

### 6. Zhang, Gupta & Zhang, "Precise Dynamic Slicing Algorithms" (ICSE 2003)

- **IDs:** OpenAlex `W2160510992` and `W4235261014`.
- **Screened:** 218 citers.
- **Finding:** These are mostly fault localization, Java/.NET/Python/Android slicers, and slicing for bloat or malware. The binary-relevant hits are listed below. None uses replay plus allocation identity.

| Work | Year / venue | Link | Relevance / why not overlapping |
|---|---|---|---|
| Efficient Utilization of Secondary Storage for Scalable Dynamic Slicing (Kargén & Shahmehri) | 2014, SCAM | https://doi.org/10.1109/scam.2014.24 | Slices x86 executables, but over an **eager** on-disk DDG with address dependences. The 2019 thesis "Scalable Dynamic Analysis of Binary Code" (https://doi.org/10.3384/diss.diva-157626) has the same scope. |
| A fast assembly-level reverse execution method via dynamic slicing | 2004 | https://doi.org/10.5555/998675.999456 | Reverse execution along a slice at assembly level. It has no allocation identity. |
| Binary Software Vulnerability Analysis Based on Bidirectional-Slicing | 2012, IMCCC | https://doi.org/10.1109/imccc.2012.203 | Binary dynamic slicing for crashes. Address-based, no replay. |
| Mandoline | 2021, ICST | https://doi.org/10.1109/icst49551.2021.00022 | "On-demand execution trace analysis" with trace-based alias analysis, but for Android/Java (managed identity). |
| MSli: Multi-stage On-Demand Program Slicing | 2026, PACMSE | https://doi.org/10.1145/3832255 | "On-demand" is static (pointer and MHP analysis), not dynamic. It does not overlap. |
| WATCHER | 2020 | see §5 | Also cites this seed. |

## Overall conclusion

**The GO verdict survives, but it must narrow further, and `verdict.md`'s recommended novelty sentence is no longer safe as written.**

1. **WATCHER (OOPSLA 2020) already does on-demand, replay-driven backward data-dependence slicing of unmodified native binaries from the failing value back to system-call input, with no recorded dependence graph.** The sentence's lead clause ("first technique that answers backward causal queries … by reconstructing only the queried dependences through checkpointed deterministic replay") is therefore **not defensible**. Add WATCHER to "Do NOT claim", to the must-cite list, and to `matrix.md`.
2. **What remains novel is precisely the part WATCHER explicitly dismisses.** WATCHER asserts that address-based last-writer tracking "naturally fixes" heap address re-utilization, and it never evaluates a UAF or reuse bug. No retrieved paper (Undangle, FreeWill, Arnold, JetStream, DrDebug, Zhang–Gupta, and the second-level sweeps of WATCHER, Ravel, iReplayer, and Postolski) resolves memory dependences against **(address, allocation-epoch)** identity inside an on-demand, replay-driven backward slice. The claim should be restated as *lifetime-qualified dependence semantics for replay-driven on-demand slicing*, plus the empirical result that address-only (WATCHER-style) slicing gives wrong or inflated chains under reuse.
3. **WATCHER is the natural address-only baseline** for the ablation in `verdict.md` §Pre-emption-1/3. Its "last-write fixes reuse" claim is a concrete statement the evaluation can test and refute. The likely failure cases are reads of uninitialised reallocated memory that pick up a prior lifetime's writer, and stale-pointer writes attributed to the new object's history.
4. **Ravel (AsiaCCS 2017)** must also be cited. It shows that replay combined with allocation/free lifetime tracking for vulnerability localization is prior art. Its def-use analysis is address-based and computed from an eager replay log, and lifetime only labels the bug class, so it does not threaten the narrowed claim.
5. **UAFSan (ISSTA 2021) and BSan (ICSE-C 2025)** strengthen the existing "identity is not new" caveat, now at binary level too. They do not affect the slicing claim.

**Residual risk:** WATCHER's citers could not be retrieved (both OpenAlex and S2 return 0, which is likely an indexing gap). A manual Google Scholar "cited by" check on WATCHER and on Ravel is the highest-value remaining step.
