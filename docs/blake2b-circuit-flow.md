# Compact Action Hash Nullifier Proof — Circuit Flow

## Goal

Prove knowledge of a nullifier such that

    BLAKE2b-256("ZTxIdOrcActCHash", nf || cmx || epk || enc[0..52]) = action_hash

without revealing the nullifier.


## Terminology

- **`p`** — field modulus
  > The Pallas base field prime, ~2^254. All field arithmetic is mod p.

- **`canonicality`**
  > A field element x has two 256-bit representations: x and x+p
  > (since x ≡ x+p mod p). Both decompose to different bytes, producing
  > different BLAKE2b hashes. The canonicality check forces x < p,
  > ensuring a unique byte representation. Required for nf and cmx
  > (which are field elements) but NOT for epk or enc — those are raw
  > bytes, not field elements. Their byte values *are* the data; there
  > is no mod-p equivalence to resolve.

- **`IV`** — initialization vector
  > 8 fixed 64-bit constants defined by BLAKE2b (RFC 7693), derived from
  > the fractional parts of sqrt(2..9). The State Init step XORs some IV
  > words with the parameter block (digest length, personalization) to
  > produce the starting hash state h.


## ZIP-244 Compact Action Hash Inputs

### Pallas field elements (canonicality checked, must be < p)

- **`nf`** — nullifier — 32 bytes
  > A unique tag derived from a note's secret key and position.
  > Publicly revealing it marks a note as spent.
  > This is the value we keep private in the proof.

- **`cmx`** — note commitment — 32 bytes
  > A Pedersen-like commitment to the note's contents (recipient,
  > value, etc.). Binds the action to a specific output note without
  > revealing its details.

### Arbitrary bytes (boolean-constrained only, may exceed p)

- **`epk`** — ephemeral public key — 32 bytes
  > A one-time Diffie-Hellman key used by the recipient to decrypt
  > the note. Serialized as a curve point.

- **`enc[0..52]`** — 52 bytes
  > The first 52 bytes of the encrypted note ciphertext. Contains
  > the encrypted plaintext header (diversifier, value, rseed).

### Other

- **`action_hash`** — 256 bits
  > The expected BLAKE2b-256 output. The verifier provides this
  > publicly; the circuit proves the inputs hash to it.

- **`"ZTxIdOrcActCHash"`** — 16 bytes
  > The BLAKE2b personalization string defined by ZIP-244 for the
  > compact action hash digest.

