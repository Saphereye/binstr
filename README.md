# binstr

Binstr is a program to extract utf-8 strings runs from any file.
Can be used as a faster alternative to gnu `strings`.

## Usage

```bash
binstr <file>
```

## Benchmark
commit: [5ee15495](https://github.com/Saphereye/binstr/commit/5ee15495aeef77ec5f12696ea7ae11232d500b14)

warmup: 3, runs: 10

| Command | Mean [s] | Min [s] | Max [s] | Relative |
|:---|---:|---:|---:|---:|
| `strings -n 1` | 7.658 ± 0.359 | 7.229 | 8.257 | 26.12 ± 1.27 |
| `binstr (1 thread)` | 0.814 ± 0.028 | 0.787 | 0.854 | 2.78 ± 0.10 |
| `binstr (2 threads)` | 0.450 ± 0.008 | 0.433 | 0.463 | 1.54 ± 0.03 |
| `binstr (4 threads)` | 0.293 ± 0.004 | 0.289 | 0.299 | 1.00 |

## Building

```bash
cargo build --release
```
