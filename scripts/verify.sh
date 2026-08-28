#!/usr/bin/env bash
set -e

TESTBIN="/usr/bin/godot"

cargo build --release
./target/release/binstr $TESTBIN > baseline.todiff
strings -n 1 $TESTBIN > strings.todiff

git diff --no-index baseline.todiff strings.todiff
