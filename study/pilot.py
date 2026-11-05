#!/usr/bin/env python3
"""Run the pilot: bugs x arms x seeds with one model. Resumable.

    python3 study/pilot.py [--model qwen-coder] [--arms A1,A4,A5] [--seeds 1,2,3] \
        [--bugs study/pilot.txt] [--out study/results/pilot]

Only bugs whose pipeline status is "ok" (study/bugs/<id>/meta.json) are run.
Finished runs (an existing output file) are skipped, so the command can be
re-run after an interruption. Runs are sequential: rr replays share the
pinned performance core.

Afterwards:  python3 study/score.py study/results/pilot
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent


def work_dirs() -> dict[str, str]:
    out = {}
    for f in ("eval-cve.jsonl", "eval-oss.jsonl"):
        for line in (REPO / "study/data" / f).read_text().splitlines():
            d = json.loads(line)
            out[d["instance_id"]] = d["work_dir"]
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default="qwen-coder")
    ap.add_argument("--arms", default="A0,A1,A4,A5")
    ap.add_argument("--seeds", default="1,2,3")
    ap.add_argument("--bugs", default=str(REPO / "study/pilot.txt"))
    ap.add_argument("--bugs-dir", default=str(REPO / "study/bugs"))
    ap.add_argument("--out", default=str(REPO / "study/results/pilot"))
    ap.add_argument("--max-calls", default="30")
    a = ap.parse_args()
    wd = work_dirs()
    bugs = [b for b in Path(a.bugs).read_text().split() if b]
    plan = []
    for bug in bugs:
        meta = Path(a.bugs_dir) / bug / "meta.json"
        status = json.loads(meta.read_text()).get("status") if meta.exists() else "not recorded"
        trace = Path(a.bugs_dir) / bug / "trace"
        indexed = (trace / "statesolver-index.ss").exists() and not (trace / "incomplete").exists()
        if status != "ok" and not (status == "already recorded" and indexed):
            print(f"skip {bug}: {status}")
            continue
        for arm in a.arms.split(","):
            for seed in a.seeds.split(","):
                plan.append((bug, arm, seed))
    print(f"{len(plan)} runs planned")
    done = 0
    for bug, arm, seed in plan:
        out = Path(a.out) / bug / arm / a.model / f"{seed}.json"
        if out.exists():
            done += 1
            continue
        d = Path(a.bugs_dir) / bug
        cmd = [sys.executable, str(REPO / "study/runner.py"), "--trace", str(d / "trace"), "--src", str(d / "src"),
               "--arm", arm, "--model", a.model, "--seed", seed, "--max-calls", a.max_calls,
               "--report", str(d / "record.log"), "--strip-prefix", wd.get(bug, ""), "--out", str(out)]
        t = time.time()
        r = subprocess.run(cmd, capture_output=True, text=True)
        done += 1
        tail = (r.stdout.strip().splitlines() or r.stderr.strip().splitlines() or ["?"])[-1]
        print(f"[{done}/{len(plan)}] {bug} {arm} seed {seed} ({time.time() - t:.0f}s): {tail[:200]}", flush=True)
        if r.returncode != 0:
            err = Path(a.out) / "errors.log"
            err.parent.mkdir(parents=True, exist_ok=True)
            with err.open("a") as f:
                f.write(f"== {bug} {arm} {seed}\n{r.stderr[-3000:]}\n")


if __name__ == "__main__":
    main()
