# Blake2b Circuit Optimization Options

## Current Bottleneck

The byte XOR lookup table (256 x 256 = 65,536 entries) forces K=17. The actual circuit logic uses only ~15,000-18,000 rows (fits in K=15).

## Comparison

| Approach | Chunk size | XOR table | Lookups/byte | Circuit rows (est.) | Min K | Proving speedup |
|---|---|---|---|---|---|---|
| Current | 8-bit | 65,536 | 1 | ~18,000 | 17 | baseline (~14s) |
| 5-bit + 3-bit | 5-bit | 1,024 | 2 | ~25,000 | 15 | ~3-4x |
| Nibble | 4-bit | 256 | 2 | ~25,000 | 15 | ~3-4x |
| 3-bit + 3-bit + 2-bit | 3-bit | 64 | 3 | ~32,000 | 15 | ~3-4x |

Notes:
- 4-bit (nibble) is the sweet spot: 4+4=8 divides evenly, cleanest split/recombine, smallest table at 2 lookups/byte
- 5-bit offers no advantage over 4-bit: same lookup count, 4x larger table, awkward 5+3 split
- 3-bit uses 3 lookups/byte with a tiny table but more circuit rows; may not fit in K=15

## Other Options (no K change)

1. **Pack XOR lookups** — 3 XOR ops per row instead of 1 (saves ~3,000 rows per compression, modest impact)
2. **Eliminate redundant `from_bytes_unchecked` calls** — Skip intermediate word packing between rotation and addition
