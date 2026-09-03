#!/usr/bin/env bash
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cargo build --release
git diff --no-index \
  <(./target/release/binstr -I -t d /usr/bin/true) \
  <(strings -t d /usr/bin/true)
