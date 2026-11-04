//! Benchmark harness and cost-model calibration.
//!
//! [`calibrate`] measures the cost-model parameters that do not need the
//! Stage 4 runtime: fresh-memory first-touch cost, copy bandwidth, stack
//! mapping and unmapping, and allocator costs for frames and segments. The
//! parameters that need a real multi-representation runtime (call overhead,
//! stack checks, heap-frame locality) keep their defaults and are reported as
//! *not measured*.
//!
//! Everything here is safe Rust: fresh memory comes from anonymous maps via
//! `memmap2`, so the workspace `unsafe_code = "forbid"` policy still holds.

use std::hint::black_box;
use std::time::Instant;

use memmap2::MmapMut;
use morphstack_cost::CostModel;
use serde::Serialize;

/// One measured parameter.
#[derive(Debug, Clone, Serialize)]
pub struct Measurement {
    /// Cost-model field, as `section.field`.
    pub parameter: &'static str,
    /// Measured value (median of repeats).
    pub value: f64,
    /// Smallest and largest repeat.
    pub min: f64,
    /// Largest repeat.
    pub max: f64,
    /// Unit.
    pub unit: &'static str,
    /// How it was measured.
    pub method: &'static str,
}

/// Calibration result: measured values and the profile built from them.
#[derive(Debug, Clone, Serialize)]
pub struct Calibration {
    /// Machine description.
    pub machine: String,
    /// Measured parameters.
    pub measurements: Vec<Measurement>,
    /// Parameters left at their defaults, with the reason.
    pub not_measured: Vec<(&'static str, &'static str)>,
    /// The calibrated profile.
    #[serde(skip)]
    pub profile: CostModel,
}

/// Run `f` `repeats` times; each run returns a per-operation cost in ns.
fn repeat(repeats: usize, mut f: impl FnMut() -> f64) -> (f64, f64, f64) {
    let mut v: Vec<f64> = (0..repeats).map(|_| f()).collect();
    v.sort_by(f64::total_cmp);
    (v[v.len() / 2], v[0], v[v.len() - 1])
}

#[allow(clippy::cast_precision_loss)]
fn per_op(start: Instant, ops: u64) -> f64 {
    start.elapsed().as_nanos() as f64 / ops as f64
}

const PAGE: usize = 4096;

/// Median, minimum, maximum.
type Spread = (f64, f64, f64);

/// First-touch cost per byte: map fresh anonymous memory and write one byte
/// per page, so every write takes a page fault and the kernel zeroes a page.
fn touch_ns_per_byte() -> std::io::Result<(f64, f64, f64)> {
    const BYTES: usize = 64 << 20;
    let mut err = None;
    let r = repeat(7, || {
        let mut map = match MmapMut::map_anon(BYTES) {
            Ok(m) => m,
            Err(e) => {
                err = Some(e);
                return f64::NAN;
            }
        };
        let start = Instant::now();
        for i in (0..BYTES).step_by(PAGE) {
            map[i] = 1;
        }
        black_box(&map);
        per_op(start, BYTES as u64)
    });
    err.map_or(Ok(r), Err)
}

/// Copy cost per byte for stack-sized blocks that fit in cache.
fn copy_ns_per_byte() -> (f64, f64, f64) {
    const BYTES: usize = 256 << 10;
    const ITERS: u64 = 2_000;
    let src = vec![7u8; BYTES];
    let mut dst = vec![0u8; BYTES];
    repeat(7, || {
        let start = Instant::now();
        for _ in 0..ITERS {
            dst.copy_from_slice(black_box(&src));
            black_box(&mut dst);
        }
        per_op(start, ITERS * BYTES as u64)
    })
}

/// Map and unmap a minimum-size stack (without touching it).
fn stack_map_unmap_ns(bytes: usize) -> std::io::Result<(Spread, Spread)> {
    const ITERS: u64 = 2_000;
    let mut maps = Vec::new();
    let mut alloc = Vec::new();
    let mut free = Vec::new();
    for _ in 0..7 {
        let start = Instant::now();
        for _ in 0..ITERS {
            maps.push(MmapMut::map_anon(bytes)?);
        }
        alloc.push(per_op(start, ITERS));
        let start = Instant::now();
        maps.clear();
        free.push(per_op(start, ITERS));
    }
    let med = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        (v[v.len() / 2], v[0], v[v.len() - 1])
    };
    Ok((med(alloc), med(free)))
}

