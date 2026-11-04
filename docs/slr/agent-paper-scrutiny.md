# Agent Paper Novelty Scrutiny: Causal-Query-Grounded LLM Debugging of Native Memory Bugs

**Date:** 2026-11-04 | **Companions:** `verdict.md`, `forward-citations.md`, `matrix.md`

**Proposed question.** Do LLM debugging agents localize the true root cause of native memory bugs (UAF, heap overflow, double free; real CVEs) more accurately, and with fewer unsupported claims, when every claim must be grounded in causal queries over a deterministic replay (why / origin / lifetime / explain-crash, allocation-lifetime aware)? The comparison arms are (a) raw debugger access (gdb plus rr reverse execution) and (b) crash logs or ASan reports only.

**Planned measurements.** Root-cause localization accuracy against ground truth; the rate of claims not supported by trace evidence; steps, tokens and cost; and an ablation without allocation epochs.

**Search method.** WebSearch (extended mode for recent work), OpenAlex `search=` with a 2023+ filter (7 queries), and Semantic Scholar `/paper/search` (2 of 6 queries completed; the other 4 returned HTTP 429). Full text or HTML was read for DebugHarness (main body), the AIxCC SoK (via pdftotext) and the arXiv abstract pages of every row marked "verified (abstract)". Anything marked *unverified* was seen only in a search snippet or a dataset listing.

---

## 1. Closest works

