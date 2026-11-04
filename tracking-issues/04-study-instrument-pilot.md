# Stage 4 — Study instrument: MCP server, Crumb agent, arms A0–A5, benchmarks, and the pilot

**Labels:** `tracking`, `stage-4`, `harness`, `research`, `crumb`

Direction: [docs/DIRECTION.md](../docs/DIRECTION.md) §2.

## Scope
- [ ] `statesolver mcp`: the query engine as MCP tools, with evidence IDs in every result, plus a session context (current event, selected allocation, hypothesis).
- [ ] Evidence model: FACT / INFERENCE / HYPOTHESIS, and a claim-citation protocol enforced the same way in every arm.
- [ ] Arms:
  - A0: ASan log only
  - A1: raw rr/gdb/pwndbg over MCP (DebugHarness replication)
  - A2: shell plus rr
  - A3: raw rr plus scripted summaries
  - A4: StateSolver
  - A5: StateSolver address-only
  - All arms share the same budgets and prompt skeleton.
- [ ] Agent runner on **Crumb** (`crumb-harness`, feature-gated), plus at least one other harness for generality, with 3 or more models (at least one open-weight).
- [ ] Benchmark loader: SEC-bench (primary), ARVO with T2L-ARVO, VulnLoc. Build each one, record it with rr, and log any recording failures.
- [ ] Annotation tooling: root-cause guideline, two annotators, κ, multi-location ground truth.
- [ ] Arm-blind claim checker: automatic replay re-checks where possible, and a blinded human rating UI for the rest.
- [ ] **Pilot:** about 20 SEC-bench bugs, 1 model, A1 against A4 (plus A5 on the reuse cases), then a go/adapt decision written to DISCOVERIES.md.

## Exit criteria
- The pilot result is recorded, and the pre-registration document is frozen before the full run.