/// Allocator cost for `size`-byte blocks, allocated and freed in LIFO order
/// at a live depth of `depth`, like frames on a call path.
fn alloc_free_ns(size: usize, depth: usize) -> (Spread, Spread) {
    const ROUNDS: u64 = 2_000;
    let mut live: Vec<Vec<u8>> = Vec::with_capacity(depth);
    let mut alloc = Vec::new();
    let mut free = Vec::new();
    for _ in 0..7 {
        let mut a = 0u128;
        let mut f = 0u128;
        for _ in 0..ROUNDS {
            let t = Instant::now();
            for _ in 0..depth {
                let mut v = Vec::<u8>::with_capacity(size);
                v.push(1);
                live.push(black_box(v));
            }
            a += t.elapsed().as_nanos();
            let t = Instant::now();
            while let Some(v) = live.pop() {
                drop(black_box(v));
            }
            f += t.elapsed().as_nanos();
        }
        #[allow(clippy::cast_precision_loss)]
        let n = (ROUNDS * depth as u64) as f64;
        #[allow(clippy::cast_precision_loss)]
        {
            alloc.push(a as f64 / n);
            free.push(f as f64 / n);
        }
    }
    let med = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        (v[v.len() / 2], v[0], v[v.len() - 1])
    };
    (med(alloc), med(free))
}

fn machine() -> String {
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split(':').nth(1))
                .map(|s| s.trim().to_owned())
        })
        .unwrap_or_else(|| "unknown cpu".to_owned());
    format!(
        "{} {} / {cpu}",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// Measure every parameter that can be measured without the runtime.
///
/// # Errors
/// Fails if anonymous memory cannot be mapped.
pub fn calibrate(base: &CostModel) -> std::io::Result<Calibration> {
    let mut p = base.clone();
    let mut m = Vec::new();
    let mut put = |parameter, (value, min, max): (f64, f64, f64), unit, method| {
        m.push(Measurement {
            parameter,
            value,
            min,
            max,
            unit,
            method,
        });
        value
    };

    p.contiguous.touch_ns_per_byte = put(
        "contiguous.touch_ns_per_byte",
        touch_ns_per_byte()?,
        "ns/byte",
        "write one byte per 4 KiB page of a fresh 64 MiB anonymous map (fault + zeroing)",
    );
    let copy = put(
        "contiguous.copy_ns_per_byte",
        copy_ns_per_byte(),
        "ns/byte",
        "copy_from_slice of a warm 256 KiB block",
    );
    p.contiguous.copy_ns_per_byte = copy;
    let min_stack = usize::try_from(base.contiguous.min_stack_bytes).unwrap_or(8192);
    let (map, unmap) = stack_map_unmap_ns(min_stack)?;
    p.contiguous.alloc_ns = put(
        "contiguous.alloc_ns",
        map,
        "ns",
        "mmap of an untouched min-size stack (no guard page)",
    );
    p.contiguous.free_ns = put(
        "contiguous.free_ns",
        unmap,
        "ns",
        "munmap of an untouched min-size stack",
    );
    let seg = usize::try_from(base.segmented.segment_bytes).unwrap_or(4096);
    let (sa, sf) = alloc_free_ns(seg, 16);
    p.segmented.alloc_ns = put(
        "segmented.alloc_ns",
        sa,
        "ns",
        "malloc of a segment, LIFO at depth 16 (warm allocator)",
    );
    p.segmented.free_ns = put("segmented.free_ns", sf, "ns", "free of a segment, LIFO");
    let (ha, hf) = alloc_free_ns(128, 32);
    p.heap.alloc_ns = put(
        "heap.alloc_ns",
        ha,
        "ns",
        "malloc of a 128-byte frame, LIFO at depth 32 (warm allocator)",
    );
    p.heap.free_ns = put("heap.free_ns", hf, "ns", "free of a 128-byte frame, LIFO");
    // Migration copy cost tracks memory bandwidth.
    for row in &mut p.migration {
        for cell in row.iter_mut() {
            if cell.ns_per_byte > 0.0 {
                cell.ns_per_byte = copy;
            }
        }
    }

    let machine = machine();
    p.name = format!("calibrated {machine}");
    Ok(Calibration {
        machine,
        measurements: m,
        not_measured: vec![
            (
                "common.call_ns / return_ns / switch_ns",
                "needs the Stage 4 runtime",
            ),
            (
                "contiguous.check_ns, segmented.check_ns, segmented.boundary_ns",
                "needs the Stage 4 runtime",
            ),
            (
                "contiguous.grow_fixed_ns",
                "needs the Stage 4 runtime (relocation bookkeeping)",
            ),
            (
                "heap.locality_ns",
                "needs the Stage 4 runtime and hardware counters",
            ),
            (
                "migration.*.fixed_ns / ns_per_frame",
                "needs the Stage 4 migration engine",
            ),
            (
                "memory.ns_per_byte_second",
                "economic parameter, not a hardware measurement; see docs/simulator.md",
            ),
        ],
        profile: p,
    })
}