| # | Work | Year / venue | Link | What it does | Overlap with our claim | Gap we can occupy |
|---|---|---|---|---|---|---|
| 1 | **DebugHarness** (Sun, Yang, Liu, Zhou, Xu) | 2026, arXiv 2604.03610 (cs.SE), verified (HTML body read) | https://arxiv.org/abs/2604.03610 | An LLM repair agent for C/C++ memory-safety bugs. It reaches **GDB + pwndbg + Mozilla rr (reverse-continue) through MCP**, plus a clangd LSP. It is evaluated on **SEC-bench** (200 bugs, 29 projects, 16 CWEs). It solves 89.5% with DeepSeek-V3.2 and 94.5% with GLM-5. Ablation: GDB-only (no rr or pwndbg) 82.0%; no debugger 77.0%. Cost is about $0.09–0.64 per bug. | **Very high.** It already has the arms "agent + rr/gdb via MCP" and "agent without debugger" on real C/C++ memory bugs, rr reverse execution motivated by UAF, and SEC-bench. | The metric is **patch success only**. There is **no root-cause localization accuracy**, and **hallucination is asserted to be mitigated but never measured**. There is no causal-query or provenance layer, no allocation-lifetime identity, and token counts are not reported. Its tool surface is *raw debugger commands*, which is exactly our "raw debugger" arm. |
| 2 | **ChatDBG** (Levin, van Kempen, Berger, Freund) | 2025, FSE (PACMSE), DOI 10.1145/3729355, verified (abstract) | https://doi.org/10.1145/3729355 | An LLM in GDB/LLDB/pdb that can "take the wheel" and issue debugger commands. It evaluates root-cause analysis and fixes on C/C++ bugs and Python. | Agent with debugger access on native bugs; root-cause framing. | Forward-only debugger, no replay, no causal queries. Root-cause correctness is judged as "actionable fix" (the Python figure is 67% → 85%; the C/C++ judging protocol was not verified). No unsupported-claim metric. Not CVE-scale. |
| 3 | **VulDebugger: "Agent That Debugs: Dynamic State-Guided Vulnerability Repair"** (Liu, …, Gao, Sun, Roychoudhury) | 2025, arXiv 2504.07634, verified (abstract) | https://arxiv.org/abs/2504.07634 | A debugger-driven repair agent. It compares actual runtime state with constraint-inferred expected state to find root causes. Tested on 50 projects, with a 60% fix rate. | Agent + debugger for vulnerability RCA. | Forward debugger with no replay or provenance. A root cause counts as correct only if it yields a correct patch (per a search snippet; *unverified*). No hallucination metric. |
| 4 | **Undo AI / `explain` + UDB MCP server** | 2025, vendor docs + CppCon 2025 poster | https://docs.undo.io/HowUndoAIWorks.html · https://undo.io/wp-content/uploads/2025/10/cppcon-explain-poster-2025-09-01-final.pdf | A time-travel debugger exposed to Claude Code, Codex and Amp over MCP. The tool surface is deliberately restricted to backwards navigation, bookmarks and logging. "The recording is the ground truth" against which the agent's hypotheses are checked. | **High in concept:** time-travel grounding of agent hypotheses, plus the marketing claim that it works "without hallucination". | **No published evaluation**, no root-cause accuracy, no hallucination measurement. It offers navigation primitives, not causal or provenance queries, and documents no lifetime-aware dataflow. |
| 5 | **karellen-rr-mcp**, **rr-mcp** (open-source MCP servers) | 2025–2026, GitHub/PyPI | https://github.com/karellen/karellen-rr-mcp · https://pypi.org/project/rr-mcp/ | rr record/replay with reverse stepping exposed to LLM clients via GDB/MI. karellen ships a Claude Code "investigator" agent. | "rr as MCP tool server" is **already commodity**. | No paper and no evaluation. Raw debugger primitives only. These are ready-made implementations of our **raw-debugger baseline arm**. |
| 6 | **Veritas** (Zheng, Pesoli, Valleri, Jana, Cavallaro) | 2026, arXiv 2605.15097, verified (abstract) | https://arxiv.org/abs/2605.15097 | Frames LLM binary vulnerability reasoning as a **semantic grounding problem**. A static slicer over lifted IR feeds the LLM, which forms claims that a validator checks with guided debugging and runtime oracles. Covers OOB bugs; reports 0 false positives in 623 validated candidates. | "Ground every claim in program-analysis evidence" is **already framed and claimed**. | Static slicing, detection rather than RCA of a given crash, no replay, no temporal or UAF bugs, and false positives are measured instead of a per-claim unsupported rate. |
| 7 | **KernelDiag** (Wang, Wang, Wang, Wang, Chen) | 2026, arXiv 2607.17722, verified (abstract) | https://arxiv.org/abs/2607.17722 | Agents build **schema-validated Evidence Graphs** of source-grounded causal links for Linux kernel crashes on kGym. Reports Top@k faulty-method localization. | Evidence-grounded causal RCA by agents, with a localization metric. | Kernel, log- and source-based, no replay or debugger, no hallucination metric. |
| 8 | **T2L / T2L-ARVO** (Xi, Shao, Dolan-Gavitt, Shafique, Karri) | 2025–26, arXiv 2510.02389 (also an IEEE version), verified (abstract) | https://arxiv.org/abs/2510.02389 | An agent for **line-level vulnerability localization** on ARVO crashes, using "runtime evidence" (crash points, stack traces). Its benchmark has 50 expert-verified cases with multi-line ground truth. Reports 58.0% detection and 54.8% line localization. | **Same primary metric** (localization vs ground truth, ARVO C/C++ crashes) and a crash-log-style arm. | No debugger, no replay, no causal queries, no hallucination metric. Its benchmark is directly reusable. |
| 9 | **Kumushi: Root-Cause-Driven Automated Vulnerability Repair** (Wang, …, Shoshitaishvili, Wang, Bao) | 2026, arXiv 2605.04251, verified (abstract) | https://arxiv.org/abs/2605.04251 | Dynamic fault localization with evidence-weighted ranking (KLEE path constraints and taint per a search snippet, *unverified*) feeds an LLM patcher. 178 C/C++ vulnerabilities; expert-judged root-cause fixes. | "Root cause, not symptom" framing; dynamic evidence fed to an LLM. | Patch-quality metric, not localization or claim support. No replay or lifetime identity. |
| 10 | **SoK: DARPA AIxCC** (Zhang et al.) | 2026, USENIX Security, verified (full text grep) | https://www.usenix.org/conference/usenixsecurity26/presentation/zhang-cen | Four finalist CRSs build **standalone LLM RCA agents**, and four give agents runtime probes including **GDB**. It finds that **37.7–45.6% of auto-validated patches were semantically wrong**, often a *wrong root cause* when the root cause and crash site are distant. | Shows LLM RCA agents with debugger access are standard practice in industrial CRSs. | No RCA accuracy measured in isolation, no replay, no claim grounding. **Useful motivation:** patch success is a poor proxy for root-cause correctness. |
| 11 | **SAN2PATCH: "Logs In, Patches Out"** (Kim, Shin, Kim, Yoon) | 2025, USENIX Security, verified (listing) | https://www.usenix.org/conference/usenixsecurity25/presentation/kim-youngjoon | Tree-of-thought LLM repair from **sanitizer logs plus source only**, including a fault-localization stage. Evaluated on VulnLoc. | This is our **"ASan report only" arm**, as a published system. | Repair metric; no dynamic queries; no hallucination metric. |
| 12 | **PatchAgent** (Yu et al.) | 2025, USENIX Security, verified (listing) | https://www.usenix.org/conference/usenixsecurity25/presentation/yu-zheng | Fault localization + patching agent with LSP and patch validator. 178 OSS-Fuzz/Huntr bugs, more than 90% repaired. | Static/log baseline for memory bugs. | No debugger, replay or RCA metric. |
| 13 | **AgenticRepair** (Fu, Mei, Thongtanunam, Tantithamthavorn) | 2026, arXiv 2607.29422, verified (abstract) | https://arxiv.org/abs/2607.29422 | Subagents engineer context, including "runtime-execution context revealing crash semantics and **memory origins**". 73% on SEC-bench. | Memory-origin context for agents. | Mechanism unspecified; success-rate metric only. |
| 14 | **CrashFixer** (Mathai et al.) | 2025, arXiv 2504.20412, *unverified beyond snippet* | https://arxiv.org/abs/2504.20412 | A kernel-crash repair agent on kGymSuite. | Crash RCA + repair agent. | Kernel; no replay grounding; repair metric. |
| 15 | **Hallucination in LLM-based APR** | 2026, arXiv 2609.04909, *unverified beyond snippet* | https://arxiv.org/abs/2609.04909 | Defines hallucination as artifacts (including "understanding" artifacts) not grounded in available repair evidence. Studies 832 Defects4J (Java) bugs. | **A prior operationalization of our "unsupported claim" metric.** | Java, no tools or replay, no intervention. We must cite it and align definitions. |
| 16 | **From Program Slices to Causal Clarity** (Porbeck, Adriano, Giese) | 2026, EASE workshop, arXiv 2604.18309, verified (abstract) | https://arxiv.org/abs/2604.18309 | Varies the debugging context (including slices) given to LLMs across 93 configurations. LLM-as-judge faithfulness and actionability scores, validated by a user study; better explanations predict repair success. | "Slice-based context improves LLM causal explanations." | Not native, not an agent, not replay; judge-based, not trace-checked. |
| 17 | **Free to Look, Free to Reason** (Bi et al.) | 2026, arXiv 2610.35767, verified (abstract) | https://arxiv.org/abs/2610.35767 | In microservice RCA, **simple harnesses beat hand-crafted RCA agents** (65–77% vs 16–53% PR@1). | Directly contradicts the hypothesis that specialized tools help. | Different domain. It is a **threat to our expected effect** and must be cited and engaged. |
| 18 | **Dolores: An LLM-Driven Debugging Agent for Root-Cause Analysis** (H. Han) | 2026, IEEE DataPort dataset, DOI 10.21227/h3ah-5k14 | https://ieee-dataport.org/documents/dolores-llm-driven-debugging-agent-root-cause-analysis | A dataset of 50 vulnerabilities (42 CVEs, 29 systems; integer overflow, heap/stack overflow, **UAF, double free**, NULL deref) for "vulnerability cause analysis" via iterative debugging. | The title signals a **parallel paper on debugger-driven LLM RCA** with root-cause ground truth. | **Paper not located; agent design and results unverified.** This is the largest unknown. Monitor arXiv. |
| 19 | **RecovSlicing: "LLM as an Execution Estimator"** (Pei et al.) | 2025, arXiv 2508.18721, verified (abstract) | https://arxiv.org/abs/2508.18721 | The LLM fills in missing dynamic dependences for practical time-travel debugging and slicing. | LLM + time-travel slicing in the same sentence. | Inverse direction: the LLM helps slicing, not slicing grounding the LLM. Not native or temporal. |
| 20 | **CodeMender** (Google DeepMind), **Naptime / Big Sleep** (Project Zero) | 2024–2025, blog/press | https://projectzero.google/2024/10/from-naptime-to-big-sleep.html | Industrial LLM agents for finding and fixing vulnerabilities, with debugger tooling (Naptime's tool set is per secondary sources, *unverified*). | Practice baseline: "agents with debuggers find root causes". | No peer-reviewed evaluation and no RCA or grounding metric. |

