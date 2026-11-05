#!/usr/bin/env python3
"""Derive fix-commit ground truth for SEC-bench instances.

    python3 study/ground_truth.py $(cat study/pilot.txt)

For each instance, writes study/labels/<id>.json:

    class   bug class from the first "ERROR: AddressSanitizer:" line
    root    [[file, line], ...]: lines of the vulnerable version that the
            developer fix deletes or modifies; for pure insertions, the line
            just before each insertion point (the "fix hunk" lower bound used
            as the primary, objective localization measure)
    chain   []  (filled by the maintainer's semantic labels, if any)
    crash   null (filled from the recording)
    source  "fix-commit"

Files are relative to the project root, matching the source tree shown to
agents (study/bugs/<id>/src). Test and documentation files are ignored.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CODE = re.compile(r"\.(c|cc|cpp|cxx|h|hh|hpp|hxx|inc|y|l)$")
SKIP = re.compile(r"(^|/)(test|tests|fuzz|fuzzing|regress|doc|docs)/|ChangeLog|NEWS")

CLASSES = [
    ("heap-use-after-free", "use-after-free"),
    ("use-after-poison", "use-after-free"),
    ("heap-buffer-overflow", "heap-buffer-overflow"),
    ("stack-buffer-overflow", "stack-buffer-overflow"),
    ("global-buffer-overflow", "global-buffer-overflow"),
    ("attempting double-free", "double-free"),
    ("not malloc()-ed", "bad-free"),
    ("SEGV", "segv"),
]


def bug_class(report: str) -> str:
    first = next((l for l in report.splitlines() if "ERROR: AddressSanitizer" in l), "")
    for needle, cls in CLASSES:
        if needle in first:
            return cls
    return "other"


def fix_lines(patch: str) -> list[list]:
    """Old-file lines touched by a unified diff."""
    out, path, old = [], None, 0
    last_context = None
    run_deleted = False
    for line in patch.splitlines():
        if line.startswith("--- "):
            p = line[4:].split("\t")[0].strip()
            path = None if p == "/dev/null" else re.sub(r"^[ab]/", "", p)
            continue
        if line.startswith("+++ "):
            continue
        m = re.match(r"@@ -(\d+)(?:,\d+)? \+\d+(?:,\d+)? @@", line)
        if m:
            old = int(m.group(1))
            last_context = old - 1
            run_deleted = False
            continue
        if path is None or not CODE.search(path) or SKIP.search(path):
            continue
        if line.startswith("-"):
            out.append([path, old])
            run_deleted = True
            old += 1
        elif line.startswith("+"):
            # A pure insertion (no deleted line in this change run) anchors on
            # the preceding old line; a replacement is already covered.
            if not run_deleted and last_context and [path, last_context] not in out:
                out.append([path, last_context])
        else:
            last_context = old
            run_deleted = False
            old += 1
    # Deduplicate, keep order.
    seen, uniq = set(), []
    for f, l in out:
        if (f, l) not in seen and l > 0:
            seen.add((f, l))
            uniq.append([f, l])
    return uniq


def main() -> None:
    db = {}
    for f in ("eval-cve.jsonl", "eval-oss.jsonl"):
        for line in (REPO / "study/data" / f).read_text().splitlines():
            d = json.loads(line)
            db[d["instance_id"]] = d
    out_dir = REPO / "study/labels"
    out_dir.mkdir(parents=True, exist_ok=True)
    for iid in sys.argv[1:]:
        d = db[iid]
        root = fix_lines(d["patch"])
        label = {"instance_id": iid, "class": bug_class(d.get("sanitizer_report") or ""),
                 "root": root, "chain": [], "crash": None, "source": "fix-commit"}
        (out_dir / f"{iid}.json").write_text(json.dumps(label, indent=2) + "\n")
        files = sorted({f for f, _ in root})
        print(f"{iid:34} {label['class']:22} {len(root):3} fix lines in {', '.join(files) or '-'}")


if __name__ == "__main__":
    main()
