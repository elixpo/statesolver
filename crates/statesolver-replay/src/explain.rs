//! `explain-crash`: from a recorded crash to an evidence-backed root cause.
//!
//! The central rule, applied to the write that produced the bad value:
//! compare the allocation the write's **address pointer came from** (its
//! provenance origin) with the allocation that **owned the written address**
//! at that moment.
//!
//! | pointer origin | owner at write time | verdict |
//! |---|---|---|
//! | same allocation | same allocation | in-bounds write |
//! | freed allocation | another allocation at the same address | use-after-free with address reuse |
//! | freed allocation | none | use-after-free |
//! | live allocation | a different allocation | out-of-bounds write into a neighbour |
//!
//! In address-only mode (study arm A5) the origin is not consulted, so the
//! explanation stops at "this instruction wrote this address".
//!
//! Every statement is evidence with an id: `F*` facts read directly from the
//! replay or index, `I*` inferences made by this rule.

use serde::Serialize;
use statesolver_trace::{AllocId, Epoch, Event, EventKind, Reg, RrPos};

use crate::Result;
use crate::provenance::{self, Carrier, Chain, Origin};
use crate::queries::{self, Insn};
use crate::session::Session;

/// Evidence tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Tier {
    /// Read directly from the replay or the allocation index.
    Fact,
    /// Derived by the engine's rules from facts.
    Inference,
}

/// One cited statement.
#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    /// Id to cite (`F3`, `I1`).
    pub id: String,
    /// Tier.
    pub tier: Tier,
    /// The statement.
    pub claim: String,
    /// Replay position it refers to, if any (re-checkable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<RrPos>,
    /// Ids of the evidence an inference rests on.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub because: Vec<String>,
}

/// A crash explanation.
#[derive(Debug, Clone, Serialize)]
pub struct Explanation {
    /// Bug class.
    pub classification: String,
    /// One-paragraph summary citing evidence ids.
    pub summary: String,
    /// All evidence, in order.
    pub evidence: Vec<Evidence>,
    /// Provenance chain of the corrupted value, if traced.
    pub value_chain: Option<Chain>,
    /// Provenance chain of the pointer used by the corrupting write.
    pub pointer_chain: Option<Chain>,
}

/// Which allocation owned an address at a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Owner {
    /// Allocation.
    pub alloc: AllocId,
    /// Its epoch.
    pub epoch: Epoch,
    /// Start address.
    pub addr: u64,
    /// Size.
    pub size: u64,
}

fn key(p: RrPos) -> (u64, u64) {
    (p.event, p.ticks)
}

fn alloc_size(index: &[Event], alloc: AllocId) -> u64 {
    index
        .iter()
        .find_map(|e| match e.kind {
            EventKind::Alloc { alloc: a, size, .. } if a == alloc => Some(size),
            _ => None,
        })
        .unwrap_or(0)
}

/// The allocation live at `addr` at `pos` (by index order).
#[must_use]
pub fn owner_at(index: &[Event], addr: u64, pos: RrPos) -> Option<Owner> {
    let mut live: Vec<Owner> = Vec::new();
    for e in index {
        let Some(p) = e.rr else { continue };
        if key(p) > key(pos) {
            break;
        }
        match e.kind {
            EventKind::Alloc {
                alloc,
                addr,
                size,
                epoch,
                ..
            } => live.push(Owner {
                alloc,
                epoch,
                addr,
                size,
            }),
            EventKind::Free { alloc, .. } => live.retain(|o| o.alloc != alloc),
            _ => {}
        }
    }
    live.into_iter()
        .find(|o| addr >= o.addr && addr < o.addr + o.size.max(1))
}

/// The index event that freed `alloc` before `pos`, if any.
#[must_use]
pub fn freed_before(index: &[Event], alloc: AllocId, pos: RrPos) -> Option<&Event> {
    index.iter().find(|e| {
        matches!(e.kind, EventKind::Free { alloc: a, .. } if a == alloc)
            && e.rr.is_some_and(|p| key(p) <= key(pos))
    })
}

struct Book {
    evidence: Vec<Evidence>,
    facts: usize,
    infs: usize,
}

