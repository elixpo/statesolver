# StateSolver discoveries

A dated log of what we learned, including dead ends, newest first. This is raw material for the paper.
Each entry: **date**, the finding, the evidence, and what it changes.
MorphStack's log is archived at [docs/archive/morphstack/DISCOVERIES.md](docs/archive/morphstack/DISCOVERIES.md).

---

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
