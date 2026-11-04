//! Temporal heap: allocation lifetimes indexed by address and time.
//!
//! The core identity of StateSolver: a heap object is `{range, alloc_event,
//! free_event, epoch}`, not an address. [`TemporalHeap::resolve`] answers
//! "which object did this address belong to at this event?", so two
//! allocations at `0x55550000` (epochs 7 and 8) stay distinct.

use std::collections::BTreeMap;

use serde::Serialize;
use statesolver_trace::{AllocId, AllocKind, Epoch, Event, EventId, EventKind};

/// One allocation lifetime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Allocation {
    /// Unique id.
    pub id: AllocId,
    /// Start address.
    pub addr: u64,
    /// Requested size.
    pub size: u64,
    /// Per-address generation.
    pub epoch: Epoch,
    /// How it was allocated.
    pub kind: AllocKind,
    /// Allocation event.
    pub alloc_event: EventId,
    /// Free event, if it was freed within the trace.
    pub free_event: Option<EventId>,
    /// Allocator call site.
    pub alloc_caller: u64,
    /// `free` call site.
    pub free_caller: Option<u64>,
    /// For `realloc`: the allocation it replaced.
    pub realloc_of: Option<AllocId>,
}

impl Allocation {
    /// Whether `addr` falls inside this allocation (zero-size counts as one byte).
    #[must_use]
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.addr && addr < self.addr.saturating_add(self.size.max(1))
    }

    /// Whether the allocation is live at `at` (allocated at or before it, not yet freed).
    #[must_use]
    pub fn live_at(&self, at: EventId) -> bool {
        self.alloc_event <= at && self.free_event.is_none_or(|f| at < f)
    }
}

/// What an address referred to at a point in time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Resolution<'a> {
    /// Inside a live allocation.
    Live {
        /// The allocation.
        allocation: &'a Allocation,
        /// Offset from its start.
        offset: u64,
    },
    /// Inside a freed allocation that has not (yet) been reused at this address.
    Freed {
        /// The most recent allocation that covered the address.
        allocation: &'a Allocation,
        /// Offset from its start.
        offset: u64,
    },
    /// Never part of a recorded heap allocation before this event.
    Unknown,
}

/// Allocation index over a whole trace.
#[derive(Debug, Default)]
pub struct TemporalHeap {
    allocations: Vec<Allocation>,
    by_id: BTreeMap<AllocId, usize>,
    by_start: BTreeMap<u64, Vec<usize>>,
    max_size: u64,
}

