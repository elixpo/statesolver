# MorphStack SLR: Novelty Verdict

**Date:** 2026-11-04 | **Inputs:** `protocol.md` (64 queries), `matrix.md` (53 works)

## Verdict: GO (provisional)

This pass found no work that does what MorphStack claims: per-activation, bidirectional, online selection and migration among contiguous, segmented, and heap activation-state representations, driven by observed runtime cost, and evaluated against a hindsight oracle with explicit switching cost.

Each ingredient does exist somewhere. The gap is real but narrow, so the claim must be worded around the **cost-driven policy plus oracle evaluation**, not around the mechanism.

**Two conditions apply.** The verdict becomes final only after (1) a native Google Scholar and ACM DL re-run of Q1–Q13, with forward-citation searches on the five works below, and (2) a read of the full claims of Oracle patent US 10,908,910 / US 11,726,786. If either turns up a cost-driven choice among two or more activation representations, the verdict becomes **PIVOT** to: "first MTS-framed, oracle-evaluated study of activation-representation policies".

## Closest works and how each falls short

1. **Hieb, Dybvig & Bruggeman, PLDI 1990 (Chez Scheme segmented stacks).** Stack segments become heap continuation objects on capture or overflow. A bounded, frame-aligned piece is copied back on reinstatement or underflow. That is bidirectional, near frame granularity, and covers stack, segmented, and heap. *It falls short because* every transition is forced by control semantics (capture, overflow, underflow). Nothing chooses a representation for cost, there are no heap frames that execute in place, and there is no oracle or competitive analysis.
2. **Cog VM context-to-stack mapping (Smalltalk; going back to Deutsch & Schiffman, POPL 1984).** Each activation is a stack frame, a married frame/context pair, or a heap context. Frames are spilled to heap contexts when stack pages run out and are mapped back to frames on use. *It falls short because* the triggers are reification and page exhaustion, there is no cost model or learning, there is no segmented-versus-contiguous choice, and nothing is evaluated against a hindsight optimum. The details are documented only in grey literature.
3. **Project Loom (JEP 444) plus Oracle patents US 11,604,729 and US 10,908,910 / 11,726,786.** Freeze copies frames into heap stack chunks. Thaw lazily copies frames back with a return barrier, and the patent says the size of each thawed subset "may be adaptive". *It falls short because* migration is driven by park/unpark, not by cost. The adaptivity tunes *how many* frames to copy within a single stack↔heap scheme, not *which representation* an activation should live in. There is no oracle and no switching-cost analysis. This is the highest-risk prior art: read the claims.
4. **Farvardin & Reppy, PLDI 2020.** An apples-to-apples comparison of six fixed strategies (contig, resize, segment, hybrid, linked, cps). *It falls short because* each strategy is fixed per program. Its "hybrid" is a one-way, size-threshold switch per stack (resizable, then segmented), with no per-activation granularity, no return path, no policy, and no oracle. It is the natural empirical baseline and a source of benchmarks.
5. **Soman, Krintz & Bacon, ISMM 2004 / Xu, CoCo, ECOOP 2013 (analogues).** These perform online runtime switching among implementations with migration: whole-heap for GCs, per-object for collections. *They fall short because* they do not apply to activation state and have no oracle or competitive bound. The analogous systems paper that does use an offline optimum is **Lin et al., INFOCOM 2011**, and the analogous MTS-based one is **OReO, ICDE 2024**. Neither concerns runtimes.

## Recommended novelty sentence

> To our knowledge, MorphStack is the first runtime that treats the representation of *each activation*—contiguous stack, segmented stacklet, or heap frame—as an **online decision driven by observed execution cost**, migrating activations in both directions, and that evaluates this policy against the **hindsight-optimal representation schedule** under an explicit, asymmetric migration-cost model, i.e. as a metrical task system over representations. Prior systems migrate activation state between stack and heap only when language semantics force it (continuation capture, reification, thread park/unpark) or fix one representation per program.

**Do not claim** any of the following as new: segmented or growable stacks, stack↔heap frame migration (including bidirectional and per-frame migration), lazy or return-barrier copying, dynamic heapification, OSR or deoptimization, runtime switching among implementations, or MTS or switching-cost algorithms.

## Must-cite list

- **Mechanism prior art:** Hieb, Dybvig & Bruggeman (PLDI 1990); Bruggeman, Waddell & Dybvig (PLDI 1996); Clinger, Hartheimer & Ost (HOSC 1999); Deutsch & Schiffman (POPL 1984); Stadler et al. (PPPJ 2009); JEP 444 and the Oracle Loom patents; Sivaramakrishnan et al. (PLDI 2021); Goldstein, Schauser & Culler (JPDC 1996); Anand et al. (PLDI 2024, doi:10.1145/3656389); D'Elia & Demetrescu (PLDI 2018).
- **Evidence that cost depends on the regime:** Appel & Shao (JFP 1996); Farvardin & Reppy (PLDI 2020); the Go contiguous-stacks design document; Ma & Zhong (HotMobile 2023).
- **Adaptive runtime analogues:** Arnold et al. (OOPSLA 2000); Soman, Krintz & Bacon (ISMM 2004); Xu (ECOOP 2013); Costa & Andrzejak (CGO 2018).
- **Theory:** Borodin, Linial & Saks (JACM 1992); Karlin et al. (Algorithmica 1988); Bubeck et al. (SODA 2019); Antoniadis et al. (ICML 2020); Dekel et al. (STOC 2014); Zhao, Wang & Zhou (AISTATS 2022; the plan's citation lists the wrong authors); Lin et al. (INFOCOM 2011); Rong et al., OReO (ICDE 2024).

## Open items and uncertainty

- A dedicated **asymmetric-MTS** reference was not found. Borodin, Linial & Saks give O(|S|²) for non-symmetric distances, and Fiat & Mendel cover unfair MTS. Search further before claiming bounds.
- Unverified details are flagged in the matrix: Kumar et al. (content), the Shao & Appel DOI, authorship of the Go and Rust posts, the Cog VM heap→stack details, the authors of the 2025 RDMA coroutine study, and the authors of Libfork.
