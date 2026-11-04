//! Trace data model: header, registers, and events.

use serde::{Deserialize, Serialize};

use crate::bytes::Bytes;

/// Name written into every trace header.
pub const FORMAT_NAME: &str = "statesolver-trace";

/// Current trace-format version.
pub const FORMAT_VERSION: u16 = 0;

/// Global, strictly increasing event ordinal (`E18442`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(pub u64);

/// Unique allocation identifier (`Allocation #41`). Never reused in a trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AllocId(pub u64);

/// Per-address allocation generation. The `n`-th allocation that starts at a
/// given address has epoch `n` (1-based), so `0x55550000 @ epoch 7` and
/// `0x55550000 @ epoch 8` are different objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Epoch(pub u32);

/// What was traced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Program {
    /// Executable path as launched.
    pub path: String,
    /// Arguments, including `argv[0]`.
    pub argv: Vec<String>,
    /// SHA-256 of the executable, hex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Trace header. Written once, before any event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// Always [`FORMAT_NAME`].
    pub format: String,
    /// Format version; readers reject newer versions.
    pub version: u16,
    /// Target architecture; `x86_64` in v0.
    pub arch: String,
    /// Target OS; `linux` in v0.
    pub os: String,
    /// Producer, for example `statesolver-record 0.1.0` or `hand-written`.
    pub recorder: String,
    /// The traced program.
    pub program: Program,
    /// Free-form notes (kernel version, environment).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Header {
    /// A v0 x86-64 Linux header.
    #[must_use]
    pub fn new(recorder: impl Into<String>, program: Program) -> Self {
        Self {
            format: FORMAT_NAME.to_owned(),
            version: FORMAT_VERSION,
            arch: "x86_64".to_owned(),
            os: "linux".to_owned(),
            recorder: recorder.into(),
            program,
            description: None,
        }
    }
}

/// x86-64 general-purpose and control registers, matching the kernel's
/// `user_regs_struct` field set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(missing_docs)]
pub enum Reg {
    Rax,
    Rbx,
    Rcx,
    Rdx,
    Rsi,
    Rdi,
    Rbp,
    Rsp,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
    Rip,
    Rflags,
    FsBase,
    GsBase,
    Cs,
    Ss,
    Ds,
    Es,
    Fs,
    Gs,
    OrigRax,
}

impl Reg {
    /// Every register, in a stable order.
    pub const ALL: [Self; 27] = [
        Self::Rax,
        Self::Rbx,
        Self::Rcx,
        Self::Rdx,
        Self::Rsi,
        Self::Rdi,
        Self::Rbp,
        Self::Rsp,
        Self::R8,
        Self::R9,
        Self::R10,
        Self::R11,
        Self::R12,
        Self::R13,
        Self::R14,
        Self::R15,
        Self::Rip,
        Self::Rflags,
        Self::FsBase,
        Self::GsBase,
        Self::Cs,
        Self::Ss,
        Self::Ds,
        Self::Es,
        Self::Fs,
        Self::Gs,
        Self::OrigRax,
    ];

    /// Lowercase name, as in `rax`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Rax => "rax",
            Self::Rbx => "rbx",
            Self::Rcx => "rcx",
            Self::Rdx => "rdx",
            Self::Rsi => "rsi",
            Self::Rdi => "rdi",
            Self::Rbp => "rbp",
            Self::Rsp => "rsp",
            Self::R8 => "r8",
            Self::R9 => "r9",
            Self::R10 => "r10",
            Self::R11 => "r11",
            Self::R12 => "r12",
            Self::R13 => "r13",
            Self::R14 => "r14",
            Self::R15 => "r15",
            Self::Rip => "rip",
            Self::Rflags => "rflags",
            Self::FsBase => "fs_base",
            Self::GsBase => "gs_base",
            Self::Cs => "cs",
            Self::Ss => "ss",
            Self::Ds => "ds",
            Self::Es => "es",
            Self::Fs => "fs",
            Self::Gs => "gs",
            Self::OrigRax => "orig_rax",
        }
    }

    /// Parse a lowercase name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.name() == name)
    }
}

