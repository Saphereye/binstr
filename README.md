# binstr

Binstr is a program to extract utf-8 strings runs from any file.
Can be used as a faster alternative to gnu `strings`.

## Usage

```bash
binstr <file>
```

## Benchmark
These were taken with main branch on commit [46b8b8](https://github.com/Saphereye/binstr/commit/46b8b8cc5ab177d8d151027e92c847d8a0630b7d).

| | mean time | vs `strings -n 1` |
|---|---|---|
| `strings -n 1` (baseline) | ~1639–1650 ms | — |
| `binstr`, 2 threads | 280.477 ms | ~5.8x faster |
| `binstr`, `RAYON_NUM_THREADS=1` | 545.870 ms | ~3.0x faster |

These were done on a ~100Mb binary and can be reproduced using `./scripts/bench.sh`

## Building

```bash
cargo build --release
```
