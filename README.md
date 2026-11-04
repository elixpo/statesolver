# StateSolver

**A reversible Linux debugger that reconstructs *why* a machine state exists, treating each heap allocation lifetime as a distinct object even when its address is reused.**

StateSolver records a native x86-64 program once. You can then move through its registers, memory, allocations, and object lifetimes, and ask causal questions:

```bash
statesolver record ./program
statesolver timeline
statesolver lifetime 0x55550000
statesolver why rax --event 91800
statesolver explain-crash
```

**The paper** asks whether LLM debugging agents find the true root cause of real memory bugs more accurately, and make fewer claims the trace doesn't support, when they answer through these allocation-lifetime-aware causal queries instead of raw replay-debugger access.

StateSolver uses [rr](https://rr-project.org) for recording and replay, and serves its queries over the CLI and as MCP tools for agents such as [Crumb](https://github.com/elixpo/crumb.elixpo).

- **Locked direction (project + paper): [docs/DIRECTION.md](docs/DIRECTION.md)**
- Design brief: [docs/research-direction.md](docs/research-direction.md)
- Literature review: [docs/slr/](docs/slr/verdict.md)
- Stage tracking: [tracking-issues/](tracking-issues/), mirrored as GitHub issues #1–#5
- Open work: [TODO.md](TODO.md)
- What we have learned: [DISCOVERIES.md](DISCOVERIES.md)
- Earlier direction: [MorphStack (archived)](docs/archive/morphstack/README.md)

**Status:** Stage 1 is done. Stage 2 (#2) is in progress: the rr backend.

## Layout

| Crate | Purpose | Stage |
|---|---|---|
| `statesolver-trace` | `.ss` trace format: events, allocation epochs, checkpoints index | 1 |
| `statesolver-record` | reference ptrace recorder; now a **test fixture** (rr is the backend) | 2 |
| `statesolver-replay` | rr replay driver (gdb remote protocol), historical state, on-demand epochs | 2 |
| `statesolver-analysis` | temporal heap, causal query engine, crash explainer, evidence IDs | 3 |
| `statesolver-harness` | MCP server, sessions, evidence model, Crumb agent, study arms | 4 |
| `statesolver-cli` | the `statesolver` binary | all |

`bug-corpus/` holds the controlled C/C++ programs used for evaluation.

Part of the [Elixpo](https://elixpo.com) project family, developed alongside [Crumb](https://github.com/elixpo/crumb.elixpo).

## License

MIT, see [LICENSE](LICENSE).
