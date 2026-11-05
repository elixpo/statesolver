#!/usr/bin/env python3
"""One-bug-at-a-time SEC-bench pipeline: pull, build, reproduce, rr record,
pack, copy out source and patch, delete the image, index on the host.

    python3 study/pipeline.py jq.cve-2023-50246 [more ids...] \
        [--sanitizer address|none] [--asan-options quarantine_size_mb=0] \
        [--out study/bugs] [--keep-image]

Output per instance (study/bugs/<id>/):
    trace/        packed rr trace (replayable on the host)
    src/          source tree (no .git) shown to agents
    patch.diff    developer fix (ground truth for localization)
    meta.json     instance metadata, the recorded command, build and repro status
    *.log         build, native repro, and record logs

The procedure follows docs/study/benchmarks.md §1 (verified on jq). Recording
happens inside the container with rr 5.9 from the upstream .deb; the
container is pinned to one performance core so rr's counters work.
"""

from __future__ import annotations

import argparse
import json
import re
import shlex
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DATA = REPO / "study/data"
RR_DEB_URL = "https://github.com/rr-debugger/rr/releases/download/5.9.0/rr-5.9.0-Linux-x86_64.deb"
RR_DEB = DATA / "rr-5.9.0-Linux-x86_64.deb"
IMAGE = "hwiwonlee/secb.eval.x86_64.{id}:patch"


def sh(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, text=True, **kw)


def instances() -> dict[str, dict]:
    out = {}
    for f in ("eval-cve.jsonl", "eval-oss.jsonl"):
        p = DATA / f
        if not p.exists():
            sys.exit(f"missing {p}; download from huggingface.co/datasets/SEC-bench/SEC-bench")
        for line in p.read_text().splitlines():
            d = json.loads(line)
            out[d["instance_id"]] = d
    return out


def performance_core() -> int:
    try:
        first = Path("/sys/devices/cpu_core/cpus").read_text().strip().split(",")[0]
        return int(first.split("-")[0]) + 2  # avoid CPU 0, which the host is busiest on
    except OSError:
        return 2


def poc_command(secb_sh: str) -> tuple[str | None, str]:
    """The single PoC command in secb_sh's repro(), plus any `cd` before it."""
    m = re.search(r"repro\(\)\s*\{(.*?)\n\}", secb_sh, re.S)
    if not m:
        return None, ""
    cwd, cmds = "", []
    for raw in m.group(1).splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or line.startswith("echo "):
            continue
        if line.startswith("cd "):
            cwd = line[3:].strip()
            continue
        cmds.append(line)
    if len(cmds) != 1 or re.search(r"\|\||&&|;|\|(?!\|)|\$\(|`", cmds[0]):
        return None, cwd  # not a single simple command: handle manually
    return cmds[0], cwd


def ensure_rr_deb() -> None:
    if not RR_DEB.exists():
        DATA.mkdir(parents=True, exist_ok=True)
        urllib.request.urlretrieve(RR_DEB_URL, RR_DEB)


INNER = r"""
set -u
cd {work_dir}
export SANITIZER={sanitizer}
# Line tables for source-level localization (SEC-bench builds ship without them).
export CFLAGS="${{CFLAGS:-}} -gline-tables-only" CXXFLAGS="${{CXXFLAGS:-}} -gline-tables-only"
( secb build ) > /out/build.log 2>&1 || {{ echo BUILD_FAILED; exit 3; }}
cd {cwd}
( timeout 120 bash -c {cmd_q} ) > /out/repro-native.log 2>&1
echo "native exit: $?" >> /out/repro-native.log
dpkg -i /rr/rr.deb > /dev/null 2>&1 || {{ echo RR_INSTALL_FAILED; exit 4; }}
export ASAN_OPTIONS={asan_options}
timeout 600 bash -c "rr record -o /out/trace {cmd}" > /out/record.log 2>&1
echo "record exit: $?" >> /out/record.log
[ -d /out/trace ] || {{ echo RECORD_FAILED; exit 5; }}
[ -e /out/trace/incomplete ] && {{ echo RECORD_INCOMPLETE; exit 6; }}
rr pack /out/trace >> /out/record.log 2>&1
tar -C {work_dir} --exclude=.git --exclude='*.o' --exclude='*.a' -cf - . | (mkdir -p /out/src && tar -C /out/src -xf -)
chown -R {uid}:{gid} /out
echo OK
"""


