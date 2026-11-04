//! `statesolver` command-line interface.

use clap::Parser;

/// Reversible Linux debugger with allocation-epoch-aware causal reconstruction.
#[derive(Parser)]
#[command(name = "statesolver", version, about)]
struct Cli {}

fn main() {
    let _ = Cli::parse();
    eprintln!("statesolver: nothing implemented yet; see elixpo/statesolver#1");
}
