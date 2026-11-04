# MorphStack — Research Plan

## Working Title

**MorphStack: Online Adaptive Representation of Activation State**

## 1. Core Research Question

> **Can a runtime dynamically choose and migrate activation records among multiple execution-state representations so that its total execution cost remains close to the best representation policy in hindsight, despite not knowing future workload behavior?**

The project is **not** trying to prove that one memory structure is universally optimal. Instead, it asks whether adaptation can get close to the best choice over time.

## 2. Research Hypothesis

A runtime that observes execution behavior and switches among:
- contiguous stack frames,
- segmented / stacklet-style frames,
- heap-resident frames,

can outperform any single fixed representation on heterogeneous or phase-changing workloads, while keeping migration overhead sufficiently low.

The target relationship is:

```text
Adaptive policy cost
      ≈
Oracle policy cost
```

with smaller worst-case losses than any fixed representation.

## 3. Novelty Boundary

Do **not** claim these individually as novel:
- growable stacks;
- segmented stacks;
- heap-resident continuations;
- stack copying or shrinking;
- stack-to-heap migration;
- dynamic heapification;
- virtual-thread stack chunks;
- online learning;
- switching-cost optimization.

The gap to investigate is the **combination**:

> **Per-activation, bidirectional, online selection and migration among multiple activation-state representations, using runtime behavior, with explicit comparison against a hindsight oracle and switching cost.**

This exact novelty claim must be verified through a systematic literature review before publication.

## 4. Prior Work That Defines the Boundary

### Go
Go used segmented stacks and moved to contiguous growing stacks because segmented stacks suffered from the hot-split problem.

Relevant sources:
- https://go.dev/s/contigstacks
- https://www.golang.design/under-the-hood/en/part4memory/ch14stack/design/

**Implication:** different stack representations have different performance regimes.

### Java Virtual Threads / Project Loom
Virtual-thread stacks are stored as heap-resident stack chunks and grow/shrink dynamically.

Source:
- https://openjdk.org/jeps/444

**Implication:** heap-resident execution stacks are already practical at high concurrency.

### Dynamic Heapification
Recent managed-runtime work performs optimistic stack allocation and moves escaping objects to the heap dynamically.

Paper:
- *Optimistic Stack Allocation and Dynamic Heapification for Managed Runtimes*
- https://doi.org/10.1145/3656389

**Implication:** migration from stack-like to heap storage is feasible, so our contribution cannot simply be "move something from stack to heap."

### Online Learning With Switching Cost
Online-learning research studies decisions made without future knowledge and compares them against changing best policies.

Example:
- *Non-stationary Online Learning with Memory and Non-stochastic Control*
- https://proceedings.mlr.press/v151/zhao22a.html

**Implication:** dynamic regret plus switching cost gives MorphStack a rigorous mathematical framework.

## 5. Formal Problem

At phase `t`, runtime state is:

```text
x_t
```

Representations:

```text
R = {
    contiguous_stack,
    segmented_stack,
    heap_frame
}
```

The runtime chooses:

```text
r_t ∈ R
```

Execution cost:

```text
C(r_t, x_t)
```

Migration cost:

```text
M(r_t, r_(t+1))
```

Total cost:

```text
C_adaptive =
Σ C(r_t, x_t)
+
Σ M(r_t, r_(t+1))
```

## 6. Oracle Baseline

Build an offline oracle that sees the entire execution trace and computes the lowest-cost representation sequence.

```text
C_oracle
```

Define regret:

```text
Regret = C_adaptive - C_oracle
```

and normalized regret:

```text
NormalizedRegret =
(C_adaptive - C_oracle) / C_oracle
```

This should be one of the headline metrics.

## 7. Research Questions

### RQ1 — Adaptation
Can an online runtime outperform all fixed activation-memory representations on mixed workloads?

### RQ2 — Oracle Distance
How close can the adaptive policy remain to the hindsight-optimal policy?

### RQ3 — Migration Cost
When does migration become too expensive to justify switching?

### RQ4 — Prediction Features
Which signals matter most?

Candidates:
```text
frame size
call depth
recursion depth
activation lifetime
time since last execution
suspension count
resume count
suspension duration
allocation pressure
active-context count
memory pressure
cache misses
TLB misses
```

### RQ5 — Granularity
Should adaptation happen per:
```text
whole context
stack segment
function activation
group of activations
execution phase
```

### RQ6 — Stability
Can the runtime avoid oscillation such as:

```text
stack → heap → stack → heap
```

