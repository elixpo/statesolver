//! `statesolver` command-line interface.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use statesolver_analysis::{Resolution, TemporalHeap};
use statesolver_trace::{EventId, EventKind, Format, TraceWriter, Validator};

/// Reversible Linux debugger with allocation-epoch-aware causal reconstruction.
#[derive(Parser)]
#[command(name = "statesolver", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Record a program into a `.ss` trace (Linux x86-64).
    #[cfg(target_os = "linux")]
    Record {
        /// Output trace.
        #[arg(short, long, default_value = "trace.ss")]
        out: PathBuf,
        /// File to use as the program's stdin.
        #[arg(long)]
        stdin: Option<PathBuf>,
        /// Program and its arguments.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Inspect, check, and convert `.ss` traces.
    #[command(subcommand)]
    Trace(TraceCmd),
    /// List every allocation lifetime in a trace.
    Allocations {
        /// Trace file.
        trace: PathBuf,
        /// Only allocations at addresses that were reused.
        #[arg(long)]
        reused: bool,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show every allocation that covered an address, and what it referred to at an event.
    Lifetime {
        /// Address (hex `0x…` or decimal).
        addr: String,
        /// Trace file.
        #[arg(long)]
        trace: PathBuf,
        /// Resolve the address at this event.
        #[arg(long)]
        event: Option<u64>,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum TraceCmd {
    /// Check a trace against the format rules. Exits non-zero on violations.
    Validate {
        /// Trace file (`.ss` or `.jsonl`; detected from content).
        path: PathBuf,
        /// Maximum violations to print.
        #[arg(long, default_value_t = 20)]
        max_errors: usize,
    },
    /// Summary statistics.
    Stats {
        /// Trace file.
        path: PathBuf,
    },
    /// Convert between `.jsonl` and `.ss`.
    Convert {
        /// Input.
        input: PathBuf,
        /// Output.
        output: PathBuf,
        /// Output container; defaults to `.jsonl` → JSONL, otherwise `.ss`.
        #[arg(long, value_enum)]
        to: Option<OutFormat>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum OutFormat {
    Jsonl,
    Ss,
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
        #[cfg(target_os = "linux")]
        Command::Record {
            out,
            stdin,
            command,
        } => record(&out, stdin, command),
        Command::Trace(TraceCmd::Validate { path, max_errors }) => validate(&path, max_errors),
        Command::Trace(TraceCmd::Stats { path }) => stats(&path),
        Command::Trace(TraceCmd::Convert { input, output, to }) => {
            let format = match to {
                Some(OutFormat::Jsonl) => Format::Jsonl,
                Some(OutFormat::Ss) => Format::Ss,
                None => Format::from_path(&output),
            };
            convert(&input, &output, format)
        }
        Command::Allocations {
            trace,
            reused,
            json,
        } => allocations(&trace, reused, json),
        Command::Lifetime {
            addr,
            trace,
            event,
            json,
        } => lifetime(&addr, &trace, event, json),
    }
}

fn open(path: &Path) -> Result<statesolver_trace::TraceReader> {
    statesolver_trace::open(path).with_context(|| format!("reading {}", path.display()))
}

fn heap(path: &Path) -> Result<TemporalHeap> {
    TemporalHeap::from_events(open(path)?).with_context(|| format!("decoding {}", path.display()))
}

fn parse_addr(s: &str) -> Result<u64> {
    let s = s.trim();
    let v = if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(h, 16)
    } else {
        s.parse()
    };
    v.with_context(|| format!("invalid address {s:?}"))
}

fn validate(path: &Path, max_errors: usize) -> Result<ExitCode> {
    let reader = open(path)?;
    let mut v = Validator::new();
    let (mut n, mut bad) = (0u64, 0usize);
    for e in reader {
        let e = e.with_context(|| format!("decoding {}", path.display()))?;
        n += 1;
        for x in v.check(&e) {
            bad += 1;
            if bad <= max_errors {
                eprintln!("{x}");
            }
        }
    }
    if bad == 0 {
        println!("ok: {n} events");
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("{bad} violation(s) in {n} events");
        Ok(ExitCode::FAILURE)
    }
}

fn stats(path: &Path) -> Result<ExitCode> {
    let reader = open(path)?;
    let h = reader.header().clone();
    let format = reader.format();
    let mut kinds: BTreeMap<&'static str, u64> = BTreeMap::new();
    let (mut n, mut input_bytes, mut syscalls_with_input) = (0u64, 0u64, 0u64);
    let mut terminal = None;
    let mut events = Vec::new();
    for e in reader {
        let e = e?;
        n += 1;
        *kinds.entry(e.kind.name()).or_default() += 1;
        if let EventKind::Syscall { writes, .. } = &e.kind
            && !writes.is_empty()
        {
            syscalls_with_input += 1;
            input_bytes += writes.iter().map(|w| w.data.0.len() as u64).sum::<u64>();
        }
        if e.kind.is_terminal() {
            terminal = Some(format!("E{} {}", e.id.0, e.kind.name()));
        }
        events.push(Ok::<_, std::convert::Infallible>(e));
    }
    let heap = TemporalHeap::from_events(events)?;
    let live_at_end = heap
        .allocations()
        .iter()
        .filter(|a| a.free_event.is_none())
        .count();
    println!(
        "trace        {} ({format:?}, v{})",
        path.display(),
        h.version
    );
    println!("program      {} {:?}", h.program.path, h.program.argv);
    println!("recorder     {}", h.recorder);
    println!("events       {n}");
    for (k, c) in &kinds {
        println!("  {k:<10} {c}");
    }
    println!(
        "allocations  {} ({} live at end), {} reused start addresses",
        heap.allocations().len(),
        live_at_end,
        heap.reused_addresses().len()
    );
    println!("input        {input_bytes} bytes from {syscalls_with_input} syscalls");
    println!(
        "terminated   {}",
        terminal.as_deref().unwrap_or("no (trace window)")
    );
    Ok(ExitCode::SUCCESS)
}

fn convert(input: &Path, output: &Path, format: Format) -> Result<ExitCode> {
    let reader = open(input)?;
    let file =
        std::fs::File::create(output).with_context(|| format!("creating {}", output.display()))?;
    let mut w = TraceWriter::new(std::io::BufWriter::new(file), format, reader.header())?;
    let mut n = 0u64;
    for e in reader {
        w.write(&e?)?;
        n += 1;
    }
    w.finish()?;
    println!("wrote {n} events to {} ({format:?})", output.display());
    Ok(ExitCode::SUCCESS)
}

fn ev(e: Option<EventId>) -> String {
    e.map_or_else(|| "-".to_owned(), |e| format!("E{}", e.0))
}

fn allocations(trace: &Path, reused: bool, json: bool) -> Result<ExitCode> {
    let h = heap(trace)?;
    let reused_set: std::collections::HashSet<u64> = h.reused_addresses().into_iter().collect();
    let list: Vec<_> = h
        .allocations()
        .iter()
        .filter(|a| !reused || reused_set.contains(&a.addr))
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&list)?);
        return Ok(ExitCode::SUCCESS);
    }
    println!(
        "{:>8}  {:>18}  {:>8}  {:>5}  {:>9}  {:>9}  kind",
        "alloc", "address", "size", "epoch", "created", "freed"
    );
    for a in list {
        println!(
            "{:>8}  {:>#18x}  {:>8}  {:>5}  {:>9}  {:>9}  {:?}{}",
            format!("#{}", a.id.0),
            a.addr,
            a.size,
            a.epoch.0,
            ev(Some(a.alloc_event)),
            ev(a.free_event),
            a.kind,
            if reused_set.contains(&a.addr) {
                "  (address reused)"
            } else {
                ""
            }
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn lifetime(addr: &str, trace: &Path, event: Option<u64>, json: bool) -> Result<ExitCode> {
    let addr = parse_addr(addr)?;
    let h = heap(trace)?;
    let history = h.history(addr);
    let resolution = event.map(|e| h.resolve(addr, EventId(e)));
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "address": format!("{addr:#x}"),
                "history": history,
                "at_event": event,
                "resolution": resolution,
            }))?
        );
        return Ok(ExitCode::SUCCESS);
    }
    println!("Address {addr:#x}");
    if history.is_empty() {
        println!("  never inside a recorded heap allocation");
    }
    for a in &history {
        println!();
        println!("  Allocation #{} (epoch {})", a.id.0, a.epoch.0);
        println!(
            "    range    {:#x}..{:#x} ({} bytes, offset {})",
            a.addr,
            a.addr + a.size.max(1),
            a.size,
            addr - a.addr
        );
        println!(
            "    created  {}  ({:?}, caller {:#x})",
            ev(Some(a.alloc_event)),
            a.kind,
            a.alloc_caller
        );
        match (a.free_event, a.free_caller) {
            (Some(f), Some(c)) => println!("    freed    {}  (caller {c:#x})", ev(Some(f))),
            _ => println!("    freed    never (live at end of trace)"),
        }
    }
    if history.len() > 1 {
        println!();
        println!(
            "  Address reused: {} different objects occupied this address over time.",
            history.len()
        );
    }
    if let (Some(e), Some(r)) = (event, resolution) {
        println!();
        match r {
            Resolution::Live { allocation, offset } => println!(
                "  At E{e}: inside live Allocation #{} / epoch {} (+{offset})",
                allocation.id.0, allocation.epoch.0
            ),
            Resolution::Freed { allocation, offset } => println!(
                "  At E{e}: inside FREED Allocation #{} / epoch {} (+{offset}); any access here is use-after-free",
                allocation.id.0, allocation.epoch.0
            ),
            Resolution::Unknown => println!("  At E{e}: not a known heap object"),
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(target_os = "linux")]
fn record(out: &Path, stdin: Option<PathBuf>, command: Vec<String>) -> Result<ExitCode> {
    let mut it = command.into_iter();
    let program = PathBuf::from(it.next().context("missing program")?);
    let opts = statesolver_record::Options {
        program,
        args: it.collect(),
        stdin,
        out: out.to_path_buf(),
    };
    let s = statesolver_record::record(&opts).context("recording")?;
    eprintln!(
        "recorded {} events to {}: {} syscalls, {} allocations, {} frees, {} marks; {}",
        s.events,
        out.display(),
        s.syscalls,
        s.allocations,
        s.frees,
        s.marks,
        s.outcome
    );
    if !s.uncaptured.is_empty() {
        eprintln!(
            "warning: syscalls without an output model (replay cannot inject them): {:?}",
            s.uncaptured
        );
    }
    if s.frees_untracked > 0 {
        eprintln!(
            "note: {} frees of pointers allocated before the entry point",
            s.frees_untracked
        );
    }
    Ok(ExitCode::SUCCESS)
}
