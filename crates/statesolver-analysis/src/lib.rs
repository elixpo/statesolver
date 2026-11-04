//! Allocation-epoch-aware temporal object graph, query-driven causal
//! reconstruction over replay, and the crash explainer.
//!
//! Tracked in Stage 4 (elixpo/statesolver#4). The temporal heap needs only
//! the trace, so it is built first.

pub mod heap;

pub use heap::{Allocation, Resolution, TemporalHeap};
