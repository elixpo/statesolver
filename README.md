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

The research question: can deterministic replay reconstruct **allocation-epoch-aware causal slices on demand**, without the cost of recording a complete dynamic dependence graph during the original run? And does treating `address + lifetime` as object identity improve root-cause localization for temporal memory bugs?

- Research direction (locked): [docs/research-direction.md](docs/research-direction.md)
- Stage tracking: [tracking-issues/](tracking-issues/), mirrored as GitHub issues #1–#5
- Open work: [TODO.md](TODO.md)
- What we have learned: [DISCOVERIES.md](DISCOVERIES.md)
- Earlier direction: [MorphStack (archived)](docs/archive/morphstack/README.md)

**Status:** Stage 1 (#1): foundations, literature review, and trace format.

## Layout

| Crate | Purpose | Stage |
|---|---|---|
| `statesolver-trace` | `.ss` trace format: events, allocation epochs, checkpoints index | 1 |
| `statesolver-record` | ptrace recorder, syscall and allocation capture (may use `unsafe`) | 2 |
| `statesolver-replay` | checkpoints, deterministic replay, historical state | 3 |
| `statesolver-analysis` | temporal object graph, query-driven provenance, crash explainer | 4 |
| `statesolver-harness` | tool API, sessions, evidence model, MCP server, Crumb agent | 5 |
| `statesolver-cli` | the `statesolver` binary | all |

`bug-corpus/` holds the controlled C/C++ programs used for evaluation.

Part of the [Elixpo](https://elixpo.com) project family, developed alongside [Crumb](https://github.com/elixpo/crumb.elixpo).

## License

MIT, see [LICENSE](LICENSE).
