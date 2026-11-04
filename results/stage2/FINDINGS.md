# Stage 2 findings (in progress)

All numbers come from the **uncalibrated default cost profile** (`morphstack profile`). They show the *shape* of the trade-offs, not real-hardware magnitudes. Calibration and the sensitivity sweep are still open items in #2.

## What the oracle shows so far

1. **No fixed representation wins everywhere** (criterion 1 is met). Contiguous wins W1 and adv-hot-split; segmented wins W2, W4, W6, server and mixed; heap wins W3, W5, burst and adv-alternating. Each fixed policy loses by 100% to 4800% on some workload.
2. **Most of the oracle's advantage comes from heterogeneity, not from switching over time.** The table splits the gain over the best fixed policy into two parts:
   - *Heterogeneity:* giving each context its own fixed representation, chosen in hindsight (baseline F).
   - *Switching:* migrating a context between representations during its lifetime (the oracle beyond F).

   | Workload | Total gain | From heterogeneity | From switching |
   |---|---|---|---|
   | w6-phase-change | 27.1% | 23.9% | 3.2% |
   | w2-deep | 11.5% | 0% | 11.5% |
   | mixed | 5.8% | 5.8% | 0% |

3. **W2 shows a genuine temporal effect**: grow in segments while the stack size is unknown, then compact into one contiguous stack sized to the high-water mark. This avoids repeated doubling and copying.
4. **D (static "heap if the entry can suspend")** matches heap exactly, because every suspending context in these workloads has a suspending entry function. It is not yet a useful baseline. A finer static rule is needed for Stage 3.

## Model corrections made while building this

- **First-touch cost.** Fresh contiguous memory now pays a page-fault and zeroing cost. Without it, migrating into a pre-sized contiguous stack was free, which inflated W2's gain from 11.5% to 43%.
- **Migration cost structure.** A migration now costs *release source + acquire destination + copy + fix-up*. Without this, the oracle "migrated" right before exit to get a cheaper free, which produced spurious 100% switching on W1.

## Implications for the research claim

With the current parameters, **choosing a representation per context** is the larger lever. **Switching over time** matters in specific regimes: deep growth, and long-lived contexts that alternate between CPU and parked phases. For Stage 3 this suggests:

- Report heterogeneity and switching separately. Don't credit all of the oracle's gain to migration.
- An online policy must at least approach F, which needs per-context prediction. Beating F needs phase detection.
- The sensitivity sweep (memory price, migration cost) has to show where the switching share grows or vanishes. That sweep answers RQ3.
