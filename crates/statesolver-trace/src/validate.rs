//! Semantic validation of an event stream.
//!
//! The allocation rules here are the invariants the causal analysis relies
//! on: ids are unique, live allocations never overlap, and epochs count the
//! allocations at each start address without gaps.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use crate::event::{AllocId, Epoch, Event, EventId, EventKind, MapOp};

/// One violation at an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// Offending event.
    pub event: EventId,
    /// Rule broken.
    pub rule: Rule,
}

/// Validity rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    /// Event ids must be 0, 1, 2, … without gaps.
    NonSequentialId {
        /// Expected id.
        expected: u64,
    },
    /// A `regs` delta appeared before any full snapshot.
    DeltaBeforeFullRegs,
    /// An allocation id was reused.
    DuplicateAllocId(AllocId),
    /// A new allocation overlaps a live one.
    Overlap {
        /// The new allocation.
        new: AllocId,
        /// The live allocation it overlaps.
        live: AllocId,
    },
    /// The epoch is not one more than the previous epoch at this address.
    WrongEpoch {
        /// Address.
        addr: u64,
        /// Expected epoch.
        expected: Epoch,
        /// Epoch in the event.
        found: Epoch,
    },
    /// `realloc_of` does not name an allocation that was freed earlier.
    BadReallocSource(AllocId),
    /// `free` of an allocation that is not live.
    FreeOfDeadAllocation(AllocId),
    /// `free` whose address or epoch disagrees with the allocation.
    FreeMismatch(AllocId),
    /// A map change with zero length or an unaligned start.
    BadRegion {
        /// Start address.
        start: u64,
        /// Length.
        len: u64,
    },
    /// An event after the process ended.
    AfterTermination,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "E{}: ", self.event.0)?;
        match &self.rule {
            Rule::NonSequentialId { expected } => write!(f, "expected event id {expected}"),
            Rule::DeltaBeforeFullRegs => f.write_str("register delta before any full snapshot"),
            Rule::DuplicateAllocId(a) => write!(f, "allocation #{} reused", a.0),
            Rule::Overlap { new, live } => {
                write!(
                    f,
                    "allocation #{} overlaps live allocation #{}",
                    new.0, live.0
                )
            }
            Rule::WrongEpoch {
                addr,
                expected,
                found,
            } => write!(f, "epoch {} at {addr:#x}, expected {}", found.0, expected.0),
            Rule::BadReallocSource(a) => {
                write!(f, "realloc source #{} was not freed earlier", a.0)
            }
            Rule::FreeOfDeadAllocation(a) => write!(f, "free of non-live allocation #{}", a.0),
            Rule::FreeMismatch(a) => write!(f, "free of #{} with wrong address or epoch", a.0),
            Rule::BadRegion { start, len } => {
                write!(f, "bad region start {start:#x} len {len}")
            }
            Rule::AfterTermination => f.write_str("event after process termination"),
        }
    }
}

struct Live {
    addr: u64,
    end: u64,
    epoch: Epoch,
}

/// Incremental validator.
#[derive(Default)]
pub struct Validator {
    next_id: u64,
    seen_full_regs: bool,
    terminated: bool,
    ids: HashSet<AllocId>,
    freed: HashSet<AllocId>,
    live: HashMap<AllocId, Live>,
    /// Live allocations by start address, for overlap checks.
    by_start: BTreeMap<u64, AllocId>,
    epochs: HashMap<u64, Epoch>,
}

const PAGE: u64 = 4096;

impl Validator {
    /// A fresh validator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn overlapping(&self, start: u64, end: u64) -> Option<AllocId> {
        // The live allocation with the greatest start below `end` is the only
        // candidate, because live allocations never overlap each other.
        self.by_start
            .range(..end)
            .next_back()
            .and_then(|(_, id)| self.live.get(id).map(|l| (*id, l)))
            .filter(|(_, l)| l.end > start)
            .map(|(id, _)| id)
    }

    /// Check one event; returns every rule it breaks. State is updated as far
    /// as the event makes sense, so validation can continue.
    #[allow(clippy::too_many_lines)]
    pub fn check(&mut self, e: &Event) -> Vec<Violation> {
        let mut out = Vec::new();
        let mut bad = |rule| out.push(Violation { event: e.id, rule });
        if e.id.0 != self.next_id {
            bad(Rule::NonSequentialId {
                expected: self.next_id,
            });
        }
        self.next_id = e.id.0 + 1;
        if self.terminated {
            bad(Rule::AfterTermination);
        }
        if e.kind.is_terminal() {
            self.terminated = true;
        }
        match &e.kind {
            EventKind::Regs { full, .. } => {
                if *full {
                    self.seen_full_regs = true;
                } else if !self.seen_full_regs {
                    bad(Rule::DeltaBeforeFullRegs);
                }
            }
            EventKind::Maps { op, regions } => {
                for r in regions {
                    let aligned = *op == MapOp::Brk || r.start % PAGE == 0;
                    if r.len == 0 || !aligned {
                        bad(Rule::BadRegion {
                            start: r.start,
                            len: r.len,
                        });
                    }
                }
            }
            EventKind::Alloc {
                alloc,
                addr,
                size,
                epoch,
                realloc_of,
                ..
            } => {
                if !self.ids.insert(*alloc) {
                    bad(Rule::DuplicateAllocId(*alloc));
                }
                if let Some(src) = realloc_of
                    && !self.freed.contains(src)
                {
                    bad(Rule::BadReallocSource(*src));
                }
                // malloc(0) still returns a unique address: treat it as one byte.
                let end = addr.saturating_add((*size).max(1));
                if let Some(live) = self.overlapping(*addr, end) {
                    bad(Rule::Overlap { new: *alloc, live });
                }
                let expected = Epoch(self.epochs.get(addr).map_or(1, |e| e.0 + 1));
                if *epoch != expected {
                    bad(Rule::WrongEpoch {
                        addr: *addr,
                        expected,
                        found: *epoch,
                    });
                }
                self.epochs.insert(*addr, (*epoch).max(expected));
                self.live.insert(
                    *alloc,
                    Live {
                        addr: *addr,
                        end,
                        epoch: *epoch,
                    },
                );
                self.by_start.insert(*addr, *alloc);
            }
            EventKind::Free {
                alloc, addr, epoch, ..
            } => match self.live.remove(alloc) {
                None => bad(Rule::FreeOfDeadAllocation(*alloc)),
                Some(l) => {
                    if l.addr != *addr || l.epoch != *epoch {
                        bad(Rule::FreeMismatch(*alloc));
                    }
                    if self.by_start.get(&l.addr) == Some(alloc) {
                        self.by_start.remove(&l.addr);
                    }
                    self.freed.insert(*alloc);
                }
            },
            EventKind::Syscall { .. }
            | EventKind::Signal { .. }
            | EventKind::Mark { .. }
            | EventKind::Nondet { .. }
            | EventKind::Exit { .. } => {}
        }
        out
    }
}
