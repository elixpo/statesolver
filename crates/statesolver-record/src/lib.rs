//! StateSolver recorder: runs an unmodified single-threaded x86-64 Linux
//! program under `ptrace` and writes a `.ss` trace.
//!
//! What is captured:
//!
//! - every syscall, with the bytes the kernel wrote into the tracee
//!   ([`syscalls::model`]), so replay can inject results and analysis can
//!   treat them as external input;
//! - address-space changes (`mmap`, `munmap`, `mprotect`, `brk`);
//! - allocation lifetimes from `int3` breakpoints on the allocator entry
//!   points (`malloc`, `calloc`, `realloc`, `free`, `aligned_alloc`,
//!   `memalign`, `posix_memalign`), installed at the program entry point once
//!   every shared library is mapped. Allocations made before the entry point
//!   (loader and library constructors) are not tracked;
//! - `ss_mark("label")` ground-truth markers;
//! - signals, with `fatal` set when the process dies from one;
//! - register snapshots and deltas at syscall boundaries.
//!
//! ASLR is disabled for the tracee, so addresses are stable between runs.
//!
//! Determinism: `rdtsc`/`rdtscp` are made to fault (`PR_SET_TSC`) and are
//! emulated and recorded as `nondet` events, and the vDSO time functions are
//! patched into real `syscall` stubs so their results are recorded.
//!
//! Limits (v0): single-threaded tracees only. `clone`, `fork`, `vfork`,
//! `clone3`, and a second `execve` stop the recording with
//! [`Error::Unsupported`]. `rdrand`/`rdseed` are not trapped yet.

#![cfg(target_os = "linux")]
#![deny(unsafe_code)]

pub mod symbols;
pub mod syscalls;

use std::collections::{BTreeMap, HashMap};
use std::ffi::c_void;
use std::io::IoSliceMut;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use nix::libc;
use nix::sys::ptrace;
use nix::sys::signal::Signal;
use nix::sys::uio::{RemoteIoVec, process_vm_readv};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::Pid;
use serde::Serialize;
use statesolver_trace::{
    AllocId, AllocKind, Bytes, Epoch, Event, EventId, EventKind, Header, MapOp, MemWrite, Program,
    Reg, Region,
};

use syscalls::Out;

/// What to record.
#[derive(Debug, Clone)]
pub struct Options {
    /// Program to run.
    pub program: PathBuf,
    /// Arguments after the program.
    pub args: Vec<String>,
    /// File to connect to the tracee's stdin.
    pub stdin: Option<PathBuf>,
    /// Output trace (`.ss` or `.jsonl`).
    pub out: PathBuf,
}

/// Recording summary.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    /// Events written.
    pub events: u64,
    /// Syscalls recorded.
    pub syscalls: u64,
    /// Syscalls without an output model, by number. Replay cannot inject these.
    pub uncaptured: BTreeMap<u64, u64>,
    /// Allocations recorded.
    pub allocations: u64,
    /// Frees recorded (including invalid ones).
    pub frees: u64,
    /// Frees of pointers never seen allocated (for example, allocated before the entry point).
    pub frees_untracked: u64,
    /// Markers recorded.
    pub marks: u64,
    /// How the process ended.
    pub outcome: String,
}

/// Recording errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// I/O.
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    /// ptrace or wait failure.
    #[error("ptrace: {0}")]
    Sys(#[from] nix::Error),
    /// Trace writing failure.
    #[error("trace: {0}")]
    Trace(#[from] statesolver_trace::Error),
    /// ELF or symbol problem.
    #[error("symbols: {0}")]
    Symbols(String),
    /// Unexpected tracee state.
    #[error("{0}")]
    Protocol(String),
    /// The program does something v0 cannot record faithfully.
    #[error("unsupported in v0: {0}")]
    Unsupported(String),
}

/// Convenience alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BpKind {
    Entry,
    Malloc,
    Calloc,
    Realloc,
    Free,
    AlignedAlloc,
    Memalign,
    PosixMemalign,
    Mark,
    Return,
}

