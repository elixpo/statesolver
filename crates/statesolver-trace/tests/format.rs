//! Round-trip, robustness, and validation tests for `.ss` v0.

use proptest::prelude::*;
use statesolver_trace::{
    AllocId, AllocKind, Bytes, Epoch, Error, Event, EventId, EventKind, Format, Header, MapOp,
    MemWrite, Program, Reg, Region, Rule, TraceReader, TraceWriter, Validator,
};

fn header() -> Header {
    Header::new(
        "test",
        Program {
            path: "./t".into(),
            argv: vec!["./t".into()],
            sha256: None,
            load_base: None,
        },
    )
}

fn kind() -> impl Strategy<Value = EventKind> {
    let reg = proptest::sample::select(Reg::ALL.to_vec());
    prop_oneof![
        (
            any::<bool>(),
            proptest::collection::vec((reg, any::<u64>()), 0..5)
        )
            .prop_map(|(full, values)| EventKind::Regs { full, values }),
        (any::<u64>(), 1..u64::MAX, "[rwx-]{3}").prop_map(|(start, len, perms)| EventKind::Maps {
            op: MapOp::Map,
            regions: vec![Region {
                start,
                len,
                perms,
                path: Some("[heap]".into())
            }],
        }),
        (
            any::<u64>(),
            any::<u64>(),
            any::<[u64; 6]>(),
            any::<u64>(),
            proptest::collection::vec(any::<u8>(), 0..64)
        )
            .prop_map(|(pc, nr, args, ret, data)| EventKind::Syscall {
                pc,
                nr,
                args,
                ret,
                writes: if data.is_empty() {
                    vec![]
                } else {
                    vec![MemWrite {
                        addr: pc,
                        data: Bytes(data),
                    }]
                },
            }),
        (
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u32>(),
            any::<u64>()
        )
            .prop_map(|(a, addr, size, ep, caller)| EventKind::Alloc {
                alloc: AllocId(a),
                addr,
                size,
                epoch: Epoch(ep),
                kind: AllocKind::Calloc,
                realloc_of: None,
                caller,
            }),
        (any::<u64>(), any::<u64>(), any::<u32>(), any::<u64>()).prop_map(
            |(a, addr, ep, caller)| EventKind::Free {
                alloc: AllocId(a),
                addr,
                epoch: Epoch(ep),
                caller
            }
        ),
        (
            any::<u32>(),
            any::<i32>(),
            proptest::option::of(any::<u64>()),
            any::<u64>(),
            any::<bool>()
        )
            .prop_map(|(signo, code, addr, pc, fatal)| EventKind::Signal {
                signo,
                code,
                addr,
                pc,
                fatal
            }),
        (".{0,20}", any::<u64>()).prop_map(|(label, pc)| EventKind::Mark { label, pc }),
        any::<i32>().prop_map(|code| EventKind::Exit { code }),
    ]
}

fn events() -> impl Strategy<Value = Vec<Event>> {
    proptest::collection::vec(kind(), 0..40).prop_map(|ks| {
        ks.into_iter()
            .enumerate()
            .map(|(i, kind)| Event {
                id: EventId(i as u64),
                rr: None,
                kind,
            })
            .collect()
    })
}

fn encode(format: Format, evs: &[Event]) -> Vec<u8> {
    let mut w = TraceWriter::new(Vec::new(), format, &header()).unwrap();
    for e in evs {
        w.write(e).unwrap();
    }
    w.finish().unwrap()
}

fn decode(bytes: Vec<u8>) -> Result<(Header, Format, Vec<Event>), Error> {
    let r = TraceReader::new(Box::new(std::io::Cursor::new(bytes)))?;
    let (h, f) = (r.header().clone(), r.format());
    Ok((h, f, r.collect::<Result<Vec<_>, _>>()?))
}

proptest! {
    #[test]
    fn round_trips_both_containers(evs in events()) {
        for format in [Format::Jsonl, Format::Ss] {
            let (h, f, got) = decode(encode(format, &evs)).unwrap();
            prop_assert_eq!(h, header());
            prop_assert_eq!(f, format);
            prop_assert_eq!(&got, &evs);
        }
    }

    #[test]
    fn truncated_or_garbage_input_never_panics(evs in events(), cut in any::<prop::sample::Index>(), junk in proptest::collection::vec(any::<u8>(), 0..64)) {
        for format in [Format::Jsonl, Format::Ss] {
            let mut bytes = encode(format, &evs);
            bytes.truncate(cut.index(bytes.len() + 1));
            bytes.extend_from_slice(&junk);
            if let Ok((_, _, got)) = decode(bytes) {
                prop_assert!(got.len() <= evs.len() + 1);
            }
        }
    }

    #[test]
    fn validator_never_panics(evs in events()) {
        let mut v = Validator::new();
        for e in &evs {
            let _ = v.check(e);
        }
    }
}

