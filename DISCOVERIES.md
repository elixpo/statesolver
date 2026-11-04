# MorphStack discoveries

A dated log of what we learned, including dead ends and model corrections, newest first. This file is the raw material for the paper's evaluation, limitations, and related-work sections.
Each entry: **date**, the finding, the evidence (a commit or file), and what it changes.

---

## 2026-11-04: The oracle migrates only at stack low points, so mechanism cost barely matters

- **Finding:** Every oracle migration happens when the stack is nearly empty: 256 B live on W2, and about 1 KB on average over W6's 62 migrations. Scaling the copy and fix-up cost from 0.01× to 100× moves the switching gain only from 13.1% to 9.3% on W2, and from 3.7% to 0.6% on W6 at the default memory price.
- **Evidence:** [results/stage2/sweep.md](results/stage2/sweep.md) and `report.json` (`bytes_migrated / migrations`).
- **Implication:** This is a design rule for Stage 3. Online policies should defer migration to low-water marks (after unwinding, or at suspension with a shallow stack), not migrate deep stacks. It also means the claim does not depend on a fast migration mechanism, which strengthens it.

## 2026-11-04: The value of switching depends on the memory price, not on migration cost

- **Finding:** On W6, switching over time is worth 16–19% when memory is cheap (0.01–0.1× the default price) but under 4% at the default price or above. In the expensive regime, heterogeneity dominates, reaching a 68% total gain at 10×. On W2, switching is a steady 9–13% at every setting. On every other workload switching is about 0%. The only exception is server at 100× memory price with near-free migration, where it reaches 4.3%.
- **Evidence:** [results/stage2/sweep.md](results/stage2/sweep.md), with a 5×5 grid per workload.
- **Implication:** Two regimes for the paper. When memory is cheap, *switching* pays: grow segmented, compact into a contiguous stack. When memory is expensive, *heterogeneity* pays: per-context selection. The calibrated memory price on real hardware decides which regime the paper's headline lives in.

## 2026-11-04: Model correction: sweeps must not scale allocation and release costs

- **Finding:** The first sweep scaled the whole migration cost, including the cost of allocating the destination and freeing the source. At 0.01×, migrating right before exit became cheap again, which produced fake switching gains of 16–41% on server, burst, and W3.
- **Fix:** `migration_scale` now multiplies only the mechanism cost (fixed, copy, fix-up). A unit test proves that migrating right before exit never pays, even when migration is free.
- **Lesson:** This is the third time the oracle has exploited a flaw in the model. Every new knob needs an "exit exploit" check.

## 2026-11-04: Most of the oracle's gain comes from heterogeneity, not switching

- **Finding:** With the uncalibrated profile, on W6 (the phase-change workload), the oracle beats the best fixed policy by **27.1%**. Of that, **23.9 points** come from giving each context its own fixed representation (baseline F, chosen in hindsight). Only **3.2 points** come from migrating contexts over time. In `mixed`, all 5.8% of the gain is heterogeneity.
- **Evidence:** [results/stage2/report.md](results/stage2/report.md), commit `0bed170`.
- **Implication:** The paper must report the two components separately. Online policies must approach F, which needs per-context prediction, before switching can matter. A sweep over memory price and migration cost will decide how strong the "switching over time" claim can be.

## 2026-11-04: A real temporal effect: grow segmented, then compact into a contiguous stack

- **Finding:** On W2 (deep recursion), the oracle grows the stack as segments while its final depth is unknown, then migrates into a single contiguous stack sized to the high-water mark. This beats every fixed policy by 11.5%, and all of the gain comes from switching.
- **Evidence:** the oracle schedule on W2, and [results/stage2/report.md](results/stage2/report.md).
- **Implication:** This candidate mechanism for the paper avoids the repeated copying of Go-style doubling. Check whether prior work (Chez Scheme or Go) already does this.

## 2026-11-04: Model correction: migrations must release the source

- **Finding:** When a migration's cost left out freeing the source, the oracle migrated right before exit to get the cheaper segmented or heap free. This produced spurious "100% of contexts switch" results on W1 and W4.
- **Fix:** migration cost = release(source) + acquire(destination) + copy + fix-up. A unit test now proves that migrating right before exit never pays.
- **Lesson:** The oracle is an adversary of the cost model. Any free lunch in the model shows up as switching, so treat any unexpected switching as a possible model bug first.

## 2026-11-04: Model correction: first-touch (page-fault) cost

- **Finding:** Without a cost for touching fresh memory, migrating into a contiguous stack pre-sized to the high-water mark was free. W2's gain was inflated from 11.5% to 43%.
- **Fix:** `touch_ns_per_byte` is now charged when the stack first grows past its high-water mark, when growth copies into a new stack, and when a contiguous stack is pre-sized on migration.

## 2026-11-04: No fixed representation wins everywhere (go/no-go criterion 1 is met)

- **Finding:** Contiguous wins W1 and adv-hot-split. Segmented wins W2, W4, W6, server, and mixed. Heap wins W3, W5, burst, and adv-alternating. Each fixed policy loses by 100% to 4800% on some workload. Hot-split costs the segmented representation about 48× on adv-hot-split.
- **Caveat:** these are uncalibrated parameters, so they show the shape of the trade-offs, not their real magnitude.

## 2026-11-04: Static baseline D is degenerate on our workloads

- **Finding:** "Heap if the entry function can suspend" equals always-heap on every workload, because every suspending context has a suspending entry function.
- **Implication:** Stage 3 needs a finer static rule, such as per-activation `can_suspend` or one that also uses frame size.

## 2026-11-04: Literature review: the mechanism is prior art; the policy and oracle are the gap

- **Finding:** Bidirectional, per-frame stack↔heap migration exists in Chez Scheme (PLDI 1990), the Cog VM, and Loom's freeze and thaw. No prior work makes a cost-driven online choice of representation evaluated against a hindsight oracle or as a metrical task system. The Oracle Loom patents claim only lazy copying through a return barrier.
- **Evidence:** [docs/slr/verdict.md](docs/slr/verdict.md).
- **Implication:** The novelty claim is about the policy and the oracle/MTS evaluation, not about the migration mechanism.

## 2026-11-04: Cross-platform determinism needs a libm-free random generator

- **Finding:** Platform `ln`, `exp`, and `cos` can differ in the last bit between Linux, macOS, and Windows, which would make seeded traces differ by platform.
- **Fix:** xoshiro256** with `ln` and `exp` built only from IEEE-exact operations, and the Marsaglia polar method for normal samples. Pinned trace digests are checked in CI on all three operating systems.
