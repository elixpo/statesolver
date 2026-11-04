# MorphStack

**Online adaptive representation of activation state.**

MorphStack studies whether a runtime can choose and migrate activation records among contiguous stacks, segmented stacklets, and heap-resident frames at runtime, while keeping total cost close to the best representation policy in hindsight.

- Research plan: [morphstack_research_plan.md](morphstack_research_plan.md)
- Stage tracking: [tracking-issues/](tracking-issues/) (mirrored as GitHub tracking issues)
- Open work: [TODO.md](TODO.md)
- What we have learned: [DISCOVERIES.md](DISCOVERIES.md)

Status: Stage 1 (#1). The literature review is in progress; trace format v0 and the `morphstack trace` CLI are working.

## Quick start

```bash
cargo run -p morphstack-cli -- trace stats docs/examples/phase-change.jsonl
cargo run -p morphstack-cli -- trace convert docs/examples/phase-change.jsonl /tmp/t.msb
cargo run -p morphstack-cli -- trace validate /tmp/t.msb
```

## Layout

| Crate | Purpose | Stage |
|---|---|---|
| `morphstack-trace` | Trace format v0 ([spec](docs/trace-format.md)): JSONL and binary, validation, statistics | 1 |
| `morphstack-workloads` | Seeded synthetic workload generators | 2 |
| `morphstack-cost` | Calibrated cost model and migration-cost matrix | 2 |
| `morphstack-sim` | Simulator, fixed baselines, hindsight oracle | 2 |
| `morphstack-policy` | Online policies | 3 |
| `morphstack-rt` | Multi-representation coroutine runtime (the only crate allowed `unsafe`) | 4 |
| `morphstack-bench` | Benchmarks and hardware counters | 4 |
| `morphstack-cli` | The `morphstack` binary | all |
| `morphstack-agent` | Optional `crumb-harness` integration | 5 |

Part of the [Elixpo](https://elixpo.com) project family, developed alongside [Crumb](https://github.com/elixpo/crumb.elixpo).

## License

MIT, see [LICENSE](LICENSE).
