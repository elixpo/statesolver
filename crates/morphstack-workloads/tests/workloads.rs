//! Every workload produces a valid, deterministic trace.

use morphstack_trace::{Format, TraceWriter, Validator};
use morphstack_workloads::{Spec, Workload, generate_vec, header};

fn spec(workload: Workload) -> Spec {
    Spec {
        workload,
        seed: 1,
        scale: 0.02,
    }
}

/// FNV-1a over the binary encoding, so the digest covers every field.
fn digest(s: &Spec) -> u64 {
    let mut w = TraceWriter::new(Vec::new(), Format::Binary, &header(s)).unwrap();
    for e in generate_vec(s) {
        w.write(&e).unwrap();
    }
    w.finish()
        .unwrap()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
        })
}

#[test]
fn every_workload_is_valid() {
    for w in Workload::ALL {
        let s = spec(w);
        let events = generate_vec(&s);
        assert!(
            events.len() > 10,
            "{} produced {} events",
            w.name(),
            events.len()
        );
        let mut v = Validator::new(&header(&s));
        for (i, e) in events.iter().enumerate() {
            let bad = v.check(e);
            assert!(bad.is_empty(), "{} event {i}: {bad:?}", w.name());
        }
    }
}

#[test]
fn names_round_trip() {
    for w in Workload::ALL {
        assert_eq!(Workload::from_name(w.name()), Some(w));
    }
}

#[test]
fn same_seed_same_trace_different_seed_different_trace() {
    for w in Workload::ALL {
        let a = digest(&spec(w));
        assert_eq!(a, digest(&spec(w)), "{} not deterministic", w.name());
        let other = Spec { seed: 2, ..spec(w) };
        if w != Workload::AdvHotSplit {
            assert_ne!(a, digest(&other), "{} ignores its seed", w.name());
        }
    }
}

/// Pinned digests. These must be identical on Linux, macOS, and Windows; CI
/// runs this test on all three. If a deliberate generator change breaks
/// them, update the values and note the change in CHANGELOG, because every
/// previously published trace changes with it.
#[test]
fn digests_are_pinned_across_platforms() {
    let got: Vec<(&str, u64)> = Workload::ALL
        .iter()
        .map(|w| (w.name(), digest(&spec(*w))))
        .collect();
    assert_eq!(got, PINNED, "digests changed:\n{got:#x?}");
}

#[allow(clippy::unreadable_literal)]
const PINNED: &[(&str, u64)] = &[
    ("w1-nested", 0xb6eaaf8d7c509c1),
    ("w2-deep", 0x8e19b0645b72aa0f),
    ("w3-shallow-massive", 0x71efc50a4bb82b06),
    ("w4-frequent-suspend", 0xfbf8b71c2ca485de),
    ("w5-long-suspend", 0xc380d245f54bbf0b),
    ("w6-phase-change", 0x5e5c7a8e16c863a8),
    ("server", 0x7357ce1b1078d8bd),
    ("burst", 0x126b419b4cd8cf3d),
    ("mixed", 0x6e7419aefc58743f),
    ("adv-alternating", 0xf358b3114ded32dd),
    ("adv-hot-split", 0xcc52473322946a5a),
];
