//! On-demand allocation index: one replay pass that derives allocation
//! lifetimes and epochs from an rr recording.
//!
//! Nothing about allocations is captured at record time. This pass replays
//! the recording with breakpoints on the allocator entry points (planted at
//! the program entry, once every shared library is mapped) and emits `alloc`,
//! `free`, `mark`, `signal`, and `exit` events, each carrying the rr event
//! number at which it happened. The logic mirrors the fixture recorder in
//! `statesolver-record`, including glibc `free` re-entry deduplication.

use std::collections::HashMap;
use std::path::Path;

use statesolver_record::symbols;
use statesolver_trace::{
    AllocId, AllocKind, Epoch, Event, EventId, EventKind, Header, Program, Reg,
};

use crate::session::{Session, Stop};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Malloc,
    Calloc,
    Realloc,
    Free,
    Aligned,
    PosixMemalign,
    Mark,
    Return,
    Entry,
    /// `__asan_report_{load,store}<size>`: the bad address is in `rdi`
    /// (`_n` variants pass the size in `rsi`; size 0 here means "read rsi").
    AsanReport {
        store: bool,
        size: u64,
    },
    /// `__asan::ReportGenericError(pc, bp, sp, addr, is_write, size, exp, fatal)`.
    AsanGeneric,
    /// `__asan::ReportDoubleFree(addr, stack)`.
    AsanDoubleFree,
    /// `__asan::ReportFreeNotMalloced(addr, stack)`.
    AsanBadFree,
}

/// ASan report hooks, by symbol name.
fn asan_hooks() -> Vec<(String, Kind)> {
    let mut v = Vec::new();
    for (access, store) in [("load", false), ("store", true)] {
        for (suffix, size) in [
            ("1", 1),
            ("2", 2),
            ("4", 4),
            ("8", 8),
            ("16", 16),
            ("_n", 0),
        ] {
            for prefix in ["__asan_report_", "__asan_report_exp_"] {
                v.push((
                    format!("{prefix}{access}{suffix}"),
                    Kind::AsanReport { store, size },
                ));
            }
        }
    }
    v.push((
        "_ZN6__asan18ReportGenericErrorEmmmmbmjb".into(),
        Kind::AsanGeneric,
    ));
    v.push((
        "_ZN6__asan16ReportDoubleFreeEmPN11__sanitizer18BufferedStackTraceE".into(),
        Kind::AsanDoubleFree,
    ));
    v.push((
        "_ZN6__asan21ReportFreeNotMallocedEmPN11__sanitizer18BufferedStackTraceE".into(),
        Kind::AsanBadFree,
    ));
    v
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    kind: Kind,
    size: u64,
    ptr_in: u64,
    memptr: u64,
    caller: u64,
    return_rsp: u64,
}

const ALLOCATOR: [(&str, Kind); 7] = [
    ("malloc", Kind::Malloc),
    ("calloc", Kind::Calloc),
    ("realloc", Kind::Realloc),
    ("free", Kind::Free),
    ("aligned_alloc", Kind::Aligned),
    ("memalign", Kind::Aligned),
    ("posix_memalign", Kind::PosixMemalign),
];

/// Index summary.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Summary {
    /// Allocations found.
    pub allocations: u64,
    /// Frees found (including invalid ones).
    pub frees: u64,
    /// Markers found.
    pub marks: u64,
    /// Breakpoint stops taken.
    pub stops: u64,
    /// How the recorded process ended.
    pub outcome: String,
}

struct Indexer<'a> {
    s: &'a mut Session,
    events: Vec<Event>,
    bps: HashMap<u64, Kind>,
    pending: Vec<Pending>,
    next_alloc: u64,
    live: HashMap<u64, (AllocId, Epoch)>,
    last_at: HashMap<u64, (AllocId, Epoch)>,
    epochs: HashMap<u64, u32>,
    summary: Summary,
    sanitizer_seen: bool,
}

