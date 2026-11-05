#!/usr/bin/env python3
"""Score agent runs against ground truth.

    python3 study/score.py RUNS_DIR [--json]

RUNS_DIR is laid out as <bug>/<arm>/<model>/<seed>.json (study/runner.py
output). Ground truth comes from bug-corpus/<bug>/meta.json ("ground_truth")
or study/labels/<bug>.json, with:

  class   expected bug class
  root    [[file, line], ...]  where the program first goes wrong
          (for benchmark bugs: the lines the fix changes)
  chain   [[file, line], ...]  later lines on the causal path (partial credit)
  crash   [file, line]         the crash site

Per run:
  class_ok        bug class matches
  root_hit        reported root cause is a root line (strict)
  chain_hit       reported root cause is a root or chain line (relaxed)
  said_crash      reported the crash site as the root cause (a classic miss)
  claims          number of claims in the final answer
  uncited         claims citing nothing
  bad_cites       citations to tool-result ids that do not exist in the run
  tool_calls, tokens, seconds, answered (a parsable final answer exists)
"""

from __future__ import annotations

import argparse
import json
import re
from collections import defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

CLASS_ALIASES = {
    "uaf": "use-after-free", "use after free": "use-after-free",
    "heap-overflow": "heap-buffer-overflow", "heap buffer overflow": "heap-buffer-overflow",
    "buffer-overflow": "heap-buffer-overflow", "double free": "double-free",
}


def norm_class(c: str | None) -> str:
    c = (c or "").strip().lower()
    return CLASS_ALIASES.get(c, c)


def ground_truth(bug: str) -> dict | None:
    for p in (REPO / "study/labels" / f"{bug}.json", REPO / "bug-corpus" / bug / "meta.json"):
        if p.exists():
            d = json.loads(p.read_text())
            return d.get("ground_truth", d)
    return None


def same_loc(rep: dict | None, loc: list) -> bool:
    if not isinstance(rep, dict):
        return False
    f, line = loc
    try:
        rline = int(rep.get("line") or -1)
    except (TypeError, ValueError):
        return False
    rfile = str(rep.get("file") or "")
    return rline == int(line) and (rfile == f or rfile.endswith("/" + f) or f.endswith("/" + rfile))


def score_run(run: dict, gt: dict) -> dict:
    final = run.get("final") or {}
    rc = final.get("root_cause")
    ids = {e["id"] for e in run.get("log", []) if "id" in e}
    claims = final.get("claims") or []
    uncited = sum(1 for c in claims if not c.get("cites"))
    bad = 0
    for c in claims:
        for cite in c.get("cites") or []:
            # Accept T<n> and Q<n>[.F<k>] style ids; Q ids must appear inside some tool result.
            t = re.match(r"T\d+", str(cite))
            if t and t.group(0) not in ids:
                bad += 1
            elif not t and not any(str(cite).split(".")[0] in e.get("result", "") for e in run.get("log", []) if "result" in e):
                bad += 1
    return {
        "answered": bool(final),
        "class_ok": norm_class(final.get("bug_class")) == gt["class"],
        "root_hit": any(same_loc(rc, l) for l in gt["root"]),
        "chain_hit": any(same_loc(rc, l) for l in gt["root"] + gt.get("chain", [])),
        "said_crash": same_loc(rc, gt["crash"]) and not any(same_loc(rc, l) for l in gt["root"]),
        "claims": len(claims),
        "uncited": uncited,
        "bad_cites": bad,
        "tool_calls": run.get("tool_calls", 0),
        "tokens": sum((run.get("usage") or {}).values()),
        "seconds": run.get("seconds", 0),
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("runs")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()
    rows = []
    for f in sorted(Path(a.runs).glob("*/*/*/*.json")):
        bug, arm, model = f.parts[-4], f.parts[-3], f.parts[-2]
        gt = ground_truth(bug)
        if gt is None:
            continue
        rows.append({"bug": bug, "arm": arm, "model": model, "seed": f.stem, **score_run(json.loads(f.read_text()), gt)})
    if a.json:
        print(json.dumps(rows, indent=2))
        return
    agg = defaultdict(list)
    for r in rows:
        agg[(r["arm"], r["model"])].append(r)
    print(f"{'arm':<4} {'model':<12} {'runs':>4} {'class':>6} {'root':>6} {'chain':>6} {'crash':>6} {'uncited':>8} {'badcite':>8} {'calls':>6} {'ktok':>7}")
    for (arm, model), rs in sorted(agg.items()):
        n = len(rs)
        rate = lambda k: f"{sum(r[k] for r in rs) / n:.0%}"
        mean = lambda k: sum(r[k] for r in rs) / n
        print(f"{arm:<4} {model:<12} {n:>4} {rate('class_ok'):>6} {rate('root_hit'):>6} {rate('chain_hit'):>6} "
              f"{rate('said_crash'):>6} {mean('uncited'):>8.1f} {mean('bad_cites'):>8.1f} {mean('tool_calls'):>6.1f} {mean('tokens') / 1000:>7.1f}")


if __name__ == "__main__":
    main()
