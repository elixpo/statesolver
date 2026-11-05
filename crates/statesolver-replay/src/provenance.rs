//! Provenance chains: where did a value come from?
//!
//! Starting from a register or memory location at a position, follow
//! producers backward: a register to the instruction that wrote it, a load to
//! the memory it read, a memory location to its last writer and that writer's
//! source register. The chain stops at an allocation return (the value *is* a
//! fresh heap pointer), syscall input, a constant, or a step budget.
//!
//! With allocation identity, the end of a pointer's chain says which
//! **allocation epoch** produced it. Comparing that with the allocation that
//! owns the address when the pointer is used is the epoch check that
//! address-only tracing cannot make.

use iced_x86::{Decoder, DecoderOptions, Instruction, Register};
use serde::Serialize;
use statesolver_trace::{AllocId, Epoch, Event, EventKind, Reg, RrPos};

use crate::Result;
use crate::queries::{self, Insn};
use crate::session::{Session, Stop};

/// Where a value currently lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Carrier {
    /// A register.
    Reg {
        /// Register name.
        reg: Reg,
    },
    /// A memory location.
    Mem {
        /// Address.
        addr: u64,
        /// Width in bytes (1, 2, 4, or 8).
        len: u8,
    },
}

/// One step of a chain: the value was produced here.
#[derive(Debug, Clone, Serialize)]
pub struct Hop {
    /// Position of the producing instruction (before it executes).
    pub pos: RrPos,
    /// The instruction.
    pub insn: Insn,
    /// What carried the value into this instruction's output.
    pub into: Carrier,
    /// What the instruction took the value from (the next link).
    pub from: Option<Carrier>,
}

/// Where a chain ended.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Origin {
    /// The value is a pointer returned by this allocation.
    Allocation {
        /// Allocation id.
        alloc: AllocId,
        /// Its epoch.
        epoch: Epoch,
        /// Its start address.
        addr: u64,
        /// Position of the allocation return.
        pos: RrPos,
    },
    /// The value came from a syscall result (external input).
    Syscall {
        /// Position of the syscall instruction.
        pos: RrPos,
    },
    /// The value is an immediate or address computed from the stack or code.
    Constant {
        /// Position of the instruction.
        pos: RrPos,
    },
    /// The chain could not be followed further.
    Unknown {
        /// Why.
        reason: String,
    },
}

/// A provenance chain.
#[derive(Debug, Clone, Serialize)]
pub struct Chain {
    /// Hops from the query backward.
    pub hops: Vec<Hop>,
    /// Where it ended.
    pub origin: Origin,
}

fn to_reg(r: Register) -> Option<Reg> {
    Reg::from_name(&format!("{:?}", r.full_register()).to_lowercase())
}

fn decode(s: &mut Session, pc: u64) -> Result<Instruction> {
    let bytes = s.mem(pc, 15).or_else(|_| s.mem(pc, 8))?;
    Ok(Decoder::with_ip(64, &bytes, pc, DecoderOptions::NONE).decode())
}

/// Address of the instruction's memory operand at the current position.
fn mem_operand(s: &mut Session, ins: &Instruction) -> Result<Option<(u64, u8)>> {
    let Some(op) = (0..ins.op_count()).find(|&i| ins.op_kind(i) == iced_x86::OpKind::Memory) else {
        return Ok(None);
    };
    let mut vals = std::collections::HashMap::new();
    for r in [ins.memory_base(), ins.memory_index()] {
        if let Some(reg) = to_reg(r) {
            vals.insert(r, s.reg(reg)?);
        }
    }
    let addr = ins.virtual_address(op, 0, |r, _, _| match r {
        Register::RIP => Some(ins.next_ip()),
        // In 64-bit mode the DS/ES/SS/CS bases are zero (FS/GS are not
        // supported yet and make the address unknown).
        Register::None | Register::DS | Register::ES | Register::SS | Register::CS => Some(0),
        _ => vals.get(&r).copied(),
    });
    let len = u8::try_from(ins.memory_size().size().clamp(1, 8)).unwrap_or(8);
    Ok(addr.map(|a| (a, len)))
}

