# binstr

Binstr is a program to extract utf-8 strings runs from any file.
Can be used as a faster alternative to gnu `strings`.

## Usage

```bash
binstr <file>
```

## Benchmark
These were taken with main branch on commit [50dbb5ab](https://github.com/Saphereye/binstr/commit/50dbb5abfbed67ad62fb82340894f1411e7c957a).

| | mean time | vs `strings -n 1` |
|---|---|---|
| `strings -n 1` (baseline) | ~1639 ms | — |
| `binstr`, 4 threads | ~168 ms | ~9.7x faster |
| `binstr`, single threaded | ~325 ms | ~5.0x faster |

These were done on a ~100Mb binary and can be reproduced using `./scripts/bench.sh`

## Building

```bash
cargo build --release
```