struct Breakpoint {
    orig: i64,
    kind: BpKind,
}

#[derive(Debug, Clone, Copy)]
struct PendingCall {
    kind: BpKind,
    size: u64,
    ptr_in: u64,
    memptr: u64,
    caller: u64,
    return_rsp: u64,
}

const ALLOCATOR_SYMBOLS: [(&str, BpKind); 7] = [
    ("malloc", BpKind::Malloc),
    ("calloc", BpKind::Calloc),
    ("realloc", BpKind::Realloc),
    ("free", BpKind::Free),
    ("aligned_alloc", BpKind::AlignedAlloc),
    ("memalign", BpKind::Memalign),
    ("posix_memalign", BpKind::PosixMemalign),
];

fn regs_list(r: &libc::user_regs_struct) -> Vec<(Reg, u64)> {
    vec![
        (Reg::Rax, r.rax),
        (Reg::Rbx, r.rbx),
        (Reg::Rcx, r.rcx),
        (Reg::Rdx, r.rdx),
        (Reg::Rsi, r.rsi),
        (Reg::Rdi, r.rdi),
        (Reg::Rbp, r.rbp),
        (Reg::Rsp, r.rsp),
        (Reg::R8, r.r8),
        (Reg::R9, r.r9),
        (Reg::R10, r.r10),
        (Reg::R11, r.r11),
        (Reg::R12, r.r12),
        (Reg::R13, r.r13),
        (Reg::R14, r.r14),
        (Reg::R15, r.r15),
        (Reg::Rip, r.rip),
        (Reg::Rflags, r.eflags),
        (Reg::FsBase, r.fs_base),
        (Reg::GsBase, r.gs_base),
        (Reg::Cs, r.cs),
        (Reg::Ss, r.ss),
        (Reg::Ds, r.ds),
        (Reg::Es, r.es),
        (Reg::Fs, r.fs),
        (Reg::Gs, r.gs),
        (Reg::OrigRax, r.orig_rax),
    ]
}

fn perms_from_prot(prot: u64) -> String {
    let p = |bit: u64, c: char| if prot & bit != 0 { c } else { '-' };
    [p(1, 'r'), p(2, 'w'), p(4, 'x')].iter().collect()
}

fn is_error(ret: u64) -> bool {
    ret.cast_signed() < 0 && ret.cast_signed() >= -4095
}

#[allow(clippy::struct_excessive_bools)]
struct Recorder<W: std::io::Write> {
    pid: Pid,
    out: statesolver_trace::TraceWriter<W>,
    next_event: u64,
    summary: Summary,
    breakpoints: HashMap<u64, Breakpoint>,
    pending: Vec<PendingCall>,
    next_alloc: u64,
    live: HashMap<u64, (AllocId, Epoch, u64)>,
    last_at: HashMap<u64, (AllocId, Epoch)>,
    epochs: HashMap<u64, u32>,
    last_regs: Vec<(Reg, u64)>,
    in_syscall: Option<(u64, [u64; 6])>,
    pending_signal: Option<(Signal, i32, Option<u64>, u64)>,
    brk: Option<u64>,
}

impl<W: std::io::Write> Recorder<W> {
    fn emit(&mut self, kind: EventKind) -> Result<()> {
        if matches!(kind, EventKind::Mark { .. }) {
            self.summary.marks += 1;
        }
        self.out.write(&Event {
            id: EventId(self.next_event),
            rr_event: None,
            kind,
        })?;
        self.next_event += 1;
        Ok(())
    }

    fn read_word(&self, addr: u64) -> Result<u64> {
        Ok(ptrace::read(self.pid, addr as *mut c_void)?.cast_unsigned())
    }

