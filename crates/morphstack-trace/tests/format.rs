//! Round-trip, robustness, and validation tests for the trace format.

use morphstack_trace::{
    ActId, CtxId, Error, Event, EventKind, FnId, Format, FunctionInfo, Header, Rule,
    StatsCollector, TraceReader, TraceWriter, Validator,
};
use proptest::prelude::*;

fn kind() -> impl Strategy<Value = EventKind> {
    let id = any::<u64>();
    prop_oneof![
        (id, proptest::option::of(any::<u64>())).prop_map(|(c, p)| EventKind::Spawn {
            ctx: CtxId(c),
            parent: p.map(CtxId),
        }),
        (id, any::<u64>(), any::<u32>(), any::<u32>()).prop_map(|(c, a, f, b)| EventKind::Call {
            ctx: CtxId(c),
            act: ActId(a),
            func: FnId(f),
            frame_bytes: b,
        }),
        (id, any::<u64>()).prop_map(|(c, a)| EventKind::Return {
            ctx: CtxId(c),
            act: ActId(a),
        }),
        id.prop_map(|c| EventKind::Suspend { ctx: CtxId(c) }),
        id.prop_map(|c| EventKind::Resume { ctx: CtxId(c) }),
        id.prop_map(|c| EventKind::Exit { ctx: CtxId(c) }),
        (
            proptest::option::of(any::<u64>()),
            proptest::option::of(any::<u64>()),
            proptest::option::of(any::<u64>()),
            proptest::option::of(any::<u64>()),
        )
            .prop_map(|(r, c, l, d)| EventKind::Sample {
                rss_bytes: r,
                cycles: c,
                llc_misses: l,
                dtlb_misses: d,
            }),
    ]
}

fn events() -> impl Strategy<Value = Vec<Event>> {
    proptest::collection::vec(
        (any::<u64>(), kind()).prop_map(|(ts, kind)| Event { ts, kind }),
        0..64,
    )
}

fn header() -> Header {
    let mut h = Header::new("test");
    h.seed = Some(7);
    h.functions.push(FunctionInfo {
        id: FnId(1),
        name: "main".into(),
        can_suspend: true,
    });
    h
}

fn encode(format: Format, h: &Header, evs: &[Event]) -> Vec<u8> {
    let mut w = TraceWriter::new(Vec::new(), format, h).unwrap();
    for e in evs {
        w.write(e).unwrap();
    }
    w.finish().unwrap()
}

fn decode(bytes: &[u8]) -> Result<(Header, Vec<Event>), Error> {
    let r = TraceReader::new(bytes)?;
    let h = r.header().clone();
    let evs = r.collect::<Result<Vec<_>, _>>()?;
    Ok((h, evs))
}

proptest! {
    #[test]
    fn round_trips_both_encodings(evs in events()) {
        let h = header();
        for format in [Format::Jsonl, Format::Binary] {
            let bytes = encode(format, &h, &evs);
            let r = TraceReader::new(&bytes[..]).unwrap();
            prop_assert_eq!(r.format(), format);
            let (h2, evs2) = decode(&bytes).unwrap();
            prop_assert_eq!(&h2, &h);
            prop_assert_eq!(&evs2, &evs);
        }
    }

    #[test]
    fn binary_events_are_smaller_than_jsonl(evs in events()) {
        let h = header();
        let payload = |f| encode(f, &h, &evs).len() - encode(f, &h, &[]).len();
        prop_assert!(payload(Format::Binary) <= payload(Format::Jsonl));
    }

    #[test]
    fn truncated_binary_errors_without_panicking(evs in events(), cut in any::<prop::sample::Index>()) {
        let bytes = encode(Format::Binary, &header(), &evs);
        let n = cut.index(bytes.len() + 1);
        // Either a clean prefix of events or an error; never a panic.
        if let Ok((_, got)) = decode(&bytes[..n]) {
            prop_assert!(got.len() <= evs.len());
            prop_assert_eq!(&got[..], &evs[..got.len()]);
        }
    }

    #[test]
    fn garbage_never_panics(mut tail in proptest::collection::vec(any::<u8>(), 0..256)) {
        let mut bytes = encode(Format::Binary, &header(), &[]);
        bytes.append(&mut tail);
        let _ = decode(&bytes);
    }

    #[test]
    fn stats_never_panic_on_invalid_traces(evs in events()) {
        let mut s = StatsCollector::new();
        for e in &evs {
            s.observe(e);
        }
        let _ = s.finish();
    }
}

fn ev(ts: u64, kind: EventKind) -> Event {
    Event { ts, kind }
}
fn spawn(ts: u64, c: u64) -> Event {
    ev(
        ts,
        EventKind::Spawn {
            ctx: CtxId(c),
            parent: None,
        },
    )
}
fn call(ts: u64, c: u64, a: u64) -> Event {
    ev(
        ts,
        EventKind::Call {
            ctx: CtxId(c),
            act: ActId(a),
            func: FnId(1),
            frame_bytes: 64,
        },
    )
}
fn ret(ts: u64, c: u64, a: u64) -> Event {
    ev(
        ts,
        EventKind::Return {
            ctx: CtxId(c),
            act: ActId(a),
        },
    )
}

