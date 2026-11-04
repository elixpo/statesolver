//! Semantic validation of an event stream.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::event::{ActId, CtxId, Event, EventKind, FnId, Header};

/// One rule violation, located by its 0-based event index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// Index of the offending event.
    pub index: u64,
    /// What went wrong.
    pub rule: Rule,
}

/// The rules a valid trace must satisfy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    /// Timestamps must be non-decreasing.
    TimeWentBackwards {
        /// Previous timestamp.
        prev: u64,
        /// This timestamp.
        ts: u64,
    },
    /// A context id was spawned twice. Ids are never reused, even after exit.
    DuplicateSpawn(CtxId),
    /// `spawn` named a parent that is not live.
    DeadParent(CtxId),
    /// Event for a context that was never spawned.
    UnknownContext(CtxId),
    /// Event for a context that already exited.
    ExitedContext(CtxId),
    /// `call` or `return` on a suspended context.
    RunWhileSuspended(CtxId),
    /// `call` reused an activation id.
    DuplicateActivation(ActId),
    /// `call` referenced a function missing from a non-empty header table.
    UnknownFunction(FnId),
    /// `return` on an empty stack.
    ReturnOnEmptyStack(CtxId),
    /// `return` did not name the innermost activation.
    ReturnMismatch {
        /// Context.
        ctx: CtxId,
        /// Innermost activation.
        expected: ActId,
        /// Activation named by the event.
        found: ActId,
    },
    /// `suspend` on an already suspended context.
    DoubleSuspend(CtxId),
    /// `resume` on a context that is not suspended.
    ResumeNotSuspended(CtxId),
    /// `exit` while suspended or with live activations.
    ExitWithLiveState {
        /// Context.
        ctx: CtxId,
        /// Live activations at exit.
        depth: usize,
        /// Whether the context was suspended.
        suspended: bool,
    },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "event {}: ", self.index)?;
        match &self.rule {
            Rule::TimeWentBackwards { prev, ts } => {
                write!(f, "timestamp {ts} is before previous {prev}")
            }
            Rule::DuplicateSpawn(c) => write!(f, "context {} spawned twice", c.0),
            Rule::DeadParent(c) => write!(f, "parent context {} is not live", c.0),
            Rule::UnknownContext(c) => write!(f, "context {} was never spawned", c.0),
            Rule::ExitedContext(c) => write!(f, "context {} already exited", c.0),
            Rule::RunWhileSuspended(c) => write!(f, "context {} is suspended", c.0),
            Rule::DuplicateActivation(a) => write!(f, "activation {} reused", a.0),
            Rule::UnknownFunction(id) => write!(f, "function {} not in header", id.0),
            Rule::ReturnOnEmptyStack(c) => write!(f, "return on empty stack of context {}", c.0),
            Rule::ReturnMismatch {
                ctx,
                expected,
                found,
            } => write!(
                f,
                "context {} returned activation {} but innermost is {}",
                ctx.0, found.0, expected.0
            ),
            Rule::DoubleSuspend(c) => write!(f, "context {} suspended twice", c.0),
            Rule::ResumeNotSuspended(c) => write!(f, "context {} resumed but not suspended", c.0),
            Rule::ExitWithLiveState {
                ctx,
                depth,
                suspended,
            } => write!(
                f,
                "context {} exited with {depth} live activations (suspended: {suspended})",
                ctx.0
            ),
        }
    }
}

#[derive(Default)]
struct CtxState {
    stack: Vec<ActId>,
    suspended: bool,
    exited: bool,
}

/// Incremental validator. Feed events in order with [`Validator::check`].
pub struct Validator {
    functions: Option<HashSet<FnId>>,
    contexts: HashMap<CtxId, CtxState>,
    activations: HashSet<ActId>,
    prev_ts: u64,
    index: u64,
}

impl Validator {
    /// A validator for a trace with the given header.
    #[must_use]
    pub fn new(header: &Header) -> Self {
        let functions =
            (!header.functions.is_empty()).then(|| header.functions.iter().map(|f| f.id).collect());
        Self {
            functions,
            contexts: HashMap::new(),
            activations: HashSet::new(),
            prev_ts: 0,
            index: 0,
        }
    }

    /// Check one event. Returns every rule it breaks; state is updated as far
    /// as the event makes sense, so validation can continue afterwards.
    pub fn check(&mut self, event: &Event) -> Vec<Violation> {
        let index = self.index;
        self.index += 1;
        let mut out = Vec::new();
        let mut bad = |rule| out.push(Violation { index, rule });

        if event.ts < self.prev_ts {
            bad(Rule::TimeWentBackwards {
                prev: self.prev_ts,
                ts: event.ts,
            });
        }
        self.prev_ts = self.prev_ts.max(event.ts);

        if let EventKind::Spawn { ctx, parent } = &event.kind {
            if let Some(p) = parent
                && self.contexts.get(p).is_none_or(|s| s.exited)
            {
                bad(Rule::DeadParent(*p));
            }
            if self.contexts.insert(*ctx, CtxState::default()).is_some() {
                bad(Rule::DuplicateSpawn(*ctx));
            }
            return out;
        }
        let Some(ctx) = event.kind.ctx() else {
            return out; // sample
        };
        let Some(state) = self.contexts.get_mut(&ctx) else {
            bad(Rule::UnknownContext(ctx));
            return out;
        };
        if state.exited {
            bad(Rule::ExitedContext(ctx));
            return out;
        }
        match &event.kind {
            EventKind::Call { act, func, .. } => {
                if state.suspended {
                    bad(Rule::RunWhileSuspended(ctx));
                }
                if !self.activations.insert(*act) {
                    bad(Rule::DuplicateActivation(*act));
                }
                if let Some(fns) = &self.functions
                    && !fns.contains(func)
                {
                    bad(Rule::UnknownFunction(*func));
                }
                state.stack.push(*act);
            }
            EventKind::Return { act, .. } => {
                if state.suspended {
                    bad(Rule::RunWhileSuspended(ctx));
                }
                match state.stack.pop() {
                    None => bad(Rule::ReturnOnEmptyStack(ctx)),
                    Some(top) if top != *act => bad(Rule::ReturnMismatch {
                        ctx,
                        expected: top,
                        found: *act,
                    }),
                    Some(_) => {}
                }
            }
            EventKind::Suspend { .. } => {
                if state.suspended {
                    bad(Rule::DoubleSuspend(ctx));
                }
                state.suspended = true;
            }
            EventKind::Resume { .. } => {
                if !state.suspended {
                    bad(Rule::ResumeNotSuspended(ctx));
                }
                state.suspended = false;
            }
            EventKind::Exit { .. } => {
                if state.suspended || !state.stack.is_empty() {
                    bad(Rule::ExitWithLiveState {
                        ctx,
                        depth: state.stack.len(),
                        suspended: state.suspended,
                    });
                }
                state.exited = true;
                state.stack.clear();
            }
            EventKind::Spawn { .. } | EventKind::Sample { .. } => unreachable!("handled above"),
        }
        out
    }
}
