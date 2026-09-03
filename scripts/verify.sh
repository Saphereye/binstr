#!/usr/bin/env bash
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TESTBIN="${TESTBIN:-$ROOT/target/bench/bench.bin}"

if [[ ! -f "$TESTBIN" ]]; then
  "$ROOT/scripts/gen.sh"
fi

cargo build --release
git diff --no-index <(./target/release/binstr -N -I "$TESTBIN") <(strings "$TESTBIN")
git diff --no-index <(./target/release/binstr -I -t d /usr/bin/true) <(strings -t d /usr/bin/true)
