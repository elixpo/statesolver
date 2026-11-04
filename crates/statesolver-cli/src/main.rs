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
        /// Output: an rr trace directory (default), or a `.ss` file with `--fixture`.
        #[arg(short, long, default_value = "trace")]
        out: PathBuf,
        /// Use the built-in ptrace recorder (test fixture) instead of rr.
        #[arg(long)]
        fixture: bool,
        /// File to use as the program's stdin.
        #[arg(long)]
        stdin: Option<PathBuf>,
        /// Program and its arguments.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Derive the allocation index of an rr trace by replaying it (cached in the trace directory).
    #[cfg(target_os = "linux")]
    Index {
        /// rr trace directory.
        trace: PathBuf,
        /// Rebuild even if a cached index exists.
        #[arg(long)]
        force: bool,
    },
    /// Where did a value come from? Provenance chain of a register or memory
    /// location at an indexed event, with the allocation it belongs to.
    #[cfg(target_os = "linux")]
    Why {
        /// rr trace directory.
        trace: PathBuf,
        /// StateSolver event id (position to ask at).
        #[arg(long)]
        event: u64,
        /// Register to explain (for example `rax`).
        #[arg(long, conflicts_with = "addr")]
        reg: Option<String>,
        /// Memory address to explain (8 bytes).
        #[arg(long)]
        addr: Option<String>,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Explain the recorded crash: classification plus a cited evidence chain.
    #[cfg(target_os = "linux")]
    ExplainCrash {
        /// rr trace directory.
        trace: PathBuf,
        /// Address-only analysis: no allocation-lifetime reasoning (study arm A5).
        #[arg(long)]
        address_only: bool,
        /// Emit JSON (with full provenance chains).
        #[arg(long)]
        json: bool,
    },
    /// Registers (and optionally memory) at an indexed event of an rr trace.
    #[cfg(target_os = "linux")]
    State {
        /// rr trace directory.
        #[arg(long)]
        trace: PathBuf,
        /// StateSolver event id (from `allocations`, `trace stats`, or the index).
        #[arg(long)]
        event: u64,
        /// Also dump memory: `ADDR:LEN` (hex address, decimal length).
        #[arg(long)]
        mem: Option<String>,
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
            fixture,
            stdin,
            command,
        } => {
            if fixture {
                record(&out, stdin, command)
            } else {
                record_rr(&out, stdin, &command)
            }
        }
        #[cfg(target_os = "linux")]
        Command::Why {
            trace,
            event,
            reg,
            addr,
            json,
        } => why(&trace, event, reg.as_deref(), addr.as_deref(), json),
        #[cfg(target_os = "linux")]
        Command::ExplainCrash {
            trace,
            address_only,
            json,
        } => explain_crash(&trace, address_only, json),
        #[cfg(target_os = "linux")]
        Command::State { trace, event, mem } => state(&trace, event, mem.as_deref()),
        #[cfg(target_os = "linux")]
        Command::Index { trace, force } => {
            index(&trace, force)?;
            Ok(ExitCode::SUCCESS)
        }
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
    let path = resolve_trace(path)?;
    statesolver_trace::open(&path).with_context(|| format!("reading {}", path.display()))
}

/// The cached allocation index inside an rr trace directory.
const INDEX_FILE: &str = "statesolver-index.ss";

/// A `.ss` file, or an rr trace directory (indexed on demand).
fn resolve_trace(path: &Path) -> Result<PathBuf> {
    if path.is_dir() {
        #[cfg(target_os = "linux")]
        return index(path, false);
        #[cfg(not(target_os = "linux"))]
        anyhow::bail!("indexing rr traces requires Linux");
    }
    Ok(path.to_path_buf())
}

fn heap(path: &Path) -> Result<TemporalHeap> {
    TemporalHeap::from_events(open(path)?).with_context(|| format!("decoding {}", path.display()))
}

