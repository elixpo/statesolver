//! Causal primitives over a replay: who last wrote a memory location, and
//! which instruction last produced a register value.
//!
//! Semantics established on rr 5.9 (see DISCOVERIES.md): a reverse-continue
//! that hits a write watchpoint stops **on** the writing instruction, before
//! it executes, so the writer's inputs (address and source registers) can be
//! read at the stop, and one forward step shows the written value.

use iced_x86::{
    Decoder, DecoderOptions, Formatter, Instruction, InstructionInfoFactory, IntelFormatter,
    OpAccess, Register,
};
use serde::Serialize;
use statesolver_trace::{Reg, RrPos};

use crate::Result;
use crate::session::{Session, Stop};

/// One decoded instruction at a position.
#[derive(Debug, Clone, Serialize)]
pub struct Insn {
    /// Address.
    pub pc: u64,
    /// Intel-syntax text.
    pub text: String,
    /// Registers the instruction reads (including address registers).
    pub reads: Vec<String>,
    /// Registers it writes.
    pub writes: Vec<String>,
    /// Registers used to compute its memory operand address, if any.
    pub address_regs: Vec<String>,
}

/// A located write to memory.
#[derive(Debug, Clone, Serialize)]
pub struct MemWrite {
    /// Position of the writing instruction (before it executes).
    pub pos: RrPos,
    /// The instruction.
    pub insn: Insn,
    /// Bytes at the watched location before the write.
    pub before: Vec<u8>,
    /// Bytes after the write.
    pub after: Vec<u8>,
}

/// A located register definition.
#[derive(Debug, Clone, Serialize)]
pub struct RegDef {
    /// Position of the defining instruction (before it executes).
    pub pos: RrPos,
    /// The instruction.
    pub insn: Insn,
    /// Register value after the instruction.
    pub value: u64,
}

fn full_reg(r: Register) -> Register {
    r.full_register()
}

fn reg_name(r: Register) -> String {
    format!("{r:?}").to_lowercase()
}

/// Decode the instruction at `pc` from replay memory.
///
/// # Errors
/// Fails if the code cannot be read.
pub fn decode_at(s: &mut Session, pc: u64) -> Result<Insn> {
    let bytes = s.mem(pc, 15).or_else(|_| s.mem(pc, 8))?;
    let mut d = Decoder::with_ip(64, &bytes, pc, DecoderOptions::NONE);
    let ins: Instruction = d.decode();
    let mut text = String::new();
    IntelFormatter::new().format(&ins, &mut text);
    let mut factory = InstructionInfoFactory::new();
    let info = factory.info(&ins);
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    for u in info.used_registers() {
        let name = reg_name(full_reg(u.register()));
        match u.access() {
            OpAccess::Read | OpAccess::CondRead => reads.push(name),
            OpAccess::Write | OpAccess::CondWrite => writes.push(name),
            OpAccess::ReadWrite | OpAccess::ReadCondWrite => {
                reads.push(name.clone());
                writes.push(name);
            }
            _ => {}
        }
    }
    let mut address_regs = Vec::new();
    for r in [ins.memory_base(), ins.memory_index()] {
        if r != Register::None && r != Register::RIP {
            address_regs.push(reg_name(full_reg(r)));
        }
    }
    reads.sort();
    reads.dedup();
    writes.sort();
    writes.dedup();
    Ok(Insn {
        pc,
        text,
        reads,
        writes,
        address_regs,
    })
}

/// The most recent write to `[addr, addr+len)` strictly before `at`.
/// `len` must be 1, 2, 4, or 8 and naturally aligned (one debug register).
/// Returns `None` if nothing in the recording wrote it.
///
/// # Errors
/// Fails on replay errors.
pub fn last_writer(s: &mut Session, addr: u64, len: u8, at: RrPos) -> Result<Option<MemWrite>> {
    s.goto(at)?;
    s.set_write_watch(addr, len)?;
    let found = loop {
        match s.reverse_cont()? {
            Stop::Watchpoint { .. } => break true,
            Stop::Begin | Stop::Exited(_) | Stop::Killed(_) => break false,
            // rr re-reports a pending signal at the starting position.
            _ => {}
        }
    };
    s.remove_write_watch(addr, len)?;
    if !found {
        return Ok(None);
    }
    let pos = s.position()?;
    let insn = decode_at(s, pos.pc)?;
    let before = s.mem(addr, usize::from(len))?;
    s.step()?;
    let after = s.mem(addr, usize::from(len))?;
    s.reverse_step()?;
    Ok(Some(MemWrite {
        pos,
        insn,
        before,
        after,
    }))
}

/// The most recent instruction before `at` that wrote register `reg`,
/// searching at most `max_steps` instructions backward.
///
/// # Errors
/// Fails on replay errors.
pub fn reg_producer(
    s: &mut Session,
    reg: Reg,
    at: RrPos,
    max_steps: u32,
) -> Result<Option<RegDef>> {
    s.goto(at)?;
    let want = reg.name();
    for _ in 0..max_steps {
        if matches!(s.reverse_step()?, Stop::Begin) {
            return Ok(None);
        }
        let pos = s.position()?;
        let insn = decode_at(s, pos.pc)?;
        if insn.writes.iter().any(|w| w == want) {
            s.step()?;
            let value = s.reg(reg)?;
            s.reverse_step()?;
            return Ok(Some(RegDef { pos, insn, value }));
        }
    }
    Ok(None)
}
