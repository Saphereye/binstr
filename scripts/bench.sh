#!/usr/bin/env bash
set -e

TESTBIN="/usr/bin/godot" # Biggest binary on my system rn
# TODO change this to a gen script

echo "Building current version..."
cargo build --release
cp ./target/release/binstr /tmp/binstr_patch

echo "Building previous version..."
STASHED=0
if ! git diff --quiet || ! git diff --cached --quiet; then
    git stash
    STASHED=1
fi

cargo build --release
cp ./target/release/binstr /tmp/binstr_baseline

if [ "$STASHED" -eq 1 ]; then
    git stash pop
fi

echo "Running benchmark..."
hyperfine --warmup 3 \
  --export-json /tmp/bench_results.json \
  -n "Patch" "/tmp/binstr_patch $TESTBIN" \
  -n "Baseline" "/tmp/binstr_baseline $TESTBIN"