#[cfg(target_os = "linux")]
fn index(trace: &Path, force: bool) -> Result<PathBuf> {
    let out = trace.join(INDEX_FILE);
    if out.exists() && !force {
        return Ok(out);
    }
    eprintln!("indexing {} (replaying once)...", trace.display());
    let started = std::time::Instant::now();
    let mut session =
        statesolver_replay::Session::start(Some(trace)).context("starting rr replay")?;
    let (header, events, s) =
        statesolver_replay::index::build(&mut session).context("replaying")?;
    drop(session);
    let mut w = statesolver_trace::create(&out, &header)?;
    for e in &events {
        w.write(e)?;
    }
    w.finish()?;
    eprintln!(
        "indexed in {:.2}s: {} allocations, {} frees, {} marks, {} stops; {} -> {}",
        started.elapsed().as_secs_f64(),
        s.allocations,
        s.frees,
        s.marks,
        s.stops,
        s.outcome,
        out.display()
    );
    Ok(out)
}

fn record_rr(out: &Path, stdin: Option<PathBuf>, command: &[String]) -> Result<ExitCode> {
    anyhow::ensure!(!out.exists(), "{} already exists", out.display());
    let paranoid =
        std::fs::read_to_string("/proc/sys/kernel/perf_event_paranoid").unwrap_or_default();
    if paranoid.trim().parse::<i32>().is_ok_and(|p| p > 1) {
        eprintln!(
            "warning: kernel.perf_event_paranoid = {}; rr needs <= 1 (sudo sysctl kernel.perf_event_paranoid=1)",
            paranoid.trim()
        );
    }
    let mut cmd = std::process::Command::new("rr");
    cmd.arg("record")
        .args(rr_bind_args())
        .arg("-o")
        .arg(out)
        .args(command);
    if let Some(path) = stdin {
        cmd.stdin(
            std::fs::File::open(&path).with_context(|| format!("opening {}", path.display()))?,
        );
    }
    let status = cmd.status().context("running rr (is it installed?)")?;
    anyhow::ensure!(out.is_dir(), "rr did not produce a trace (status {status})");
    anyhow::ensure!(
        !out.join("incomplete").exists(),
        "rr terminated abnormally and {} is incomplete; re-record it",
        out.display()
    );
    eprintln!("recorded {} (program status {status})", out.display());
    Ok(ExitCode::SUCCESS)
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

#[cfg(target_os = "linux")]
fn state(trace: &Path, event: u64, mem: Option<&str>) -> Result<ExitCode> {
    use statesolver_trace::Reg;
    let index = index(trace, false)?;
    let e = statesolver_trace::open(&index)?
        .filter_map(Result::ok)
        .find(|e| e.id.0 == event)
        .with_context(|| format!("no event E{event} in the index"))?;
    let pos = e.rr.context("event has no rr position")?;
    let mut s = statesolver_replay::Session::start(Some(trace)).context("starting rr replay")?;
    s.goto(pos).context("replaying to the event")?;
    println!(
        "E{event} ({}) at rr event {}, tick {}, pc {:#x}",
        e.kind.name(),
        pos.event,
        pos.ticks,
        pos.pc
    );
    let regs = [
        Reg::Rax,
        Reg::Rbx,
        Reg::Rcx,
        Reg::Rdx,
        Reg::Rsi,
        Reg::Rdi,
        Reg::Rbp,
        Reg::Rsp,
        Reg::R8,
        Reg::R9,
        Reg::R10,
        Reg::R11,
        Reg::R12,
        Reg::R13,
        Reg::R14,
        Reg::R15,
        Reg::Rip,
        Reg::Rflags,
    ];
    for chunk in regs.chunks(4) {
        let line: Vec<String> = chunk
            .iter()
            .map(|r| format!("{:>6} {:#018x}", r.name(), s.reg(*r).unwrap_or(0)))
            .collect();
        println!("  {}", line.join("  "));
    }
    if let Some(spec) = mem {
        let (a, l) = spec.split_once(':').context("--mem expects ADDR:LEN")?;
        let addr = parse_addr(a)?;
        let len: usize = l.parse().context("bad length")?;
        let bytes = s.mem(addr, len)?;
        for (i, row) in bytes.chunks(16).enumerate() {
            let hex: Vec<String> = row.iter().map(|b| format!("{b:02x}")).collect();
            let ascii: String = row
                .iter()
                .map(|&b| {
                    if b.is_ascii_graphic() {
                        char::from(b)
                    } else {
                        '.'
                    }
                })
                .collect();
            println!(
                "  {:#014x}  {:<48} {ascii}",
                addr + 16 * i as u64,
                hex.join(" ")
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(target_os = "linux")]
fn rr_bind_args() -> Vec<String> {
    statesolver_replay::rr_record_args()
}

#[cfg(not(target_os = "linux"))]
fn rr_bind_args() -> Vec<String> {
    Vec::new()
}

#[cfg(target_os = "linux")]
fn explain_crash(trace: &Path, address_only: bool, json: bool) -> Result<ExitCode> {
    let index_path = index(trace, false)?;
    let events: Vec<statesolver_trace::Event> =
        statesolver_trace::open(&index_path)?.collect::<std::result::Result<_, _>>()?;
    let mut s = statesolver_replay::Session::start(Some(trace)).context("starting rr replay")?;
    let x = statesolver_replay::explain::explain_crash(&mut s, &events, address_only)
        .context("explaining the crash")?;
    if json {
        println!("{}", serde_json::to_string_pretty(&x)?);
        return Ok(ExitCode::SUCCESS);
    }
    println!("Classification: {}", x.classification);
    println!();
    println!("{}", x.summary);
    println!();
    println!("Evidence:");
    for e in &x.evidence {
        let because = if e.because.is_empty() {
            String::new()
        } else {
            format!("  [from {}]", e.because.join(", "))
        };
        println!(
            "  {:<4} {:<9} {}{because}",
            e.id,
            format!("{:?}", e.tier).to_uppercase(),
            e.claim
        );
    }
    for (title, chain) in [
        ("Pointer provenance", &x.pointer_chain),
        ("Value provenance", &x.value_chain),
    ] {
        if let Some(c) = chain {
            println!();
            println!("{title}:");
            for h in &c.hops {
                println!("  {:#014x}  {}", h.pos.pc, h.insn.text);
            }
            println!("  origin: {}", serde_json::to_string(&c.origin)?);
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(target_os = "linux")]
fn why(
    trace: &Path,
    event: u64,
    reg: Option<&str>,
    addr: Option<&str>,
    json: bool,
) -> Result<ExitCode> {
    use statesolver_replay::provenance::{Carrier, Origin};
    let index_path = index(trace, false)?;
    let events: Vec<statesolver_trace::Event> =
        statesolver_trace::open(&index_path)?.collect::<std::result::Result<_, _>>()?;
    let at = events
        .iter()
        .find(|e| e.id.0 == event)
        .and_then(|e| e.rr)
        .with_context(|| format!("no event E{event} with an rr position"))?;
    let start = match (reg, addr) {
        (Some(r), _) => Carrier::Reg {
            reg: statesolver_trace::Reg::from_name(r)
                .with_context(|| format!("unknown register {r}"))?,
        },
        (None, Some(a)) => Carrier::Mem {
            addr: parse_addr(a)? & !7,
            len: 8,
        },
        (None, None) => anyhow::bail!("give --reg or --addr"),
    };
    let mut s = statesolver_replay::Session::start(Some(trace)).context("starting rr replay")?;
    let chain = statesolver_replay::provenance::trace(&mut s, &events, start, at, 16, 4096)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&chain)?);
        return Ok(ExitCode::SUCCESS);
    }
    println!("{start:?} at E{event}:");
    for h in &chain.hops {
        println!(
            "  {:#014x}  {:<36} rr {}:{}",
            h.pos.pc, h.insn.text, h.pos.event, h.pos.ticks
        );
    }
    match &chain.origin {
        Origin::Allocation {
            alloc, epoch, addr, ..
        } => {
            println!(
                "  origin: returned by Allocation #{} (epoch {}) at {addr:#x}",
                alloc.0, epoch.0
            );
            let freed = statesolver_replay::explain::freed_before(&events, *alloc, at);
            if let Some(f) = freed {
                println!(
                    "  note:   Allocation #{} was already freed at E{} - this value is a dangling pointer at E{event}",
                    alloc.0, f.id.0
                );
            }
        }
        other => println!("  origin: {}", serde_json::to_string(other)?),
    }
    Ok(ExitCode::SUCCESS)
}