/// Where a register's value came from, for the instruction that wrote it.
///
/// Pointer arithmetic keeps provenance: `add rdi, rdx` and `lea rdi,
/// [rax+8]` continue with the pointer (`rdi`, `rax`), not the offset. A
/// `lea` based on `rsp`/`rbp` or `rip` produces a stack or code address,
/// which is a constant origin.
fn next_carrier(
    s: &mut Session,
    ins: &Instruction,
    insn: &Insn,
    reg: Reg,
) -> Result<Option<Carrier>> {
    if insn.text.starts_with("lea") {
        return Ok(to_reg(ins.memory_base())
            .filter(|r| !matches!(r, Reg::Rsp | Reg::Rbp | Reg::Rip))
            .map(|reg| Carrier::Reg { reg }));
    }
    // Read-modify-write arithmetic on the tracked register keeps the
    // pointer (`add rdi, 8`, `add rdi, [mem]`). A `mov` that merely uses the
    // register as an address (`mov rax, [rax]`) is a load, not arithmetic.
    let arithmetic = matches!(
        ins.mnemonic(),
        iced_x86::Mnemonic::Add
            | iced_x86::Mnemonic::Sub
            | iced_x86::Mnemonic::Adc
            | iced_x86::Mnemonic::Sbb
            | iced_x86::Mnemonic::Or
            | iced_x86::Mnemonic::And
            | iced_x86::Mnemonic::Inc
            | iced_x86::Mnemonic::Dec
            | iced_x86::Mnemonic::Neg
            | iced_x86::Mnemonic::Imul
            | iced_x86::Mnemonic::Xadd
    );
    let rmw = arithmetic && insn.reads.iter().any(|r| r == reg.name());
    if let Some((addr, len)) = mem_operand(s, ins)? {
        // `add rdi, [mem]`: pointer arithmetic with two sources. Follow the
        // one that holds a pointer-like value, preferring the register.
        if rmw {
            let reg_val = s.reg(reg)?;
            let mem_val = s.read_u64(addr).unwrap_or(0);
            let pointer_like = |v: u64| (0x1_0000..0x8000_0000_0000).contains(&v);
            if pointer_like(reg_val) || !pointer_like(mem_val) {
                return Ok(Some(Carrier::Reg { reg }));
            }
        }
        return Ok(Some(Carrier::Mem { addr, len }));
    }
    // Read-modify-write of the tracked register (add, sub, and, inc, ...).
    if rmw {
        return Ok(Some(Carrier::Reg { reg }));
    }
    Ok(insn
        .reads
        .iter()
        .filter(|r| *r != reg.name() && !r.starts_with("rflags") && *r != "rsp")
        .find_map(|r| Reg::from_name(r))
        .map(|reg| Carrier::Reg { reg }))
}

/// For a store from vector register `v` at `at`, find the vector load that
/// filled `v` and return the source bytes that ended up `offset` bytes into
/// the stored data (memcpy-style SIMD copies).
fn vector_source(
    s: &mut Session,
    v: Register,
    at: RrPos,
    offset: u64,
    len: u8,
    max_steps: u32,
) -> Result<Option<Carrier>> {
    let full = format!("{:?}", v.full_register()).to_lowercase();
    s.goto(at)?;
    for _ in 0..max_steps {
        if matches!(s.reverse_step()?, Stop::Begin) {
            return Ok(None);
        }
        let pos = s.position()?;
        let insn = queries::decode_at(s, pos.pc)?;
        if insn.writes.contains(&full) {
            let ins = decode(s, pos.pc)?;
            return Ok(mem_operand(s, &ins)?.map(|(src, _)| Carrier::Mem {
                addr: src + offset,
                len,
            }));
        }
    }
    Ok(None)
}

/// Index events that are allocations, for matching allocation returns.
fn allocations(index: &[Event]) -> Vec<(RrPos, AllocId, Epoch, u64)> {
    index
        .iter()
        .filter_map(|e| match (&e.kind, e.rr) {
            (
                EventKind::Alloc {
                    alloc, addr, epoch, ..
                },
                Some(pos),
            ) => Some((pos, *alloc, *epoch, *addr)),
            _ => None,
        })
        .collect()
}

