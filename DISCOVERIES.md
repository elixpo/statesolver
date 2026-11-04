# MorphStack discoveries

A dated log of what we learned, including dead ends and model corrections, newest first. This file is the raw material for the paper's evaluation, limitations, and related-work sections.
Each entry: **date**, the finding, the evidence (a commit or file), and what it changes.

---

## 2026-11-04: Switching survives through one shared warm stack; the oracle rediscovers the Cog VM design

- **Finding:** On W6, the warm-pool oracle runs CPU phases on a contiguous stack and compacts each worker into segments when it parks. On a single core only one worker runs at a time, so the pool's peak is **one 512 KB stack (0.5 MiB)** shared by all 32 workers.
  - Charging the pool's idle resident memory at the default memory price costs 0.227 ms.
  - Against a 0.674 ms switching gain, that leaves **+0.446 ms net, about 7.2% beyond F** (6.22 ms). The total gain over the best fixed policy is about 36%.
- **W2 does not count:** it is a single context, so no earlier stack exists to reuse, and its first deep stack must page-fault. The "warm" 15% is unattainable there.
- **Mixed:** the net is −0.01 ms, so there is no gain.
- **Interpretation:** the cost-optimal schedule is "a few shared hot run stacks, with parked contexts compacted". The Cog VM uses this design (a small set of stack pages, contexts spilled to the heap), and the literature review lists it as prior art. The oracle finding it independently validates the model. Our potential contribution is the **cost-driven decision of when to spill and when to compact**, with regret against the oracle, not the mechanism.
- **Evidence:** a scratch pool-pricing check (price idle pool bytes = peak contiguous capacity × duration − in-use area), not yet part of the simulator. Making it a first-class pool model is a TODO.
- **Status of "test first":** granularity has no effect. Switching over time is worth about 7% on phase-changing, multi-context workloads, **only** with a shared warm stack pool. Heterogeneity is worth 5–29%. Still missing: a real trace.

## 2026-11-04: Finer granularity cannot help; switching depends on page faults for fresh stacks

- **Test 1, granularity:** two new bounds on the calibrated profile.
  - **Ideal bound:** at every step, the cheapest execution cost of any representation plus the cheapest memory cost of any representation, with free migration. No scheme at any granularity can do better.
  - **Free-migration bound:** the best pure representation at every step, with free migration.

  The gap between the two, which is what only per-activation or per-segment *mixing* could win, is **0.0–0.1% on every workload**. All of the headroom over F (up to 24% on burst, about 20% on W2, 17% on W6, 11% on W3) is blocked by **migration cost**.
- **Test 2, which migration cost:** the copy and fix-up mechanism doesn't matter (the oracle migrates at low points), so the remaining candidates are acquire and release. Replacing `mmap`/`munmap` (425/678 ns) with a stack pool (20/15 ns):

  | Profile | W2 switching | W6 switching / total |
  |---|---|---|
  | mmap (calibrated) | 0% | 0% / 28.8% |
  | Pool, page faults kept | 0% | 0% / 28.8% |
  | **Pool, warm stacks (no first-touch)** | **15.4%** | **7.7% / 36.5%** |

- **Conclusion:** at context granularity, whether switching over time pays depends on one thing: **whether migrating into a contiguous stack can avoid page-faulting fresh memory**. Warm stacks from a pool bring it back (W2 15%, W6 8%). Fresh memory removes it. This is a sharp, testable condition for the paper.
- **Caveat:** a pool of warm, deep stacks holds resident memory that the model does not yet charge for, so the warm-pool numbers are an optimistic upper bound. Next step: model the pool explicitly, with pooled bytes priced at the memory price.
- **Evidence:** `ideal_bound`, `free_migration_bound`, and the columns in [results/stage2/calibrated/report.md](results/stage2/calibrated/report.md).

## 2026-11-04: A segment-boundary crossing costs about 1 ns, so per-context switching over time is worth nothing on this machine