impl Indexer<'_> {
    fn emit(&mut self, kind: EventKind) -> Result<()> {
        let at = self.s.position()?;
        let id = EventId(self.events.len() as u64);
        self.events.push(Event {
            id,
            rr: Some(at),
            kind,
        });
        Ok(())
    }

    fn bp(&mut self, addr: u64, kind: Kind) -> Result<()> {
        if let std::collections::hash_map::Entry::Vacant(e) = self.bps.entry(addr) {
            e.insert(kind);
            self.s.set_breakpoint(addr)?;
        }
        Ok(())
    }

    fn alloc(
        &mut self,
        addr: u64,
        size: u64,
        kind: AllocKind,
        realloc_of: Option<AllocId>,
        caller: u64,
    ) -> Result<()> {
        let id = AllocId(self.next_alloc);
        self.next_alloc += 1;
        let e = self.epochs.entry(addr).or_insert(0);
        *e += 1;
        let epoch = Epoch(*e);
        self.live.insert(addr, (id, epoch));
        self.last_at.insert(addr, (id, epoch));
        self.summary.allocations += 1;
        self.emit(EventKind::Alloc {
            alloc: id,
            addr,
            size,
            epoch,
            kind,
            realloc_of,
            caller,
        })
    }

    fn free(&mut self, ptr: u64, caller: u64) -> Result<Option<AllocId>> {
        if ptr == 0 {
            return Ok(None);
        }
        let Some((id, epoch)) = self
            .live
            .remove(&ptr)
            .or_else(|| self.last_at.get(&ptr).copied())
        else {
            return Ok(None); // allocated before the entry point
        };
        self.summary.frees += 1;
        self.emit(EventKind::Free {
            alloc: id,
            addr: ptr,
            epoch,
            caller,
        })?;
        Ok(Some(id))
    }

    fn at_entry(&mut self, exe: &str) -> Result<()> {
        let maps = symbols::parse_maps(&self.s.maps()?);
        let mut found: HashMap<String, (u8, u64)> = HashMap::new();
        let hooks = asan_hooks();
        let names: Vec<&str> = ALLOCATOR
            .iter()
            .map(|(n, _)| *n)
            .chain(["ss_mark"])
            .chain(hooks.iter().map(|(n, _)| n.as_str()))
            .collect();
        for image in symbols::images(&maps) {
            if image.path.starts_with("/usr/lib/rr/") {
                continue;
            }
            if let Ok(syms) = symbols::resolve(&image, &names) {
                // The executable's own allocator wins (ASan and custom
                // allocators interpose malloc there), then libc, then others.
                let rank = if image.path == exe {
                    0
                } else if image.path.contains("libc.so") {
                    1
                } else {
                    2
                };
                for (name, addr) in syms {
                    match found.get(&name) {
                        Some(&(r, _)) if r <= rank => {}
                        _ => {
                            found.insert(name, (rank, addr));
                        }
                    }
                }
            }
        }
        for (name, kind) in ALLOCATOR {
            if let Some(&(_, a)) = found.get(name) {
                self.bp(a, kind)?;
            }
        }
        if let Some(&(_, a)) = found.get("ss_mark") {
            self.bp(a, Kind::Mark)?;
        }
        for (name, kind) in hooks {
            if let Some(&(_, a)) = found.get(&name) {
                self.bp(a, kind)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn on_hit(&mut self, addr: u64, kind: Kind, exe: &str) -> Result<()> {
        self.summary.stops += 1;
        let rsp = self.s.reg(Reg::Rsp)?;
        match kind {
            Kind::AsanReport { .. }
            | Kind::AsanGeneric
            | Kind::AsanDoubleFree
            | Kind::AsanBadFree => {
                // Only the first report: ASan's own report path calls further hooks.
                if !self.sanitizer_seen {
                    self.sanitizer_seen = true;
                    let caller = self.s.read_u64(rsp)?;
                    let (rdi, rsi) = (self.s.reg(Reg::Rdi)?, self.s.reg(Reg::Rsi)?);
                    let (kind, addr, size, pc) = match kind {
                        Kind::AsanReport { store, size } => {
                            let size = if size == 0 { rsi } else { size };
                            (if store { "store" } else { "load" }, rdi, size, caller)
                        }
                        Kind::AsanGeneric => {
                            let addr = self.s.reg(Reg::Rcx)?;
                            let is_write = self.s.reg(Reg::R8)? & 1 == 1;
                            let size = self.s.reg(Reg::R9)?;
                            (if is_write { "store" } else { "load" }, addr, size, rdi)
                        }
                        Kind::AsanDoubleFree => ("double-free", rdi, 0, caller),
                        _ => ("bad-free", rdi, 0, caller),
                    };
                    self.emit(EventKind::Sanitizer {
                        kind: kind.into(),
                        addr,
                        size,
                        pc,
                    })?;
                }
            }
            Kind::Entry => self.at_entry(exe)?,
            Kind::Mark => {
                let ptr = self.s.reg(Reg::Rdi)?;
                let raw = self.s.mem(ptr, 128).unwrap_or_default();
                let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
                let label = String::from_utf8_lossy(&raw[..end]).into_owned();
                let caller = self.s.read_u64(rsp)?;
                self.summary.marks += 1;
                self.emit(EventKind::Mark { label, pc: caller })?;
            }
            Kind::Return => {
                if let Some(pos) = self
                    .pending
                    .iter()
                    .rposition(|p| p.return_rsp == rsp && p.caller == addr)
                {
                    let p = self.pending.remove(pos);
                    let rax = self.s.reg(Reg::Rax)?;
                    self.finish(p, rax)?;
                }
            }
            Kind::Free => {
                let caller = self.s.read_u64(rsp)?;
                let reentry = self
                    .pending
                    .iter()
                    .any(|p| p.kind == Kind::Free && p.return_rsp == rsp + 8 && p.caller == caller);
                if !reentry && self.pending.is_empty() {
                    let ptr = self.s.reg(Reg::Rdi)?;
                    self.free(ptr, caller)?;
                    self.pending.push(Pending {
                        kind,
                        size: 0,
                        ptr_in: ptr,
                        memptr: 0,
                        caller,
                        return_rsp: rsp + 8,
                    });
                    self.bp(caller, Kind::Return)?;
                }
            }
            _ if !self.pending.is_empty() => {}
            _ => {
                let caller = self.s.read_u64(rsp)?;
                let (rdi, rsi, rdx) = (
                    self.s.reg(Reg::Rdi)?,
                    self.s.reg(Reg::Rsi)?,
                    self.s.reg(Reg::Rdx)?,
                );
                let (size, ptr_in, memptr) = match kind {
                    Kind::Malloc => (rdi, 0, 0),
                    Kind::Calloc => (rdi.saturating_mul(rsi), 0, 0),
                    Kind::Realloc => (rsi, rdi, 0),
                    Kind::Aligned => (rsi, 0, 0),
                    Kind::PosixMemalign => (rdx, 0, rdi),
                    _ => (0, 0, 0),
                };
                self.pending.push(Pending {
                    kind,
                    size,
                    ptr_in,
                    memptr,
                    caller,
                    return_rsp: rsp + 8,
                });
                self.bp(caller, Kind::Return)?;
            }
        }
        Ok(())
    }

    fn finish(&mut self, p: Pending, rax: u64) -> Result<()> {
        match p.kind {
            Kind::Malloc if rax != 0 => {
                self.alloc(rax, p.size, AllocKind::Malloc, None, p.caller)?;
            }
            Kind::Calloc if rax != 0 => {
                self.alloc(rax, p.size, AllocKind::Calloc, None, p.caller)?;
            }
            Kind::Aligned if rax != 0 => {
                self.alloc(rax, p.size, AllocKind::Aligned, None, p.caller)?;
            }
            Kind::PosixMemalign if rax == 0 => {
                let ptr = self.s.read_u64(p.memptr)?;
                self.alloc(ptr, p.size, AllocKind::Aligned, None, p.caller)?;
            }
            Kind::Realloc => {
                if p.ptr_in == 0 {
                    if rax != 0 {
                        self.alloc(rax, p.size, AllocKind::Malloc, None, p.caller)?;
                    }
                } else if rax != 0 {
                    let old = self.free(p.ptr_in, p.caller)?;
                    self.alloc(rax, p.size, AllocKind::Realloc, old, p.caller)?;
                } else if p.size == 0 {
                    self.free(p.ptr_in, p.caller)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn exec_file(s: &mut Session) -> Result<String> {
    // rr knows the recorded executable. In a packed trace (recorded elsewhere,
    // for example in a container) the replay maps rr's copy of it,
    // `mmap_pack_<n>_<name>`, so match by file name among the mappings.
    let recorded = s.exec_file()?;
    let name = recorded.rsplit('/').next().unwrap_or(&recorded).to_string();
    let maps = symbols::parse_maps(&s.maps()?);
    let paths: Vec<String> = maps.iter().filter_map(|m| m.path.clone()).collect();
    paths
        .iter()
        .find(|p| **p == recorded)
        .or_else(|| {
            paths.iter().find(|p| {
                let base = p.rsplit('/').next().unwrap_or(p);
                base.starts_with("mmap_") && base.ends_with(&format!("_{name}"))
            })
        })
        .filter(|p| Path::new(p).exists())
        .cloned()
        .ok_or_else(|| {
            Error::Protocol(format!(
                "cannot find a mapping of the executable {recorded}"
            ))
        })
}

/// Replay the whole recording once and derive its allocation index.
///
/// # Errors
/// Fails on replay or symbol errors.
#[allow(clippy::too_many_lines)]
pub fn build(s: &mut Session) -> Result<(Header, Vec<Event>, Summary)> {
    // Run to the program's entry point so its image is mapped.
    let exe = exec_file(s)?;
    let data = std::fs::read(&exe)
        .map_err(|e| Error::Protocol(format!("reading executable {exe}: {e}")))?;
    let file = object::File::parse(&*data).map_err(|e| Error::Protocol(e.to_string()))?;
    let maps = symbols::parse_maps(&s.maps()?);
    let base = symbols::images(&maps)
        .into_iter()
        .find(|i| i.path == exe)
        .map_or(0, |i| i.base);
    let first = object::Object::segments(&file)
        .map(|seg| object::ObjectSegment::address(&seg))
        .min()
        .unwrap_or(0);
    let entry = object::Object::entry(&file);
    let entry = if object::Object::kind(&file) == object::ObjectKind::Dynamic {
        base + entry - first
    } else {
        entry
    };

    let mut ix = Indexer {
        s,
        events: Vec::new(),
        bps: HashMap::new(),
        pending: Vec::new(),
        next_alloc: 0,
        live: HashMap::new(),
        last_at: HashMap::new(),
        epochs: HashMap::new(),
        summary: Summary::default(),
        sanitizer_seen: false,
    };
    ix.bp(entry, Kind::Entry)?;
    let mut pending_signal: Option<(u8, u64)> = None;
    loop {
        match ix.s.cont()? {
            // rr delivers SIGKILL (9) when it tears the replay down after the
            // real fatal signal; it is not part of the recorded execution.
            Stop::Signal(9) | Stop::Step | Stop::Watchpoint { .. } | Stop::Begin => {}
            Stop::Breakpoint => {
                let rip = ix.s.reg(Reg::Rip)?;
                if let Some(kind) = ix.bps.get(&rip).copied() {
                    ix.on_hit(rip, kind, &exe)?;
                    let still = kind == Kind::Return && ix.pending.iter().any(|p| p.caller == rip);
                    let rearm = !matches!(kind, Kind::Entry | Kind::Return) || still;
                    if !rearm {
                        ix.bps.remove(&rip);
                    }
                    ix.s.step_over_breakpoint(rip, rearm)?;
                }
            }
            Stop::Signal(sig) => {
                if let Some((sg, pc)) = pending_signal.take() {
                    ix.emit(EventKind::Signal {
                        signo: u32::from(sg),
                        code: 0,
                        addr: None,
                        pc,
                        fatal: false,
                    })?;
                }
                pending_signal = Some((sig, ix.s.reg(Reg::Rip)?));
            }
            Stop::Killed(sig) => {
                let pc = pending_signal.map_or(0, |(_, pc)| pc);
                ix.emit(EventKind::Signal {
                    signo: u32::from(sig),
                    code: 0,
                    addr: None,
                    pc,
                    fatal: true,
                })?;
                ix.summary.outcome = format!("killed by signal {sig}");
                break;
            }
            Stop::Exited(code) => {
                // rr ends the replay right after replaying a fatal signal, and
                // reports that as an exit: a pending signal here is the death.
                if let Some((sig, pc)) = pending_signal.take() {
                    ix.emit(EventKind::Signal {
                        signo: u32::from(sig),
                        code: 0,
                        addr: None,
                        pc,
                        fatal: true,
                    })?;
                    ix.summary.outcome = format!("killed by signal {sig}");
                } else {
                    ix.emit(EventKind::Exit {
                        code: i32::from(code),
                    })?;
                    ix.summary.outcome = format!("exited with status {code}");
                }
                break;
            }
        }
    }
    let header = Header {
        description: Some("allocation index derived by replaying an rr recording".into()),
        ..Header::new(
            format!("statesolver-replay {}", env!("CARGO_PKG_VERSION")),
            Program {
                path: exe.clone(),
                argv: vec![exe],
                sha256: None,
                load_base: Some(base),
            },
        )
    };
    let summary = ix.summary.clone();
    Ok((header, ix.events, summary))
}
