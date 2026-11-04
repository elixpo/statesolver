//! `morphstack` command-line interface.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use morphstack_cost::{CostModel, Rep};
use morphstack_sim::{Baseline, Outcome, Workload, oracle, price};
use morphstack_trace::{Format, StatsCollector, TraceWriter, Validator};
use serde::Serialize;

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
    /// Price every baseline and the hindsight oracle on a trace.
    Sim {
        /// Trace file.
        path: PathBuf,
        /// Cost profile (TOML). Defaults to the built-in uncalibrated profile.
        #[arg(long)]
        profile: Option<PathBuf>,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Generate every workload, simulate it, and write the characterization report.
    Characterize {
        /// Size multiplier.
        #[arg(long, default_value_t = 1.0)]
        scale: f64,
        /// Seed.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Cost profile (TOML).
        #[arg(long)]
        profile: Option<PathBuf>,
        /// Write `report.md` and `report.json` here.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Sweep memory price and migration cost; report where switching pays (RQ3).
    Sweep {
        /// Size multiplier.
        #[arg(long, default_value_t = 0.5)]
        scale: f64,
        /// Seed.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Write `sweep.md` and `sweep.json` here.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Measure cost-model parameters on this machine and write a profile.
    Calibrate {
        /// Write `<name>.toml` (profile) and `<name>.json` (measurements) here.
        #[arg(long, default_value = "results/profiles")]
        out: PathBuf,
        /// Profile file stem.
        #[arg(long, default_value = "local")]
        name: String,
    },
    /// Print the built-in cost profile as TOML.
    Profile,
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
        Command::Sim {
            path,
            profile,
            json,
        } => sim(&path, profile.as_deref(), json),
        Command::Characterize {
            scale,
            seed,
            profile,
            out,
        } => characterize(scale, seed, profile.as_deref(), out.as_deref()),
        Command::Sweep { scale, seed, out } => sweep(scale, seed, out.as_deref()),
        Command::Calibrate { out, name } => {
            let c = morphstack_bench::calibrate(&CostModel::default())?;
            println!("{}", c.machine);
            for m in &c.measurements {
                println!(
                    "  {:<32}{:>12.4} {:<8} [{:.4} .. {:.4}]  {}",
                    m.parameter, m.value, m.unit, m.min, m.max, m.method
                );
            }
            println!("not measured:");
            for (p, why) in &c.not_measured {
                println!("  {p:<64} {why}");
            }
            std::fs::create_dir_all(&out)?;
            let toml_path = out.join(format!("{name}.toml"));
            std::fs::write(&toml_path, c.profile.to_toml())?;
            std::fs::write(
                out.join(format!("{name}.json")),
                serde_json::to_string_pretty(&c)?,
            )?;
            println!("wrote {}", toml_path.display());
            Ok(ExitCode::SUCCESS)
        }
        Command::Profile => {
            print!("{}", CostModel::default().to_toml());
            Ok(ExitCode::SUCCESS)
        }
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

fn load_profile(path: Option<&Path>) -> Result<CostModel> {
    match path {
        None => Ok(CostModel::default()),
        Some(p) => {
            let text = std::fs::read_to_string(p)
                .with_context(|| format!("reading profile {}", p.display()))?;
            CostModel::from_toml(&text).with_context(|| format!("parsing profile {}", p.display()))
        }
    }
}

#[derive(Serialize)]
struct PolicyRow {
    policy: String,
    #[serde(flatten)]
    outcome: Outcome,
    total_ns: f64,
    regret_vs_oracle: f64,
}

#[derive(Serialize)]
struct SimReport {
    trace: String,
    profile: String,
    contexts: usize,
    steps: usize,
    rows: Vec<PolicyRow>,
    best_fixed: String,
    oracle_gain_over_best_fixed: f64,
    /// Share of the oracle's gain explained by per-context static choice.
    heterogeneity_gain: f64,
    /// Share of the oracle's gain that needs switching over time.
    switching_gain: f64,
    /// Headroom between hindsight-static and the ideal bound (free migration,
    /// any granularity), relative to the best fixed policy.
    ideal_headroom: f64,
    /// Part of the headroom blocked only by migration cost (static → free-migration bound).
    blocked_by_migration_cost: f64,
    /// Part of the headroom only finer-grained mixing could reach (free-migration → ideal bound).
    needs_finer_granularity: f64,
    /// Oracle schedule share of contexts that migrate at least once.
    contexts_switching: f64,
}

fn simulate(name: &str, model: &CostModel, w: &Workload) -> SimReport {
    let oracle_schedule = oracle::solve(model, w);
    let oracle_out = price(model, w, &oracle_schedule);
    let best = oracle_out.cost.total();
    let mut rows: Vec<PolicyRow> = Baseline::ALL
        .iter()
        .map(|b| {
            let outcome = price(model, w, &b.schedule(w));
            let total = outcome.cost.total();
            PolicyRow {
                policy: b.name().to_owned(),
                outcome,
                total_ns: total,
                regret_vs_oracle: (total - best) / best,
            }
        })
        .collect();
    let best_fixed = rows
        .iter()
        .take(3)
        .min_by(|a, b| a.total_ns.total_cmp(&b.total_ns))
        .map(|r| (r.policy.clone(), r.total_ns))
        .unwrap_or_default();
    let stat_out = price(model, w, &morphstack_sim::hindsight_static(model, w));
    let stat = stat_out.cost.total();
    rows.push(PolicyRow {
        policy: "F hindsight-static".to_owned(),
        outcome: stat_out,
        total_ns: stat,
        regret_vs_oracle: (stat - best) / best,
    });
    let switching = oracle_schedule
        .iter()
        .filter(|s| s.windows(2).any(|p| p[0] != p[1]))
        .count();
    rows.push(PolicyRow {
        policy: "E oracle".to_owned(),
        outcome: oracle_out,
        total_ns: best,
        regret_vs_oracle: 0.0,
    });
    #[allow(clippy::cast_precision_loss)]
    let contexts_switching = switching as f64 / w.contexts.len().max(1) as f64;
    SimReport {
        trace: name.to_owned(),
        profile: model.name.clone(),
        contexts: w.contexts.len(),
        steps: w.steps(),
        best_fixed: best_fixed.0,
        oracle_gain_over_best_fixed: (best_fixed.1 - best) / best_fixed.1,
        heterogeneity_gain: (best_fixed.1 - stat) / best_fixed.1,
        ideal_headroom: (stat - morphstack_sim::ideal_bound(model, w).total()) / best_fixed.1,
        blocked_by_migration_cost: (stat - morphstack_sim::free_migration_bound(model, w))
            / best_fixed.1,
        needs_finer_granularity: (morphstack_sim::free_migration_bound(model, w)
            - morphstack_sim::ideal_bound(model, w).total())
            / best_fixed.1,
        switching_gain: (stat - best) / best_fixed.1,
        contexts_switching,
        rows,
    }
}

fn print_report(r: &SimReport) {
    println!(
        "{}: {} contexts, {} steps, profile {}",
        r.trace, r.contexts, r.steps, r.profile
    );
    println!(
        "  {:<22}{:>12}{:>11}{:>11}{:>11}{:>11}{:>10}{:>10}{:>9}  share C/S/H",
        "policy", "total ms", "exec", "growth", "memory", "migrate", "migr #", "peak MiB", "regret"
    );
    for row in &r.rows {
        let c = &row.outcome.cost;
        let ms = |v: f64| v / 1e6;
        #[allow(clippy::cast_precision_loss)]
        let peak = row.outcome.peak_bytes as f64 / f64::from(1u32 << 20);
        let sh = row.outcome.time_share;
        println!(
            "  {:<22}{:>12.3}{:>11.3}{:>11.3}{:>11.3}{:>11.3}{:>10}{:>10.1}{:>8.1}%  {:.0}/{:.0}/{:.0}",
            row.policy,
            ms(row.total_ns),
            ms(c.exec),
            ms(c.growth),
            ms(c.memory),
            ms(c.migration),
            row.outcome.migrations,
            peak,
            row.regret_vs_oracle * 100.0,
            sh[0] * 100.0,
            sh[1] * 100.0,
            sh[2] * 100.0,
        );
    }
    println!(
        "  best fixed: {}; oracle gain over it: {:.2}% (heterogeneity {:.2}% + switching {:.2}%); contexts the oracle migrates: {:.1}%",
        r.best_fixed,
        r.oracle_gain_over_best_fixed * 100.0,
        r.heterogeneity_gain * 100.0,
        r.switching_gain * 100.0,
        r.contexts_switching * 100.0
    );
}

fn sim(path: &Path, profile: Option<&Path>, json: bool) -> Result<ExitCode> {
    let model = load_profile(profile)?;
    let reader = open(path)?;
    let header = reader.header().clone();
    let w = Workload::from_events(&header, reader)
        .with_context(|| format!("decoding {}", path.display()))?;
    let r = simulate(&path.display().to_string(), &model, &w);
    if json {
        println!("{}", serde_json::to_string_pretty(&r)?);
    } else {
        print_report(&r);
    }
    Ok(ExitCode::SUCCESS)
}

fn characterize(
    scale: f64,
    seed: u64,
    profile: Option<&Path>,
    out: Option<&Path>,
) -> Result<ExitCode> {
    use morphstack_workloads::{Spec, Workload as Gen};
    use std::fmt::Write as _;
    let model = load_profile(profile)?;
    let mut reports = Vec::new();
    for g in Gen::ALL {
        let spec = Spec {
            workload: g,
            seed,
            scale,
        };
        let events = morphstack_workloads::generate_vec(&spec);
        let w = Workload::from_events::<std::convert::Infallible>(
            &morphstack_workloads::header(&spec),
            events.into_iter().map(Ok),
        )?;
        let r = simulate(g.name(), &model, &w);
        print_report(&r);
        println!();
        reports.push(r);
    }
    let mut md = String::new();
    writeln!(
        md,
        "# Stage 2 workload characterization\n\nGenerated by `morphstack characterize --scale {scale} --seed {seed}` with profile `{}`. Costs in ms of CPU-equivalent time.\n",
        model.name
    )?;
    writeln!(
        md,
        "| Workload | A contig | B seg | C heap | D static | F hindsight-static | Oracle | Best fixed | Oracle gain | from heterogeneity | from switching | Ideal headroom | blocked by migration cost | needs finer granularity | Ctx switching |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
    )?;
    for r in &reports {
        let t: Vec<String> = r
            .rows
            .iter()
            .map(|x| format!("{:.2}", x.total_ns / 1e6))
            .collect();
        writeln!(
            md,
            "| {} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% |",
            r.trace,
            t.join(" | ") + " | " + r.best_fixed.split_whitespace().nth(1).unwrap_or(""),
            r.oracle_gain_over_best_fixed * 100.0,
            r.heterogeneity_gain * 100.0,
            r.switching_gain * 100.0,
            r.ideal_headroom * 100.0,
            r.blocked_by_migration_cost * 100.0,
            r.needs_finer_granularity * 100.0,
            r.contexts_switching * 100.0
        )?;
    }
    let winners: std::collections::BTreeSet<&str> =
        reports.iter().map(|r| r.best_fixed.as_str()).collect();
    writeln!(
        md,
        "\n**Go/no-go criterion 1** (no fixed representation wins everywhere): {}.  \n**Distinct best-fixed policies:** {}.",
        if winners.len() > 1 { "met" } else { "NOT met" },
        winners.into_iter().collect::<Vec<_>>().join(", ")
    )?;
    let _ = Rep::ALL;
    if let Some(dir) = out {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("report.md"), &md)?;
        std::fs::write(
            dir.join("report.json"),
            serde_json::to_string_pretty(&reports)?,
        )?;
        println!("wrote {}", dir.display());
    } else {
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Serialize)]
struct SweepPoint {
    workload: String,
    memory_multiplier: f64,
    migration_multiplier: f64,
    best_fixed: String,
    oracle_gain: f64,
    heterogeneity_gain: f64,
    switching_gain: f64,
    oracle_migrations: u64,
}

fn sweep(scale: f64, seed: u64, out: Option<&Path>) -> Result<ExitCode> {
    use morphstack_workloads::{Spec, Workload as Gen};
    use std::fmt::Write as _;
    const MEM: [f64; 5] = [0.01, 0.1, 1.0, 10.0, 100.0];
    const MIG: [f64; 5] = [0.01, 0.1, 1.0, 10.0, 100.0];
    let base = CostModel::default();
    let mut points = Vec::new();
    let mut md = format!(
        "# Sensitivity sweep\n\nGenerated by `morphstack sweep --scale {scale} --seed {seed}` from profile `{}`. \\\nEach cell shows **switching gain / total oracle gain** over the best fixed policy, in percent. \\\nRows multiply the memory price ({} ns/byte·s), and columns multiply the migration mechanism cost (copy and fix-up; allocation and release are not scaled).\n",
        base.name, base.memory.ns_per_byte_second
    );
    for g in Gen::ALL {
        let spec = Spec {
            workload: g,
            seed,
            scale,
        };
        let events = morphstack_workloads::generate_vec(&spec);
        let w = Workload::from_events::<std::convert::Infallible>(
            &morphstack_workloads::header(&spec),
            events.into_iter().map(Ok),
        )?;
        writeln!(md, "\n## {}\n", g.name())?;
        writeln!(
            md,
            "| memory × \\ migration × | {} |\n|---|{}",
            MIG.map(|m| m.to_string()).join(" | "),
            "---|".repeat(MIG.len())
        )?;
        for mm in MEM {
            let mut cells = Vec::new();
            for gm in MIG {
                let mut model = base.clone();
                model.memory.ns_per_byte_second *= mm;
                model.migration_scale = gm;
                let fixed: Vec<(Rep, f64)> = Rep::ALL
                    .iter()
                    .map(|&r| {
                        (
                            r,
                            price(&model, &w, &Baseline::Fixed(r).schedule(&w))
                                .cost
                                .total(),
                        )
                    })
                    .collect();
                let (best_rep, best_fixed) = fixed
                    .iter()
                    .copied()
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .unwrap_or((Rep::Contiguous, 0.0));
                let stat = price(&model, &w, &morphstack_sim::hindsight_static(&model, &w))
                    .cost
                    .total();
                let o = price(&model, &w, &oracle::solve(&model, &w));
                let best = o.cost.total();
                let p = SweepPoint {
                    workload: g.name().to_owned(),
                    memory_multiplier: mm,
                    migration_multiplier: gm,
                    best_fixed: best_rep.name().to_owned(),
                    oracle_gain: (best_fixed - best) / best_fixed,
                    heterogeneity_gain: (best_fixed - stat) / best_fixed,
                    switching_gain: (stat - best) / best_fixed,
                    oracle_migrations: o.migrations,
                };
                cells.push(format!(
                    "{:.1} / {:.1}",
                    p.switching_gain * 100.0,
                    p.oracle_gain * 100.0
                ));
                points.push(p);
            }
            writeln!(md, "| **{mm}** | {} |", cells.join(" | "))?;
        }
        eprintln!("swept {}", g.name());
    }
    if let Some(dir) = out {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("sweep.md"), &md)?;
        std::fs::write(
            dir.join("sweep.json"),
            serde_json::to_string_pretty(&points)?,
        )?;
        println!("wrote {}", dir.display());
    } else {
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}