- **Finding:** Switching onto a cached segment and back with `corosensei::on_stack` costs **0.93 ns per crossing** (median of 9 repeats, range 0.88–1.16 ns), about 100× below the 100–120 ns threshold at which switching starts to pay. With every measurable parameter calibrated, **the oracle never migrates a context on any workload: the switching gain is 0.0%.** The heterogeneity gain remains: W6 28.8%, burst 7.1%, mixed 4.8%.
- **Why segmented stacks looked bad historically:** the hot-split problem is mostly the cost of allocating and freeing a segment on every oscillation. The model prices that separately (11 + 7.7 ns measured with a warm allocator), so adv-hot-split costs segmented 2.42 ms against 0.27 ms for contiguous, about 9×, rather than the 48× under the default parameters.
- **Caveats:**
  - Warm-cache microbenchmark only.
  - Real segmented code also needs a split check in every prologue (not yet measured) and argument copying across boundaries.
  - Per-activation granularity (RQ5) is not yet explored and could behave differently.
  - The per-call split-check sweep (0.4–2 ns) did not bring switching back.
- **Evidence:** [results/profiles/x86_64-i5-13450hx.txt](results/profiles/x86_64-i5-13450hx.txt), [results/stage2/calibrated/report.md](results/stage2/calibrated/report.md).
- **Implication: this is a research-direction decision.** On current x86-64 hardware, the plan's thesis of *switching over time* at context granularity looks like the **negative result** anticipated in plan section 25. Activation-state representation looks effectively like a per-context choice. **Online per-context selection**, which approaches F, is supported by robust gains of 5–29%. Options: (a) pivot the headline to online per-context selection, and report the switching negative result as a finding; (b) test whether finer granularity (per activation or per segment) or real traces revive switching before deciding.

## 2026-11-04: Calibrated allocator costs remove the switching gain, unless segment boundaries are expensive

- **Finding:** Calibration on an x86-64 i5-13450HX gave these values (defaults in brackets):
  - first touch: 0.245 ns/byte (0.1)
  - copy: 0.015 ns/byte (0.08)
  - mmap / munmap: 404 / 676 ns (400 / 300)
  - segment malloc / free: 11 / 7.7 ns (60 / 40)
  - frame malloc / free: 9.2 / 5.4 ns (18 / 12)

  With these values, **the gain from switching over time is 0% on every workload**, and the oracle never migrates a context. Cheap segments make always-segmented good enough for deep recursion. The per-context heterogeneity gain remains: W6 26%, burst 7%, mixed 5%.
- **But:** the switching gain depends on one parameter that has not been measured yet, `segmented.boundary_ns`, the cost of a call or return that crosses a segment boundary. Switching gain by boundary cost:

  | `boundary_ns` | W2 | W6 |
  |---|---|---|
  | 60–100 | 0% | 0–5% |
  | 120 | 3% | 9% |
  | 150 | 15% | 14% |
  | 200 | 30% | 15% |
  | 400 | 42% | 11% |

  The split-check cost (0.4–2 ns) and the heap locality penalty (2.5–10 ns) barely matter.
- **Evidence:** [results/profiles/x86_64-i5-13450hx.toml](results/profiles/x86_64-i5-13450hx.toml), [results/stage2/calibrated/report.md](results/stage2/calibrated/report.md), and the bisection in this entry.
- **Implication:** This is the decisive measurement for the temporal-switching claim. A real segment-switch (the morestack path: switch the stack pointer, link the segment, copy arguments) must be measured. That needs `unsafe` code, so pull a minimal segment-switch microbenchmark forward into `morphstack-rt` (Stage 4). Go's move away from segmented stacks suggests boundary crossings were expensive in practice, but we need our own number. Meanwhile, the **heterogeneity** result (per-context selection, 5–43% over the best fixed policy) is robust across every setting tried, and is the safer headline.

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