```
╔══════════════════════════════════════════════════════════════════════╗
║                         CIRCUIT INPUTS                               ║
╠══════════════════════════════════════════════════════════════════════╣
║                                                                      ║
║  PRIVATE (auxiliary witness)        PUBLIC INPUTS                    ║
║  ┌─────────────────────┐           ┌──────────────────────────┐      ║
║  │ nullifier           │           │ cmx                      │      ║
║  │ 32B, Pallas field   │           │ 32B, Pallas field        │      ║
║  └─────────┬───────────┘           ├──────────────────────────┤      ║
║            │                       │ epk                      │      ║
║            │                       │ 32B, curve point         │      ║
║            │                       ├──────────────────────────┤      ║
║            │                       │ enc[0..52]               │      ║
║            │                       │ 52B, ciphertext          │      ║
║            │                       ├──────────────────────────┤      ║
║            │                       │ expected action_hash     │      ║
║            │                       │ 256 bits (4 x 64-bit)    │      ║
║            │                       └────────┬─────────────────┘      ║
╚════════════╪════════════════════════════════╪════════════════════════╝
             │                                │
             ▼                                ▼
╔══════════════════════════════════════════════════════════════════════╗
║  process_hybrid(field_inputs, byte_inputs, personalization)          ║
║    field_inputs  = [nullifier, cmx]                                  ║
║    byte_inputs   = epk(32) ++ enc(52) = 84 bytes                     ║
║    personalization = "ZTxIdOrcActCHash"                              ║
╚════════════════════════╤═════════════════════════════════════════════╝
                         │
          ┌──────────────┼──────────────┐
          ▼              ▼              ▼
┌──────────────┐  ┌────────────┐  ┌──────────────────┐
│ State Init   │  │ Field Path │  │   Byte Path      │
│ (see below)  │  │ nf, cmx    │  │ epk, enc[0..52]  │
└──────┬───────┘  └─────┬──────┘  └─────────┬────────┘
       │                │                   │
       │                ▼                   ▼
       │         ┌─────────────┐     ┌─────────────┐
       │         │ 4+4 = 8     │     │ 4+7 = 11    │
       │         │ 64-bit words│     │ 64-bit words│
       │         └──────┬──────┘     └──────┬──────┘
       │                │                   │
       │                └───────┬───────────┘
       │                        ▼
       │              ┌────────────────────┐
       │              │  Block Assembly    │
       │              │  19 words total    │
       │              │  Block 1: [0..15]  │
       │              │  Block 2: [16..18] │
       │              │  + 13 zero words   │
       │              │  (BLAKE2b requires │
       │              │  full 16-word      │
       │              │  blocks)           │
       │              └────────┬───────────┘
       │                       │
       ▼                       ▼
   ┌──────────────────────────────────────────────────────────┐
   │  Compression Loop                                        │
   │                                                          │
   │  h = 8-word (512-bit) running state, starts from         │
   │      State Init (IV with personalization XORed in).      │
   │  m = the 16-word message block being compressed.         │
   │                                                          │
   │  Step 1: compress(h, m=block1, t=128, f=false)           │
   │          Mixes block 1 into h. t=128 means 128 bytes     │
   │          processed so far. f=false: not the last block.  │
   │                                                          │
   │  Step 2: compress(h, m=block2, t=148, f=true)            │
   │          Mixes block 2 into h. t=148 = total real input  │
   │          bytes (2x32 + 84). f=true: final block, which   │
   │          flips v[14] to signal finalization.             │
   │                                                          │
   │  After step 2, h holds the full BLAKE2b hash.            │
   └──────────────────────────┬───────────────────────────────┘
                              │
                              ▼
              ┌──────────────────────────────────────────┐
              │ Take h[0..3] (first 4 of 8 words)        │
              │ = 256-bit BLAKE2b-256 digest of          │
              │   nf || cmx || epk || enc[0..52]         │
              │ (h[4..7] discarded — only needed         │
              │  for BLAKE2b-512)                        │
              └──────────────────┬───────────────────────┘
                                 │
                                 ▼
              ┌──────────────────────────────────────────┐
              │ encode_result()                          │
              │                                          │
              │ The hash is 4 x 64-bit words, but halo2  │
              │ public inputs must be field elements.    │
              │ So we pack pairs of words into fields:   │
              │   field_0 = word0 + word1 * 2^64         │
              │   field_1 = word2 + word3 * 2^64         │
              │                                          │
              │ [s_result_encode] gate constrains the    │
              │ packing is correct. 4 words → 2 fields.  │
              └──────────────────┬───────────────────────┘
                                 │
                                 ▼
              ┌──────────────────────────────────────────┐
              │ constrain_instance() x 2                 │
              │                                          │
              │ The verifier provides the expected       │
              │ action_hash (also packed as 2 fields)    │
              │ in the instance column (public input).   │
              │ The prover's field_0, field_1 live in    │
              │ advice cells (private computation).      │
              │                                          │
              │ Two equality constraints:                │
              │   advice[field_0] == instance[row 0]     │
              │   advice[field_1] == instance[row 1]     │
              │                                          │
              │ Both must match. If the prover used the  │
              │ wrong nullifier, the hash won't match,   │
              │ and the proof is invalid.                │
              └──────────────────────────────────────────┘


═══════════════════════════════════════════════════════════════════
 STATE INIT — BLAKE2b-256 (RFC 7693 §2.5)
═══════════════════════════════════════════════════════════════════

  h[0] = IV[0] XOR 0x01010020       parameter block (little-endian):
                                      0x 01 01 00 20
                                         │  │  │  └── digest length = 0x20 = 32 bytes
                                         │  │  └───── key length    = 0
                                         │  └──────── fanout        = 1 (sequential)
                                         └─────────── depth         = 1 (sequential)
  h[1] = IV[1]
  h[2] = IV[2]
  h[3] = IV[3]
  h[4] = IV[4]
  h[5] = IV[5]
  h[6] = IV[6] XOR "ZTxIdOrc"       first 8 bytes of personalization
  h[7] = IV[7] XOR "ActCHash"       last 8 bytes of personalization

  How constants enter the circuit (Blake2bWord::from_constant_u64()):
    The compression function XORs values bit-by-bit, so every
    word — even a known constant like IV[0] — must be decomposed
    into individual bits the circuit can operate on:
      u64 → 8 bytes (each in its own cell) → 64 bits (each boolean-constrained)
    [s_word_decompose] proves the 8 bytes reconstruct the word.
    [s_byte_decompose] proves each byte's 8 bits reconstruct that byte.


═══════════════════════════════════════════════════════════════════
 FIELD ELEMENT PATH — nullifier, cmx (canonicality checked)
═══════════════════════════════════════════════════════════════════

  field_decompose() pipeline per field element:

  ┌──────────────────────────────────────────────────────────┐
  │ 1. field → 32 bytes → 256 bits                           │
  │    Blake2bByte::from_u8() for each byte                  │
  │    [s_byte_decompose] bool-constrains every bit          │
  ├──────────────────────────────────────────────────────────┤
  │ 2. 32 bytes → 8 x 32-bit words                           │
  │    assign_word_32_from_bytes()                           │
  │    [s_word_decompose] gate (upper 4 bytes zeroed)        │
  ├──────────────────────────────────────────────────────────┤
  │ 3. Recomposition check                                   │
  │    [s_field_decompose] gate                              │
  │    field = w1 + w2*2^32 + w3*2^64 + ... + w8*2^224       │
  ├──────────────────────────────────────────────────────────┤
  │ 4. CANONICALITY CHECK (see detail below)                 │
  │    Ensures 256-bit decomposition < p                     │
  │    Prevents x vs x+p ambiguity                           │
  ├──────────────────────────────────────────────────────────┤
  │ 5. Combine pairs → 4 x 64-bit words                      │
  │    [s_word_combine] gate                                 │
  │    w64 = w32_lo + w32_hi * 2^32                          │
  └──────────────────────────────────────────────────────────┘

  Canonicality check detail (check_canonicality):

    Pallas p = 0x40000000_00000000_..._224698fc_..._00000001

    ┌───────────────────────────────────────────────────────┐
    │ a. compute_lower_128_and_diff()                       │
    │    lower_128 = w1 + w2*2^32 + w3*2^64 + w4*2^96       │
    │    diff = p_lower - 1 - lower_128                     │
    ├───────────────────────────────────────────────────────┤
    │ b. [s_canonicality] gate:                             │
    │    - bit[255] must be 0                               │
    │    - lower_128 decomposition correct                  │
    │    - bit[254] * (diff - (p_lower-1-lower_128)) = 0    │
    │    - bit[254] * (diff - sum_of_diff_words) = 0        │
    ├───────────────────────────────────────────────────────┤
    │ c. [s_high_bit_zero] x 126 rows:                      │
    │    bit[254] * bit[i] = 0   for i in [128..254)        │
    │    (if bit 254 is set, all bits 128-253 must be 0)    │
    ├───────────────────────────────────────────────────────┤
    │ d. Range-check each diff word (4 words):              │
    │    word → 4 bytes → bits                              │
    │    [s_word_decompose] + [s_byte_decompose]            │
    │    Proves diff >= 0, thus lower_128 < p_lower         │
    └───────────────────────────────────────────────────────┘


═══════════════════════════════════════════════════════════════════
 RAW BYTE PATH — epk (32B), enc[0..52] (52B)
═══════════════════════════════════════════════════════════════════

  bytes_to_words() pipeline:

  ┌──────────────────────────────────────────────────────────┐
  │ 1. Each byte → 8 bits                                    │
  │    [s_byte_decompose] gate (boolean constraints only)    │
  │    NO canonicality — values can exceed field modulus p   │
  ├──────────────────────────────────────────────────────────┤
  │ 2. Pad bits to multiple of 64                            │
  │    (84 bytes = 672 bits → needs 32 zero-padding bits)    │
  ├──────────────────────────────────────────────────────────┤
  │ 3. Pack into 64-bit Blake2bWords via from_bits()         │
  │    84 bytes → 11 words (704 bits / 64)                   │
  └──────────────────────────────────────────────────────────┘


═══════════════════════════════════════════════════════════════════
 COMPRESSION — compress() (RFC 7693 §3.2)
═══════════════════════════════════════════════════════════════════

  Called once per block. For 148-byte input: 2 calls.

  ┌────────────────────────────────────────────────────────────┐
  │ 1. Init working vector v[0..15]:                           │
  │    v[0..7]  = h[0..7]            (current state)           │
  │    v[8..11] = IV[0..3]                                     │
  │    v[12]    = IV[4] XOR t_lo     (byte counter low)        │
  │    v[13]    = IV[5] XOR t_hi     (byte counter high)       │
  │    v[14]    = IV[6] XOR 0xFF..FF (if final block)          │
  │    v[15]    = IV[7]                                        │
  ├────────────────────────────────────────────────────────────┤
  │ 2. 12 rounds of mixing:                                    │
  │    Each round uses SIGMA[round % 10] permutation and       │
  │    performs 8 G() calls (4 column + 4 diagonal):           │
  │                                                            │
  │    Column:   G(0,4,8,12)  G(1,5,9,13)                      │
  │              G(2,6,10,14) G(3,7,11,15)                     │
  │                                                            │
  │    Diagonal: G(0,5,10,15) G(1,6,11,12)                     │
  │              G(2,7,8,13)  G(3,4,9,14)                      │
  │                                                            │
  │    Total: 12 rounds x 8 G calls = 96 G invocations         │
  ├────────────────────────────────────────────────────────────┤
  │ 3. Finalize:                                               │
  │    h[i] = h[i] XOR v[i] XOR v[i+8]   for i = 0..7          │
  │    (two word_xor + from_bits per word)                     │
  └────────────────────────────────────────────────────────────┘


═══════════════════════════════════════════════════════════════════
 G MIXING FUNCTION — G(v, a, b, c, d, x, y) (RFC 7693 §3.1)
═══════════════════════════════════════════════════════════════════

  The atomic mixing unit. Uses Add-Rotate-XOR (ARX) to diffuse
  message words into the state. Called 8 times per round (4 column
  + 4 diagonal) x 12 rounds = 96 calls, ensuring every word is
  thoroughly entangled with every other.

  All arithmetic is mod 2^64. Rotations are right-rotations.

  ┌──────────────────────────────────────────────────────────┐
  │ v[a] = v[a] + v[b] + x          [s_word_add x 2]         │
  │ v[d] = (v[d] XOR v[a]) >>> 32   [s_byte_xor x 8]         │
  │ v[c] = v[c] + v[d]              [s_word_add]             │
  │ v[b] = (v[b] XOR v[c]) >>> 24   [s_byte_xor x 8]         │
  │ v[a] = v[a] + v[b] + y          [s_word_add x 2]         │
  │ v[d] = (v[d] XOR v[a]) >>> 16   [s_byte_xor x 8]         │
  │ v[c] = v[c] + v[d]              [s_word_add]             │
  │ v[b] = (v[b] XOR v[c]) >>> 63   [s_byte_xor x 8]         │
  └──────────────────────────────────────────────────────────┘

  Per G call: 6 additions + 4 XOR/rotates + 8 word reconstructions
  Per block:  96 G calls = 576 additions, 384 XORs


═══════════════════════════════════════════════════════════════════
 CUSTOM CONSTRAINT GATES (9 total)
═══════════════════════════════════════════════════════════════════

  Gate                  Constraint
  ───────────────────── ──────────────────────────────────────────
  s_field_decompose     field = w1 + w2*2^32 + ... + w8*2^224
  s_word_decompose      word = b1 + b2*2^8 + ... + b8*2^56
  s_byte_decompose      byte = sum(bit_i * 2^i) + bool_check(each)
  s_byte_xor            out = a + b - 2ab  (per bit, 8 bits)
  s_word_add            lhs + rhs = out + carry*2^64, bool(carry)
  s_result_encode       field = w1 + w2*2^64
  s_canonicality        bit255=0, lower128 decomp, diff check
  s_high_bit_zero       bit254 * bit = 0
  s_word_combine        w64 = w32_lo + w32_hi*2^32
```
