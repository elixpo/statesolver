//! The `.ss` trace format: a global event timeline with register deltas,
//! memory-map changes, syscall results, signals, and allocation lifetimes
//! identified by `{id, address, size, epoch}`.
//!
//! Tracked in Stage 1 (elixpo/statesolver#1).