impl Book {
    fn fact(&mut self, claim: String, at: Option<RrPos>) -> String {
        self.facts += 1;
        let id = format!("F{}", self.facts);
        self.evidence.push(Evidence {
            id: id.clone(),
            tier: Tier::Fact,
            claim,
            at,
            because: vec![],
        });
        id
    }
    fn infer(&mut self, claim: String, because: Vec<String>) -> String {
        self.infs += 1;
        let id = format!("I{}", self.infs);
        self.evidence.push(Evidence {
            id: id.clone(),
            tier: Tier::Inference,
            claim,
            at: None,
            because,
        });
        id
    }
}

fn ev_id(index: &[Event], pos: RrPos) -> String {
    index.iter().find(|e| e.rr == Some(pos)).map_or_else(
        || format!("rr {}:{}", pos.event, pos.ticks),
        |e| format!("E{}", e.id.0),
    )
}

/// Explain the recorded crash.
///
/// # Errors
/// Fails on replay errors.
#[allow(clippy::too_many_lines)]
pub fn explain_crash(s: &mut Session, index: &[Event], address_only: bool) -> Result<Explanation> {
    let mut b = Book {
        evidence: vec![],
        facts: 0,
        infs: 0,
    };
    let crash = index.iter().rev().find_map(|e| match (&e.kind, e.rr) {
        (
            EventKind::Signal {
                signo, fatal: true, ..
            },
            Some(p),
        ) => Some((*signo, p)),
        _ => None,
    });
    let Some((signo, crash)) = crash else {
        return Ok(Explanation {
            classification: "no crash".into(),
            summary: "The recording ends without a fatal signal.".into(),
            evidence: vec![],
            value_chain: None,
            pointer_chain: None,
        });
    };

    // Double free: glibc aborts inside the second free of a dead allocation.
    if signo == 6
        && let Some(explanation) = explain_double_free(s, index, &mut b, crash)?
    {
        return Ok(explanation);
    }

    s.goto(crash)?;
    let insn = queries::decode_at(s, crash.pc)?;
    let f_crash = b.fact(
        format!(
            "Signal {signo} at pc {:#x} executing `{}`",
            crash.pc, insn.text
        ),
        Some(crash),
    );

    // The bad value: an indirect call/jump target, or a dereferenced pointer.
    let target = crash_carrier(&insn);
    let Some(start) = target else {
        return Ok(Explanation {
            classification: "unclassified crash".into(),
            summary: format!(
                "Signal {signo} at {:#x} ({f_crash}); the faulting operand is not traced yet.",
                crash.pc
            ),
            evidence: b.evidence,
            value_chain: None,
            pointer_chain: None,
        });
    };
    let value_chain = provenance::trace(s, index, start, crash, 12, 4096)?;
    // The last write to memory on the value's chain is the corrupting write.
    let write = value_chain
        .hops
        .iter()
        .find(|h| matches!(h.into, Carrier::Mem { .. }));
    let Some(write) = write.cloned() else {
        let summary = format!(
            "The faulting value ({f_crash}) was not stored in memory on its way; see the value chain."
        );
        return Ok(Explanation {
            classification: "corrupted register value".into(),
            summary,
            evidence: b.evidence,
            value_chain: Some(value_chain),
            pointer_chain: None,
        });
    };
    let Carrier::Mem { addr: waddr, .. } = write.into else {
        unreachable!("filtered on Mem")
    };
    let f_write = b.fact(
        format!(
            "`{}` at {} wrote the bad value to {waddr:#x}",
            write.insn.text,
            ev_id(index, write.pos)
        ),
        Some(write.pos),
    );
    let owner = owner_at(index, waddr, write.pos);
    let f_owner = b.fact(
        owner.map_or_else(
            || format!("At that moment {waddr:#x} belonged to no live heap allocation"),
            |o| format!(
                "At that moment {waddr:#x} belonged to Allocation #{} (epoch {}, {} bytes at {:#x}, offset {})",
                o.alloc.0, o.epoch.0, o.size, o.addr, waddr - o.addr
            ),
        ),
        Some(write.pos),
    );
    if let Origin::Syscall { pos } = value_chain.origin {
        b.fact(
            format!(
                "The written value originates from syscall input at {}",
                ev_id(index, pos)
            ),
            Some(pos),
        );
    }

    if address_only {
        let summary = format!(
            "Crash ({f_crash}) on a value last written by `{}` ({f_write}) into {waddr:#x} ({f_owner}).",
            write.insn.text
        );
        return Ok(Explanation {
            classification: "corrupted memory (address-only analysis)".into(),
            summary,
            evidence: b.evidence,
            value_chain: Some(value_chain),
            pointer_chain: None,
        });
    }

    // Lifetime-aware step: where did the writer's address pointer come from?
    let Some(areg) = write
        .insn
        .address_regs
        .first()
        .and_then(|r| Reg::from_name(r))
    else {
        return Ok(Explanation {
            classification: "corrupted memory".into(),
            summary: format!(
                "Crash ({f_crash}) on a value written by {f_write}; the store address is not register-based."
            ),
            evidence: b.evidence,
            value_chain: Some(value_chain),
            pointer_chain: None,
        });
    };
    let pointer_chain =
        provenance::trace(s, index, Carrier::Reg { reg: areg }, write.pos, 16, 4096)?;
    let (classification, summary) = match (&pointer_chain.origin, owner) {
        (
            Origin::Allocation {
                alloc,
                epoch,
                addr,
                pos,
            },
            owner,
        ) => {
            let f_origin = b.fact(
                format!(
                    "The pointer `{}` used by that write was produced by Allocation #{} (epoch {}) at {}",
                    areg.name(), alloc.0, epoch.0, ev_id(index, *pos)
                ),
                Some(*pos),
            );
            let freed = freed_before(index, *alloc, write.pos);
            let f_freed = freed.map(|e| {
                b.fact(
                    format!(
                        "Allocation #{} was freed at E{} before the write",
                        alloc.0, e.id.0
                    ),
                    e.rr,
                )
            });
            match (freed, owner) {
                (Some(_), Some(o)) if o.alloc != *alloc => {
                    let i = b.infer(
                        format!(
                            "Stale pointer: it belongs to Allocation #{} epoch {} (freed), but the address {addr:#x} had been reused by Allocation #{} epoch {}. The write corrupted the new object.",
                            alloc.0, epoch.0, o.alloc.0, o.epoch.0
                        ),
                        vec![f_origin.clone(), f_freed.clone().unwrap_or_default(), f_owner.clone(), f_write.clone()],
                    );
                    (
                        "use-after-free with address reuse".to_string(),
                        format!(
                            "Crash ({f_crash}). The bad value was written by {f_write} through a pointer from Allocation #{} epoch {} ({f_origin}), which had been freed ({}), into memory then owned by Allocation #{} epoch {} ({f_owner}). {i}",
                            alloc.0,
                            epoch.0,
                            f_freed.unwrap_or_default(),
                            o.alloc.0,
                            o.epoch.0
                        ),
                    )
                }
                (Some(_), None) => {
                    let i = b.infer(
                        format!("Use-after-free: the write went through a pointer to freed Allocation #{} epoch {}.", alloc.0, epoch.0),
                        vec![f_origin.clone(), f_freed.clone().unwrap_or_default(), f_owner.clone()],
                    );
                    (
                        "use-after-free".to_string(),
                        format!(
                            "Crash ({f_crash}) on a value written by {f_write} through a dangling pointer ({f_origin}, {}). {i}",
                            f_freed.unwrap_or_default()
                        ),
                    )
                }
                (None, Some(o)) if o.alloc != *alloc => {
                    let size = alloc_size(index, *alloc);
                    let off = waddr.saturating_sub(*addr);
                    let i = b.infer(
                        format!(
                            "Out-of-bounds write: a pointer into live Allocation #{} ({size} bytes) wrote at offset {off}, {} bytes past its end, into Allocation #{}.",
                            alloc.0,
                            off.saturating_sub(size),
                            o.alloc.0
                        ),
                        vec![f_origin.clone(), f_owner.clone(), f_write.clone()],
                    );
                    (
                        "heap buffer overflow".to_string(),
                        format!(
                            "Crash ({f_crash}). {f_write} wrote through a pointer derived from Allocation #{} ({f_origin}) into neighbouring Allocation #{} ({f_owner}). {i}",
                            alloc.0, o.alloc.0
                        ),
                    )
                }
                _ => (
                    "corrupted in-bounds data".to_string(),
                    format!(
                        "Crash ({f_crash}) on a value written in bounds by {f_write} ({f_origin}, {f_owner}); see the value chain for where the value came from."
                    ),
                ),
            }
        }
        (other, _) => (
            "corrupted memory".to_string(),
            format!(
                "Crash ({f_crash}) on a value written by {f_write} ({f_owner}); the writer's pointer origin is {other:?}."
            ),
        ),
    };
    Ok(Explanation {
        classification,
        summary,
        evidence: b.evidence,
        value_chain: Some(value_chain),
        pointer_chain: Some(pointer_chain),
    })
}

