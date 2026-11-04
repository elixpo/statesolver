# Stage 4 — Multi-representation coroutine runtime and real benchmarks

**Labels:** `tracking`, `stage-4`, `runtime`, `unsafe`

## Outcome

Build a real Rust coroutine runtime in which a running context's activation state can **actually migrate** between contiguous, segmented, and heap-frame representations, driven by the policy chosen in Stage 3. Then show that the simulator's predictions hold on real hardware.

This is the heaviest stage and the only one with `unsafe` code.

## Scope

### Runtime (`morphstack-rt`)
- [ ] API (Rust-first, with a C ABI added later):
  ```rust
  let ctx = rt.spawn(f);   // returns a context handle
  yield_now();             // cooperative yield
  suspend();               // park until resumed
  rt.resume(ctx);
  ```
- [ ] Context switching through hand-written assembly for x86-64 System V first. Keep the switch code in one module behind a small trait so aarch64 (macOS) and Windows x64 can be added in the same way.
- [ ] Three representations:
  - **Contiguous**: mmap'd stack with a guard page and growth by copy (Go-style).
  - **Segmented**: linked stacklets with a split-check prologue and a fix for hot-split thrashing.
  - **Heap frame**: each activation allocated separately as an explicit frame object.
- [ ] **Known frame layouts** (plan section 18): coroutine bodies are written against a frame-descriptor macro or proc-macro that records slot offsets and which slots hold references. Migration needs exactly this information, and it is what makes migration sound.
- [ ] Migration engine: copy the frame, rewrite references using the descriptors, install the new descriptor, then resume (plan section 17). Run it in debug builds with a verifier that checks every migrated frame against a shadow copy.
- [ ] A policy hook that embeds `morphstack-policy` and keeps the decision path free of allocations.
- [ ] A trace emitter, so the runtime records traces in format v0. This closes the loop: runtime trace → simulator → oracle regret for the *real* run.

### Safety and correctness
- [ ] Wrap every `unsafe` block with `// SAFETY:`, and run `cargo miri` on everything that is not context-switch code.
- [ ] Differential testing: every workload produces bit-identical program output under all three fixed representations and under the adaptive policy.
- [ ] Fuzz the migration engine with `cargo-fuzz` using randomized frame layouts and migration points.

### Benchmarks (`morphstack-bench`)
- [ ] Port W1–W6 and the adversarial suite onto the runtime.
- [ ] Collect every metric in plan section 20: latency percentiles, peak RSS, bytes per live activation, migration statistics, and cache, LLC, and TLB misses via `perf_event_open` on Linux.
- [ ] External baselines for context: Tokio tasks, `may` or `corosensei` stackful coroutines, and Go goroutines, run on the same workloads.
- [ ] **Model validation**: compare simulator-predicted cost with measured cost. Report the correlation and the error bars. If they disagree, fix the cost model and re-run Stage 3.

## Platform support (honest scope)
| Component | Linux x86-64 | macOS aarch64 | Windows x64 |
|---|---|---|---|
| Simulator, oracle, policies, CLI | yes | yes | yes |
| Runtime, research claims | **yes (primary)** | stretch goal | stretch goal |
| HW-counter benchmarks | yes | partial | no |

## Exit criteria
- Differential tests and the migration fuzzer run clean in CI for at least 24 CPU-hours in total.
- Paper graphs 1 and 2 (throughput, and memory against concurrency) are produced from **real** runs.
- Simulator and runtime agree within the error bars set in the validation report.