/// How an allocation was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocKind {
    /// `malloc`.
    Malloc,
    /// `calloc`.
    Calloc,
    /// `realloc` (paired with a preceding free of `realloc_of`).
    Realloc,
    /// `aligned_alloc`, `posix_memalign`, `memalign`.
    Aligned,
    /// Direct anonymous `mmap` treated as an allocation.
    Mmap,
}

/// Kind of address-space change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapOp {
    /// New mapping (`mmap`, or present at start).
    Map,
    /// Removed (`munmap`).
    Unmap,
    /// Permission change (`mprotect`).
    Protect,
    /// Program break moved (`brk`); `start` is the old break, `len` the growth.
    Brk,
}

/// One memory region in a map change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    /// Start address.
    pub start: u64,
    /// Length in bytes.
    pub len: u64,
    /// `r`, `w`, `x` permission letters, for example `r-x`.
    pub perms: String,
    /// Backing file or pseudo-path (`[heap]`, `[stack]`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Bytes the kernel wrote into the tracee during a syscall. Replay injects
/// these instead of re-executing the syscall; analysis treats them as
/// external input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemWrite {
    /// Destination address.
    pub addr: u64,
    /// Bytes written.
    pub data: Bytes,
}

/// One trace event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Global ordinal: 0, 1, 2, … with no gaps.
    pub id: EventId,
    /// What happened.
    #[serde(flatten)]
    pub kind: EventKind,
}

/// Event payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum EventKind {
    /// Register values: a full snapshot (`full = true`) or the registers that
    /// changed since the previous `regs` event.
    Regs {
        /// Whether this is a full snapshot.
        full: bool,
        /// Register values.
        values: Vec<(Reg, u64)>,
    },
    /// Address-space change.
    Maps {
        /// Operation.
        op: MapOp,
        /// Affected regions.
        regions: Vec<Region>,
    },
    /// A completed syscall.
    Syscall {
        /// Instruction pointer of the `syscall` instruction.
        pc: u64,
        /// Syscall number (x86-64 numbering).
        nr: u64,
        /// Arguments `rdi, rsi, rdx, r10, r8, r9`.
        args: [u64; 6],
        /// Return value (`rax` after the syscall), as raw bits.
        ret: u64,
        /// Memory the kernel wrote into the tracee.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        writes: Vec<MemWrite>,
    },
    /// A heap object came into existence.
    Alloc {
        /// Unique id.
        alloc: AllocId,
        /// Start address returned to the program.
        addr: u64,
        /// Requested size in bytes.
        size: u64,
        /// Generation of this start address.
        epoch: Epoch,
        /// How it was allocated.
        kind: AllocKind,
        /// For `realloc`: the allocation it replaced (freed just before).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        realloc_of: Option<AllocId>,
        /// Return address of the allocator call.
        caller: u64,
    },
    /// A heap object was released.
    Free {
        /// The allocation released.
        alloc: AllocId,
        /// Its start address.
        addr: u64,
        /// Its epoch.
        epoch: Epoch,
        /// Return address of the `free` call.
        caller: u64,
    },
    /// A signal was delivered to the tracee.
    Signal {
        /// Signal number.
        signo: u32,
        /// `si_code`.
        code: i32,
        /// `si_addr` for faults.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        addr: Option<u64>,
        /// Instruction pointer at delivery.
        pc: u64,
        /// Whether the process died from it.
        fatal: bool,
    },
    /// Ground-truth or user marker (`ss_mark("label")` in the bug corpus).
    Mark {
        /// Label.
        label: String,
        /// Instruction pointer of the marker call.
        pc: u64,
    },
    /// Normal process exit.
    Exit {
        /// Exit status.
        code: i32,
    },
}

impl EventKind {
    /// Short lowercase name, matching the JSONL `ev` tag.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Regs { .. } => "regs",
            Self::Maps { .. } => "maps",
            Self::Syscall { .. } => "syscall",
            Self::Alloc { .. } => "alloc",
            Self::Free { .. } => "free",
            Self::Signal { .. } => "signal",
            Self::Mark { .. } => "mark",
            Self::Exit { .. } => "exit",
        }
    }

    /// Whether the process ends at this event.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Exit { .. } | Self::Signal { fatal: true, .. })
    }
}
