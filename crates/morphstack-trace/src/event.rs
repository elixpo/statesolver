//! Trace data model: header, identifiers, and events.

use serde::{Deserialize, Serialize};

/// Name written into every trace header.
pub const FORMAT_NAME: &str = "morphstack-trace";

/// Current trace-format version.
pub const FORMAT_VERSION: u16 = 0;

/// Execution context (coroutine, task, goroutine, thread) identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CtxId(pub u64);

/// Activation (one dynamic function invocation) identifier. Unique per trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActId(pub u64);

/// Static function identifier, described by [`FunctionInfo`] in the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FnId(pub u32);

/// Static facts about a function, used by static baselines such as
/// "heap if the function can suspend".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionInfo {
    /// Identifier referenced by `call` events.
    pub id: FnId,
    /// Human-readable name.
    pub name: String,
    /// Whether an activation of this function may suspend (directly or via a callee).
    #[serde(default)]
    pub can_suspend: bool,
}

/// Trace header. Written once, before any event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// Always [`FORMAT_NAME`].
    pub format: String,
    /// Format version; readers reject versions newer than [`FORMAT_VERSION`].
    pub version: u16,
    /// Producer of the trace, for example `morphstack-workloads/w6` or `tokio-capture`.
    pub source: String,
    /// Generator seed, when the trace is synthetic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Free-form description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Function table. May be empty when the producer has no static information.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<FunctionInfo>,
}

impl Header {
    /// A header for the current format version with the given source.
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            format: FORMAT_NAME.to_owned(),
            version: FORMAT_VERSION,
            source: source.into(),
            seed: None,
            description: None,
            functions: Vec::new(),
        }
    }
}

/// One trace event. Timestamps are nanoseconds from an arbitrary origin and
/// must be non-decreasing across the whole trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Timestamp in nanoseconds.
    pub ts: u64,
    /// What happened.
    #[serde(flatten)]
    pub kind: EventKind,
}

/// Event payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum EventKind {
    /// A new context becomes runnable.
    Spawn {
        /// New context.
        ctx: CtxId,
        /// Spawning context, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<CtxId>,
    },
    /// An activation is pushed on `ctx`.
    Call {
        /// Context the activation runs on.
        ctx: CtxId,
        /// New activation; never reused within a trace.
        act: ActId,
        /// Called function.
        #[serde(rename = "fn")]
        func: FnId,
        /// Frame size in bytes (locals, spills, saved registers, return address).
        frame_bytes: u32,
    },
    /// The innermost activation of `ctx` returns. `act` must be that activation.
    Return {
        /// Context.
        ctx: CtxId,
        /// Returning activation.
        act: ActId,
    },
    /// `ctx` suspends with its activations live.
    Suspend {
        /// Context.
        ctx: CtxId,
    },
    /// A suspended `ctx` resumes.
    Resume {
        /// Context.
        ctx: CtxId,
    },
    /// `ctx` terminates. Its activation stack must be empty.
    Exit {
        /// Context.
        ctx: CtxId,
    },
    /// Process-wide resource sample. All fields are optional.
    Sample {
        /// Resident set size.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rss_bytes: Option<u64>,
        /// CPU cycles since the previous sample.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cycles: Option<u64>,
        /// Last-level cache misses since the previous sample.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        llc_misses: Option<u64>,
        /// Data TLB misses since the previous sample.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dtlb_misses: Option<u64>,
    },
}

impl EventKind {
    /// The context this event belongs to, if it is context-scoped.
    #[must_use]
    pub fn ctx(&self) -> Option<CtxId> {
        match self {
            Self::Spawn { ctx, .. }
            | Self::Call { ctx, .. }
            | Self::Return { ctx, .. }
            | Self::Suspend { ctx }
            | Self::Resume { ctx }
            | Self::Exit { ctx } => Some(*ctx),
            Self::Sample { .. } => None,
        }
    }

    /// Short lowercase name, matching the JSONL `ev` tag.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Spawn { .. } => "spawn",
            Self::Call { .. } => "call",
            Self::Return { .. } => "return",
            Self::Suspend { .. } => "suspend",
            Self::Resume { .. } => "resume",
            Self::Exit { .. } => "exit",
            Self::Sample { .. } => "sample",
        }
    }
}