impl TemporalHeap {
    /// Build from an event stream. Frees of unknown allocations are ignored;
    /// validate the trace first.
    ///
    /// # Errors
    /// Propagates errors from the stream.
    pub fn from_events<E>(events: impl IntoIterator<Item = Result<Event, E>>) -> Result<Self, E> {
        let mut h = Self::default();
        for e in events {
            let e = e?;
            match e.kind {
                EventKind::Alloc {
                    alloc,
                    addr,
                    size,
                    epoch,
                    kind,
                    realloc_of,
                    caller,
                } => {
                    let i = h.allocations.len();
                    h.allocations.push(Allocation {
                        id: alloc,
                        addr,
                        size,
                        epoch,
                        kind,
                        alloc_event: e.id,
                        free_event: None,
                        alloc_caller: caller,
                        free_caller: None,
                        realloc_of,
                    });
                    h.by_id.insert(alloc, i);
                    h.by_start.entry(addr).or_default().push(i);
                    h.max_size = h.max_size.max(size.max(1));
                }
                EventKind::Free { alloc, caller, .. } => {
                    if let Some(&i) = h.by_id.get(&alloc) {
                        let a = &mut h.allocations[i];
                        if a.free_event.is_none() {
                            a.free_event = Some(e.id);
                            a.free_caller = Some(caller);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(h)
    }

    /// All allocations, in allocation order.
    #[must_use]
    pub fn allocations(&self) -> &[Allocation] {
        &self.allocations
    }

    /// Look up by id.
    #[must_use]
    pub fn get(&self, id: AllocId) -> Option<&Allocation> {
        self.by_id.get(&id).map(|&i| &self.allocations[i])
    }

    /// Every allocation that ever covered `addr`, in allocation order.
    #[must_use]
    pub fn history(&self, addr: u64) -> Vec<&Allocation> {
        let lo = addr.saturating_sub(self.max_size.saturating_sub(1));
        let mut out: Vec<&Allocation> = self
            .by_start
            .range(lo..=addr)
            .flat_map(|(_, v)| v.iter().map(|&i| &self.allocations[i]))
            .filter(|a| a.contains(addr))
            .collect();
        out.sort_by_key(|a| a.alloc_event);
        out
    }

    /// What `addr` referred to at event `at`.
    #[must_use]
    pub fn resolve(&self, addr: u64, at: EventId) -> Resolution<'_> {
        let mut latest: Option<&Allocation> = None;
        for a in self.history(addr) {
            if a.alloc_event > at {
                break;
            }
            if a.live_at(at) {
                return Resolution::Live {
                    allocation: a,
                    offset: addr - a.addr,
                };
            }
            latest = Some(a);
        }
        latest.map_or(Resolution::Unknown, |a| Resolution::Freed {
            allocation: a,
            offset: addr - a.addr,
        })
    }

    /// Start addresses allocated more than once: the address-reuse sites that
    /// address-only tracing conflates.
    #[must_use]
    pub fn reused_addresses(&self) -> Vec<u64> {
        self.by_start
            .iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(a, _)| *a)
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unnecessary_wraps)]
mod tests {
    use super::*;

    fn alloc(id: u64, a: u64, addr: u64, size: u64, epoch: u32) -> Result<Event, ()> {
        Ok(Event {
            id: EventId(id),
            rr: None,
            kind: EventKind::Alloc {
                alloc: AllocId(a),
                addr,
                size,
                epoch: Epoch(epoch),
                kind: AllocKind::Malloc,
                realloc_of: None,
                caller: 0,
            },
        })
    }
    fn free(id: u64, a: u64, addr: u64, epoch: u32) -> Result<Event, ()> {
        Ok(Event {
            id: EventId(id),
            rr: None,
            kind: EventKind::Free {
                alloc: AllocId(a),
                addr,
                epoch: Epoch(epoch),
                caller: 0,
            },
        })
    }

    #[test]
    fn same_address_resolves_to_different_objects_over_time() {
        let h = TemporalHeap::from_events([
            alloc(0, 41, 0x1000, 32, 1),
            free(1, 41, 0x1000, 1),
            alloc(2, 79, 0x1000, 64, 2),
        ])
        .unwrap();
        let at = |e| match h.resolve(0x1010, EventId(e)) {
            Resolution::Live { allocation, .. } => format!("live #{}", allocation.id.0),
            Resolution::Freed { allocation, .. } => format!("freed #{}", allocation.id.0),
            Resolution::Unknown => "unknown".into(),
        };
        assert_eq!(at(0), "live #41");
        assert_eq!(at(1), "freed #41");
        assert_eq!(at(2), "live #79");
        assert_eq!(h.history(0x1010).len(), 2);
        assert_eq!(h.reused_addresses(), vec![0x1000]);
    }

    #[test]
    fn interior_and_out_of_range_addresses() {
        let h =
            TemporalHeap::from_events([alloc(0, 1, 0x1000, 16, 1), alloc(1, 2, 0x2000, 4096, 1)])
                .unwrap();
        assert!(matches!(
            h.resolve(0x100f, EventId(5)),
            Resolution::Live { offset: 15, .. }
        ));
        assert_eq!(h.resolve(0x1010, EventId(5)), Resolution::Unknown);
        assert!(matches!(
            h.resolve(0x2fff, EventId(5)),
            Resolution::Live { offset: 4095, .. }
        ));
        assert_eq!(
            h.resolve(0x1000, EventId(0)).clone(),
            h.resolve(0x1000, EventId(0))
        );
    }
}
