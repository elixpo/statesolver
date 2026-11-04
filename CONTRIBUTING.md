# Contributing to StateSolver

StateSolver is a research project and debugger from Elixpo: a reversible Linux debugger that reconstructs allocation-epoch-aware causal explanations on demand, using deterministic replay.

## Before starting

- Read the [research direction](docs/research-direction.md) and the stage tracking issues (#1 to #5).
- Open an issue before making a substantial change to the trace format, replay semantics, or the causal-reconstruction algorithm. These affect the paper's results.
- Use the *Research question or related work* issue template to suggest papers.

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

## Tracking

- Update [TODO.md](TODO.md) in the same commit as the work.
- Record every finding, dead end, and design correction in [DISCOVERIES.md](DISCOVERIES.md), with its date and evidence.

## Rules for research code

- **Collect facts first, analyse second, visualise third.** The UI and the agent never touch ptrace or `/proc`; they go through the analysis API.
- **`unsafe` stays in `statesolver-record` and `statesolver-replay`.** Every block needs a `// SAFETY:` comment that justifies it.
- **Results never depend on an LLM.** Agentic features live behind the optional harness feature, and every factual claim must come from a query result.
- **Every bug-corpus entry has a written ground truth** (`ROOT_CAUSE.md`) before it is used in an evaluation.
- **Every paper number comes from one command.** No hand-written numbers.
- **Citations must be verified.** Never add a reference you have not located yourself.

## License

Contributions are licensed under the MIT License, as described in [LICENSE](LICENSE).
