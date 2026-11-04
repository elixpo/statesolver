# MorphStack discoveries

A dated log of what we learned, including dead ends and model corrections, newest first. This file is the raw material for the paper's evaluation, limitations, and related-work sections.
Each entry: **date**, the finding, the evidence (a commit or file), and what it changes.

---

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
