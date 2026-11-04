//! Record the bug corpus end to end. Skips when gcc or ptrace is unavailable.
#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::Command;

use statesolver_trace::{EventKind, Rule, Validator};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bug-corpus")
}

fn build(name: &str) -> Option<PathBuf> {
    let dir = corpus().join(name);
    let ok = Command::new("sh")
        .arg(corpus().join("common/build.sh"))
        .arg(dir.join("src"))
        .status()
        .ok()?
        .success();
    ok.then(|| dir.join(name))
}

fn record(name: &str, stdin: Option<&str>) -> Option<(Vec<EventKind>, Vec<Rule>)> {
    let Some(bin) = build(name) else {
        eprintln!("skipping {name}: build failed (gcc missing?)");
        return None;
    };
    let out =
        std::env::temp_dir().join(format!("statesolver-test-{name}-{}.ss", std::process::id()));
    let opts = statesolver_record::Options {
        program: bin,
        args: vec![],
        stdin: stdin.map(|s| corpus().join(name).join(s)),
        out: out.clone(),
    };
    if let Err(e) = statesolver_record::record(&opts) {
        eprintln!("skipping {name}: cannot record ({e})");
        return None;
    }
    let mut v = Validator::new();
    let (mut kinds, mut rules) = (Vec::new(), Vec::new());
    for e in statesolver_trace::open(&out).unwrap() {
        let e = e.unwrap();
        rules.extend(v.check(&e).into_iter().map(|x| x.rule));
        kinds.push(e.kind);
    }
    let _ = std::fs::remove_file(out);
    Some((kinds, rules))
}

fn count(kinds: &[EventKind], name: &str) -> usize {
    kinds.iter().filter(|k| k.name() == name).count()
}

fn marks(kinds: &[EventKind]) -> Vec<String> {
    kinds
        .iter()
        .filter_map(|k| match k {
            EventKind::Mark { label, .. } => Some(label.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn uaf_reuse_records_one_free_and_an_address_reuse() {
    let Some((kinds, rules)) = record("001-uaf-reuse", None) else {
        return;
    };
    assert!(rules.is_empty(), "{rules:?}");
    // One logical free even though glibc's free re-enters its own entry.
    assert_eq!(count(&kinds, "free"), 1);
    let user_addrs: Vec<(u64, u32)> = kinds
        .iter()
        .filter_map(|k| match k {
            EventKind::Alloc {
                addr,
                size: 32,
                epoch,
                ..
            } => Some((*addr, epoch.0)),
            _ => None,
        })
        .collect();
    assert_eq!(user_addrs.len(), 2, "{user_addrs:?}");
    assert_eq!(user_addrs[0].0, user_addrs[1].0, "address reused");
    assert_eq!((user_addrs[0].1, user_addrs[1].1), (1, 2));
    assert_eq!(
        marks(&kinds),
        [
            "stale-pointer-saved",
            "root-cause:free-without-clearing-cache",
            "stale-write"
        ]
    );
    assert!(matches!(
        kinds.last(),
        Some(EventKind::Signal {
            signo: 11,
            fatal: true,
            ..
        })
    ));
}

#[test]
fn double_free_is_recorded_as_evidence() {
    let Some((kinds, rules)) = record("002-double-free", None) else {
        return;
    };
    // 1 shared buffer + 4 noise buffers; 1 + 4 + the double free.
    assert_eq!(count(&kinds, "alloc"), 5);
    assert_eq!(count(&kinds, "free"), 6);
    assert_eq!(rules.len(), 1, "{rules:?}");
    assert!(matches!(rules[0], Rule::FreeOfDeadAllocation(_)));
    assert!(matches!(
        kinds.last(),
        Some(EventKind::Signal {
            signo: 6,
            fatal: true,
            ..
        })
    ));
}

#[test]
fn overflow_captures_external_input() {
    let Some((kinds, rules)) = record("003-heap-overflow-input", Some("input/overflow.bin")) else {
        return;
    };
    assert!(rules.is_empty(), "{rules:?}");
    let stdin_reads: Vec<&[u8]> = kinds
        .iter()
        .filter_map(|k| match k {
            EventKind::Syscall {
                nr: 0,
                args,
                writes,
                ..
            } if args[0] == 0 => writes.first().map(|w| w.data.0.as_slice()),
            _ => None,
        })
        .collect();
    assert_eq!(stdin_reads.len(), 2);
    assert_eq!(stdin_reads[0], [48]); // the untrusted length byte
    assert_eq!(stdin_reads[1].len(), 48);
    assert_eq!(count(&kinds, "alloc"), 2);
}

fn build_program(name: &str, extra: &[&str]) -> Option<PathBuf> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/programs/{name}.c"));
    let out = std::env::temp_dir().join(format!("statesolver-prog-{name}-{}", std::process::id()));
    let ok = Command::new("gcc")
        .args(["-O0", "-g", "-o"])
        .arg(&out)
        .arg(&src)
        .args(extra)
        .status()
        .ok()?
        .success();
    ok.then_some(out)
}

fn record_program(bin: PathBuf) -> Result<Vec<EventKind>, statesolver_record::Error> {
    let out = bin.with_extension("ss");
    let opts = statesolver_record::Options {
        program: bin,
        args: vec![],
        stdin: None,
        out: out.clone(),
    };
    statesolver_record::record(&opts)?;
    let kinds = statesolver_trace::open(&out)
        .unwrap()
        .map(|e| e.unwrap().kind)
        .collect();
    let _ = std::fs::remove_file(out);
    Ok(kinds)
}

#[test]
fn vdso_time_and_tsc_become_recorded_events() {
    let Some(bin) = build_program("timing", &[]) else {
        return;
    };
    let kinds = match record_program(bin) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("skipping: {e}");
            return;
        }
    };
    let syscall = |nr: u64, len: usize| {
        kinds.iter().any(|k| {
            matches!(k, EventKind::Syscall { nr: n, writes, .. }
            if *n == nr && writes.first().is_some_and(|w| w.data.0.len() == len))
        })
    };
    assert!(syscall(228, 16), "clock_gettime not recorded as a syscall");
    assert!(syscall(96, 16), "gettimeofday not recorded as a syscall");
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, EventKind::Syscall { nr: 201, .. })),
        "time()"
    );
    let insns: Vec<&str> = kinds
        .iter()
        .filter_map(|k| match k {
            EventKind::Nondet { insn, .. } => Some(insn.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        insns.contains(&"rdtsc") && insns.contains(&"rdtscp"),
        "{insns:?}"
    );
    assert!(matches!(kinds.last(), Some(EventKind::Exit { code: 0 })));
}

#[test]
fn threads_are_refused_clearly() {
    let Some(bin) = build_program("thread", &["-pthread"]) else {
        return;
    };
    match record_program(bin) {
        Err(statesolver_record::Error::Unsupported(msg)) => assert!(msg.contains("clone"), "{msg}"),
        Err(e) => eprintln!("skipping: {e}"),
        Ok(_) => panic!("a threaded program must not record silently"),
    }
}
