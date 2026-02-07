# Compact Action Hash Proof — Circuit Flow

## Goal

Prove knowledge of nullifiers such that

    BLAKE2b-256("ZTxIdOrcActCHash", action_1 || action_2) = action_hash

where each action = nf || cmx || epk || enc[0..52] (148B), without
revealing the nullifiers. The circuit always hashes exactly **2 actions**
(296B total), matching ZIP-244's `hashOrchardActions`.


## Terminology

- **`p`** — field modulus
  > The Pallas base field prime, ~2^254. All field arithmetic is mod p.

- **`IV`** — initialization vector
  > 8 fixed 64-bit constants defined by BLAKE2b (RFC 7693), derived from
  > the fractional parts of sqrt(2..9).

- **`byte-level representation`**
  > Blake2bWord stores a packed 64-bit value alongside its 8 individual
  > byte cells. All XOR and rotation operations work at the byte level
  > using lookup tables, eliminating the need for bit-level cells.


## Circuit Inputs (2 actions)

Each action contributes 148B. For 2 actions: 296B total.

### Per-action inputs

- **`nf`** — nullifier — 32B, split into two 16B field elements
  > `nf_lo` (lower 16B) and `nf_hi` (upper 16B).
  > Each half trivially fits in the Pallas field (16B << 32B), so no
  > canonicality check is needed. Range-checked to 16B via byte
  > decomposition.

- **`cmx`** — note commitment — 32B, split into two 16B field elements
  > `cmx_lo` (lower 16B) and `cmx_hi` (upper 16B).
  > Same split representation as nf. No canonicality check.

- **`epk`** — ephemeral public key — 32B (raw)

- **`enc[0..52]`** — 52B (raw)

### Output

