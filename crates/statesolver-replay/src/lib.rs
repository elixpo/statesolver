//! rr-backed replay for StateSolver.
//!
//! [`session::Session`] runs `rr replay -s <port>` and drives it over the gdb
//! remote serial protocol ([`rsp`]): forward and reverse execution,
//! breakpoints, hardware watchpoints, registers, memory, and rr's event
//! position. [`index::build`] derives allocation lifetimes and epochs by
//! replaying, so nothing beyond rr's own recording is captured.
//!
//! Linux x86-64 only; requires rr and hardware performance counters.

#![cfg(target_os = "linux")]

pub mod index;
pub mod rsp;
pub mod session;

pub use session::{Session, Stop};

/// Replay errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// I/O.
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    /// Unexpected reply or state.
    #[error("rr protocol: {0}")]
    Protocol(String),
    /// Memory read failed.
    #[error("address {0:#x} is not mapped at this point in the replay")]
    Unmapped(u64),
    /// Symbol resolution failed.
    #[error("symbols: {0}")]
    Symbols(String),
}

/// Convenience alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;
