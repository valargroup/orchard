# BLAKE2b-256 Halo 2 Circuit Audit Report

**Date:** 2026-02-06
**Scope:** `src/circuit/blake2b.rs` (1846 lines), `tests/blake2b.rs` (742 lines)
**Circuit:** Standalone BLAKE2b-256 at K=17 (not integrated into Orchard Action circuit at K=11)
**Methodology:** Per-component verification following the [QEDIT NU5 audit](zcash-nu5-qedit-audit.md) format
**Reference:** RFC 7693 (BLAKE2b specification)

## Table of Contents

- [Overview](#overview)
  - [Legend and stats](#legend-and-stats)
- [1. Specification Constants](#1-specification-constants)
  - [BA-001] Initialization Vectors (IV)
  - [BA-002] SIGMA Permutation Table
  - [BA-003] Rotation Constants
  - [BA-004] Round Count
  - [BA-005] Pallas Modulus Lower 128 Bits
  - [BA-006] Parameter Block Initialization
- [2. Gate Analysis](#2-gate-analysis)
  - [BA-007] Field Decompose Gate
  - [BA-008] Word Decompose Gate
  - [BA-009] Byte Decompose Gate
  - [BA-010] Byte XOR Gate
  - [BA-011] Word Add Gate
  - [BA-012] Result Encode Gate
  - [BA-013] Canonicality Gate
  - [BA-014] High Bit Zero Gate
  - [BA-015] Word Combine Gate
- [3. Core Operations](#3-core-operations)
  - [BA-016] `field_decompose`
  - [BA-017] `add_mod_u64` — Output Range Check Delegation *(Minor)*
  - [BA-018] Chained Additions: Intermediate Value Soundness *(Minor)*
  - [BA-019] `word_xor` / `byte_xor`
  - [BA-020] `word_rotate`
  - [BA-021] G Mixing Function
  - [BA-022] `compress`
  - [BA-023] `process` / `process_hybrid` Byte Counter *(Warning)*
  - [BA-024] `process_hybrid` Field/Byte Separation
  - [BA-025] `encode_result`
  - [BA-026] `bytes_to_words`
  - [BA-027] `from_bits`
  - [BA-028] `from_word`
  - [BA-029] `from_constant_u64`
- [4. Canonicality Check](#4-canonicality-check)
  - [BA-030] bit[255] = 0
  - [BA-031] bits[128..254) = 0 When bit[254] = 1
  - [BA-032] lower_128 < p_lower When bit[254] = 1
  - [BA-033] Copy Constraints in Canonicality Region
  - [BA-034] Diff Word Range Check Connection
  - [BA-035] Unconditional Diff Range Checking *(Minor)*
  - [BA-036] Edge Case Analysis
- [5. Test Coverage Analysis](#5-test-coverage-analysis)
  - [BA-037] Missing RFC 7693 Appendix A Test Vectors *(Warning)*
  - [BA-038] Missing Canonicality Attack Negative Test *(Warning)*
  - [BA-039] Reference Implementation Shares Byte Counter Deviation *(Warning)*
  - [BA-040] Missing Multi-Block `process()` Test *(Minor)*
  - [BA-041] Missing Negative Test for Wrong Hash Output *(Minor)*
  - [BA-042] Missing Boundary Field Element Tests *(Minor)*
  - [BA-043] Basic Satisfaction Tests Lack Output Verification *(Minor)*
  - [BA-044] `blake2b_simd` Comparison Covers Target Use Case
  - [BA-045] Docstring References BLAKE2s Instead of BLAKE2b *(Minor)*
- [6. Summary](#6-summary)

---

## Overview

This document presents the findings of a comprehensive audit of the BLAKE2b-256 Halo 2 circuit implementation in the Orchard repository. The circuit was ported from Anoma/Taiga's BLAKE2s circuit, converted to 64-bit words, and has had multiple soundness fixes applied. Its target use case is proving knowledge of a nullifier for a compact action hash (ZIP-244).

The audit covers constraint correctness, variable connectivity (copy constraints), range checks, RFC 7693 specification conformance, canonicality soundness, and test coverage.

### Legend and stats

In this audit, we report on four types of findings, each identified by a unique identifier `[BA-NNN]`.

> [!CAUTION]
> In red we describe **Critical Issues**, findings that can be exploited to break circuit soundness or produce invalid proofs.

We have not found any critical issues in this review.

> [!WARNING]
> In yellow we describe **Warning Issues**, findings that could result in incorrect behavior or reduced assurance, but whose exploitability has not been demonstrated.

We have found a total of 4 warning issues:
- [BA-023] `process` / `process_hybrid` byte counter deviates from RFC 7693
- [BA-037] Missing RFC 7693 Appendix A standard test vectors
- [BA-038] Missing canonicality attack negative test
- [BA-039] Reference implementation shares byte counter deviation

> [!NOTE]
> In blue we describe **Minor Issues**, findings that have no meaningful impact on soundness but should be addressed, such as missing documentation, minor inefficiencies, test coverage gaps, and naming issues.

We have found a total of 8 minor issues:
- [BA-017] `add_mod_u64` output range check delegation undocumented
- [BA-018] Chained additions intermediate value invariant undocumented
- [BA-035] Unconditional diff range checking (minor inefficiency)
- [BA-040] Missing multi-block `process()` test
- [BA-041] Missing negative test for wrong hash output
- [BA-042] Missing boundary field element tests near p-1
- [BA-043] Basic satisfaction tests lack output verification
- [BA-045] Docstring references BLAKE2s instead of BLAKE2b

> [!TIP]
> In green we describe **No Issues Found**, positive findings worth reporting.

We have reported a total of 33 findings with no issues. These confirm correctness of constants, gates, core operations, canonicality checks, and copy constraint connectivity.

---

## 1. Specification Constants

> [!TIP]
> ### [BA-001] Initialization Vectors (IV)
> - **Description:** The 8 initialization vectors at lines 70–79 are the first 64 bits of the fractional parts of the square roots of the first 8 primes (2, 3, 5, 7, 11, 13, 17, 19), as specified in RFC 7693 §2.6.
>
>   | Index | Code value             | RFC 7693 value         | Match |
>   |-------|------------------------|------------------------|-------|
>   | IV[0] | `0x6a09e667f3bcc908`  | `0x6a09e667f3bcc908`  | pass     |
>   | IV[1] | `0xbb67ae8584caa73b`  | `0xbb67ae8584caa73b`  | pass     |
>   | IV[2] | `0x3c6ef372fe94f82b`  | `0x3c6ef372fe94f82b`  | pass     |
>   | IV[3] | `0xa54ff53a5f1d36f1`  | `0xa54ff53a5f1d36f1`  | pass     |
>   | IV[4] | `0x510e527fade682d1`  | `0x510e527fade682d1`  | pass     |
>   | IV[5] | `0x9b05688c2b3e6c1f`  | `0x9b05688c2b3e6c1f`  | pass     |
>   | IV[6] | `0x1f83d9abfb41bd6b`  | `0x1f83d9abfb41bd6b`  | pass     |
>   | IV[7] | `0x5be0cd19137e2179`  | `0x5be0cd19137e2179`  | pass     |
>
> - No issues found.

> [!TIP]
> ### [BA-002] SIGMA Permutation Table
> - **Description:** The 10x16 message permutation table at lines 88–99 matches RFC 7693 §2.7 exactly. All 160 entries verified.
> - No issues found.

> [!TIP]
> ### [BA-003] Rotation Constants
> - **Description:** The rotation constants at lines 103–106 are correct for BLAKE2b.
>
>   | Constant | Code value | RFC 7693 (BLAKE2b) | BLAKE2s (must NOT match) |
>   |----------|-----------|---------------------|--------------------------|
>   | R1       | 32        | 32                  | 16                       |
>   | R2       | 24        | 24                  | 12                       |
>   | R3       | 16        | 16                  | 8                        |
>   | R4       | 63        | 63                  | 7                        |
>
>   All four constants are correct for BLAKE2b and correctly differ from BLAKE2s values.
> - No issues found.

> [!TIP]
> ### [BA-004] Round Count
> - **Description:** `ROUNDS = 12` at line 109 is correct for BLAKE2b (BLAKE2s uses 10).
> - No issues found.

> [!TIP]
> ### [BA-005] Pallas Modulus Lower 128 Bits
> - **Description:** The constant `PALLAS_MODULUS_LOWER_128` at line 83 is verified against the Pallas base field modulus.
>   The Pallas modulus is:
>   ```
>   p = 0x40000000_00000000_00000000_00000000_224698fc_094cf91b_992d30ed_00000001
>   ```
>   Lower 128 bits: `0x224698fc_094cf91b_992d30ed_00000001`. The code value matches.
> - No issues found.

> [!TIP]
> ### [BA-006] Parameter Block Initialization
> - **Description:** The parameter block initialization at line 580 computes:
>   ```
>   h[0] = IV[0] ^ 0x01010000 ^ 32
>   ```
>   Per RFC 7693 §2.5, for sequential mode with no key (kk=0) and 32-byte output (nn=32):
>   ```
>   IV[0] ^ (0x01 | (0x01 << 8) | (0x00 << 16) | (0x00 << 24) | (nn))
>   = IV[0] ^ 0x01010000 ^ 32
>   = IV[0] ^ 0x01010020
>   ```
>   Personalization is correctly applied to h[6] and h[7] via XOR with the 16-byte personalization string read as two little-endian u64 values (lines 587–596). This matches RFC 7693 §2.5 parameter block bytes 48–63.
> - No issues found.

---

## 2. Gate Analysis

> [!TIP]
> ### [BA-007] Field Decompose Gate (`s_field_decompose`)
> - **Description:** The gate at lines 253–277 constrains:
>   ```
>   word_1 + word_2 * 2^32 + word_3 * 2^64 + word_4 * 2^96
>   + word_5 * 2^128 + word_6 * 2^160 + word_7 * 2^192 + word_8 * 2^224
>   = field_element
>   ```
>   Coefficient verification:
>
>   | Word   | Expression                           | Value  |
>   |--------|--------------------------------------|--------|
>   | word_1 | (implicit 1)                         | 2^0    |
>   | word_2 | `F::from(1 << 32)`                   | 2^32   |
>   | word_3 | `F::from_u128(1 << 64)`              | 2^64   |
>   | word_4 | `F::from_u128(1 << 96)`              | 2^96   |
>   | word_5 | `F::from_u128(1 << 64).square()`     | 2^128  |
>   | word_6 | `F::from_u128(1 << 80).square()`     | 2^160  |
>   | word_7 | `F::from_u128(1 << 96).square()`     | 2^192  |
>   | word_8 | `F::from_u128(1 << 112).square()`    | 2^224  |
>
>   The `.square()` technique is necessary because `F::from_u128()` can only represent up to 2^127, and coefficients above 2^128 are needed. Using `(2^k)^2 = 2^(2k)` correctly produces the required powers. All 8 coefficients verified correct.
>   Layout: Row 0 = words in `advices[0..8]`, Row 1 = field element in `advices[0]`. Matches usage in `field_decompose()` at lines 1123–1133 where words are copy-constrained and field element is copy-constrained.
> - No issues found.

> [!TIP]
> ### [BA-008] Word Decompose Gate (`s_word_decompose`)
> - **Description:** The gate at lines 280–304 constrains:
>   ```
>   byte_1 + byte_2 * 2^8 + byte_3 * 2^16 + byte_4 * 2^24
>   + byte_5 * 2^32 + byte_6 * 2^40 + byte_7 * 2^48 + byte_8 * 2^56
>   = word
>   ```
>   This supports both 64-bit words (all 8 bytes used) and 32-bit words (bytes 5–8 set to zero via `assign_advice_from_constant(F::ZERO)` at lines 1342–1348). The constant constraint on zero bytes ensures the gate correctly reduces to a 32-bit decomposition when needed.
> - No issues found.

> [!TIP]
> ### [BA-009] Byte Decompose Gate (`s_byte_decompose`)
> - **Description:** The gate at lines 306–346 enforces two categories of constraints:
>   1. **Decomposition:** `bit_1 + bit_2*2 + bit_3*4 + ... + bit_8*128 = byte`
>   2. **Boolean checks:** `bool_check(bit_i)` for all 8 bits (lines 336–343)
>
>   The `bool_check` function constrains `b * (1 - b) = 0`, ensuring each bit is exactly 0 or 1. Without these boolean constraints (which were a soundness fix per the code comments), a malicious prover could use non-binary values satisfying the decomposition equation. Both constraint types are correctly wrapped in `Constraints::with_selector`.
> - No issues found.

> [!TIP]
> ### [BA-010] Byte XOR Gate (`s_byte_xor`)
> - **Description:** The gate at lines 348–363 constrains 8 parallel XOR operations:
>   ```
>   lhs_bit + rhs_bit - 2 * lhs_bit * rhs_bit - out_bit = 0
>   ```
>   This is the algebraic form of `out = lhs XOR rhs` for boolean inputs:
>
>   | lhs | rhs | lhs + rhs - 2*lhs*rhs | XOR |
>   |-----|-----|-----------------------|-----|
>   | 0   | 0   | 0                     | 0   |
>   | 0   | 1   | 1                     | 1   |
>   | 1   | 0   | 1                     | 1   |
>   | 1   | 1   | 0                     | 0   |
>
>   The output bits are inherently boolean when inputs are boolean (since `a + b - 2ab` is in `{0,1}` for `a,b` in `{0,1}`). No explicit boolean check is needed on output bits.
>   Layout: Row -1 = lhs bits, Row 0 = rhs bits (selector row), Row +1 = output bits. The selector is enabled at the middle row (line 1536: `enable(&mut region, 1)`), matching the `Rotation::prev`/`cur`/`next` pattern.
> - No issues found.

> [!TIP]
> ### [BA-011] Word Add Gate (`s_word_add`)
> - **Description:** The gate at lines 366–382 constrains:
>   ```
>   lhs + rhs - carry * 2^64 - out = 0    (equality check)
>   carry * (1 - carry) = 0                (carry is boolean)
>   ```
>   This constrains `out = (lhs + rhs) mod 2^64` with carry in {0, 1}.
>   **Critical note:** The gate does NOT range-check `out` to [0, 2^64). Callers MUST pass the output through `from_word()` or `from_bits()` to establish that the result is a valid 64-bit value. See [BA-017] for verification of all call sites.
> - No issues found.

> [!TIP]
> ### [BA-012] Result Encode Gate (`s_result_encode`)
> - **Description:** The gate at lines 386–398 constrains:
>   ```
>   word_1 + word_2 * 2^64 = field_element
>   ```
>   This packs two 64-bit words into a single field element (128 bits, well within the 255-bit Pallas field). Input words are copy-constrained in `encode_result()` (lines 766–772).
> - No issues found.

> [!TIP]
> ### [BA-013] Canonicality Gate (`s_canonicality`)
> - **Description:** The gate at lines 426–491 enforces 4 constraints (detailed analysis in [Section 4](#4-canonicality-check--deep-analysis)):
>   1. `bit_255 = 0` — always active
>   2. `lower_128 = word_1 + word_2*2^32 + word_3*2^64 + word_4*2^96` — always active
>   3. `bit_254 * (lower_128_diff - (p_lower - 1 - lower_128)) = 0` — conditional on bit_254
>   4. `bit_254 * (lower_128_diff - diff_w1 - diff_w2*2^32 - diff_w3*2^64 - diff_w4*2^96) = 0` — conditional on bit_254
>
>   Layout (2 rows):
>   - Row 0: `advices[0..9]` = bit_255, bit_254, lower_128_diff, diff_w1..diff_w4, w1, w2, w3
>   - Row 1: `advices[0..1]` = w4, lower_128
>
>   All column/rotation assignments match the usage in `check_canonicality()` (lines 1234–1275).
> - No issues found.

> [!TIP]
> ### [BA-014] High Bit Zero Gate (`s_high_bit_zero`)
> - **Description:** The gate at lines 500–509 constrains:
>   ```
>   bit_254 * bit_to_check = 0
>   ```
>   Applied 126 times for bits[128..254) (indices 128, 129, ..., 253). This correctly covers all bits between the lower 128 bits and bit 254. The range `bits[128..254]` in Rust (line 1280) yields exactly 126 elements (254 - 128 = 126). Both `bit_254` and each `bit_to_check` are copy-constrained to their actual cells from field decomposition.
> - No issues found.

> [!TIP]
> ### [BA-015] Word Combine Gate (`s_word_combine`)
> - **Description:** The gate at lines 523–534 constrains:
>   ```
>   word_32_lo + word_32_hi * 2^32 = word_64
>   ```
>   This bridges the 32-bit decomposition used for canonicality checking to the 64-bit words used in BLAKE2b operations. Input 32-bit words are copy-constrained in `word_combine()` (lines 1396–1407) to the same cells verified by `word_decompose_32` and the canonicality check.
> - No issues found.

---

## 3. Core Operations

> [!TIP]
> ### [BA-016] `field_decompose`
> - **Description:** The `field_decompose` function (lines 1084–1160) establishes a complete constraint chain from a field element to 4 x 64-bit `Blake2bWord` values:
>   1. **Field to 256 bits:** 32 bytes extracted, each decomposed to 8 boolean bits via `Blake2bByte::from_u8` (s_byte_decompose gate with boolean checks). Lines 1092–1098.
>   2. **Bits to 32-bit words:** Groups of 4 bytes assembled into 32-bit words, each constrained via `word_decompose_32` (s_word_decompose gate). Lines 1102–1120.
>   3. **32-bit words to field reconstruction:** 8 x 32-bit words constrained to equal the original field element via s_field_decompose gate. Lines 1123–1133.
>   4. **Canonicality check:** Bits and 32-bit words verified to represent a value < p. Line 1139.
>   5. **32-bit to 64-bit words:** Pairs of 32-bit words combined via `word_combine` (s_word_combine gate). Lines 1145–1157.
>
>   Copy constraint chain verified:
>   - Byte cells from step 1 are copy-constrained into `word_decompose_32` regions (step 2)
>   - 32-bit word cells from step 2 are copy-constrained into the `s_field_decompose` region (step 3)
>   - The field element from the caller is copy-constrained into the `s_field_decompose` region (step 3)
>   - Bits and words from steps 1–2 are copy-constrained into the canonicality region (step 4)
>   - 32-bit words from step 2 are copy-constrained into `word_combine` regions (step 5)
>
>   No gaps in the constraint chain. A malicious prover cannot substitute different values at any stage.
> - No issues found.

> [!NOTE]
> ### [BA-017] `add_mod_u64` — Output Range Check Delegation
> - **Description:** The `add_mod_u64` function (lines 1581–1616) returns an `AssignedCell` that is constrained by the s_word_add gate but is NOT range-checked to [0, 2^64) within the function itself. The gate only ensures:
>   ```
>   lhs + rhs = carry * 2^64 + out,  carry in {0, 1}
>   ```
>   This means `out` could theoretically be any field element satisfying this equation. The caller is responsible for range-checking via `from_word()` or `from_bits()`.
>
>   Carry detection (line 1597): `sum.to_repr().as_ref()[8]` reads byte 8 of the field element representation to detect 64-bit overflow. This is sound because two 64-bit values sum to at most 2^65 - 2, which fits well within the 255-bit Pallas field, so the field representation faithfully captures the integer sum.
>
>   Verification of all call sites in G function (lines 984–1081):
>
>   | Line | Operation           | Range-checked by            |
>   |------|--------------------|-----------------------------|
>   | 994  | v[a] + v[b]        | Intermediate — see [BA-018] |
>   | 999  | (v[a]+v[b]) + x    | `from_word()` at line 1001  |
>   | 1017 | v[c] + v[d]        | `from_word()` at line 1022  |
>   | 1038 | v[a] + v[b]        | Intermediate — see [BA-018] |
>   | 1043 | (v[a]+v[b]) + y    | `from_word()` at line 1045  |
>   | 1061 | v[c] + v[d]        | `from_word()` at line 1066  |
>
>   All final results are range-checked. Intermediate results are addressed in [BA-018].
> - **Recommendation:** Add a doc comment to `add_mod_u64` explicitly stating that the caller must range-check the output, or consider creating a wrapper that combines add + range-check.

> [!NOTE]
> ### [BA-018] Chained Additions: Intermediate Value Soundness
> - **Description:** In the G function, the computation `v[a] + v[b] + x` is performed as two chained `add_mod_u64` calls where the intermediate result `sum_a_b` skips range checking (lines 994–1001, 1038–1045). This is sound but undocumented.
>
>   **Soundness argument:** Let the two constraints be:
>   ```
>   (A) v[a] + v[b] = carry1 * 2^64 + sum_a_b     (carry1 in {0,1})
>   (B) sum_a_b + x = carry2 * 2^64 + result       (carry2 in {0,1})
>   ```
>   Substituting (A) into (B):
>   ```
>   v[a] + v[b] + x = (carry1 + carry2) * 2^64 + result
>   ```
>   Since v[a], v[b], x are all range-checked to [0, 2^64), their integer sum is at most 3*(2^64 - 1) < 2^66. Combined with `result` in [0, 2^64) (enforced by `from_word()` on the final output), the total carry `carry1 + carry2` in {0, 1, 2} is exactly representable by two boolean carry values. This uniquely determines `result = (v[a] + v[b] + x) mod 2^64`.
>
>   The intermediate `sum_a_b` is not uniquely determined (carry1 and carry2 can be swapped when the total is 1), but the final result is always correct regardless.
>
>   **Important:** This argument relies on all three operands being strict 64-bit values AND on all arithmetic happening within the Pallas field (p ~ 2^254), so no field wraparound can occur for sums up to ~2^66.
> - **Recommendation:** Document this invariant in a code comment near the chained additions in the G function, explaining why the intermediate result does not need range checking.

> [!TIP]
> ### [BA-019] `word_xor` / `byte_xor`
> - **Description:** The `word_xor` function (lines 1562–1578) XORs two 64-bit words by processing 8 bytes of 8 bits each through `byte_xor`. The `byte_xor` function (lines 1525–1560):
>   - Input bits come from `Blake2bWord.get_bits()`, which are cells constrained to be boolean by the s_byte_decompose gate chain
>   - Input bits are copy-constrained into the XOR region (lines 1542–1543)
>   - Output bits are computed via the s_byte_xor gate algebraic formula
>   - Output bits flow into `from_bits()` which re-constrains them through byte decomposition and word decomposition
>
>   The XOR output bits are inherently boolean when inputs are boolean (see [BA-010]), providing implicit range checking.
> - No issues found.

> [!TIP]
> ### [BA-020] `word_rotate`
> - **Description:** The `word_rotate` function (lines 1648–1658) performs right rotation of 64 bits as a pure bit permutation with zero constraints:
>   ```rust
>   bits.iter().skip(by).chain(bits.iter()).take(64)
>   ```
>   For right rotation by `by` positions, bit[i] of the result = bit[(i + by) % 64] of the input.
>   The code produces: `[bits[by], bits[by+1], ..., bits[63], bits[0], ..., bits[by-1]]`
>
>   Verified for all four rotation amounts used:
>   - R1=32: bits[32..64] ++ bits[0..32]
>   - R2=24: bits[24..64] ++ bits[0..24]
>   - R3=16: bits[16..64] ++ bits[0..16]
>   - R4=63: bits[63] ++ bits[0..63]
>
>   Since this is a pure permutation of already-constrained cells, no additional constraints are needed. The rotated bits are subsequently processed by `from_bits()` which establishes the byte/word reconstruction constraints.
> - No issues found.

> [!TIP]
> ### [BA-021] G Mixing Function
> - **Description:** The G function (lines 984–1081) implements all 8 steps of RFC 7693 §3.1:
>
>   | Step | RFC 7693 specification                  | Code (lines)    | Verified |
>   |------|-----------------------------------------|-----------------|----------|
>   | 1    | v[a] := (v[a] + v[b] + x) mod 2^w      | 993–1002        | pass     |
>   | 2    | v[d] := (v[d] ^ v[a]) >>> R1            | 1005–1013       | pass     |
>   | 3    | v[c] := (v[c] + v[d]) mod 2^w           | 1016–1023       | pass     |
>   | 4    | v[b] := (v[b] ^ v[c]) >>> R2            | 1026–1034       | pass     |
>   | 5    | v[a] := (v[a] + v[b] + y) mod 2^w       | 1037–1046       | pass     |
>   | 6    | v[d] := (v[d] ^ v[a]) >>> R3            | 1049–1057       | pass     |
>   | 7    | v[c] := (v[c] + v[d]) mod 2^w           | 1060–1067       | pass     |
>   | 8    | v[b] := (v[b] ^ v[c]) >>> R4            | 1070–1078       | pass     |
>
>   Variable indices (a, b, c, d), message words (x, y), and rotation constants (R1–R4) all match the specification. Every final addition result passes through `from_word()` and every XOR/rotation result passes through `from_bits()`, ensuring complete range checking.
> - No issues found.

> [!TIP]
> ### [BA-022] `compress`
> - **Description:** The compression function (lines 843–963) matches RFC 7693 §3.2:
>
>   **Initialization** (lines 851–879):
>   - [x] v[0..7] = h[0..7] (state copy)
>   - [x] v[8..11] = IV[0..3]
>   - [x] v[12] = IV[4] ^ (t as u64) — low 64 bits of counter
>   - [x] v[13] = IV[5] ^ ((t >> 64) as u64) — high 64 bits of counter
>   - [x] v[14] = IV[6] ^ (f ? u64::MAX : 0) — finalization flag
>   - [x] v[15] = IV[7]
>
>   **Round structure** (lines 881–941):
>   - [x] 12 rounds (ROUNDS = 12)
>   - [x] SIGMA cycling: `SIGMA[i % 10]` for rounds 10 and 11
>
>   **Column mixing** (lines 884–911):
>   - [x] G(v, 0, 4, 8, 12, m[s[0]], m[s[1]])
>   - [x] G(v, 1, 5, 9, 13, m[s[2]], m[s[3]])
>   - [x] G(v, 2, 6, 10, 14, m[s[4]], m[s[5]])
>   - [x] G(v, 3, 7, 11, 15, m[s[6]], m[s[7]])
>
>   **Diagonal mixing** (lines 913–940):
>   - [x] G(v, 0, 5, 10, 15, m[s[8]], m[s[9]])
>   - [x] G(v, 1, 6, 11, 12, m[s[10]], m[s[11]])
>   - [x] G(v, 2, 7, 8, 13, m[s[12]], m[s[13]])
>   - [x] G(v, 3, 4, 9, 14, m[s[14]], m[s[15]])
>
>   **Finalization** (lines 944–960):
>   - [x] h[i] = h[i] ^ v[i] ^ v[i+8] for i = 0..7 (two sequential word_xor + from_bits)
> - No issues found.

> [!WARNING]
> ### [BA-023] `process` / `process_hybrid` Byte Counter
> - **Description:** Both `process()` (line 638) and `process_hybrid()` (line 743) use `total_bytes.max(128)` as the byte counter `t` for the final block compression. This deviates from RFC 7693, which specifies that `t` should be the actual number of input bytes processed.
>
>   **RFC 7693 §3.2:**
>   > The 2w-bit offset counter t is updated only upon calling F, and counts the number of data bytes input to the hash algorithm
>
>   For inputs < 128 bytes:
>   - Circuit uses: `t = 128` (clamped minimum)
>   - RFC specifies: `t = actual_input_bytes` (e.g., 0 for empty, 32 for one field element, 64 for two)
>
>   **Impact analysis:**
>   - For the target use case (compact action hash, 148 bytes): `max(148, 128) = 148` — correct, no deviation
>   - For empty input: `max(0, 128) = 128` — deviates from RFC (should be 0)
>   - For inputs of 1–3 field elements (32–96 bytes): deviates from RFC
>
>   The deviation is intentional: BLAKE2b always processes at least one 128-byte block even for empty input, and the code comment at line 638 says "At minimum one 128-byte block." However, standard BLAKE2b implementations (e.g., `blake2b_simd`) use the actual byte count, not the block size.
>
>   **Consequence:** The circuit produces non-standard BLAKE2b-256 hashes for inputs under 128 bytes. Hashes for inputs >= 128 bytes are standard-compliant.
> - **Recommendation:** Document this deviation explicitly in the function-level documentation. If standard BLAKE2b compatibility for small inputs is ever needed, the counter should be changed to use the actual byte count.

> [!TIP]
> ### [BA-024] `process_hybrid` Field/Byte Separation
> - **Description:** The `process_hybrid` function (lines 664–749) correctly separates field element inputs (canonicality-checked via `field_decompose`) from raw byte inputs (boolean-constrained only via `bytes_to_words`). This distinction is critical:
>   - **Field inputs** (nullifier, cmx): Must be canonical field elements (< p) to prevent hash collision attacks via non-canonical representations
>   - **Byte inputs** (epk, enc[0..52]): Arbitrary byte data that may exceed p, so canonicality checks are correctly omitted
>
>   The byte counter computation at line 710 (`field_inputs.len() * 32 + byte_inputs.len()`) correctly accounts for the different sizes.
> - No issues found.

> [!TIP]
> ### [BA-025] `encode_result`
> - **Description:** The `encode_result` function (lines 753–795) packs the 4-word (256-bit) hash result into 2 field elements. The fold computation:
>   ```rust
>   words.into_iter().rev().fold(F::ZERO, |acc, word| acc * F::from_u128(1u128 << 64) + word)
>   ```
>   For words [w1, w2], reversed = [w2, w1]: acc = 0 -> acc = w2 -> acc = w2 * 2^64 + w1.
>   This produces `w1 + w2 * 2^64`, matching the s_result_encode gate constraint. Input words are copy-constrained via `word.get_word().copy_advice()` at lines 767–772.
> - No issues found.

> [!TIP]
> ### [BA-026] `bytes_to_words`
> - **Description:** The `bytes_to_words` function (lines 1441–1502) converts raw byte cells to 64-bit `Blake2bWord` values:
>   1. Each byte is decomposed to 8 bits with boolean constraints via `byte_decompose()` (lines 1470–1474). The original byte cell is copy-constrained into the decomposition region.
>   2. Zero-bit padding uses `assign_free_constant(F::ZERO)` (line 1482), which creates constant-constrained zero cells.
>   3. Bits are assembled into 64-bit words via `from_bits()`, which establishes the complete byte-to-word reconstruction chain.
>
>   No canonicality check is performed, which is correct for arbitrary byte data.
> - No issues found.

> [!TIP]
> ### [BA-027] `from_bits`
> - **Description:** `Blake2bWord::from_bits` (lines 1690–1731) constructs a word from 64 bits:
>   1. Groups of 8 bits -> byte (via `byte_decompose` with boolean checks and reconstruction constraint)
>   2. 8 bytes -> word (via `word_decompose` with reconstruction constraint)
>   3. Both bytes and word are freshly assigned as free advice, then copy-constrained via the decomposition gates
>
>   Complete range-check chain: boolean bits -> [0,255] bytes -> [0, 2^64) word.
> - No issues found.

> [!TIP]
> ### [BA-028] `from_word`
> - **Description:** `Blake2bWord::from_word` (lines 1734–1754) constructs a word structure from an assigned 64-bit word cell:
>   1. 8 byte values extracted from word representation
>   2. Each byte decomposed to 8 boolean bits via `Blake2bByte::from_u8`
>   3. Word cell copy-constrained into `word_decompose` region (line 1749)
>
>   The input word cell is the SAME cell (via copy constraint) that appears in the `word_decompose` gate, ensuring the bits and bytes correspond to the word.
> - No issues found.

> [!TIP]
> ### [BA-029] `from_constant_u64`
> - **Description:** `Blake2bWord::from_constant_u64` (lines 1621–1646) constructs a word from a known constant:
>   1. 8 bytes decomposed to constant-constrained bits via `Blake2bByte::from_constant_u8` (uses `assign_advice_from_constant`)
>   2. Word assigned as constant via `assign_free_constant`
>   3. Word decomposition gate constrains word = sum(bytes * 2^(8i))
>
>   All values are constant-constrained, preventing a malicious prover from substituting different values.
> - No issues found.

---

## 4. Canonicality Check

This section analyzes the canonicality check (`check_canonicality`, lines 1178–1321), the most soundness-critical component of the circuit.

### Threat Model

A field element `f` has a canonical 256-bit representation (the unique integer in [0, p)) and potentially a non-canonical representation `f + p` (which equals `f` mod p but has different bits). If a prover could use either representation, the same field element would produce two different BLAKE2b hashes, breaking the binding property of the hash.

The Pallas modulus in binary:
```
bit 255:      0
bit 254:      1
bits 253–128: all 0
bits 127–0:   0x224698fc_094cf91b_992d30ed_00000001
```

> [!TIP]
> ### [BA-030] Constraint: bit[255] = 0
> - **Description:** Constraint 1 of the canonicality gate (line 456) directly constrains `bit_255 = 0`. This is unconditional (not multiplied by any conditional expression). The bit cell is copy-constrained from the actual bit decomposition (line 1241). The bit is guaranteed boolean by the s_byte_decompose chain.
>   This eliminates all values >= 2^255.
> - No issues found.

> [!TIP]
> ### [BA-031] Constraint: bits[128..254) = 0 When bit[254] = 1
> - **Description:** The s_high_bit_zero gate is applied 126 times (lines 1280–1290) for bits at indices 128, 129, ..., 253. Each application constrains: `bit_254 * bit[i] = 0`.
>   - Range `bits[128..254]` in Rust yields indices 128..=253, exactly 126 elements
>   - Both `bit_254` and each `bit[i]` are copy-constrained to actual decomposition cells
>   - When `bit_254 = 0`: constraints trivially satisfied (0 * anything = 0)
>   - When `bit_254 = 1`: each `bit[i]` must be 0
>
>   This correctly eliminates values where bit 254 is set and any bit in [128, 253] is also set, which would correspond to values >= 2^254 + 2^128 > p.
> - No issues found.

> [!TIP]
> ### [BA-032] Constraint: lower_128 < p_lower When bit[254] = 1
> - **Description:** Constraints 3 and 4 of the canonicality gate work together:
>   **Constraint 3** (line 469):
>   ```
>   bit_254 * (lower_128_diff - (p_lower - 1 - lower_128)) = 0
>   ```
>   When bit_254 = 1: `lower_128_diff = p_lower - 1 - lower_128`
>
>   **Constraint 4** (line 475):
>   ```
>   bit_254 * (lower_128_diff - dw1 - dw2*2^32 - dw3*2^64 - dw4*2^96) = 0
>   ```
>   When bit_254 = 1: `lower_128_diff = dw1 + dw2*2^32 + dw3*2^64 + dw4*2^96`
>
>   Each `dw_i` is range-checked to [0, 2^32) via `word_decompose_32` -> `byte_decompose` -> boolean bits (see [BA-034]). Therefore `lower_128_diff` is in [0, 2^128).
>   Combined: `p_lower - 1 - lower_128` is in [0, 2^128), which implies `lower_128 <= p_lower - 1 < p_lower`.
> - No issues found.

> [!TIP]
> ### [BA-033] Copy Constraints in Canonicality Region
> - **Description:** All witness values in the canonicality region are properly connected to their source cells:
>
>   | Value    | Source                         | Connected via           | Line |
>   |----------|--------------------------------|-------------------------|------|
>   | bit_255  | bits[255] from byte decompose  | `copy_advice`           | 1241 |
>   | bit_254  | bits[254] from byte decompose  | `copy_advice`           | 1242 |
>   | words[0] | 32-bit word from field decompose | `copy_advice`         | 1261 |
>   | words[1] | 32-bit word from field decompose | `copy_advice`         | 1262 |
>   | words[2] | 32-bit word from field decompose | `copy_advice`         | 1263 |
>   | words[3] | 32-bit word from field decompose | `copy_advice`         | 1266 |
>
>   The `lower_128_diff` and `diff_word_*` cells are freshly assigned (not copy-constrained to external cells), which is correct — they are internal witness values constrained only by the canonicality gate and their own range checks.
> - No issues found.

> [!TIP]
> ### [BA-034] Diff Word Range Check Connection
> - **Description:** This is the most critical connection in the canonicality check: the diff_word cells in the canonicality gate region must be the SAME cells that are range-checked via `word_decompose_32`.
>
>   Verification:
>   1. `diff_cells` are assigned at `advices[3..7]`, row 0 in the canonicality region (lines 1250–1258)
>   2. These cells are returned from `assign_region` as `diff_word_cells` (line 1273)
>   3. Each `diff_word_cell` is passed to `word_decompose_32` (lines 1313–1317)
>   4. Inside `word_decompose_32`, `word.copy_advice()` (line 1350) creates a copy constraint between the diff_word cell and the word position in the decomposition gate
>
>   This ensures the range-checked value is provably identical to the value used in constraint 4 of the canonicality gate. A malicious prover cannot use a different (non-range-checked) value.
> - No issues found.

> [!NOTE]
> ### [BA-035] Unconditional Diff Range Checking
> - **Description:** The `word_decompose_32` calls for diff_word cells (lines 1295–1318) are executed unconditionally, regardless of whether bit_254 is 0 or 1. When bit_254 = 0, constraints 3 and 4 are disabled (multiplied by 0), so the diff_word values are unconstrained by the gate — but they are still range-checked.
>   **Impact:** This wastes ~4 x (1 word_decompose + 4 byte_decompose) = ~20 regions of constraints when bit_254 = 0. The prover can trivially satisfy these by setting diff = 0 with all words and bytes being 0.
>   **Soundness:** Not a soundness issue. The extra constraints are always satisfiable and do not interfere with the canonicality logic.
> - **Recommendation:** This is a minor inefficiency. If circuit size is a concern, the range checks could be conditionally applied, but the complexity trade-off likely isn't worth it.

> [!TIP]
> ### [BA-036] Edge Case Analysis
> - **Description:** Verified behavior at canonicality boundaries:
>
>   **Case 1: value = p - 1 (maximum canonical value)**
>   - bit_255 = 0, bit_254 = 1, bits[128..254) = all 0
>   - lower_128 = p_lower - 1 = `0x224698fc_094cf91b_992d30ed_00000000`
>   - diff = p_lower - 1 - lower_128 = 0
>   - diff decomposes as [0, 0, 0, 0] -> valid
>
>   **Case 2: value = p (first non-canonical value)**
>   - As a field element, p = 0 mod p
>   - As 256 bits: bit_255 = 0, bit_254 = 1, bits[128..254) = all 0
>   - lower_128 = p_lower = `0x224698fc_094cf91b_992d30ed_00000001`
>   - diff = p_lower - 1 - p_lower = -1 (mod p) — a large field element
>   - Cannot decompose into 4 x 32-bit words -> constraint fails -> rejected
>
>   **Case 3: value with bit_254 = 0**
>   - x < 2^254 < p, canonicality guaranteed
>   - Constraints 3 and 4 disabled (multiplied by bit_254 = 0) -> trivially satisfied
>
>   **Case 4: value with bit_254 = 1 and bit[200] = 1**
>   - s_high_bit_zero: bit_254 * bit[200] = 1 * 1 = 1 != 0 -> constraint fails -> rejected
> - No issues found.

---

## 5. Test Coverage Analysis

> [!WARNING]
> ### [BA-037] Missing RFC 7693 Appendix A Test Vectors
> - **Description:** RFC 7693 Appendix A provides standard test vectors for BLAKE2b. None of the existing tests verify against these vectors.
>   The existing tests use either:
>   - A local reference implementation (which shares the byte counter deviation — see [BA-039])
>   - `blake2b_simd` crate (only for the 148-byte compact action hash case)
>
>   Without standard test vectors, compliance with the official specification cannot be independently confirmed for all input sizes.
> - **Recommendation:** Add tests using RFC 7693 Appendix A test vectors. Note that due to the byte counter deviation ([BA-023]), inputs under 128 bytes will NOT match standard BLAKE2b. If this is intentional, document it and test against both the circuit's expected output and the standard output (documenting the difference).

> [!WARNING]
> ### [BA-038] Missing Canonicality Attack Negative Test
> - **Description:** There is no test that verifies the circuit correctly REJECTS a non-canonical field element representation. A negative test should:
>   1. Construct a witness where the 256-bit decomposition represents a value >= p (e.g., `field_value + p`)
>   2. Run `MockProver::run()` and verify that `prover.verify()` returns an error
>
>   This would provide concrete evidence that the canonicality check ([BA-030] through [BA-036]) works as intended in practice, not just in theory.
> - **Recommendation:** Add a negative test that attempts to use a non-canonical representation and verifies the MockProver rejects it.

> [!WARNING]
> ### [BA-039] Reference Implementation Shares Byte Counter Deviation
> - **Description:** The reference BLAKE2b implementation in `tests/blake2b.rs` (lines 181–321) uses the same `total_input_bytes.max(128)` byte counter (line 311) as the circuit. This means the reference is not truly independent for inputs under 128 bytes — it will produce the same non-standard hash as the circuit, making the comparison test non-diagnostic for the byte counter deviation.
>   The `blake2b_simd` comparison in `test_compact_hash_nullifier_proof` IS an independent reference, but only covers the 148-byte case where `max(128)` has no effect.
> - **Recommendation:** If standard BLAKE2b compatibility for all input sizes is desired, test against `blake2b_simd` for small inputs as well. If non-standard behavior for small inputs is intentional, document this clearly.

> [!NOTE]
> ### [BA-040] Missing Multi-Block `process()` Test
> - **Description:** All existing `process()` tests use at most 2 field elements (64 bytes = 1 block). There is no test exercising multi-block processing (> 4 field elements / > 128 bytes) through the `process()` function. The `process_hybrid()` function is tested with a multi-block case (148 bytes in `test_compact_hash_nullifier_proof`), but `process()` itself is not.
> - **Recommendation:** Add a test with 6+ field elements to exercise the multi-block loop in `process()` (lines 627–629).

> [!NOTE]
> ### [BA-041] Missing Negative Test for Wrong Hash Output
> - **Description:** No test verifies that the MockProver rejects an incorrect hash output. All existing tests either don't check the output (satisfaction-only tests) or provide the correct expected values. A negative test would:
>   1. Compute the correct hash
>   2. Modify one word of the expected output
>   3. Verify `MockProver::run()` with the wrong public inputs returns an error
>
>   This would confirm that the circuit's public input constraints actually bind the output.
> - **Recommendation:** Add a negative test that provides wrong expected hash words and verifies rejection.

> [!NOTE]
> ### [BA-042] Missing Boundary Field Element Tests
> - **Description:** No test exercises field elements near the Pallas modulus boundary (p - 1, p - 2, etc.) where the canonicality check is most critical. The existing tests use small values (1, 2, 0) and mid-range test vectors, none of which trigger the `bit_254 = 1` path in the canonicality check.
> - **Recommendation:** Add tests with field elements near p - 1 to exercise the full canonicality check path.

> [!NOTE]
> ### [BA-043] Basic Satisfaction Tests Lack Output Verification
> - **Description:** `test_blake2b_circuit` (lines 112–121) and `test_blake2b_empty_input` (lines 124–178) only verify that the circuit is satisfiable (i.e., `prover.verify() == Ok(())`), without checking that the hash output matches any expected value. These tests would pass even if the circuit computed a completely wrong hash, as long as internal constraints are consistent.
> - **Recommendation:** Either add public input constraints with expected values, or rely on the more comprehensive reference tests (`test_blake2b_against_reference`, `test_blake2b_zeros_against_reference`) for correctness verification. The satisfaction-only tests still have value for detecting constraint violations.

> [!TIP]
> ### [BA-044] `blake2b_simd` Comparison Covers Target Use Case
> - **Description:** The `test_compact_hash_nullifier_proof` test (lines 571–742) compares the circuit output against `blake2b_simd`, an independent and well-tested BLAKE2b implementation. This test covers the primary use case: 148-byte compact action hash with the ZIP-244 personalization string `"ZTxIdOrcActCHash"`.
>   The test exercises:
>   - [x] Hybrid input processing (field elements + raw bytes)
>   - [x] Canonicality checking on field inputs (nullifier, cmx)
>   - [x] Boolean-only constraint on byte inputs (epk, enc_prefix)
>   - [x] Multi-block processing (148 bytes = 2 blocks)
>   - [x] Hash output compared against independent reference
>   - [x] Public input constraints verified
>
>   This provides strong evidence that the circuit produces correct BLAKE2b-256 hashes for its intended use case.
> - No issues found.

> [!NOTE]
> ### [BA-045] Docstring References BLAKE2s Instead of BLAKE2b
> - **Description:** Several comments and docstrings reference "BLAKE2s" when they should say "BLAKE2b":
>
>   | Location | Text | Should be |
>   |----------|------|-----------|
>   | Line 238 | "Configure the BLAKE2s chip" | "Configure the BLAKE2b chip" |
>   | Line 553 | "Construct a new BLAKE2s chip" | "Construct a new BLAKE2b chip" |
>   | Line 1166 | "different BLAKE2s outputs" | "different BLAKE2b outputs" |
>
>   These are remnants from the BLAKE2s -> BLAKE2b port and do not affect functionality.
> - **Recommendation:** Update the docstrings to reference BLAKE2b.

---

## 6. Summary

### Finding Statistics

| Severity         | Count | Finding IDs |
|------------------|-------|-------------|
| **Critical**     | 0     | —           |
| **Warning**      | 4     | BA-023, BA-037, BA-038, BA-039 |
| **Minor**        | 8     | BA-017, BA-018, BA-035, BA-040, BA-041, BA-042, BA-043, BA-045 |
| **No Issues Found** | 33 | BA-001 through BA-016, BA-019 through BA-022, BA-024 through BA-034, BA-036, BA-044 |

### Overall Assessment

**No critical issues were found.** The BLAKE2b-256 circuit is sound for its target use case (compact action hash for ZIP-244 nullifier proofs).

**Key positive findings:**
- All cryptographic constants match RFC 7693
- The canonicality check is sound: copy constraints, range checks, and conditional logic correctly prevent non-canonical field element representations
- The G mixing function, compression function, and finalization all match the RFC specification exactly
- All gate polynomials are correct and selector enablement matches usage sites
- Copy constraints form a complete chain from field element inputs through to hash output
- The chained addition optimization (skipping intermediate range checks) is sound, with a clear arithmetic argument
- The hybrid input system correctly distinguishes field elements (canonicality-checked) from raw bytes (boolean-constrained only)

**Key concerns:**
- The byte counter deviation from RFC 7693 for inputs under 128 bytes means the circuit does NOT produce standard BLAKE2b hashes for small inputs. This is acceptable if documented, but the deviation should not be relied upon for interoperability with standard BLAKE2b implementations.
- Test coverage gaps (no standard test vectors, no canonicality negative tests, no boundary value tests) reduce confidence in edge-case correctness. The target use case (148-byte compact action hash) is well-tested against an independent reference.

### Verification

- [x] `cargo test --test blake2b --all-features` — all 5 tests pass
- [x] `cargo clippy -- -D warnings` — no warnings
- [x] All line number references verified against `src/circuit/blake2b.rs` and `tests/blake2b.rs`
- [x] All finding IDs used exactly once and are sequential (BA-001 through BA-045)
