#!/bin/sh
# Shared deterministic build for corpus programs: no optimisation, frame
# pointers, debug info, non-PIE so addresses in ROOT_CAUSE.md are stable.
set -eu
dir=$(cd "$(dirname "$1")" && pwd)
name=$(basename "$dir")
out="$dir/$name"
${CC:-gcc} -O0 -g -fno-omit-frame-pointer -no-pie -I"$dir/../common" \
    -o "$out" "$dir"/src/*.c
echo "$out"