/// The operand holding the bad value at the crash.
fn crash_carrier(insn: &Insn) -> Option<Carrier> {
    let t = insn.text.as_str();
    if (t.starts_with("call") || t.starts_with("jmp"))
        && let Some(r) = t.split_whitespace().nth(1).and_then(Reg::from_name)
    {
        // `call rax`.
        return Some(Carrier::Reg { reg: r });
    }
    // A memory access through a bad pointer: trace the base register.
    insn.address_regs
        .first()
        .and_then(|r| Reg::from_name(r))
        .map(|reg| Carrier::Reg { reg })
}

fn explain_double_free(
    s: &mut Session,
    index: &[Event],
    b: &mut Book,
    crash: RrPos,
) -> Result<Option<Explanation>> {
    // The last free of an already-freed allocation.
    let mut freed: Vec<AllocId> = Vec::new();
    let mut second = None;
    for e in index {
        if let EventKind::Free { alloc, .. } = e.kind {
            if freed.contains(&alloc) {
                second = Some(e);
            }
            freed.push(alloc);
        }
    }
    let Some(second) = second else {
        return Ok(None);
    };
    let EventKind::Free {
        alloc, addr, epoch, ..
    } = second.kind
    else {
        return Ok(None);
    };
    let Some(pos) = second.rr else {
        return Ok(None);
    };
    let first = index
        .iter()
        .find(|e| matches!(e.kind, EventKind::Free { alloc: a, .. } if a == alloc))
        .expect("first free exists");
    let created = index
        .iter()
        .find(|e| matches!(e.kind, EventKind::Alloc { alloc: a, .. } if a == alloc));
    let f_abort = b.fact(
        "The process aborted (SIGABRT) inside the allocator".into(),
        Some(crash),
    );
    let f_alloc = created.map(|e| {
        b.fact(
            format!(
                "Allocation #{} (epoch {}) was created at E{} at {addr:#x}",
                alloc.0, epoch.0, e.id.0
            ),
            e.rr,
        )
    });
    let f_first = b.fact(
        format!("Allocation #{} was freed at E{}", alloc.0, first.id.0),
        first.rr,
    );
    let f_second = b.fact(
        format!(
            "free({addr:#x}) was called again at E{} on the already-freed Allocation #{}",
            second.id.0, alloc.0
        ),
        Some(pos),
    );
    let chain = provenance::trace(s, index, Carrier::Reg { reg: Reg::Rdi }, pos, 16, 4096)?;
    let i = b.infer(
        format!("Double free of Allocation #{}: the pointer passed to the second free traces back to that same allocation, so two owners released it.", alloc.0),
        [Some(f_first.clone()), Some(f_second.clone()), f_alloc.clone()].into_iter().flatten().collect(),
    );
    Ok(Some(Explanation {
        classification: "double free".into(),
        summary: format!(
            "The process aborted ({f_abort}) because free() was called on Allocation #{} ({f_second}) after it had already been freed ({f_first}). {i}",
            alloc.0
        ),
        evidence: std::mem::take(&mut b.evidence),
        value_chain: None,
        pointer_chain: Some(chain),
    }))
}