fn rules(evs: &[Event]) -> Vec<Rule> {
    let mut v = Validator::new(&header());
    evs.iter()
        .flat_map(|e| v.check(e))
        .map(|x| x.rule)
        .collect()
}

#[test]
fn valid_trace_has_no_violations() {
    let evs = [
        spawn(0, 1),
        call(1, 1, 10),
        call(2, 1, 11),
        ev(3, EventKind::Suspend { ctx: CtxId(1) }),
        ev(9, EventKind::Resume { ctx: CtxId(1) }),
        ret(10, 1, 11),
        ret(11, 1, 10),
        ev(12, EventKind::Exit { ctx: CtxId(1) }),
    ];
    assert_eq!(rules(&evs), Vec::<Rule>::new());

    let mut s = StatsCollector::new();
    for e in &evs {
        s.observe(e);
    }
    let s = s.finish();
    assert_eq!(s.max_depth, 2);
    assert_eq!(s.peak_live_frame_bytes, 128);
    assert_eq!(s.suspension_ns.max, 6);
    assert_eq!(s.depth_at_suspend.max, 2);
    assert_eq!(s.activation_lifetime_ns.max, 10);
    assert_eq!(s.contexts_live_at_end, 0);
}

#[test]
fn each_rule_is_detected() {
    let c = CtxId(1);
    let cases: Vec<(Vec<Event>, Rule)> = vec![
        (
            vec![spawn(5, 1), spawn(4, 2)],
            Rule::TimeWentBackwards { prev: 5, ts: 4 },
        ),
        (vec![spawn(0, 1), spawn(0, 1)], Rule::DuplicateSpawn(c)),
        (
            vec![ev(
                0,
                EventKind::Spawn {
                    ctx: CtxId(2),
                    parent: Some(c),
                },
            )],
            Rule::DeadParent(c),
        ),
        (vec![call(0, 1, 1)], Rule::UnknownContext(c)),
        (
            vec![
                spawn(0, 1),
                ev(1, EventKind::Exit { ctx: c }),
                call(2, 1, 1),
            ],
            Rule::ExitedContext(c),
        ),
        (
            vec![
                spawn(0, 1),
                ev(1, EventKind::Suspend { ctx: c }),
                call(2, 1, 1),
            ],
            Rule::RunWhileSuspended(c),
        ),
        (
            vec![spawn(0, 1), call(1, 1, 7), call(2, 1, 7)],
            Rule::DuplicateActivation(ActId(7)),
        ),
        (
            vec![
                spawn(0, 1),
                ev(
                    1,
                    EventKind::Call {
                        ctx: c,
                        act: ActId(1),
                        func: FnId(99),
                        frame_bytes: 8,
                    },
                ),
            ],
            Rule::UnknownFunction(FnId(99)),
        ),
        (vec![spawn(0, 1), ret(1, 1, 1)], Rule::ReturnOnEmptyStack(c)),
        (
            vec![spawn(0, 1), call(1, 1, 1), call(2, 1, 2), ret(3, 1, 1)],
            Rule::ReturnMismatch {
                ctx: c,
                expected: ActId(2),
                found: ActId(1),
            },
        ),
        (
            vec![
                spawn(0, 1),
                ev(1, EventKind::Suspend { ctx: c }),
                ev(2, EventKind::Suspend { ctx: c }),
            ],
            Rule::DoubleSuspend(c),
        ),
        (
            vec![spawn(0, 1), ev(1, EventKind::Resume { ctx: c })],
            Rule::ResumeNotSuspended(c),
        ),
        (
            vec![
                spawn(0, 1),
                call(1, 1, 1),
                ev(2, EventKind::Exit { ctx: c }),
            ],
            Rule::ExitWithLiveState {
                ctx: c,
                depth: 1,
                suspended: false,
            },
        ),
    ];
    for (evs, want) in cases {
        let got = rules(&evs);
        assert!(got.contains(&want), "expected {want:?}, got {got:?}");
    }
}

#[test]
fn rejects_wrong_format_and_future_version() {
    let mut h = header();
    h.format = "something-else".into();
    let bytes = encode(Format::Jsonl, &h, &[]);
    assert!(matches!(decode(&bytes), Err(Error::WrongFormat(_))));

    let mut h = header();
    h.version = morphstack_trace::FORMAT_VERSION + 1;
    let bytes = encode(Format::Jsonl, &h, &[]);
    assert!(matches!(
        decode(&bytes),
        Err(Error::UnsupportedVersion { .. })
    ));
}

#[test]
fn example_trace_is_valid() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/examples/phase-change.jsonl"
    );
    let r = morphstack_trace::open(path).unwrap();
    let mut v = Validator::new(r.header());
    let mut n = 0;
    for e in r {
        let e = e.unwrap();
        assert!(v.check(&e).is_empty(), "event {n}: {e:?}");
        n += 1;
    }
    assert!(n > 20);
}