def run_one(inst: dict, out_root: Path, sanitizer: str, asan_options: str, keep: bool) -> dict:
    iid = inst["instance_id"]
    out = out_root / iid
    meta = {"instance_id": iid, "project": inst["project_name"], "sanitizer": sanitizer,
            "asan_options": asan_options, "started": time.strftime("%Y-%m-%dT%H:%M:%S")}
    if (out / "trace").exists():
        meta["status"] = "already recorded"
        return meta
    out.mkdir(parents=True, exist_ok=True)
    (out / "patch.diff").write_text(inst["patch"])
    (out / "sanitizer_report.txt").write_text(inst.get("sanitizer_report") or "")
    cmd, cwd = poc_command(inst["secb_sh"])
    meta["command"], meta["cwd"] = cmd, cwd or inst["work_dir"]
    if not cmd:
        meta["status"] = "manual: repro() is not a single simple command"
        return meta
    img = IMAGE.format(id=iid)
    t0 = time.time()
    if sh(["docker", "pull", "-q", img], capture_output=True).returncode != 0:
        meta["status"] = "pull failed"
        return meta
    meta["pull_s"] = round(time.time() - t0, 1)
    import os
    script = INNER.format(work_dir=shlex.quote(inst["work_dir"]), sanitizer=shlex.quote(sanitizer),
                          cwd=shlex.quote(cwd or inst["work_dir"]), cmd_q=shlex.quote(cmd), cmd=cmd,
                          asan_options=shlex.quote(asan_options), uid=os.getuid(), gid=os.getgid())
    t1 = time.time()
    r = sh(["docker", "run", "--rm", "--memory=5g", f"--cpuset-cpus={performance_core()}",
            "--cap-add=SYS_PTRACE", "--security-opt", "seccomp=unconfined",
            "-v", f"{out.resolve()}:/out", "-v", f"{RR_DEB.resolve()}:/rr/rr.deb:ro",
            "--entrypoint", "bash", img, "-c", script], capture_output=True)
    meta["container_s"] = round(time.time() - t1, 1)
    meta["container_status"] = (r.stdout.strip().splitlines() or ["?"])[-1]
    if not keep:
        sh(["docker", "rmi", "-f", img], capture_output=True)
        sh(["docker", "image", "prune", "-f"], capture_output=True)
    native = (out / "repro-native.log").read_text(errors="replace") if (out / "repro-native.log").exists() else ""
    meta["native_crash"] = bool(re.search(r"ERROR: AddressSanitizer|Segmentation fault|SIGSEGV|core dumped|double free|munmap_chunk|free\(\)", native))
    if meta["container_status"] != "OK":
        meta["status"] = f"failed: {meta['container_status']}"
        return meta
    # Index on the host: proves the packed trace replays here.
    binary = REPO / "target/release/statesolver"
    t2 = time.time()
    idx = sh([str(binary), "index", str(out / "trace")], capture_output=True)
    meta["index_s"] = round(time.time() - t2, 1)
    meta["index"] = (idx.stderr.strip().splitlines() or ["?"])[-1]
    meta["status"] = "ok" if idx.returncode == 0 else "recorded; host index failed"
    meta["trace_mb"] = round(sum(p.stat().st_size for p in (out / "trace").rglob("*") if p.is_file()) / 1e6, 1)
    return meta


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("ids", nargs="+")
    ap.add_argument("--sanitizer", default="address", choices=["address", "none"])
    ap.add_argument("--asan-options", default="detect_leaks=0")
    ap.add_argument("--out", default=str(REPO / "study/bugs"))
    ap.add_argument("--keep-image", action="store_true")
    a = ap.parse_args()
    ensure_rr_deb()
    db = instances()
    out_root = Path(a.out)
    for iid in a.ids:
        if iid not in db:
            print(f"{iid}: unknown instance id")
            continue
        meta = run_one(db[iid], out_root, a.sanitizer, a.asan_options, a.keep_image)
        (out_root / iid).mkdir(parents=True, exist_ok=True)
        (out_root / iid / "meta.json").write_text(json.dumps(meta, indent=2))
        with open(out_root / "manifest.jsonl", "a") as m:
            m.write(json.dumps(meta) + "\n")
        print(f"{iid}: {meta['status']}  {meta.get('index', '')}")


if __name__ == "__main__":
    main()