**Benchmarks seen (not competitors):** SEC-bench (NeurIPS 2025), ARVO (arXiv 2408.02153 / 2606.17283), CyberGym (arXiv 2506.02548), Magma (SIGMETRICS 2021; arXiv 2009.01120), VulnLoc (AsiaCCS 2021), AutoPatchBench (Meta, 2025), and VLoc Bench (arXiv 2609.15939; file-level and multi-language, so not suitable).

**Not found:** any work that measures **per-claim support of an agent's root-cause narrative against replay trace evidence**, or that compares **causal-query tools vs raw time-travel debugger primitives at equal budget**. No work combines **allocation-lifetime-aware provenance** with an LLM agent.

---

## 2. Verdict: **GO (narrowed), with a pivot in framing**

- **The system-level novelty is gone.** An LLM agent driving rr reverse execution through MCP on C/C++ memory bugs is published (DebugHarness), productized (Undo), and open-sourced (karellen-rr-mcp, rr-mcp). The paper must not be framed as "a time-travel debugging agent".
- **The empirical question is still open, and it is a good one.** Nobody has measured (i) **root-cause localization accuracy** (rather than patch success) and (ii) the **rate of unsupported causal claims**, as a function of the **abstraction level of the dynamic evidence interface**: logs, then raw replay debugger, then causal provenance queries. Nobody has run (iii) a **lifetime-identity ablation** inside an agent loop. The AIxCC SoK's finding that roughly 40% of validated patches have the wrong root cause, and DebugHarness's unmeasured hallucination claim, are ready-made motivation.
- **Real risk of a null or negative result.** "Free to Look, Free to Reason" shows that in another RCA domain, minimal harnesses beat crafted tools. DebugHarness reports the no-debugger arm already at 77%. Run a **pilot on about 20 bugs before committing**. If causal queries do not beat raw rr+gdb on localization or claim support by a meaningful margin, re-scope to a measurement paper ("how often do debugger-equipped agents state unsupported causes?"). That version is still publishable but weaker.
- **Watch item.** The Dolores paper (if one exists) could pre-empt the "debugger agent RCA with ground truth" angle. Re-search before submission.