    fn read_mem(&self, addr: u64, len: u64) -> Vec<u8> {
        let len = usize::try_from(len.min(64 << 20)).unwrap_or(0);
        let mut buf = vec![0u8; len];
        let remote = [RemoteIoVec {
            base: usize::try_from(addr).unwrap_or(0),
            len,
        }];
        let n = process_vm_readv(self.pid, &mut [IoSliceMut::new(&mut buf)], &remote).unwrap_or(0);
        buf.truncate(n);
        buf
    }

    fn read_cstr(&self, addr: u64) -> String {
        let raw = self.read_mem(addr, 256);
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        String::from_utf8_lossy(&raw[..end]).into_owned()
    }

    fn insert_bp(&mut self, addr: u64, kind: BpKind) -> Result<()> {
        if self.breakpoints.contains_key(&addr) {
            return Ok(());
        }
        let orig = ptrace::read(self.pid, addr as *mut c_void)?;
        let patched = (orig & !0xff) | 0xcc;
        ptrace::write(self.pid, addr as *mut c_void, patched)?;
        self.breakpoints.insert(addr, Breakpoint { orig, kind });
        Ok(())
    }

    fn remove_bp(&mut self, addr: u64) -> Result<Option<BpKind>> {
        if let Some(bp) = self.breakpoints.remove(&addr) {
            ptrace::write(self.pid, addr as *mut c_void, bp.orig)?;
            return Ok(Some(bp.kind));
        }
        Ok(None)
    }

    fn regs_event(&mut self) -> Result<()> {
        let now = regs_list(&ptrace::getregs(self.pid)?);
        let changed: Vec<(Reg, u64)> = now
            .iter()
            .zip(&self.last_regs)
            .filter(|(a, b)| a.1 != b.1)
            .map(|(a, _)| *a)
            .collect();
        self.last_regs = now;
        if !changed.is_empty() {
            self.emit(EventKind::Regs {
                full: false,
                values: changed,
            })?;
        }
        Ok(())
    }

    fn initial_state(&mut self, program: &Path) -> Result<()> {
        let regs = ptrace::getregs(self.pid)?;
        self.last_regs = regs_list(&regs);
        self.emit(EventKind::Regs {
            full: true,
            values: self.last_regs.clone(),
        })?;
        let maps = symbols::parse_maps(&std::fs::read_to_string(format!(
            "/proc/{}/maps",
            self.pid
        ))?);
        let regions = maps
            .iter()
            .map(|m| Region {
                start: m.start,
                len: m.end - m.start,
                perms: m.perms.chars().take(3).collect(),
                path: m.path.clone(),
            })
            .collect();
        self.emit(EventKind::Maps {
            op: MapOp::Map,
            regions,
        })?;
        // Entry point of the main executable.
        let data = std::fs::read(program)?;
        let file = object::File::parse(&*data).map_err(|e| Error::Symbols(e.to_string()))?;
        let entry = object::Object::entry(&file);
        let exe = std::fs::canonicalize(program)?
            .to_string_lossy()
            .into_owned();
        let base = symbols::images(&maps)
            .into_iter()
            .find(|i| i.path == exe)
            .map_or(0, |i| i.base);
        let first_vaddr = object::Object::segments(&file)
            .map(|s| object::ObjectSegment::address(&s))
            .min()
            .unwrap_or(0);
        let entry_rt = if object::Object::kind(&file) == object::ObjectKind::Dynamic {
            base + entry - first_vaddr
        } else {
            entry
        };
        self.insert_bp(entry_rt, BpKind::Entry)
    }

