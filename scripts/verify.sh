#!/usr/bin/env bash
set -e

TESTBIN="/usr/bin/godot"

cargo build --release
git diff --no-index <(./target/release/binstr -N -I "$TESTBIN") <(strings "$TESTBIN")
git diff --no-index <(./target/release/binstr -I -t d /usr/bin/true) <(strings -t d /usr/bin/true)