---

## 3. Defensible claim wording

> We present a controlled study of how the *form* of dynamic evidence available to an LLM debugging agent affects root-cause diagnosis of native memory-safety bugs. On N real C/C++ vulnerabilities (UAF, double free, heap overflow) with root-cause ground truth, we compare agents that receive only sanitizer reports, agents with raw record-and-replay debugger access (rr + gdb, including reverse execution), and agents restricted to causal provenance queries over the same replay (why / origin / lifetime / explain-crash). Each claim must cite the query result that supports it. Holding the model, budget and harness fixed, we measure root-cause localization accuracy, the fraction of stated causal claims that are not supported by the replayed execution, and cost. An ablation that replaces allocation-lifetime identity with raw-address identity isolates the contribution of lifetime-aware dependences on bugs whose memory is reused.

Short form for the abstract:

> *To our knowledge, this is the first study that measures both root-cause localization accuracy and trace-verified claim support for LLM debugging agents on real native memory bugs, and that isolates the effect of replacing raw replay-debugger access with allocation-lifetime-aware causal queries.*

Keep "to our knowledge" and both conjuncts. Each half alone is weaker: DebugHarness has rr agents, and T2L has localization accuracy.

---

## 4. What must NOT be claimed

- **First LLM agent with time-travel, reverse or replay debugging**, or "first rr MCP server": DebugHarness, Undo AI, karellen-rr-mcp, rr-mcp.
- **First LLM debugging agent for native or memory-safety bugs**: ChatDBG, VulDebugger, DebugHarness, PatchAgent, SAN2PATCH, AIxCC CRSs.
- **First to ground LLM vulnerability or crash reasoning in program-analysis evidence**: Veritas (semantic grounding, slicer + validator), KernelDiag (evidence graphs), Undo ("the recording is the ground truth").
- **First to measure hallucination in LLM debugging or repair**: arXiv 2609.04909 (APR hallucination), 2604.18309 (explanation faithfulness). Claim only *trace-verified, per-claim* support in *agentic native-memory RCA*.
- **First vulnerability localization benchmark or metric**: T2L-ARVO, VulnLoc, VLoc Bench, RCABench (2023).
- **"Eliminates hallucination" or "hallucination-free"**, even if the rate is near zero. Requiring citations constrains the *format*; the agent can still misread a correct query result. Report residual misinterpretation separately.
- **That raw rr/gdb agents "cannot" find lifetimes or reuse**: they can, manually (DebugHarness's UAF case study traces a stale pointer with watchpoints). Claim a difference in *rate, cost and claim support*, not in capability.
- **Generalization beyond what was run**: no claims for multi-process, kernel, JIT, Windows, or custom allocators unless evaluated.
- Every item in `verdict.md` §"Do NOT claim" still applies. The agent paper must not re-sell the StateSolver technique novelty; cite the technique paper for that.

---

## 5. Recommended benchmarks (single-process Linux x86-64 C/C++, rr-recordable)

**Ground-truth note:** none of these ships a "root cause" label beyond a fix patch or bug location. A **root-cause annotation layer is unavoidable**. Define it as a tuple: (a) *defect location*, meaning the fix hunk lines, with multiple acceptable lines allowed as RCABench and T2L do; (b) for UAF and double free, *the premature free site and the stale-pointer holder*; (c) for overflow, *the size/bound computation that is wrong*. Use two annotators with κ reported, and release the annotations.

| Rank | Benchmark | Why it fits | rr suitability | Link |
|---|---|---|---|---|
| **1 (primary)** | **SEC-bench** (Lee, Zhang, Lu, Zhang; NeurIPS 2025) | Real C/C++ CVEs with Docker, a PoC and a **gold patch**; mostly memory-safety (UAF, heap overflow, double free among the CWEs). **DebugHarness used the same 200 instances**, so its rr+gdb configuration can be replicated as the raw-debugger arm on identical bugs. | PoC binaries are ordinary single-process sanitizer builds. Docker needs `--cap-add=SYS_PTRACE`, `seccomp=unconfined`, `perf_event_paranoid<=1`, and a host with a working PMU (bare metal or a vPMU-enabled VM; rr fails on most cloud VMs without it). | https://proceedings.neurips.cc/paper_files/paper/2025/hash/a9168f1c54e5147027f1e8cf83e1a775-Abstract-Conference.html · arXiv 2506.11791 |
| **2 (scale + contamination control)** | **ARVO** (OSS-Fuzz reproductions; 5,000+ C/C++ memory bugs, 12k images in the 2026 version), with **T2L-ARVO**'s 50 expert-verified line-level cases as a ready-made localization subset | Bisected **fix commit** per bug, and every case reproduces. Bugs can be **filtered by report date after the model cutoff** to address memorization. T2L-ARVO gives directly comparable localization numbers. | libFuzzer harness plus a single input file means a single-process replay. Use the ASan builds already in the images; rr records ASan binaries. | https://arxiv.org/abs/2408.02153 · https://github.com/n132/arvo · https://arxiv.org/abs/2510.02389 |
| 3 (cross-check) | **VulnLoc** 43 CVEs (AsiaCCS 2021), as packaged with functional tests in the SAN2PATCH artifact | Classic single-binary CVEs (binutils, libtiff, libxml2, …) with **developer patch as ground truth**. Directly comparable with SAN2PATCH, the published "ASan-report-only" system. | Small, single-process command-line programs, ideal for rr. | https://github.com/VulnLoc/VulnLoc · https://www.usenix.org/conference/usenixsecurity25/presentation/kim-youngjoon |
| 4 (optional, ground-truth precision) | **Magma** v1.x (118 forward-ported real bugs in libpng, libtiff, libxml2, poppler, openssl, sqlite3, php) | **Canary at the bug condition** gives a precise defect location independent of patch style. Forward-porting reduces verbatim memorization. | Single-process fuzz drivers. *Unverified:* which bugs have shipped PoVs, and whether that includes enough UAF/DF cases; check the Magma survival-time tables and PoV directory first. | https://arxiv.org/abs/2009.01120 · https://hexhive.epfl.ch/magma/ |
| watch | **Dolores** dataset (50 vulns, 42 CVEs, includes UAF/DF, built for debugger-based cause analysis) | Possibly the closest root-cause-labeled set. | Paywalled; contents *unverified*. | https://doi.org/10.21227/h3ah-5k14 |

**Suggested corpus.** About 120–200 bugs. Take all UAF and double-free cases from SEC-bench and ARVO, plus a matched sample of heap overflows. Include a **reuse-present stratum**: bugs where the freed chunk is reallocated before the faulting access, detected automatically from an allocator log. The epoch ablation is only meaningful there, so report it per stratum.

---

## 6. Strongest reviewer objections and how to pre-empt them

1. **"DebugHarness already did this."** Replicate DebugHarness's toolset (GDB + pwndbg + rr over MCP) as the **raw-debugger arm**, on its SEC-bench instances, with the same model family. Report its patch success too, to show the replication is faithful, then show what patch success hides (root-cause accuracy and unsupported-claim rate).
2. **"The unsupported-claim metric is rigged for your arm."** Here, the structured arm emits citeable query IDs and the baselines do not. Impose the **same claim protocol on every arm**: every causal claim cites a tool-call ID (or an ASan line or source line). Verify claims with an **arm-blind** checker. The automatic part re-executes the claim as a replay query and checks agreement. The human part covers the rest: two raters, blinded to arm, with κ reported. Report uncited claims and cited-but-contradicted claims separately.
3. **"Tool-interface confound: you changed both abstraction *and* verbosity, tool count, and prompt."** Match token budgets, step caps and system prompts. Add a **"raw rr + scripted summaries" arm** that gives the same amount of output but no causal semantics. Report tokens per arm. Use at least 3 models (one open-weight) and at least 3 seeds per bug, with CIs and paired tests (McNemar or bootstrap).
4. **"ASan already prints the alloc and free stacks for UAF, so the logs arm is trivially as good."** Pre-register a root-cause definition that goes beyond the free site (*why* it was freed early, *which* pointer stayed live, *where* the bound was computed). Stratify results by **crash-to-root-cause distance** (same function / same file / cross-file). Report where the arms tie. A null result on near-crash bugs is expected and makes the study more credible.
5. **"Memorization of public CVEs."** Report results separately on bugs disclosed **after each model's training cutoff** (ARVO date filter). Add a check where the agent states the CVE ID unprompted. Optionally include Magma forward-ported bugs.
6. **"Simple harnesses beat crafted tools" (Free to Look).** Include a **minimal-harness arm**: a coding agent with a shell, rr installed, and no MCP. This shows the effect is not just an artifact of a weak raw arm.
7. **"The epoch ablation only matters on a tiny subset."** Report the size of the reuse-present stratum up front, and give the ablation effect **only** on that stratum alongside the whole corpus. If the stratum is small, the ablation is secondary, and the paper must say so.
8. **"Ground truth is subjective."** Give the annotation guideline, two annotators, κ, multi-location acceptance, and released labels. Report sensitivity using fix-hunk-only ground truth as a strict lower bound.
9. **"rr is impractical (PMU, Docker, overhead)."** Report recording overhead and the bugs that failed to record. State the hardware requirements (no cloud VMs without vPMU) as a threat to validity.
10. **"Cost."** Report USD, tokens, wall time, replay time and query latency per bug. A Pareto plot of accuracy against cost is expected after Free to Look and DebugHarness.

---

## 7. arXiv category and target venues

- **arXiv:** primary **cs.SE**; cross-list **cs.CR** and **cs.AI**. Use cs.LG only if model-training content is added.
- **Target venues** (check current CFP dates; none were verified here):
  1. **ICSE** (research track). Empirical SE plus tool study, and the venue cluster where ChatDBG, PatchAgent-style and agent-RCA work appears.
  2. **FSE (ESEC/FSE)**, where ChatDBG appeared. Its artifact track suits a released annotation layer and MCP server.
  3. **ISSTA**, for dynamic analysis plus replay plus testing. The alternative, if the security framing dominates, is **USENIX Security**, where SAN2PATCH, PatchAgent and the AIxCC SoK appeared.

---

## Sources located (beyond the table)

- AIxCC SoK full text: https://gts3.org/assets/papers/2026/zhang:aixcc-sok.pdf
- OSS-CRS: https://arxiv.org/abs/2603.08566
- CyberGym: https://arxiv.org/abs/2506.02548
- Meta AutoPatchBench: https://engineering.fb.com/2025/04/29/ai-research/autopatchbench-benchmark-ai-powered-security-fixes/
- RCABench (2023): https://arxiv.org/abs/2303.05029
- VLoc Bench: https://arxiv.org/abs/2609.15939
- Evidence-tracing survey (agents): https://arxiv.org/abs/2606.04990
- Undo MCP blog: https://undo.io/resources/time-travel-ai-code-assistant/

## Unverified / open

- Dolores: the paper and its agent design were not found, only the dataset listing.
- The details of VulDebugger's root-cause scoring, Kumushi's KLEE/taint mechanism, and the Naptime tool set come from snippets only.
- How ChatDBG judges correctness for C/C++ root causes was not read in full text.
- Magma PoV availability for UAF/DF bugs is unconfirmed.
- Semantic Scholar rate-limited 4 of 6 queries. Google Scholar and the ACM DL were not queried. Forward citations of DebugHarness and ChatDBG were not swept; that is the highest-value remaining check.
