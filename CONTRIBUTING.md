# Contributing to MorphStack

MorphStack is a research project and CLI from Elixpo. It asks whether a runtime can migrate activation records between contiguous stacks, segmented stacklets, and heap frames online while staying close to a hindsight-optimal policy.

## Before starting

- Read the [research plan](morphstack_research_plan.md) and the stage tracking issues (#1 to #5).
- Open an issue before making a substantial change to the trace format, the cost model, the oracle, or a policy. These affect the paper's results.
- Use the *Research question or related work* issue template to suggest papers.

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

## Tracking

- Update [TODO.md](TODO.md) in the same commit as the work.
- Record every finding, dead end, and model correction in [DISCOVERIES.md](DISCOVERIES.md), with its date and evidence.

## Rules for research code

- **Determinism.** Every generator, simulation, and report must give the same output from the same seed and cost profile.
- **No hand-written numbers in the paper.** Every figure and table comes from `morphstack report`.
- **`unsafe` stays in `morphstack-rt`.** Every block needs a `// SAFETY:` comment that justifies it.
- **Results never depend on an LLM.** Agentic features live behind the optional `morphstack-agent` crate.
- **Citations must be verified.** Never add a reference you have not located yourself.

## License

Contributions are licensed under the MIT License, as described in [LICENSE](LICENSE).
