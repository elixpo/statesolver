#!/usr/bin/env python3
"""Score the engine-only baseline (study/results/engine) against study/labels."""
import json
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
ok = total = 0
for b in (REPO / "study/pilot.txt").read_text().split():
    total += 1
    label = json.loads((REPO / f"study/labels/{b}.json").read_text())["class"]
    try:
        got = json.loads((REPO / f"study/results/engine/{b}.json").read_text())["classification"]
    except Exception:
        got = "(no result)"
    hit = got.replace(" ", "-").startswith(label)
    ok += hit
    print(f"{b:30} label={label:22} engine={got:40} {'ok' if hit else 'MISS'}")
print(f"\nclass accuracy: {ok}/{total}")