when behavior fluctuates near thresholds?

## 8. Phase 1 — Systematic Literature Review

Search aggressively for:

```text
adaptive stack representation
dynamic stack representation
runtime activation record migration
activation record heapification
continuation frame migration
stack heap hybrid runtime
adaptive continuations
runtime stacklets
online activation placement
dynamic execution-state representation
adaptive coroutine stacks
stack cache continuation
frame relocation runtime
```

Search:
- ACM Digital Library
- IEEE Xplore
- USENIX
- arXiv
- Springer
- ScienceDirect
- Google Scholar

Create a matrix:

| Work | Stack | Segmented | Heap | Migration | Bidirectional | Per Activation | Online Policy | Oracle Comparison |
|---|---|---|---|---|---|---|---|---|

Proceed only if no existing work already implements the complete idea.

## 9. Phase 2 — Trace Study

Before building a runtime, determine whether real workloads contain enough phase variation for adaptation to matter.

Collect:
```text
activation lifetime
maximum depth
frame size
suspension duration
resume frequency
recursion behavior
working-set size
concurrency
```

Workload classes:
```text
ordinary synchronous server
deep recursion
many shallow coroutines
rare suspension
frequent suspension
long-lived suspended tasks
burst concurrency
mixed CPU + I/O
```

### Go/No-Go Test

If behavior is mostly stationary, adaptive representation may provide little value.

If the best representation changes meaningfully across phases, the research case becomes much stronger.

## 10. Phase 3 — Simulator First

Do **not** start by modifying a production runtime.

Build a trace-driven simulator.

Input:
```text
activation events
suspend/resume events
frame sizes
lifetimes
memory pressure
```

Simulate:
```text
contiguous policy
segmented policy
heap-frame policy
adaptive policy
oracle policy
```

Model:
```text
allocation cost
copy cost
migration cost
pointer-fixup cost
cache penalty
memory footprint
resume cost
```

## 11. Baselines

Implement:

### A — Always Contiguous
```text
all activations → contiguous stack
```

### B — Always Segmented
```text
new segment when required
```

### C — Always Heap
```text
every activation independently allocated
```

### D — Static Heuristic
Example:
```text
if function_can_suspend:
    heap
else:
    stack
```

### E — Oracle
Offline optimal policy with full future knowledge.

## 12. First Adaptive Policy

Do not begin with ML.

Use thresholds:

```text
if suspension_count >= S
and expected_idle_time >= I:
    migrate_to_heap

if resume_frequency >= R
and hot_execution_time >= H:
    migrate_to_contiguous
```

Add hysteresis:

```text
switch only if
expected_savings >
migration_cost × safety_factor
```

## 13. Second Adaptive Policy

After the heuristic baseline, try an online decision algorithm.

Candidates:
```text
multi-armed bandit
contextual bandit
online convex optimization
expert-advice algorithm
Markov decision process
```

A contextual-bandit formulation is attractive:

```text
context:
runtime features

actions:
stack / segmented / heap

reward:
-negative total cost
```

Switching cost must be explicitly penalized.

## 14. Optional Lifetime Predictor

Estimate:
```text
P(short-lived)
P(long-lived)
P(frequently suspended)
P(recursive)
P(hot after resume)
```

Start simple:
```text
logistic regression
decision tree
gradient boosting
```

Avoid making deep learning necessary.

## 15. Runtime Prototype

Only after the simulator shows value should you implement a real runtime.

Use a **custom coroutine / continuation runtime** rather than modifying a full language runtime initially.

Possible API:

```c
context_t *spawn(fn);
yield();
suspend();
resume(ctx);
```

Internally support multiple frame representations.

## 16. Representations

### Contiguous Stack
Strengths:
```text
excellent locality
cheap call/return
cheap deallocation
```

Weaknesses:
```text
growth relocation
poor fit for many cold suspended contexts
```

### Stacklets / Segments
Strengths:
```text
incremental growth
no whole-stack copy
```

Weaknesses:
```text
metadata
pointer chasing
boundary effects
```

### Heap-Resident Frames
Strengths:
```text
arbitrary lifetime
cheap suspension
detached continuations
```

Weaknesses:
```text
allocation overhead
locality loss
metadata overhead
```

## 17. Migration Mechanism

Migration must preserve:
```text
locals
arguments
return continuation
saved registers
references
frame metadata
```

Example:
```text
contiguous frame
      ↓
copy active frame
      ↓
rewrite references
      ↓
install heap-frame descriptor
      ↓
resume
```

