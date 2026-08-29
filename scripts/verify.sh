#!/usr/bin/env bash
set -e

TESTBIN="/usr/bin/godot"

cargo build --release
git diff --no-index <(./target/release/binstr -N -I /usr/bin/godot) <(strings /usr/bin/godot)
