#!/bin/sh
# Engine-only baseline: run `statesolver explain-crash` (no LLM) on every
# pilot bug. Resumable: bugs with a complete result are skipped.
#
#   sh study/engine_sweep.sh            # all bugs in study/pilot.txt
#   python3 study/engine_score.py       # summary table afterwards
set -u
cd "$(dirname "$0")/.."
out=study/results/engine
mkdir -p "$out"
for b in $(cat study/pilot.txt); do
  if python3 -c "import json,sys; json.load(open('$out/$b.json'))" 2>/dev/null; then
    echo "$b: done"; continue
  fi
  start=$(date +%s)
  timeout 900 ./target/release/statesolver explain-crash --json "study/bugs/$b/trace" > "$out/$b.json" 2> "$out/$b.err"
  code=$?
  cls=$(python3 -c "import json; print(json.load(open('$out/$b.json'))['classification'])" 2>/dev/null || echo "FAILED: $(tail -c 150 $out/$b.err | tr '\n' ' ')")
  echo "$b: exit=$code $(( $(date +%s) - start ))s  $cls"
done
