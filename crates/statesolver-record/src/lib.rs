//! Records an unmodified x86-64 Linux program: process control through
//! ptrace, register deltas, memory maps, syscall results, and allocation
//! epochs.
//!
//! Tracked in Stage 2 (elixpo/statesolver#2). This crate (and
//! `statesolver-replay`) will be the only ones allowed `unsafe`.
