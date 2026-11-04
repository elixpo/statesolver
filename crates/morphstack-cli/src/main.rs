//! `morphstack` command-line interface.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use morphstack_trace::{Format, StatsCollector, TraceWriter, Validator};

#[derive(Parser)]
#[command(
    name = "morphstack",
    version,
    about = "Online adaptive activation-state representation: traces, simulation, and oracle analysis"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect, check, and convert activation-event traces.
    #[command(subcommand)]
    Trace(TraceCmd),
    /// Generate a synthetic workload trace.
    Gen {
        /// Workload name, or `list` to show all.
        workload: String,
        /// Output file (`.msb` for binary, otherwise JSONL).
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Seed.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Size multiplier (contexts, iterations).
        #[arg(long, default_value_t = 1.0)]
        scale: f64,
    },
}

#[derive(Subcommand)]
enum TraceCmd {
    /// Check a trace against the format rules. Exits non-zero on violations.
    Validate {
        /// Trace file (JSONL or `.msb`; detected from content).
        path: PathBuf,
        /// Maximum violations to print.
        #[arg(long, default_value_t = 20)]
        max_errors: usize,
    },
    /// Print summary statistics.
    Stats {
        /// Trace file.
        path: PathBuf,
        /// Emit JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Convert between JSONL and binary.
    Convert {
        /// Input trace.
        input: PathBuf,
        /// Output trace.
        output: PathBuf,
        /// Output encoding; defaults to `.msb` → binary, otherwise JSONL.
        #[arg(long, value_enum)]
        to: Option<OutFormat>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum OutFormat {
    Jsonl,
    Msb,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Gen {
            workload,
            out,
            seed,
            scale,
        } => gen_trace(&workload, out.as_deref(), seed, scale),
        Command::Trace(TraceCmd::Validate { path, max_errors }) => validate(&path, max_errors),
        Command::Trace(TraceCmd::Stats { path, json }) => stats(&path, json),
        Command::Trace(TraceCmd::Convert { input, output, to }) => {
            let format = match to {
                Some(OutFormat::Jsonl) => Format::Jsonl,
                Some(OutFormat::Msb) => Format::Binary,
                None => Format::from_path(&output),
            };
            convert(&input, &output, format)
        }
    }
}

fn open(path: &Path) -> Result<morphstack_trace::TraceReader<std::io::BufReader<std::fs::File>>> {
    morphstack_trace::open(path).with_context(|| format!("reading {}", path.display()))
}

fn validate(path: &Path, max_errors: usize) -> Result<ExitCode> {
    let reader = open(path)?;
    let mut validator = Validator::new(reader.header());
    let (mut events, mut violations) = (0u64, 0u64);
    for event in reader {
        let event = event.with_context(|| format!("decoding {}", path.display()))?;
        events += 1;
        for v in validator.check(&event) {
            violations += 1;
            if violations <= max_errors as u64 {
                eprintln!("{v}");
            }
        }
    }
    if violations == 0 {
        println!("ok: {events} events");
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("{violations} violation(s) in {events} events");
        Ok(ExitCode::FAILURE)
    }
}

fn stats(path: &Path, json: bool) -> Result<ExitCode> {
    let reader = open(path)?;
    let header = reader.header().clone();
    let format = reader.format();
    let mut collector = StatsCollector::new();
    for event in reader {
        collector.observe(&event.with_context(|| format!("decoding {}", path.display()))?);
    }
    let s = collector.finish();
    if json {
        println!("{}", serde_json::to_string_pretty(&s)?);
        return Ok(ExitCode::SUCCESS);
    }
    println!(
        "trace        {} ({format:?}, v{})",
        path.display(),
        header.version
    );
    println!("source       {}", header.source);
    if let Some(seed) = header.seed {
        println!("seed         {seed}");
    }
    println!("events       {}", s.events);
    for (name, n) in &s.by_kind {
        if *n > 0 {
            println!("  {name:<10} {n}");
        }
    }
    println!("duration     {} ns", s.duration_ns);
    println!(
        "contexts     {} spawned, peak {} live, peak {} suspended, {} live at end",
        s.contexts, s.peak_live_contexts, s.peak_suspended_contexts, s.contexts_live_at_end
    );
    println!(
        "activations  {}, max depth {}, peak live frame bytes {}",
        s.activations, s.max_depth, s.peak_live_frame_bytes
    );
    println!();
    println!(
        "{:<24}{:>10}{:>14}{:>12}{:>12}{:>12}{:>14}",
        "distribution", "count", "mean", "p50", "p90", "p99", "max"
    );
    for (name, d) in [
        ("frame bytes", &s.frame_bytes),
        ("activation lifetime ns", &s.activation_lifetime_ns),
        ("suspension ns", &s.suspension_ns),
        ("depth at suspend", &s.depth_at_suspend),
    ] {
        println!(
            "{name:<24}{:>10}{:>14.1}{:>12}{:>12}{:>12}{:>14}",
            d.count, d.mean, d.p50, d.p90, d.p99, d.max
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn convert(input: &Path, output: &Path, format: Format) -> Result<ExitCode> {
    let reader = open(input)?;
    let file =
        std::fs::File::create(output).with_context(|| format!("creating {}", output.display()))?;
    let mut writer = TraceWriter::new(std::io::BufWriter::new(file), format, reader.header())?;
    let mut n = 0u64;
    for event in reader {
        writer.write(&event.with_context(|| format!("decoding {}", input.display()))?)?;
        n += 1;
    }
    writer.finish()?;
    println!("wrote {n} events to {} ({format:?})", output.display());
    Ok(ExitCode::SUCCESS)
}

fn gen_trace(name: &str, out: Option<&Path>, seed: u64, scale: f64) -> Result<ExitCode> {
    use morphstack_workloads::{Spec, Workload};
    if name == "list" {
        for w in Workload::ALL {
            println!("{}", w.name());
        }
        return Ok(ExitCode::SUCCESS);
    }
    let workload = Workload::from_name(name)
        .with_context(|| format!("unknown workload {name:?}; try `morphstack gen list`"))?;
    anyhow::ensure!(scale > 0.0 && scale.is_finite(), "scale must be positive");
    let out = out.context("--out is required")?;
    let spec = Spec {
        workload,
        seed,
        scale,
    };
    let mut writer = morphstack_trace::create(out, &morphstack_workloads::header(&spec))
        .with_context(|| format!("creating {}", out.display()))?;
    let mut n = 0u64;
    morphstack_workloads::generate(&spec, &mut |e| {
        n += 1;
        writer.write(&e)
    })?;
    writer.finish()?;
    println!("wrote {n} events to {}", out.display());
    Ok(ExitCode::SUCCESS)
}
