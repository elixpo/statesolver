# Stage 5 — Interaction harness (Crumb agent), visual UI, release, and paper

**Labels:** `tracking`, `stage-5`, `harness`, `ui`, `release`, `paper`, `crumb`

## Outcome

The same analysis engine is usable from the CLI, an evidence-grounded agent, and a visual timeline. The tool ships the way Crumb does, and the paper is written last.

## Scope

### Harness (research direction §20)
- [ ] A stable tool API: `get_event`, `get_register`, `read_memory`, `get_allocation_history`, `find_last_writer`, `find_pointer_origin`, `build_causal_slice`, `rewind`, `search_events`, and `explain_crash`.
- [ ] Investigation sessions that track the current event, selected allocation, and active hypothesis.
- [ ] Transport: JSON-RPC over stdio first, then an **MCP server** (`statesolver mcp`).
- [ ] An evidence model: every statement is tagged **FACT** (from the trace), **INFERENCE** (from the engine), or **HYPOTHESIS** (from the agent), and a hypothesis must be verified by queries before it is shown as a root cause.
- [ ] **Crumb as the agent:** `statesolver-harness` embeds `crumb-harness` behind a cargo feature (`statesolver ask "why did this crash?"`). StateSolver is also published as a Crumb marketplace package (MCP capability plus instruction skill). The core debugger never depends on an LLM.

### Visual UI (research direction §15, Stage 2B)
- [ ] A timeline, register and memory inspectors, allocation lifetime bars with visible epoch changes, a stack view, a source and disassembly view, and a clickable causal graph. All are consumers of the same analysis API, so the UI never calls ptrace.

### Release
- [ ] `install.sh` and Debian packaging (the recorder is Linux-only; trace analysis is cross-platform), Homebrew for analysis-only builds, and Sigstore-signed releases. Reuse Crumb's pipeline.
- [ ] Listings on elixpo.com and research.elixpo.com, and on `statesolver.elixpo.com`.

### Paper (last)
- [ ] Native Google Scholar and ACM DL re-run of the literature review, with forward citations.
- [ ] Paper plus an artifact (bug corpus, traces, `statesolver bench --all`). Possible arXiv categories: cs.SE (primary) and cs.PL / cs.CR. Possible venues: ICSE, FSE, ISSTA, ASE, or CGO. arXiv needs an endorsement.
