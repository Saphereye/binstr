#!/usr/bin/env bash
set -euo pipefail

TESTBIN="/usr/bin/godot"
RUNS="${1:-10}"

PATCH="/tmp/binstr_patch"
BASELINE="/tmp/binstr_baseline"

echo "Building current version..."
cargo build --release
cp target/release/binstr "$PATCH"

echo "Building previous version..."
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

NUM_CPUS=$(nproc)
NUM_CORES=$(lscpu -p=CORE | grep -v '^#' | sort -u | wc -l)
if (( NUM_CORES == 0 )); then
    echo "Could not determine CPU topology"
    exit 1
fi
CPU_STEP=$((NUM_CPUS / NUM_CORES))

CPUS_A=""
CPUS_B=""
for ((i = 0; i < CPU_STEP; i++)); do
    for ((cpu = i; cpu < NUM_CPUS; cpu += CPU_STEP * 2)); do
        CPUS_A+="${CPUS_A:+,}$cpu"
    done
    for ((cpu = i + CPU_STEP; cpu < NUM_CPUS; cpu += CPU_STEP * 2)); do
        CPUS_B+="${CPUS_B:+,}$cpu"
    done
done

echo "CPU sets: A: $CPUS_A, B: $CPUS_B"

BASE_TIMES=()
PATCH_TIMES=()

echo -e "\nRunning $RUNS iterations...\n"
printf "%3s %10s %10s %10s\n" "run" "baseline(ms)" "patch(ms)" "diff(ms)"

for ((i = 0; i < RUNS; i++)); do
    if (( i % 2 == 0 )); then
        BASE_CPUS="$CPUS_A"
        PATCH_CPUS="$CPUS_B"
    else
        BASE_CPUS="$CPUS_B"
        PATCH_CPUS="$CPUS_A"
    fi

    # --- Baseline ---
    start=$(date +%s.%N)
    taskset -c "$BASE_CPUS" "$BASELINE" "$TESTBIN" > /dev/null
    end=$(date +%s.%N)
    BASE_TIME=$(awk "BEGIN {printf \"%.6f\", $end - $start}")
    BASE_TIMES+=("$BASE_TIME")

    # --- Patch ---
    start=$(date +%s.%N)
    taskset -c "$PATCH_CPUS" "$PATCH" "$TESTBIN" > /dev/null
    end=$(date +%s.%N)
    PATCH_TIME=$(awk "BEGIN {printf \"%.6f\", $end - $start}")
    PATCH_TIMES+=("$PATCH_TIME")

    # Format for the table (in ms, 3 decimals)
    BASE_MS=$(awk -v t="$BASE_TIME" 'BEGIN { printf "%.3f", t * 1000 }')
    PATCH_MS=$(awk -v t="$PATCH_TIME" 'BEGIN { printf "%.3f", t * 1000 }')
    DIFF_MS=$(awk -v p="$PATCH_TIME" -v b="$BASE_TIME" 'BEGIN { printf "%+.3f", (p - b) * 1000 }')
    printf "%3d %10s %10s %10s\n" "$((i+1))" "$BASE_MS" "$PATCH_MS" "$DIFF_MS"
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

    # One‑tailed critical values (α=0.05)
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

    # Print results in milliseconds
    printf "Number of runs: %d\n", n
    printf "Mean baseline time: %.3f ms\n", mean_base * 1000
    printf "Mean patch time:    %.3f ms\n", mean_patch * 1000
    printf "Mean difference (patch - baseline): %+.3f ms\n", mean_diff * 1000
    printf "Std deviation of difference:        %.3f ms\n", std_diff * 1000
    printf "t-statistic = %.4f\n", t

    if (crit != "N/A") {
        printf "Critical t (one‑tailed, α=0.05, df=%d): %.3f\n", df, crit
        if (t < -crit)
            printf "Patch is faster\n"
        else if (t > crit)
            printf "Patch is slower\n"
        else
            printf "Inconclusive, timings are basically same.\n"
    } else {
        printf "Not enough samples.\n"
    }
}' <<< "dummy"
