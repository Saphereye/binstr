#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEFAULT_BIN="${TESTBIN:-$ROOT/target/bench/bench.bin}"

# Default: 1 thread / 1 core (low noise). Pass `mt` for all cores on both binaries.
WARMUP="${WARMUP:-3}"

all_cpus() {
  local n max=$(( $(nproc) - 1 ))
  local cpus=""
  local i
  for ((i = 0; i <= max; i++)); do
    cpus+="${cpus:+,}$i"
  done
  printf '%s\n' "$cpus"
}

usage() {
  cat <<EOF
usage: $0 [runs] [testbin]
       $0 mt [runs] [testbin]
       $0 <baseline> <patch> [testbin] [runs]
       $0 mt <baseline> <patch> [testbin] [runs]

Default: 1 thread, CPU 0 (low-noise A/B).
  mt:     RAYON_NUM_THREADS=\$(nproc), taskset all cores — same for baseline and patch.

Throughput vs strings: ./scripts/stats.sh
EOF
}

if [[ "${1:-}" == "mt" ]]; then
  MT=1
  shift
  THREADS="${THREADS:-$(nproc)}"
  CPUS="${CPUS:-$(all_cpus)}"
else
  MT=0
  THREADS="${THREADS:-1}"
  CPUS="${CPUS:-0}"
fi

build_pair() {
  PATCH="/tmp/binstr_patch"
  BASELINE="/tmp/binstr_baseline"

  echo "Building patch (working tree)..."
  cargo build --release
  cp target/release/binstr "$PATCH"

  echo "Building baseline (clean tree)..."
  STASHED=0
  if ! git diff --quiet || ! git diff --cached --quiet; then
    git stash push -q
    STASHED=1
  fi
  cargo build --release
  cp target/release/binstr "$BASELINE"
  if (( STASHED )); then
    git stash pop -q
  fi
}