    /// If the tracee faulted on `rdtsc`/`rdtscp` (made to fault by
    /// `PR_SET_TSC`), execute it on the tracer's behalf, record the result,
    /// and skip the instruction. Returns whether it was handled.
    fn try_emulate_tsc(&mut self) -> Result<bool> {
        let mut regs = ptrace::getregs(self.pid)?;
        let code = self.read_mem(regs.rip, 3);
        let (insn, len) = match code.as_slice() {
            [0x0f, 0x31, ..] => ("rdtsc", 2),
            [0x0f, 0x01, 0xf9] => ("rdtscp", 3),
            _ => return Ok(false),
        };
        // SAFETY: rdtsc is available on every x86-64 CPU and has no memory
        // effects; the tracer itself is not TSC-restricted.
        #[allow(unsafe_code)]
        let tsc = unsafe { core::arch::x86_64::_rdtsc() };
        let pc = regs.rip;
        regs.rax = tsc & 0xffff_ffff;
        regs.rdx = tsc >> 32;
        let mut values = vec![(Reg::Rax, regs.rax), (Reg::Rdx, regs.rdx)];
        if insn == "rdtscp" {
            regs.rcx = 0;
            values.push((Reg::Rcx, 0));
        }
        regs.rip += len;
        ptrace::setregs(self.pid, regs)?;
        self.emit(EventKind::Nondet {
            pc,
            insn: insn.to_owned(),
            values,
        })?;
        Ok(true)
    }

    /// Replace the vDSO time functions with `mov eax, NR; syscall; ret` so
    /// their results go through recorded syscalls instead of reading the
    /// kernel's shared time page (as rr does).
    fn patch_vdso(&mut self, maps: &[symbols::MapLine]) -> Result<()> {
        let Some(vdso) = maps.iter().find(|m| m.path.as_deref() == Some("[vdso]")) else {
            return Ok(());
        };
        let image = self.read_mem(vdso.start, vdso.end - vdso.start);
        let Ok(file) = object::File::parse(&*image) else {
            return Ok(());
        };
        let first_vaddr = object::Object::segments(&file)
            .map(|s| object::ObjectSegment::address(&s))
            .min()
            .unwrap_or(0);
        for sym in object::Object::dynamic_symbols(&file) {
            let Ok(name) = object::ObjectSymbol::name(&sym) else {
                continue;
            };
            let nr: u32 = match name.trim_start_matches("__vdso_") {
                "clock_gettime" => 228,
                "gettimeofday" => 96,
                "time" => 201,
                "clock_getres" => 229,
                "getcpu" => 309,
                _ => continue,
            };
            let addr = vdso.start + object::ObjectSymbol::address(&sym) - first_vaddr;
            let n = nr.to_le_bytes();
            let stub = [0xb8, n[0], n[1], n[2], n[3], 0x0f, 0x05, 0xc3];
            ptrace::write(self.pid, addr as *mut c_void, i64::from_le_bytes(stub))?;
        }
        Ok(())
    }

    /// At the entry point every `DT_NEEDED` library is mapped: plant allocator
    /// and marker breakpoints.
    fn at_entry(&mut self) -> Result<()> {
        let maps = symbols::parse_maps(&std::fs::read_to_string(format!(
            "/proc/{}/maps",
            self.pid
        ))?);
        self.patch_vdso(&maps)?;
        let names: Vec<&str> = ALLOCATOR_SYMBOLS
            .iter()
            .map(|(n, _)| *n)
            .chain(["ss_mark"])
            .collect();
        let mut found: HashMap<String, u64> = HashMap::new();
        for image in symbols::images(&maps) {
            if let Ok(syms) = symbols::resolve(&image, &names) {
                for (name, addr) in syms {
                    // Prefer libc's allocator over any other definition.
                    let is_libc = image.path.contains("libc.so");
                    if is_libc || !found.contains_key(&name) {
                        found.insert(name, addr);
                    }
                }
            }
        }
        for (name, kind) in ALLOCATOR_SYMBOLS {
            if let Some(&addr) = found.get(name) {
                self.insert_bp(addr, kind)?;
            }
        }
        if let Some(&addr) = found.get("ss_mark") {
            self.insert_bp(addr, BpKind::Mark)?;
        }
        Ok(())
    }

