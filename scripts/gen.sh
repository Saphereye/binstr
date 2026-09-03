#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$root/target/bench/bench.bin}"
size_mb="${2:-173}"
seed="${SEED:-42}"
gen="$root/target/bench/gen"

mkdir -p "$(dirname "$out")" "$(dirname "$gen")"

if [[ ! -x "$gen" || "$0" -nt "$gen" ]]; then
  cc -O3 -x c - -o "$gen" <<'EOF'
// TODO: rewrite this in rust
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

static uint32_t seed;

static uint32_t rnd(void)
{
  seed = (uint32_t)(seed * 1103515245u + 12345u) & 0x7fffffffu;
  return seed;
}

static uint8_t entropy_byte(void)
{
  uint32_t r = rnd() % 256;

  if (rnd() % 1000 < 95)
    return 0;
  if (r >= 32 && r <= 126 && rnd() % 100 < 65)
    return (uint8_t)(128 + (r % 128));
  return (uint8_t)r;
}

static void string_region(size_t n)
{
  size_t filled = 0;

  while (filled < n) {
    size_t gap = 9 + rnd() % 26;
    size_t i;

    for (i = 0; i < gap && filled < n; i++, filled++)
      putchar(0);
    if (filled >= n)
      break;

    size_t len = 4 + rnd() % 57;

    if (filled + len + 1 > n)
      len = n - filled - 1;
    if (len < 4)
      break;

    for (i = 0; i < len; i++)
      putchar((char)(32 + rnd() % 95));
    putchar(0);
    filled += len + 1;
  }

  while (filled < n)
    putchar(0), filled++;
}

int main(int argc, char **argv)
{
  unsigned blocks = 173;
  unsigned base_seed = 42;

  if (argc > 1)
    blocks = (unsigned)atoi(argv[1]);
  if (argc > 2)
    base_seed = (unsigned)atoi(argv[2]);

  setvbuf(stdout, NULL, _IOFBF, 1 << 20);

  for (unsigned block = 0; block < blocks; block++) {
    seed = base_seed + block * 9973u;
    for (size_t i = 0; i < 768u * 1024u; i++)
      putchar((char)entropy_byte());
    string_region(256u * 1024u);
  }

  return 0;
}
EOF
fi

echo "Generating ${size_mb}MB bench file (seed=$seed) -> $out"
LC_ALL=C "$gen" "$size_mb" "$seed" >"$out"
echo "Wrote $(numfmt --to=iec "$(stat -c%s "$out")") ($(strings "$out" | wc -l) strings)"
