# StateSolver: locked direction

**Locked on 2026-11-04** by the maintainer, after three scrutiny passes:
- [docs/slr/verdict.md](slr/verdict.md), 68 works on the technique;
- [docs/slr/forward-citations.md](slr/forward-citations.md), citation graphs of the closest works;
- [docs/slr/agent-paper-scrutiny.md](slr/agent-paper-scrutiny.md), the agent-study paper.

**Change policy:** this direction does not move unless the work turns up something genuinely interesting to adapt to. Any change must be written up in `DISCOVERIES.md` with its evidence, then reflected here with a dated entry under "Changes".

The original brief, [research-direction.md](research-direction.md), remains the design reference for the debugger (data model, `ObjectID`, command surface, evidence tiers). This document decides **what we build first, what the paper claims, and how it is evaluated.**

---

## 1. The project (primary)

**StateSolver is an open-source Linux crash explainer built on rr.** You record a failing native program once, then ask causal questions about it:

```bash
statesolver record ./program            # rr record underneath
statesolver explain-crash               # evidence chain from the crash to the root cause
statesolver why rax --event E           # producer chain of a value
statesolver origin 0x55550020           # which allocation (address, epoch) a pointer belongs to
statesolver lifetime 0x55550000         # every object that lived at this address
statesolver mcp                         # the same queries as MCP tools for agents (Crumb, Claude Code, ...)
```

**Why it is needed:** rr gives free record, replay, and reverse execution, but the "why" has to be worked out by hand. Pernosco answers "why", but it is commercial. Existing rr MCP servers expose raw debugger commands. Nothing open answers causal questions **with allocation-lifetime identity** and **evidence IDs that an agent must cite**.

**Architecture:**

```text
rr record ──► rr trace ──► rr replay (gdb remote protocol)
                               │
                 statesolver replay driver (Rust)
                               │
        ┌──────────── query engine ─────────────┐
        │ temporal heap: epochs derived on demand│
        │ during replay (alloc/free breakpoints  │
        │ in the replay, never at record time)   │
        │ producer search: watchpoints + reverse │
        │ execution + instruction semantics      │
        └───────────────────┬────────────────────┘
                            │ every answer gets an evidence ID
              CLI ─────────┼───────── MCP server ──► Crumb agent / other agents
```

- **rr is the recorder and replayer.** StateSolver's own ptrace recorder (`statesolver-record`) is kept as a **test fixture and teaching reference only**. It is about 100× slower than rr on allocation-heavy code (DISCOVERIES, 2026-11-04).
- **Nothing extra is recorded.** Allocation epochs and dependences are reconstructed **when a query needs them**, by replaying.
- **Evidence model** (research direction §20.7): every statement is a FACT (a query result, with its ID), an INFERENCE (engine-derived, with its ID), or a HYPOTHESIS (agent-proposed, and unverified until a query confirms it).

## 2. The paper (secondary, written last)

**Research question:** do LLM debugging agents localize the true root cause of real native memory bugs more accurately, and make fewer claims unsupported by the trace, when they answer through **allocation-lifetime-aware causal queries over a deterministic replay** instead of raw replay-debugger access?

**Claim, worded to survive review:**

> To our knowledge, this is the first study that measures both **root-cause localization accuracy** and **trace-verified claim support** for LLM debugging agents on real native memory bugs, and that isolates the effect of replacing raw replay-debugger access with **allocation-lifetime-aware causal queries**.

**Do not claim:** the first time-travel or rr agent (DebugHarness, Undo AI, rr MCP servers); the first grounding or evidence-graph agent (Veritas, KernelDiag); "hallucination-free"; epoch identity, on-demand slicing, or replay-time provenance on their own (Undangle, FreeWill, WATCHER, Arnold).

### Study design (pre-registered before the full run)

**Arms** (same model, prompt skeleton, token budget, and step cap):

| Arm | Tools |
|---|---|
| A0 | crash log and ASan report plus source only (SAN2PATCH-style) |
| A1 | **raw rr/gdb/pwndbg over MCP**, a faithful DebugHarness replication |
| A2 | minimal harness: shell with rr installed, no MCP (the "Free to Look" control) |
| A3 | raw rr plus scripted summaries: the same output volume as A4, without causal semantics |
| **A4** | **StateSolver causal queries** (`why`, `origin`, `lifetime`, `history`, `explain-crash`) |
| A5 | A4 **without epochs**, i.e. address-only dependence, WATCHER-style (the ablation) |

**Metrics:**
- root-cause localization accuracy against annotated ground truth (top-1 and top-3, with multi-location acceptance and a fix-hunk-only lower bound);
- **unsupported-claim rate**: every causal claim must cite a tool-call ID, ASan line, or source line in **every** arm. An arm-blind checker re-runs claims as replay queries where it can; two blinded raters handle the rest, and κ is reported. Uncited claims and contradicted claims are reported separately;
- patch success (to show the A1 replication matches DebugHarness);
- tokens, USD, wall time, replay and query latency, with an accuracy-against-cost Pareto plot.

**Benchmarks:**
- **SEC-bench** (primary; matches DebugHarness);
- **ARVO** with the **T2L-ARVO** 50 expert-labelled cases (line-level ground truth; post-cutoff stratum);
- **VulnLoc** as a cross-check.

We add our own root-cause annotation layer (guideline, two annotators, κ, released labels).

**Analysis:**
- at least 3 models (at least one open-weight), at least 3 seeds per bug, paired tests (McNemar or bootstrap) with confidence intervals;
- strata: crash-to-root-cause distance (same function, same file, cross-file), reuse-present against no reuse (the epoch ablation is reported on the reuse stratum), and before against after the training cutoff;
- recording failures and rr hardware limits reported as threats to validity.

**Pilot gate:** about 20 SEC-bench bugs, 1 model, arms A1 against A4 (plus A5 on the reuse cases). If A4 shows no measurable gain in localization or unsupported claims, **stop and adapt** before scaling, for example to a tool paper plus negative-result study. A null result is a real risk and gets reported honestly.

**Where:** arXiv **cs.SE** primary, cross-listed to **cs.AI** and cs.CR. Venues: ICSE, FSE, or ISSTA, with USENIX Security as the alternative if the security framing dominates.

## 3. Stages (GitHub issues)

| # | Stage | Exit |
|---|---|---|
| #1 | Foundations, literature review, trace format | Done (kept open by the maintainer) |
| #2 | **rr backend**: record wrapper, replay driver over gdb remote protocol, historical state, on-demand allocation epochs | `lifetime` and `origin` answered from an rr trace of the corpus |
| #3 | **Causal query engine**: producer search, temporal object graph, `why`/`origin`/`history`/`explain-crash`, address-only mode for A5, evidence IDs | Corpus bugs explained correctly, with ground-truth checks in CI |
| #4 | **Study instrument**: MCP server, Crumb agent, arms A0–A5, benchmark loader, annotation tooling, **pilot** | Pilot result and go/adapt decision |
| #5 | **Full study, release, paper**: SEC-bench/ARVO/VulnLoc runs, packaging (Debian, install scripts), listings, arXiv | Paper and artifact public |

## Changes

- 2026-11-04: initial lock. Changes from the brief:
  - rr replaces the custom recorder and replayer;
  - allocation epochs are derived during replay instead of recorded;
  - the paper is an agent study, with lifetime-aware slicing as the measured ingredient.
