//! Coroutine runtime that migrates live activation state between contiguous,
//! segmented, and heap-frame representations.
//!
//! This is the only crate that will permit `unsafe` code, starting in Stage 4
//! (elixpo/morphstack#4). Until then it inherits the workspace `forbid`.
