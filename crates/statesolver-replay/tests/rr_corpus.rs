//! Record the bug corpus with rr and derive allocation indexes by replay.
//! Skips when gcc, rr, or hardware performance counters are unavailable.
#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use statesolver_trace::{EventKind, Rule, Validator};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bug-corpus")
}

fn rr_record(name: &str, stdin: Option<&str>) -> Option<PathBuf> {
    // Tests run in parallel and may record the same program: unique dirs.
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    // One recording at a time keeps the pinned performance core uncontended.
    static RECORDING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = RECORDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = corpus().join(name);
    let built = Command::new("sh")
        .arg(corpus().join("common/build.sh"))
        .arg(dir.join("src"))
        .stdout(Stdio::null())
        .status()
        .ok()?
        .success();
    if !built {
        eprintln!("skipping {name}: build failed");
        return None;
    }
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out =
        std::env::temp_dir().join(format!("statesolver-rr-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let mut cmd = Command::new("rr");
    cmd.current_dir(&dir)
        .arg("record")
        .args(statesolver_replay::rr_record_args())
        .arg("-o")
        .arg(&out)
        .arg(dir.join(name))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(s) = stdin {
        cmd.stdin(std::fs::File::open(dir.join(s)).ok()?);
    }
    match cmd.status() {
        Ok(_) if out.is_dir() => {
            assert!(
                !out.join("incomplete").exists(),
                "rr left an incomplete trace for {name}"
            );
            Some(out)
        }
        _ => {
            eprintln!("skipping {name}: rr record unavailable (rr missing or no PMU access)");
            None
        }
    }
}

fn index(name: &str, stdin: Option<&str>) -> Option<(Vec<EventKind>, Vec<Rule>)> {
    let trace = rr_record(name, stdin)?;
    let mut s = statesolver_replay::Session::start(Some(&trace)).ok()?;
    let (_, events, _) = statesolver_replay::index::build(&mut s).expect("index");
    drop(s);
    let _ = std::fs::remove_dir_all(trace);
    let mut v = Validator::new();
    let rules = events
        .iter()
        .flat_map(|e| v.check(e))
        .map(|x| x.rule)
        .collect();
    assert!(
        events.iter().all(|e| e.rr.is_some()),
        "every event has an rr position"
    );
    assert!(
        events
            .windows(2)
            .all(|w| (w[0].rr.unwrap().event, w[0].rr.unwrap().ticks)
                <= (w[1].rr.unwrap().event, w[1].rr.unwrap().ticks)),
        "rr positions are monotonic"
    );
    Some((events.into_iter().map(|e| e.kind).collect(), rules))
}

fn count(k: &[EventKind], name: &str) -> usize {
    k.iter().filter(|e| e.name() == name).count()
}

#[test]
fn uaf_reuse_epochs_from_replay() {
    let Some((k, rules)) = index("001-uaf-reuse", None) else {
        return;
    };
    assert!(rules.is_empty(), "{rules:?}");
    assert_eq!(count(&k, "free"), 1);
    let users: Vec<(u64, u32)> = k
        .iter()
        .filter_map(|e| match e {
            EventKind::Alloc {
                addr,
                size: 32,
                epoch,
                ..
            } => Some((*addr, epoch.0)),
            _ => None,
        })
        .collect();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].0, users[1].0, "address reused");
    assert_eq!((users[0].1, users[1].1), (1, 2));
    assert!(matches!(
        k.last(),
        Some(EventKind::Signal {
            signo: 11,
            fatal: true,
            ..
        })
    ));
}

#[test]
fn double_free_from_replay() {
    let Some((k, rules)) = index("002-double-free", None) else {
        return;
    };
    assert_eq!((count(&k, "alloc"), count(&k, "free")), (5, 6));
    assert!(
        matches!(rules.as_slice(), [Rule::FreeOfDeadAllocation(_)]),
        "{rules:?}"
    );
    assert!(matches!(
        k.last(),
        Some(EventKind::Signal {
            signo: 6,
            fatal: true,
            ..
        })
    ));
}

#[test]
fn overflow_from_replay() {
    let Some((k, rules)) = index("003-heap-overflow-input", Some("input/overflow.bin")) else {
        return;
    };
    assert!(rules.is_empty(), "{rules:?}");
    assert_eq!(count(&k, "alloc"), 2);
    assert!(matches!(
        k.last(),
        Some(EventKind::Signal {
            signo: 11,
            fatal: true,
            ..
        })
    ));
}

#[test]
fn goto_lands_on_the_exact_instruction_instance() {
    let Some(trace) = rr_record("001-uaf-reuse", None) else {
        return;
    };
    let mut s = statesolver_replay::Session::start(Some(&trace)).expect("rr replay");
    let (_, events, _) = statesolver_replay::index::build(&mut s).expect("index");
    drop(s);
    // A fresh session, visiting positions out of order (backward and forward).
    let mut s = statesolver_replay::Session::start(Some(&trace)).expect("rr replay");
    let mut checked = 0;
    for e in events.iter().rev() {
        let pos = e.rr.expect("position");
        match &e.kind {
            EventKind::Alloc { addr, .. } => {
                s.goto(pos).expect("goto alloc");
                assert_eq!(
                    s.reg(statesolver_trace::Reg::Rax).unwrap(),
                    *addr,
                    "rax at malloc return"
                );
                checked += 1;
            }
            EventKind::Mark { label, .. } => {
                s.goto(pos).expect("goto mark");
                let p = s.reg(statesolver_trace::Reg::Rdi).unwrap();
                let raw = s.mem(p, label.len()).unwrap();
                assert_eq!(String::from_utf8_lossy(&raw), label.as_str());
                checked += 1;
            }
            _ => continue,
        }
        assert_eq!(s.position().unwrap(), pos);
    }
    drop(s);
    let _ = std::fs::remove_dir_all(trace);
    assert!(checked >= 6, "checked {checked}");
}