fn ev(id: u64, kind: EventKind) -> Event {
    Event {
        id: EventId(id),
        rr: None,
        kind,
    }
}
fn alloc(id: u64, a: u64, addr: u64, size: u64, epoch: u32) -> Event {
    ev(
        id,
        EventKind::Alloc {
            alloc: AllocId(a),
            addr,
            size,
            epoch: Epoch(epoch),
            kind: AllocKind::Malloc,
            realloc_of: None,
            caller: 0,
        },
    )
}
fn free(id: u64, a: u64, addr: u64, epoch: u32) -> Event {
    ev(
        id,
        EventKind::Free {
            alloc: AllocId(a),
            addr,
            epoch: Epoch(epoch),
            caller: 0,
        },
    )
}

fn rules(evs: &[Event]) -> Vec<Rule> {
    let mut v = Validator::new();
    evs.iter()
        .flat_map(|e| v.check(e))
        .map(|x| x.rule)
        .collect()
}

#[test]
fn reuse_after_free_is_valid_and_bumps_the_epoch() {
    let evs = [
        alloc(0, 1, 0x1000, 32, 1),
        free(1, 1, 0x1000, 1),
        alloc(2, 2, 0x1000, 32, 2),
        free(3, 2, 0x1000, 2),
        alloc(4, 3, 0x1000, 64, 3),
    ];
    assert_eq!(rules(&evs), Vec::<Rule>::new());
}

#[test]
fn each_rule_is_detected() {
    let cases: Vec<(Vec<Event>, Rule)> = vec![
        (
            vec![alloc(1, 1, 0x10, 8, 1)],
            Rule::NonSequentialId { expected: 0 },
        ),
        (
            vec![ev(
                0,
                EventKind::Regs {
                    full: false,
                    values: vec![],
                },
            )],
            Rule::DeltaBeforeFullRegs,
        ),
        (
            vec![
                alloc(0, 1, 0x10, 8, 1),
                free(1, 1, 0x10, 1),
                alloc(2, 1, 0x100, 8, 1),
            ],
            Rule::DuplicateAllocId(AllocId(1)),
        ),
        (
            vec![alloc(0, 1, 0x10, 32, 1), alloc(1, 2, 0x20, 8, 1)],
            Rule::Overlap {
                new: AllocId(2),
                live: AllocId(1),
            },
        ),
        (
            vec![
                alloc(0, 1, 0x10, 8, 1),
                free(1, 1, 0x10, 1),
                alloc(2, 2, 0x10, 8, 1),
            ],
            Rule::WrongEpoch {
                addr: 0x10,
                expected: Epoch(2),
                found: Epoch(1),
            },
        ),
        (
            vec![ev(
                0,
                EventKind::Alloc {
                    alloc: AllocId(2),
                    addr: 0x10,
                    size: 8,
                    epoch: Epoch(1),
                    kind: AllocKind::Realloc,
                    realloc_of: Some(AllocId(1)),
                    caller: 0,
                },
            )],
            Rule::BadReallocSource(AllocId(1)),
        ),
        (
            vec![free(0, 9, 0x10, 1)],
            Rule::FreeOfDeadAllocation(AllocId(9)),
        ),
        (
            vec![alloc(0, 1, 0x10, 8, 1), free(1, 1, 0x18, 1)],
            Rule::FreeMismatch(AllocId(1)),
        ),
        (
            vec![ev(
                0,
                EventKind::Maps {
                    op: MapOp::Map,
                    regions: vec![Region {
                        start: 0x1001,
                        len: 4096,
                        perms: "rw-".into(),
                        path: None,
                    }],
                },
            )],
            Rule::BadRegion {
                start: 0x1001,
                len: 4096,
            },
        ),
        (
            vec![ev(0, EventKind::Exit { code: 0 }), alloc(1, 1, 0x10, 8, 1)],
            Rule::AfterTermination,
        ),
    ];
    for (evs, want) in cases {
        let got = rules(&evs);
        assert!(got.contains(&want), "expected {want:?}, got {got:?}");
    }
}

#[test]
fn double_free_is_detected_as_free_of_dead_allocation() {
    let evs = [
        alloc(0, 1, 0x10, 8, 1),
        free(1, 1, 0x10, 1),
        free(2, 1, 0x10, 1),
    ];
    assert_eq!(rules(&evs), vec![Rule::FreeOfDeadAllocation(AllocId(1))]);
}

#[test]
fn example_trace_is_valid() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/examples/uaf-reuse.jsonl"
    );
    let r = statesolver_trace::open(path).unwrap();
    let mut v = Validator::new();
    let mut n = 0;
    for e in r {
        let e = e.unwrap();
        assert!(v.check(&e).is_empty(), "{e:?}");
        n += 1;
    }
    assert_eq!(n, 13);
}
