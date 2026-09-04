#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNS="${1:-10}"
SIZE_MB="${2:-1024}"
TESTBIN="${3:-$ROOT/target/bench/bench.bin}"
BIN="$ROOT/target/release/binstr"
WARMUP="${WARMUP:-3}"
HF_OUT="$(mktemp)"
trap 'rm -f "$HF_OUT"' EXIT

[[ -f "$TESTBIN" ]] || "$ROOT/scripts/gen.sh" "$TESTBIN" "$SIZE_MB"
cargo build --release --manifest-path "$ROOT/Cargo.toml" >/dev/null

hyperfine --warmup "$WARMUP" --runs "$RUNS" --export-markdown "$HF_OUT" \
  -n "strings -n 1" "strings -n 1 '$TESTBIN' > /dev/null" \
  -n "binstr (1 thread)" "RAYON_NUM_THREADS=1 '$BIN' -N -I '$TESTBIN' > /dev/null" \
  -n "binstr (2 threads)" "RAYON_NUM_THREADS=2 '$BIN' -N -I '$TESTBIN' > /dev/null" \
  -n "binstr (4 threads)" "RAYON_NUM_THREADS=4 '$BIN' -N -I '$TESTBIN' > /dev/null" \
  >&2

HASH=$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)
SHORT="${HASH:0:8}"
BASE=$(git -C "$ROOT" remote get-url origin 2>/dev/null | sed -E 's#.*github.com[:/]([^/]+)/([^/.]+)(\.git)?#https://github.com/\1/\2#')
echo "commit: [$SHORT]($BASE/commit/$HASH)"
echo ""
echo "warmup: $WARMUP, runs: $RUNS"
echo
cat "$HF_OUT"