resolve_tool() {
  local tool=$1

  if [[ -f "$tool" && -x "$tool" ]]; then
    printf '%s\n' "$tool"
  elif command -v "$tool" >/dev/null; then
    command -v "$tool"
  else
    return 1
  fi
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -ge 2 && ! "$1" =~ ^[0-9]+$ ]]; then
  if ! BASELINE=$(resolve_tool "$1"); then
    echo "Missing baseline tool: $1" >&2
    exit 1
  fi
  if ! PATCH=$(resolve_tool "$2"); then
    echo "Missing patch tool: $2" >&2
    exit 1
  fi
  shift 2
  if [[ $# -ge 1 && "$1" =~ ^[0-9]+$ ]]; then
    RUNS=$1
    TESTBIN="${2:-$DEFAULT_BIN}"
  else
    TESTBIN="${1:-$DEFAULT_BIN}"
    RUNS="${2:-10}"
  fi
else
  RUNS="${1:-10}"
  TESTBIN="${2:-$DEFAULT_BIN}"
  build_pair
fi

if [[ ! -f "$TESTBIN" ]]; then
  "$ROOT/scripts/gen.sh" "$TESTBIN"
fi

run_scan() {
  local tool=$1

  if [[ "$(basename "$tool")" == "strings" ]]; then
    taskset -c "$CPUS" env LC_ALL=C "$tool" "$TESTBIN" > /dev/null
  else
    taskset -c "$CPUS" env RAYON_NUM_THREADS="$THREADS" "$tool" -N -I "$TESTBIN" > /dev/null
  fi
}

time_scan() {
  local tool=$1

  run_scan "$tool"

  local start end
  start=$(date +%s.%N)
  run_scan "$tool"
  end=$(date +%s.%N)
  awk -v s="$start" -v e="$end" 'BEGIN { printf "%.6f", e - s }'
}

echo "Baseline: $BASELINE"
echo "Patch:    $PATCH"
echo "Test file: $TESTBIN ($(numfmt --to=iec "$(stat -c%s "$TESTBIN")"))"
echo "Setup: RAYON_NUM_THREADS=$THREADS, taskset -c $CPUS$([[ $MT -eq 1 ]] && echo ' (mt)')"

BASE_TIMES=()
PATCH_TIMES=()

for ((w = 0; w < WARMUP; w++)); do
  run_scan "$BASELINE"
  run_scan "$PATCH"
done

echo -e "\nRunning $RUNS iterations...\n"
printf "%3s %10s %10s %10s\n" "run" "baseline(ms)" "patch(ms)" "diff(ms)"

for ((i = 0; i < RUNS; i++)); do
  BASE_TIME=$(time_scan "$BASELINE")
  BASE_TIMES+=("$BASE_TIME")

  PATCH_TIME=$(time_scan "$PATCH")
  PATCH_TIMES+=("$PATCH_TIME")

  BASE_MS=$(awk -v t="$BASE_TIME" 'BEGIN { printf "%.3f", t * 1000 }')
  PATCH_MS=$(awk -v t="$PATCH_TIME" 'BEGIN { printf "%.3f", t * 1000 }')
  DIFF_MS=$(awk -v p="$PATCH_TIME" -v b="$BASE_TIME" 'BEGIN { printf "%+.3f", (p - b) * 1000 }')
  printf "%3d %10s %10s %10s\n" "$((i + 1))" "$BASE_MS" "$PATCH_MS" "$DIFF_MS"
done

echo
echo "=== Results (ms) ==="

awk -v base_times="${BASE_TIMES[*]}" -v patch_times="${PATCH_TIMES[*]}" '
BEGIN {
    split(base_times, b, " ")
    split(patch_times, p, " ")
    n = length(b)
    if (n == 0) exit

    sum_b = 0
    sum_p = 0
    sum_diff = 0
    sum_sq_diff = 0

    for (i = 1; i <= n; i++) {
        diff = p[i] - b[i]
        sum_b += b[i]
        sum_p += p[i]
        sum_diff += diff
        sum_sq_diff += diff * diff
    }

    mean_base = sum_b / n
    mean_patch = sum_p / n
    mean_diff = sum_diff / n

    if (n > 1) {
        var = (sum_sq_diff - sum_diff * sum_diff / n) / (n - 1)
        std_diff = sqrt(var)
        se = std_diff / sqrt(n)
        t = mean_diff / se
    } else {
        std_diff = 0
        t = 0
    }

    df = n - 1

    if (df >= 1 && df <= 30) {
        crit_table[1]=6.314;  crit_table[2]=2.920;  crit_table[3]=2.353;  crit_table[4]=2.132;
        crit_table[5]=2.015;  crit_table[6]=1.943;  crit_table[7]=1.895;  crit_table[8]=1.860;
        crit_table[9]=1.833;  crit_table[10]=1.812; crit_table[11]=1.796; crit_table[12]=1.782;
        crit_table[13]=1.771; crit_table[14]=1.761; crit_table[15]=1.753; crit_table[16]=1.746;
        crit_table[17]=1.740; crit_table[18]=1.734; crit_table[19]=1.729; crit_table[20]=1.725;
        crit_table[21]=1.721; crit_table[22]=1.717; crit_table[23]=1.714; crit_table[24]=1.711;
        crit_table[25]=1.708; crit_table[26]=1.706; crit_table[27]=1.703; crit_table[28]=1.701;
        crit_table[29]=1.699; crit_table[30]=1.697;
        crit = crit_table[df]
    } else if (df > 30) {
        crit = 1.645
    } else {
        crit = "N/A"
    }

    printf "Number of runs: %d\n", n
    printf "Mean baseline time: %.3f ms\n", mean_base * 1000
    printf "Mean patch time:    %.3f ms\n", mean_patch * 1000
    printf "Mean difference (patch - baseline): %+.3f ms\n", mean_diff * 1000
    printf "Std deviation of difference:        %.3f ms\n", std_diff * 1000
    printf "t-statistic = %.4f\n", t

    if (crit != "N/A") {
        printf "Critical t (one-tailed, alpha=0.05, df=%d): %.3f\n", df, crit
        if (t < -crit)
            printf "Patch is faster\n"
        else if (t > crit)
            printf "Patch is slower\n"
        else
            printf "Inconclusive, timings are basically same.\n"
    } else {
        printf "Not enough samples.\n"
    }
}'