## 18. Safe Initial Restrictions

Start with:
```text
Linux
x86-64
single runtime
custom coroutine API
known frame layouts
no arbitrary C pointers into movable frames
single process
```

The purpose is to test the research idea, not arbitrary native compatibility.

## 19. Experimental Workloads

### W1 — Normal Nested Calls
Expected winner:
```text
contiguous
```

### W2 — Deep Recursion
Tests:
```text
growth
copy cost
segmentation
```

### W3 — Massive Shallow Concurrency
Example:
```text
100k–1M suspended contexts
```

### W4 — Frequent Suspend/Resume
Tests switching and migration.

### W5 — Long Suspensions
Tests compact/cold storage.

### W6 — Phase-Changing Workload
Example:
```text
Phase A:
CPU-heavy recursion

Phase B:
many suspended tasks

Phase C:
CPU-heavy processing again
```

This is likely the most important workload because the best representation changes over time.

## 20. Metrics

Measure:
```text
execution time
throughput
p50 latency
p95 latency
p99 latency
peak RSS
bytes per live activation
migration count
bytes migrated
migration time
allocation count
cache misses
LLC misses
TLB misses
suspend time
resume time
regret relative to oracle
```

## 21. Key Paper Graphs

1. **Throughput vs workload**
2. **Memory usage vs concurrency**
3. **Adaptive cost / Oracle cost**
4. **Migration frequency vs threshold**
5. **Chosen representation over time**

Example:

```text
time →
STACK STACK STACK | HEAP HEAP | STACK STACK
```

## 22. Ablation Study

Disable one signal at a time:

```text
-no lifetime
-no suspension frequency
-no cache counters
-no memory pressure
-no hysteresis
```

Measure how much the policy degrades.

## 23. Adversarial Evaluation

Construct workloads designed to fool the policy:

```text
short suspend
long suspend
short suspend
long suspend
...
```

or behavior that changes immediately after migration.

This tests whether switching cost causes thrashing.

## 24. Go / No-Go Criteria

The research is promising if:

1. no fixed representation dominates every workload;
2. the best representation changes within at least some realistic workloads;
3. the adaptive policy identifies those phases cheaply enough to approach the oracle.

A useful **research target**, not an assumed result, is:

```text
adaptive within roughly 5–15% of oracle
```

on mixed workloads while avoiding the worst failures of fixed policies.

## 25. Negative Results Are Still Valuable

If:

```text
migration overhead > adaptation benefit
```

for almost all realistic workloads, that is itself a meaningful finding.

A possible negative-result conclusion:

> Activation-state representation is effectively a coarse-grained or one-way decision on current hardware because migration cost dominates the value of runtime phase adaptation.

## 26. Potential Contributions

If successful:

1. formal online formulation of activation-state representation selection with migration cost;
2. multi-representation runtime capable of changing activation layout during execution;
3. phase-aware online selection policy;
4. offline oracle for hindsight-optimal placement;
5. workload characterization showing when each representation wins;
6. evaluation of regret, memory, migration, cache behavior, and latency.

## 27. Paper Structure

```text
1. Introduction
2. Motivation
3. Background
4. Problem Formulation
5. MorphStack Design
6. Online Policy
7. Implementation
8. Evaluation
9. Ablation + Adversarial Results
10. Related Work
11. Limitations
12. Conclusion
```

## 28. Development Sequence

```text
Literature review
       ↓
Trace collection
       ↓
Workload characterization
       ↓
Cost model
       ↓
Offline oracle
       ↓
Trace simulator
       ↓
Fixed baselines
       ↓
Adaptive heuristic
       ↓
Online policy
       ↓
Prototype runtime
       ↓
Benchmarks
       ↓
Ablations
       ↓
Paper
```

## 29. Immediate First Milestone

Before writing the runtime:

### Task 1
Build the literature matrix.

### Task 2
Define the activation-event trace format.

### Task 3
Build 5–8 synthetic workload generators.

### Task 4
Implement the offline oracle and fixed-policy simulator.

Only after those four steps should you decide whether MorphStack deserves a full runtime implementation.

## Final Research Thesis

> **Execution-state memory should not necessarily use one representation for its entire lifetime. A runtime may be able to observe control-flow behavior and migrate activation state between representations, approaching the performance of an oracle that knows future behavior while avoiding pathological cases of fixed policies.**

The goal is not to defeat fundamental memory-allocation trade-offs.

The goal is to determine whether an **online adaptive system can move intelligently across those trade-offs**.