    fn new_alloc(
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
        self.live.insert(addr, (id, epoch, size));
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

    fn free_ptr(&mut self, ptr: u64, caller: u64) -> Result<Option<AllocId>> {
        if ptr == 0 {
            return Ok(None);
        }
        let (id, epoch) = if let Some((id, epoch, _)) = self.live.remove(&ptr) {
            (id, epoch)
        } else if let Some(&(id, epoch)) = self.last_at.get(&ptr) {
            // Double free or free of a dead object: record it as it happened.
            (id, epoch)
        } else {
            self.summary.frees_untracked += 1;
            return Ok(None);
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

    fn on_breakpoint(&mut self, addr: u64, kind: BpKind) -> Result<()> {
        let regs = ptrace::getregs(self.pid)?;
        if std::env::var_os("STATESOLVER_DEBUG").is_some() {
            eprintln!(
                "bp {kind:?} at {addr:#x} rdi={:#x} rsp={:#x} pending={}",
                regs.rdi,
                regs.rsp,
                self.pending.len()
            );
        }
        match kind {
            BpKind::Entry => self.at_entry()?,
            BpKind::Mark => {
                let label = self.read_cstr(regs.rdi);
                let caller = self.read_word(regs.rsp)?;
                self.emit(EventKind::Mark { label, pc: caller })?;
            }
            BpKind::Return => {
                if let Some(pos) = self
                    .pending
                    .iter()
                    .rposition(|p| p.return_rsp == regs.rsp && p.caller == addr)
                {
                    let p = self.pending.remove(pos);
                    self.finish_call(p, regs.rax)?;
                }
            }
            BpKind::Free => {
                // glibc's free can jump back to its own entry (same stack,
                // pointer, and return address) on its slow path: that is one
                // logical call. Track frees in flight until they return.
                let caller = self.read_word(regs.rsp)?;
                let reentry = self.pending.iter().any(|p| {
                    p.kind == BpKind::Free && p.return_rsp == regs.rsp + 8 && p.caller == caller
                });
                if !reentry && self.pending.is_empty() {
                    // Recorded at entry: a double free aborts inside free and
                    // never returns, and that event is the evidence.
                    self.free_ptr(regs.rdi, caller)?;
                    self.pending.push(PendingCall {
                        kind: BpKind::Free,
                        size: 0,
                        ptr_in: regs.rdi,
                        memptr: 0,
                        caller,
                        return_rsp: regs.rsp + 8,
                    });
                    self.insert_bp(caller, BpKind::Return)?;
                }
            }
            _ if !self.pending.is_empty() => {} // allocator re-entering itself
            _ => {
                let caller = self.read_word(regs.rsp)?;
                let (size, ptr_in, memptr) = match kind {
                    BpKind::Malloc => (regs.rdi, 0, 0),
                    BpKind::Calloc => (regs.rdi.saturating_mul(regs.rsi), 0, 0),
                    BpKind::Realloc => (regs.rsi, regs.rdi, 0),
                    BpKind::AlignedAlloc | BpKind::Memalign => (regs.rsi, 0, 0),
                    BpKind::PosixMemalign => (regs.rdx, 0, regs.rdi),
                    _ => (0, 0, 0),
                };
                self.pending.push(PendingCall {
                    kind,
                    size,
                    ptr_in,
                    memptr,
                    caller,
                    return_rsp: regs.rsp + 8,
                });
                self.insert_bp(caller, BpKind::Return)?;
            }
        }
        Ok(())
    }

    fn finish_call(&mut self, p: PendingCall, rax: u64) -> Result<()> {
        // The return breakpoint is removed by `step_over`, which must still
        // rewind rip over it; removing it here would resume mid-instruction.
        match p.kind {
            BpKind::Malloc if rax != 0 => {
                self.new_alloc(rax, p.size, AllocKind::Malloc, None, p.caller)?;
            }
            BpKind::Calloc if rax != 0 => {
                self.new_alloc(rax, p.size, AllocKind::Calloc, None, p.caller)?;
            }
            BpKind::AlignedAlloc | BpKind::Memalign if rax != 0 => {
                self.new_alloc(rax, p.size, AllocKind::Aligned, None, p.caller)?;
            }
            BpKind::PosixMemalign if rax == 0 => {
                let ptr = self.read_word(p.memptr)?;
                self.new_alloc(ptr, p.size, AllocKind::Aligned, None, p.caller)?;
            }
            BpKind::Realloc => {
                if p.ptr_in == 0 {
                    if rax != 0 {
                        self.new_alloc(rax, p.size, AllocKind::Malloc, None, p.caller)?;
                    }
                } else if rax != 0 {
                    let old = self.free_ptr(p.ptr_in, p.caller)?;
                    self.new_alloc(rax, p.size, AllocKind::Realloc, old, p.caller)?;
                } else if p.size == 0 {
                    self.free_ptr(p.ptr_in, p.caller)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Step over the breakpoint at `addr` (rip is `addr + 1`), re-arming it
    /// unless it was one-shot.
    fn step_over(&mut self, addr: u64) -> Result<()> {
        let Some(kind) = self.remove_bp(addr)? else {
            return Ok(());
        };
        let mut regs = ptrace::getregs(self.pid)?;
        regs.rip = addr;
        ptrace::setregs(self.pid, regs)?;
        // A step can report its trap before the instruction retires; keep
        // stepping until rip leaves the breakpoint address.
        for _ in 0..4 {
            ptrace::step(self.pid, None)?;
            match waitpid(self.pid, None)? {
                WaitStatus::Stopped(_, Signal::SIGTRAP) => {}
                other => {
                    return Err(Error::Protocol(format!(
                        "unexpected stop while stepping: {other:?}"
                    )));
                }
            }
            let rip = ptrace::getregs(self.pid)?.rip;
            if std::env::var_os("STATESOLVER_DEBUG").is_some() {
                eprintln!("  stepped {addr:#x} -> rip {rip:#x}");
            }
            if rip != addr {
                break;
            }
        }
        let still_pending = kind == BpKind::Return && self.pending.iter().any(|p| p.caller == addr);
        if !matches!(kind, BpKind::Entry | BpKind::Return) || still_pending {
            self.insert_bp(addr, kind)?;
        }
        Ok(())
    }

    fn capture_writes(&self, nr: u64, args: &[u64; 6], ret: u64) -> Option<Vec<MemWrite>> {
        let model = syscalls::model(nr)?;
        let mut out = Vec::new();
        if is_error(ret) {
            return Some(out);
        }
        let mut push = |addr: u64, len: u64| {
            if addr != 0 && len > 0 {
                let data = self.read_mem(addr, len);
                if !data.is_empty() {
                    out.push(MemWrite {
                        addr,
                        data: Bytes(data),
                    });
                }
            }
        };
        for o in model {
            match *o {
                Out::RetBytes { buf } => push(args[buf], ret),
                Out::Fixed { buf, len } => push(args[buf], len),
                Out::SizedBy { buf, len_arg } => push(args[buf], args[len_arg]),
                Out::Iovec => {
                    let mut left = ret;
                    let iov = self.read_mem(args[1], args[2].min(1024) * 16);
                    for chunk in iov.as_chunks::<16>().0 {
                        if left == 0 {
                            break;
                        }
                        let base = u64::from_le_bytes(chunk[..8].try_into().unwrap_or_default());
                        let len = u64::from_le_bytes(chunk[8..].try_into().unwrap_or_default());
                        let n = len.min(left);
                        push(base, n);
                        left -= n;
                    }
                }
                Out::Nothing => {}
            }
        }
        Some(out)
    }

    fn on_syscall_exit(&mut self, nr: u64, args: [u64; 6]) -> Result<()> {
        let regs = ptrace::getregs(self.pid)?;
        let ret = regs.rax;
        let writes = if let Some(w) = self.capture_writes(nr, &args, ret) {
            w
        } else {
            *self.summary.uncaptured.entry(nr).or_default() += 1;
            Vec::new()
        };
        self.regs_event()?;
        self.summary.syscalls += 1;
        self.emit(EventKind::Syscall {
            pc: regs.rip.wrapping_sub(2),
            nr,
            args,
            ret,
            writes,
        })?;
        if is_error(ret) {
            return Ok(());
        }
        let map = |op, start, len, perms: String| EventKind::Maps {
            op,
            regions: vec![Region {
                start,
                len,
                perms,
                path: None,
            }],
        };
        match nr {
            9 if args[1] > 0 => {
                self.emit(map(MapOp::Map, ret, args[1], perms_from_prot(args[2])))?;
            }
            11 if args[1] > 0 => self.emit(map(MapOp::Unmap, args[0], args[1], "---".into()))?,
            10 if args[1] > 0 => self.emit(map(
                MapOp::Protect,
                args[0],
                args[1],
                perms_from_prot(args[2]),
            ))?,
            12 => {
                if let Some(old) = self.brk
                    && ret > old
                {
                    self.emit(map(MapOp::Brk, old, ret - old, "rw-".into()))?;
                }
                self.brk = Some(ret);
            }
            _ => {}
        }
        Ok(())
    }

    fn run(&mut self) -> Result<()> {
        let mut deliver: Option<Signal> = None;
        loop {
            ptrace::syscall(self.pid, deliver.take())?;
            let status = waitpid(self.pid, None)?;
            if std::env::var_os("STATESOLVER_DEBUG").is_some() {
                let rip = ptrace::getregs(self.pid).map_or(0, |r| r.rip);
                eprintln!("wait {status:?} rip={rip:#x}");
            }
            match status {
                WaitStatus::PtraceSyscall(_) => {
                    if let Some((nr, args)) = self.in_syscall.take() {
                        self.on_syscall_exit(nr, args)?;
                    } else {
                        let r = ptrace::getregs(self.pid)?;
                        if let Some(what) = match r.orig_rax {
                            56 | 435 => Some("clone/clone3 (threads or child processes)"),
                            57 | 58 => Some("fork/vfork (child processes)"),
                            59 | 322 => Some("a second execve/execveat"),
                            _ => None,
                        } {
                            let _ = nix::sys::signal::kill(self.pid, Signal::SIGKILL);
                            return Err(Error::Unsupported(format!(
                                "the program called {what} at {:#x}",
                                r.rip.wrapping_sub(2)
                            )));
                        }
                        self.in_syscall =
                            Some((r.orig_rax, [r.rdi, r.rsi, r.rdx, r.r10, r.r8, r.r9]));
                    }
                }
                WaitStatus::Stopped(_, Signal::SIGTRAP) => {
                    let rip = ptrace::getregs(self.pid)?.rip;
                    let addr = rip.wrapping_sub(1);
                    if let Some(kind) = self.breakpoints.get(&addr).map(|b| b.kind) {
                        self.on_breakpoint(addr, kind)?;
                        self.step_over(addr)?;
                    }
                }
                WaitStatus::Stopped(_, Signal::SIGSEGV) if self.try_emulate_tsc()? => {}
                WaitStatus::Stopped(_, sig) => {
                    let info = ptrace::getsiginfo(self.pid)?;
                    let fault = matches!(
                        sig,
                        Signal::SIGSEGV | Signal::SIGBUS | Signal::SIGILL | Signal::SIGFPE
                    );
                    // SAFETY: `si_addr` is defined for fault signals, which is
                    // the only case in which it is read.
                    #[allow(unsafe_code)]
                    let addr = fault.then(|| unsafe { info.si_addr() } as u64);
                    let pc = ptrace::getregs(self.pid)?.rip;
                    if let Some((s, code, a, p)) = self.pending_signal.take() {
                        self.emit(EventKind::Signal {
                            signo: s as u32,
                            code,
                            addr: a,
                            pc: p,
                            fatal: false,
                        })?;
                    }
                    self.pending_signal = Some((sig, info.si_code, addr, pc));
                    deliver = Some(sig);
                }
                WaitStatus::Exited(_, code) => {
                    if let Some((s, c, a, p)) = self.pending_signal.take() {
                        self.emit(EventKind::Signal {
                            signo: s as u32,
                            code: c,
                            addr: a,
                            pc: p,
                            fatal: false,
                        })?;
                    }
                    self.emit(EventKind::Exit { code })?;
                    self.summary.outcome = format!("exited with status {code}");
                    return Ok(());
                }
                WaitStatus::Signaled(_, sig, _) => {
                    let (code, addr, pc) = match self.pending_signal.take() {
                        Some((s, c, a, p)) if s == sig => (c, a, p),
                        _ => (0, None, 0),
                    };
                    self.emit(EventKind::Signal {
                        signo: sig as u32,
                        code,
                        addr,
                        pc,
                        fatal: true,
                    })?;
                    self.summary.outcome = format!("killed by {sig:?}");
                    return Ok(());
                }
                other => {
                    return Err(Error::Protocol(format!("unexpected wait status {other:?}")));
                }
            }
        }
    }
}

/// Record one execution.
///
/// # Errors
/// Fails if the program cannot be started or traced, or the trace cannot be written.
pub fn record(opts: &Options) -> Result<Summary> {
    let mut cmd = Command::new(&opts.program);
    cmd.args(&opts.args);
    if let Some(path) = &opts.stdin {
        cmd.stdin(Stdio::from(std::fs::File::open(path)?));
    }
    // SAFETY: the closure runs in the forked child before exec and calls only
    // async-signal-safe syscalls (personality, prctl, ptrace); it allocates nothing.
    #[allow(unsafe_code)]
    unsafe {
        cmd.pre_exec(|| {
            nix::sys::personality::set(nix::sys::personality::Persona::ADDR_NO_RANDOMIZE)
                .map_err(std::io::Error::from)?;
            // Make rdtsc/rdtscp fault so the recorder can emulate and record them.
            if libc::prctl(libc::PR_SET_TSC, libc::PR_TSC_SIGSEGV, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            ptrace::traceme().map_err(std::io::Error::from)?;
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    let pid = Pid::from_raw(i32::try_from(child.id()).map_err(|e| Error::Protocol(e.to_string()))?);
    match waitpid(pid, None)? {
        WaitStatus::Stopped(_, Signal::SIGTRAP) => {}
        other => {
            return Err(Error::Protocol(format!(
                "tracee did not stop at exec: {other:?}"
            )));
        }
    }
    ptrace::setoptions(
        pid,
        ptrace::Options::PTRACE_O_TRACESYSGOOD | ptrace::Options::PTRACE_O_EXITKILL,
    )?;

    let program = Program {
        path: opts.program.display().to_string(),
        argv: std::iter::once(opts.program.display().to_string())
            .chain(opts.args.iter().cloned())
            .collect(),
        sha256: None,
    };
    let mut header = Header::new(
        format!("statesolver-record {}", env!("CARGO_PKG_VERSION")),
        program,
    );
    header.description = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .map(|k| format!("kernel {}", k.trim()));
    let out = statesolver_trace::create(&opts.out, &header)?;
    let mut r = Recorder {
        pid,
        out,
        next_event: 0,
        summary: Summary::default(),
        breakpoints: HashMap::new(),
        pending: Vec::new(),
        next_alloc: 0,
        live: HashMap::new(),
        last_at: HashMap::new(),
        epochs: HashMap::new(),
        last_regs: Vec::new(),
        in_syscall: None,
        pending_signal: None,
        brk: None,
    };
    r.initial_state(&opts.program)?;
    r.run()?;
    r.summary.events = r.next_event;
    let summary = r.summary.clone();
    r.out.finish()?;
    Ok(summary)
}
