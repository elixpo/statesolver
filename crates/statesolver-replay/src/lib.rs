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

/// On hybrid Intel CPUs (P-cores and E-cores), rr's branch counter only
/// works on the performance cores, and rr binds to a random CPU by default:
/// on an E-core it aborts with "Got 0 branch events". Returns a performance
/// core to pass as `rr record --bind-to-cpu`, or `None` on non-hybrid
/// machines. Replay runs on the CPU stored in the trace, so only recording
/// needs it.
#[must_use]
pub fn performance_core() -> Option<u32> {
    let list = std::fs::read_to_string("/sys/devices/cpu_core/cpus").ok()?;
    let first = list.trim().split([',', '-']).next()?;
    first.parse().ok()
}

/// Arguments to put after `rr record` for a reliable recording on this machine.
#[must_use]
pub fn rr_record_args() -> Vec<String> {
    performance_core()
        .map(|c| vec![format!("--bind-to-cpu={c}")])
        .unwrap_or_default()
}

/// Convenience alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;