/// Follow `start` back from `at` for at most `max_hops` hops; each register
/// search looks back at most `max_steps` instructions.
///
/// # Errors
/// Fails on replay errors.
#[allow(clippy::too_many_lines)]
pub fn trace(
    s: &mut Session,
    index: &[Event],
    start: Carrier,
    at: RrPos,
    max_hops: usize,
    max_steps: u32,
) -> Result<Chain> {
    let allocs = allocations(index);
    let mut hops = Vec::new();
    let mut carrier = start;
    let mut pos = at;
    for _ in 0..max_hops {
        match carrier {
            Carrier::Reg { reg } => {
                // An allocation return delivering this register's value?
                s.goto(pos)?;
                let value = s.reg(reg)?;
                if reg == Reg::Rax
                    && let Some(&(p, alloc, epoch, addr)) =
                        allocs.iter().find(|a| a.0 == pos && a.3 == value)
                {
                    return Ok(Chain {
                        hops,
                        origin: Origin::Allocation {
                            alloc,
                            epoch,
                            addr,
                            pos: p,
                        },
                    });
                }
                let Some(def) = producer_or_alloc(s, reg, pos, max_steps, &allocs)? else {
                    return Ok(Chain {
                        hops,
                        origin: Origin::Unknown {
                            reason: format!(
                                "no writer of {} within {max_steps} instructions",
                                reg.name()
                            ),
                        },
                    });
                };
                let (dpos, insn) = match def {
                    Ok(found) => found,
                    Err(origin) => return Ok(Chain { hops, origin }),
                };
                s.goto(dpos)?;
                let ins = decode(s, dpos.pc)?;
                if insn.text.starts_with("syscall") {
                    hops.push(Hop {
                        pos: dpos,
                        insn,
                        into: carrier,
                        from: None,
                    });
                    return Ok(Chain {
                        hops,
                        origin: Origin::Syscall { pos: dpos },
                    });
                }
                let from = next_carrier(s, &ins, &insn, reg)?;
                hops.push(Hop {
                    pos: dpos,
                    insn,
                    into: carrier,
                    from,
                });
                match from {
                    Some(c) => {
                        carrier = c;
                        pos = dpos;
                    }
                    None => {
                        return Ok(Chain {
                            hops,
                            origin: Origin::Constant { pos: dpos },
                        });
                    }
                }
            }
            Carrier::Mem { addr, len } => {
                let aligned = addr & !u64::from(len.max(1) - 1);
                let Some(mut w) = queries::last_writer(s, aligned, len, pos)? else {
                    return Ok(Chain {
                        hops,
                        origin: Origin::Unknown {
                            reason: format!("{addr:#x} was not written during the recording"),
                        },
                    });
                };
                // Loop collapse: if this location is rewritten by an instruction
                // already on the chain (a loop counter or cursor, `p = p + 1`),
                // skip every write by the loop's instructions back to the write
                // that started it.
                let loop_pcs: Vec<u64> = hops.iter().map(|h| h.pos.pc).collect();
                // A cursor step writes "old value plus a small delta" (c++, p += 8):
                // the value derives from the location itself, so skip it too.
                let is_step = |w: &queries::MemWrite| {
                    let word = |b: &[u8]| {
                        let mut v = [0u8; 8];
                        v[..b.len().min(8)].copy_from_slice(&b[..b.len().min(8)]);
                        i64::from_le_bytes(v)
                    };
                    let d = word(&w.after).wrapping_sub(word(&w.before));
                    d != 0 && d.abs() <= 4096
                };
                if loop_pcs.contains(&w.pos.pc) || is_step(&w) {
                    let mut skipped = 0u32;
                    while (loop_pcs.contains(&w.pos.pc) || is_step(&w)) && skipped < 100_000 {
                        match queries::last_writer(s, aligned, len, w.pos)? {
                            Some(prev) => w = prev,
                            None => break,
                        }
                        skipped += 1;
                    }
                    if let Some(last) = hops.last_mut() {
                        last.insn.text = format!(
                            "{}   [skipped {skipped} earlier loop or cursor-step writes]",
                            last.insn.text
                        );
                    }
                }
                s.goto(w.pos)?;
                let ins = decode(s, w.pos.pc)?;
                if w.insn.text.starts_with("syscall") {
                    hops.push(Hop {
                        pos: w.pos,
                        insn: w.insn.clone(),
                        into: carrier,
                        from: None,
                    });
                    return Ok(Chain {
                        hops,
                        origin: Origin::Syscall { pos: w.pos },
                    });
                }
                // The stored value's source: the register operand of the store,
                // or, for SIMD copies, the matching bytes of the vector load.
                let src_reg = (0..ins.op_count())
                    .filter(|&i| ins.op_kind(i) == iced_x86::OpKind::Register)
                    .map(|i| ins.op_register(i))
                    .next();
                let src = match src_reg {
                    Some(r) if to_reg(r).is_some() => to_reg(r).map(|reg| Carrier::Reg { reg }),
                    Some(r) if r.is_xmm() || r.is_ymm() || r.is_zmm() => {
                        let offset =
                            mem_operand(s, &ins)?.map_or(0, |(dst, _)| addr.saturating_sub(dst));
                        vector_source(s, r, w.pos, offset, len, max_steps)?
                    }
                    _ => None,
                };
                hops.push(Hop {
                    pos: w.pos,
                    insn: w.insn.clone(),
                    into: carrier,
                    from: src,
                });
                match src {
                    Some(c) => {
                        carrier = c;
                        pos = w.pos;
                    }
                    None => {
                        return Ok(Chain {
                            hops,
                            origin: Origin::Constant { pos: w.pos },
                        });
                    }
                }
            }
        }
    }
    Ok(Chain {
        hops,
        origin: Origin::Unknown {
            reason: format!("stopped after {max_hops} hops"),
        },
    })
}

/// Search backward for the writer of `reg`, stopping early if the search
/// crosses an indexed allocation return that delivered the value in `rax`.
#[allow(clippy::type_complexity)]
fn producer_or_alloc(
    s: &mut Session,
    reg: Reg,
    at: RrPos,
    max_steps: u32,
    allocs: &[(RrPos, AllocId, Epoch, u64)],
) -> Result<Option<std::result::Result<(RrPos, Insn), Origin>>> {
    s.goto(at)?;
    let target = s.reg(reg)?;
    let want = reg.name();
    for _ in 0..max_steps {
        let here = s.position()?;
        if reg == Reg::Rax
            && let Some(&(p, alloc, epoch, addr)) =
                allocs.iter().find(|a| a.0 == here && a.3 == target)
        {
            return Ok(Some(Err(Origin::Allocation {
                alloc,
                epoch,
                addr,
                pos: p,
            })));
        }
        if matches!(s.reverse_step()?, Stop::Begin) {
            return Ok(None);
        }
        let pos = s.position()?;
        let insn = queries::decode_at(s, pos.pc)?;
        if insn.writes.iter().any(|w| w == want) {
            return Ok(Some(Ok((pos, insn))));
        }
    }
    Ok(None)
}
