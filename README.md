# binstr

Binstr is a program to extract utf-8 strings runs from any file.
Can be used as a faster alternative to gnu `strings`.

## Usage

```bash
binstr <file>
```

## Benchmark
commit: [71c2c7a5](https://github.com/Saphereye/binstr/commit/71c2c7a5d34b17ca39c28ff233dcd7b6fb0e95b7)

warmup: 3, runs: 10

| Command | Mean [s] | Min [s] | Max [s] | Relative |
|:---|---:|---:|---:|---:|
| `LC_ALL=C strings -n 1` | 7.472 ± 0.240 | 7.053 | 7.764 | 22.90 ± 0.86 |
| `binstr (1 thread)` | 0.754 ± 0.014 | 0.735 | 0.774 | 2.31 ± 0.06 |
| `binstr (2 threads)` | 0.412 ± 0.026 | 0.394 | 0.467 | 1.26 ± 0.08 |
| `binstr (4 threads)` | 0.326 ± 0.006 | 0.319 | 0.341 | 1.00 |

## Scripts

Generate the benchmark table (stdout, markdown):

```bash
./scripts/stats.sh [runs] [size_mb] [testbin]
```

Compare a code change against git HEAD (builds both binaries):

```bash
./scripts/bench.sh [runs] [testbin]              # 1 thread, CPU 0
./scripts/bench.sh mt [runs] [testbin]           # all cores, both binaries
./scripts/bench.sh <baseline> <patch> [runs]   # two binaries directly
```

Test file defaults to `target/bench/bench.bin` (1 GB). Generate with `./scripts/gen.sh`.

## Building

```bash
cargo build --release
```