- **`action_hash`** — 32B, split into 2 field elements (the full hash
  doesn't fit in one ~255-bit field element)
  > `hash_0 = word0 + word1 * 2^64` (lower 128 bits) and
  > `hash_1 = word2 + word3 * 2^64` (upper 128 bits).
  > Packed by `encode_result()` and exposed as public inputs via
  > `constrain_instance()`.

- **`"ZTxIdOrcActCHash"`** — 16B BLAKE2b personalization (hardcoded)

```
╔══════════════════════════════════════════════════════════════════════╗
║                         CIRCUIT INPUTS (2 ACTIONS)                   ║
╠══════════════════════════════════════════════════════════════════════╣
║                                                                      ║
║  PRIVATE (auxiliary witness)         PUBLIC INPUTS                   ║
║  ┌───────────────────────────┐      ┌──────────────────────────┐     ║
║  │ Action 1:                 │      │ Action 1:                │     ║
║  │   nf_1_lo (Fp, 16B)       │      │   cmx_1_lo (Fp, 16B)     │     ║
║  │   nf_1_hi (Fp, 16B)       │      │   cmx_1_hi (Fp, 16B)     │     ║
║  │                           │      │   epk_1    (32B)         │     ║
║  ├───────────────────────────┤      │   enc_1    (52B)         │     ║
║  │ Action 2:                 │      ├──────────────────────────┤     ║
║  │   nf_2_lo (Fp, 16B)       │      │ Action 2:                │     ║
║  │   nf_2_hi (Fp, 16B)       │      │   cmx_2_lo (Fp, 16B)     │     ║
║  └───────────┬───────────────┘      │   cmx_2_hi (Fp, 16B)     │     ║
║              │                      │   epk_2    (32B)         │     ║
║              │                      │   enc_2    (52B)         │     ║
║              │                      ├──────────────────────────┤     ║
║              │                      │ expected action_hash     │     ║
║              │                      │ 2 field elements         │     ║
║              │                      └────────┬─────────────────┘     ║
╚══════════════╪═══════════════════════════════╪═══════════════════════╝
               │                               │
               ▼                               ▼
╔══════════════════════════════════════════════════════════════════════╗
║  process_compact_action_hash(action_1, action_2, personalization)    ║
╚════════════════════════════════╤═════════════════════════════════════╝
                                 │
              ┌──────────────────┼──────────────┐
              ▼                  ▼              ▼
    ┌──────────────┐   ┌─────────────┐   ┌──────────────────┐
    │ field_       │   │ Range-check │   │ Range-check      │
    │ decompose_   │   │ epk bytes   │   │ enc bytes        │
    │ split        │   │ (32B each)  │   │ (52B each)       │
    │ nf (32B),    │   └──────┬──────┘   └─────────┬────────┘
    │ cmx (32B)    │          │                    │
    │ per action   │          │                    │
    └──────┬───────┘          │                    │
           │                  │                    │
           └──────────────────┼────────────────────┘
                              │
                    ┌─────────▼──────────────────┐
                    │ Concatenate all bytes in   │
                    │ message order (per action):│
                    │ nf‖cmx‖epk‖enc = 148B      │
                    │ × 2 actions = 296B         │
                    └─────────┬──────────────────┘
                              │
                    ┌─────────▼──────────────────┐
                    │  Pack into 37 x 64-bit     │
                    │  words [s_word_decompose]  │
                    └─────────┬──────────────────┘
                              │
                    ┌─────────▼──────────────────┐
                    │  Block Assembly            │
                    │  Block 1: words [0..15]    │
                    │  Block 2: words [16..31]   │
                    │  Block 3: words [32..36]   │
                    │    + 11 zero-pad words     │
                    └─────────┬──────────────────┘
                              │
                              ▼
  ┌──────────────────────────────────────────────────────────┐
  │  Compression Loop (3 calls for 296B input)               │
  │                                                          │
  │  compress(h, block1, t=128, f=false)                     │
  │  compress(h, block2, t=256, f=false)                     │
  │  compress(h, block3, t=296, f=true)                      │
  │                                                          │
  │  After final compress, h holds the BLAKE2b hash.         │
  └──────────────────────────┬───────────────────────────────┘
                             │
                             ▼
             ┌──────────────────────────────────────────┐
             │ Take h[0..3] (first 4 of 8 words)        │
             │ = 32B BLAKE2b-256 digest                 │
             └──────────────────┬───────────────────────┘
                                │
                                ▼
             ┌──────────────────────────────────────────┐
             │ encode_result()                          │
             │                                          │
             │ Pack 4 words into 2 field elements:      │
             │   field_0 = word0 + word1 * 2^64         │
             │   field_1 = word2 + word3 * 2^64         │
             │                                          │
             │ [s_result_encode] gate constrains the    │
             │ packing. 4 words → 2 fields.             │
             └──────────────────┬───────────────────────┘
                                │
                                ▼
             ┌──────────────────────────────────────────┐
             │ constrain_instance() x 2                 │
             │                                          │
             │ The verifier provides the expected       │
             │ action_hash (packed as 2 fields) in the  │
             │ instance column (public input).          │
             │                                          │
             │   advice[field_0] == instance[row 0]     │
             │   advice[field_1] == instance[row 1]     │
             └──────────────────────────────────────────┘
```


═══════════════════════════════════════════════════════════════════
 FIELD DECOMPOSITION — nf, cmx (16B halves, no canonicality)
═══════════════════════════════════════════════════════════════════

  Each 32B value (nf, cmx) is 256 bits, which exceeds the Pallas
  field capacity (~255 bits). So each is split into **two 16B halves**
  (lo + hi). Since 128 bits << ~255 bits, each half trivially fits
  in the field — no canonicality check is needed.

  **Why this is sound:** In the full Orchard circuit, the nullifier is
  derived from a Poseidon hash (which outputs an Fp element). The Orchard
  circuit constrains `nf = nf_lo + nf_hi * 2^128` and knows nf is
  canonical because it's a constrained Fp output. The Blake2b sub-circuit
  doesn't need to independently verify canonicality — it just needs the
  byte decomposition to be correct, which the 16B range checks
  guarantee.

  `field_decompose_split()` pipeline per (lo, hi) pair:

```
  ┌──────────────────────────────────────────────────────────┐
  │ 1. Decompose lo (16B) into 16 individual bytes           │
  │    Prover provides byte values as witness. Each byte     │
  │    range-checked to [0,255] via byte_range lookup table  │
  │    (prevents prover from claiming a "byte" is e.g. 300). │
  ├──────────────────────────────────────────────────────────┤
  │ 2. Pack 16 bytes into 2 x 64-bit words (8 bytes each)    │
  │    [s_word_decompose] gate constrains:                   │
  │    word = b0 + b1*2^8 + b2*2^16 + ... + b7*2^56          │
  │    Produces Blake2bWord structs for compression.         │
  ├──────────────────────────────────────────────────────────┤
  │ 3. Recomposition check (integrity)                       │
  │    [s_result_encode] gate (reused):                      │
  │    lo == word_0 + word_1 * 2^64                          │
  │    Proves the bytes actually represent the original lo.  │
  │    Without this, prover could hash wrong data.           │
  ├──────────────────────────────────────────────────────────┤
  │ 4. Repeat steps 1-3 for hi → 2 more 64-bit words         │
  ├──────────────────────────────────────────────────────────┤
  │ Result: 4 x 64-bit Blake2bWords per 32B input value      │
  │         No canonicality check needed (16B << ~255 bits). │
  └──────────────────────────────────────────────────────────┘
```


═══════════════════════════════════════════════════════════════════
 BLOCK LAYOUT — 2 actions (296B → 3 blocks)
═══════════════════════════════════════════════════════════════════

```
  Action 1 (148B):
    nf_1 (32B) + cmx_1 (32B) + epk_1 (32B) + enc_1 (52B)

  Action 2 (148B):
    nf_2 (32B) + cmx_2 (32B) + epk_2 (32B) + enc_2 (52B)

  Total: 296B = 37 x 64-bit words

  Block 1 (words 0-15):  128B
    nf_1(32) + cmx_1(32) + epk_1(32) + enc_1[0..32]

  Block 2 (words 16-31): 128B
    enc_1[32..52](20) + nf_2(32) + cmx_2(32) + epk_2(32) + enc_2[0..12]

  Block 3 (words 32-36): 40B + 88B zero padding
    enc_2[12..52](40) + zeros(88)

  Compression calls:
    compress(h, block1, t=128, f=false)
    compress(h, block2, t=256, f=false)
    compress(h, block3, t=296, f=true)
```


═══════════════════════════════════════════════════════════════════
 COMPRESSION — compress() (RFC 7693 §3.2)
═══════════════════════════════════════════════════════════════════

  Called once per block. For 296B input: 3 calls.

```
  ┌────────────────────────────────────────────────────────────┐
  │ 1. Init working vector v[0..15]:                           │
  │    v[0..7]  = h[0..7]            (current state)           │
  │    v[8..11] = IV[0..3]           (constants)               │
  │    v[12]    = IV[4] XOR t_lo     (constant)                │
  │    v[13]    = IV[5] XOR t_hi     (constant)                │
  │    v[14]    = IV[6] XOR 0xFF..FF (if final, constant)      │
  │    v[15]    = IV[7]              (constant)                │
  ├────────────────────────────────────────────────────────────┤
  │ 2. 12 rounds of mixing:                                    │
  │    Each round: 8 G() calls (4 column + 4 diagonal)         │
  │    Total: 12 × 8 = 96 G invocations per compress call      │
  ├────────────────────────────────────────────────────────────┤
  │ 3. Finalize:                                               │
  │    h[i] = h[i] XOR v[i] XOR v[i+8]   for i = 0..7         │
  │    (two byte-level word_xor + word reconstruction)         │
  └────────────────────────────────────────────────────────────┘
```


═══════════════════════════════════════════════════════════════════
 G MIXING FUNCTION — G(v, a, b, c, d, x, y) (RFC 7693 §3.1)
═══════════════════════════════════════════════════════════════════

  The atomic mixing unit. Uses Add-Rotate-XOR (ARX) at the **byte level**.
  Called 96 times per compression call.

  All arithmetic is mod 2^64. Rotations are right-rotations.

```
  ┌──────────────────────────────────────────────────────────┐
  │ v[a] = v[a] + v[b] + x          [s_word_add x 2]         │
  │ v[d] = (v[d] XOR v[a]) >>> 32   byte shuffle [4..7,0..3] │
  │ v[c] = v[c] + v[d]              [s_word_add]             │
  │ v[b] = (v[b] XOR v[c]) >>> 24   byte shuffle [3..7,0..2] │
  │ v[a] = v[a] + v[b] + y          [s_word_add x 2]         │
  │ v[d] = (v[d] XOR v[a]) >>> 16   byte shuffle [2..7,0..1] │
  │ v[c] = v[c] + v[d]              [s_word_add]             │
  │ v[b] = (v[b] XOR v[c]) >>> 63   [s_left_shift_1] gate    │
  └──────────────────────────────────────────────────────────┘
```

  **XOR:** Each byte XOR splits both input bytes into nibbles, performs
  two 4-bit XOR lookups (lo and hi), and recombines. 1 row per byte XOR,
  using 9 of 10 advice columns.

  **Rotation details:**
  - **R1=32:** Pure byte shuffle `[4,5,6,7,0,1,2,3]` — **zero constraints**
  - **R2=24:** Pure byte shuffle `[3,4,5,6,7,0,1,2]` — **zero constraints**
  - **R3=16:** Pure byte shuffle `[2,3,4,5,6,7,0,1]` — **zero constraints**
  - **R4=63:** Equivalent to left-rotate by 1 bit. Uses `s_left_shift_1`
    gate: `2 * byte_in + carry_in = byte_out + 256 * carry_out`,
    `bool_check(carry_out)`.
    3 constraints per byte × 8 bytes = **24 constraints** per R4 rotation.

  **Per G call:**
  - 6 additions (s_word_add)
  - 4 XOR operations (8 byte lookups each = 32 lookups)
  - 3 free byte shuffles (R1, R2, R3)
  - 1 left-shift-1 (R4, 24 constraints)
  - 4 word reconstructions from bytes (s_word_decompose)

  **Per compression call:** 96 G calls


═══════════════════════════════════════════════════════════════════
 BLAKE2b WORD REPRESENTATION
═══════════════════════════════════════════════════════════════════

```
  pub struct Blake2bWord<F: PrimeField> {
      word: AssignedCell<F, F>,        // packed 64-bit value
      bytes: [AssignedCell<F, F>; 8],  // 8 byte cells, each in [0,255]
  }
```

  All operations work at the byte level:
  - XOR: nibble-level lookup (2 lookups per byte, 16 per word)
  - Rotation: byte shuffle (free) or s_left_shift_1 (R4 only)
  - Addition: operates on packed word (unchanged)
  - Decomposition: word ↔ bytes via s_word_decompose (unchanged)


═══════════════════════════════════════════════════════════════════
 LOOKUP TABLES
═══════════════════════════════════════════════════════════════════

  Two lookup tables are loaded once during synthesis:

  **1. Nibble XOR Table** (256 entries):
```
    (lhs, rhs, out) for all lhs, rhs in [0, 15]
    out = lhs XOR rhs

    Each byte XOR uses 9 columns on 1 row:
      A0=byte_a  A1=byte_b  A2=lo_a  A3=hi_a
      A4=lo_b    A5=hi_b    A6=lo_out A7=hi_out  A8=out_byte

    Gate (q_nibble_xor):
      byte_a = lo_a + hi_a * 16
      byte_b = lo_b + hi_b * 16
      out_byte = lo_out + hi_out * 16

    Lookups (2, sharing same table):
      (lo_a, lo_b, lo_out) in XOR table
      (hi_a, hi_b, hi_out) in XOR table
```

  **2. Byte Range Table** (256 entries):
```
    [0, 1, 2, ..., 255]

    Lookup constraint (q_range_check_8 selector):
      (s * val, range_table)

    Used to range-check byte cells to [0, 255].
```


═══════════════════════════════════════════════════════════════════
 CUSTOM GATES AND LOOKUPS
═══════════════════════════════════════════════════════════════════

  **Gates:**

```
  Gate                Purpose                              Cost
  ─────────────────── ──────────────────────────────────── ──────────
  s_word_decompose    word = b1 + b2*2^8 + ... + b8*2^56  1 constraint
  s_word_add          lhs + rhs = out + carry*2^64         2 constraints
  s_result_encode     field = w1 + w2*2^64                 1 constraint
  s_word_combine      w64 = w32_lo + w32_hi*2^32           1 constraint
  s_left_shift_1      2*in + c_in = out + 256*c_out        3 constraints/byte
  q_nibble_xor        byte = lo + hi*16 (decompose)        3 constraints
```

  **Lookups:**

```
  Lookup              Purpose                              Cost
  ─────────────────── ──────────────────────────────────── ──────────
  nibble_xor_lo       (lo_a, lo_b, lo_out) in XOR table    1 lookup/byte
  nibble_xor_hi       (hi_a, hi_b, hi_out) in XOR table    1 lookup/byte
  byte_range          val in [0, 255]                       1 lookup/byte
```
