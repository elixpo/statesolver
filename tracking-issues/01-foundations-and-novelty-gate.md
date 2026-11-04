# Stage 1 — Foundations, literature review, and novelty gate

**Labels:** `tracking`, `stage-1`, `research`, `infra`

## Outcome

Set up a MorphStack repository that meets Crumb's quality bar from the first commit, and settle the question that decides whether there is a paper at all: **has someone already published per-activation, bidirectional, online multi-representation migration that is evaluated against a hindsight oracle?**

## Why this comes first

The research plan (section 8) says to continue only if no prior work implements the complete idea. Every later stage depends on that answer, along with the trace format and the repository conventions. Changing any of them after Stage 2 starts would mean rework across every crate.

## Scope

### Repository and engineering baseline (reuse Crumb's setup)
- [ ] Create `elixpo/<name>` and reuse Crumb's governance files: `LICENSE` (MIT), `CONTRIBUTING`, `CODE_OF_CONDUCT`, `SECURITY`, `GOVERNANCE`, `TRADEMARKS`, `CODEOWNERS`, and the issue and PR templates.
- [ ] Rust workspace with `edition = "2024"`, a pinned `rust-toolchain.toml`, and clippy `all` + `pedantic` set to warn, matching Crumb.
- [ ] Unsafe policy: set `unsafe_code = "forbid"` for the workspace. **Only** `morphstack-rt` (Stage 4) may override it to `deny`, and each `unsafe` block there needs an `// SAFETY:` comment.
- [ ] CI that mirrors Crumb's `ci.yml`, `quality-gate.yml`, `release-gate.yml`, and dependabot setup, running on the Linux, macOS, and Windows matrix.
- [ ] Workspace skeleton with empty crates:
  ```text
  crates/morphstack-trace      trace format, reader/writer, validation
  crates/morphstack-workloads  synthetic generators (Stage 2)
  crates/morphstack-cost       parametric + calibrated cost model (Stage 2)
  crates/morphstack-sim        simulator, fixed baselines, oracle (Stage 2)
  crates/morphstack-policy     online policies (Stage 3)
  crates/morphstack-rt         coroutine runtime, only crate allowed `unsafe` (Stage 4)
  crates/morphstack-bench      benchmark harness + perf counters (Stage 4)
  crates/morphstack-cli        `morphstack` binary (all stages, shipped in Stage 5)
  crates/morphstack-agent      optional, feature-gated crumb-harness integration (Stage 5)
  paper/                       LaTeX source, figures generated from `results/`
  ```

### Systematic literature review
- [ ] Run the search terms in plan section 8 against ACM DL, IEEE Xplore, USENIX, arXiv, Springer, ScienceDirect, and Google Scholar. Record each query, its date, and its hit count in `docs/slr/protocol.md` so the review can be reproduced.
- [ ] Add these search areas, which the plan does not list yet:
  - **Metrical task systems (MTS)** and **smoothed online optimization**. Choosing among a few representations with a migration cost between each pair *is* an MTS. Starting points: Borodin–Linial–Saks (1992); work on asymmetric MTS, since stack→heap and heap→stack costs differ; and online algorithms with switching costs.
  - **Stack caching and register windows** (SPARC, Ertl).
  - Lazy stack allocation in Scheme and ML: Hieb–Dybvig–Bruggeman's segmented stacks, Chez Scheme, SML/NJ heap frames, and Clinger et al.'s *Implementation strategies for first-class continuations*.
  - Stack representation in OCaml 5 effect handlers (fibers), WasmFX/stack switching, Kotlin and C# async state machines, and Rust `async` lowering.
  - Prior work on adaptive or learned runtime decisions, such as JIT tiering and GC policy selection, to position the online-policy contribution.
- [ ] Fill in the comparison matrix in `docs/slr/matrix.md` using the columns in plan section 8.
- [ ] Write a one-page **novelty verdict**: GO (the gap exists), PIVOT (narrow the claim, e.g. to "first MTS-framed evaluation"), or STOP.

### Trace format v0
- [ ] Write the spec in `docs/trace-format.md`. Events: `spawn`, `call`, `return`, `suspend`, `resume`, `exit`, `mem_sample`. Fields: `ts_ns`, `ctx_id`, `act_id`, `fn_id`, `frame_bytes`, `depth`, `can_suspend`, and optional hardware counters.
- [ ] Two encodings: JSONL, which is readable and used for tests, and a compact binary format for traces with more than 10⁷ events. Both get a version header.
- [ ] `morphstack trace validate` and `morphstack trace stats` subcommands.

### Name and identity
- [ ] Confirm the final name, then reserve it on crates.io, GitHub, and `<name>.elixpo.com`.

## Exit criteria
- CI is green on all three operating systems, and `cargo clippy -- -D warnings` is clean.
- The SLR matrix and the novelty verdict are merged.
- The trace format v0 spec is merged, with a validator and round-trip tests.

## Go/no-go gate
A **STOP** or **PIVOT** verdict blocks Stage 2 until the claim in the research plan's section 3 is rewritten.
