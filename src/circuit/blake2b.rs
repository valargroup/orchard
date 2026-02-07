//! BLAKE2b-256 circuit implementation for Halo2.
//!
//! Provides a constrained BLAKE2b-256 hash function for use in Orchard compact
//! action hashes (ZIP-244).
//!
//! Field element inputs (nf, cmx) are single Fp values, converted to 32 bytes
//! each. Bytes are range-checked via a lookup table and packed into 64-bit words
//! — BLAKE2b's native word size — via `s_word_decompose` and `s_word_combine` gates.
//! Recomposition is verified via `s_result_encode` and `s_field_recompose` gates.
//!
//! XOR operations use a 16×16 nibble-level lookup table. Each byte XOR is split
//! into two nibble XORs (lo and hi halves), with decompose/recompose gates.
//! Rotations by multiples of 8 bits (R1=32, R2=24, R3=16) are free byte
//! shuffles; R4=63 (left-rotate-1) uses a dedicated shift gate.
//!
//! BLAKE2b-256 parameters (RFC 7693):
//!               | BLAKE2b-256      |
//! --------------+------------------+
//!  Bits in word | w = 64           |
//!  Rounds in F  | r = 12           |
//!  Block bytes  | bb = 128         |
//!  Hash bytes   | nn = 32          |
//!  Key bytes    | 0 <= kk <= 64    |
//!  Input bytes  | 0 <= ll < 2**128 |
//! --------------+------------------+
//!  G Rotation   | (R1, R2, R3, R4) |
//!   constants = | (32, 24, 16, 63) |
//! --------------+------------------+

use alloc::vec;
use alloc::vec::Vec;
use core::marker::PhantomData;
use group::ff::PrimeField;
use halo2_gadgets::utilities::bool_check;
use halo2_proofs::{
    circuit::{AssignedCell, Layouter, Value},
    plonk::{
        Advice, Column, ConstraintSystem, Constraints, Error, Selector, TableColumn,
    },
    poly::Rotation,
};

// Advice column indices (for readability in gate layouts).
const A0: usize = 0;
const A1: usize = 1;
const A2: usize = 2;
const A3: usize = 3;
const A4: usize = 4;
const A5: usize = 5;
const A6: usize = 6;
const A7: usize = 7;
const A8: usize = 8;
const A9: usize = 9;
const A10: usize = 10;
const A11: usize = 11;
const A12: usize = 12;
const A13: usize = 13;
const A14: usize = 14;
const A15: usize = 15;
const A16: usize = 16;
const A17: usize = 17;

// ----------------
// Value helpers
// ----------------

/// Extract the least-significant byte from a field element's little-endian representation.
///
/// Used to recover a byte value from a field element known to be in [0, 255].
fn f_to_u8_le<F: PrimeField>(f: &F) -> u8 {
    let repr = f.to_repr();
    repr.as_ref()[0]
}

/// Reconstruct a word `Value` from assigned byte cells (little-endian).
///
/// Bytes are stored least-significant-first. Reconstruction uses Horner's method
/// on the reversed array: `fold(0, |acc, byte| acc * 256 + byte)`.
fn word_value_from_bytes<F: PrimeField>(bytes: &[AssignedCell<F, F>]) -> Value<F> {
    let byte_values: Value<Vec<_>> = bytes.iter().map(|byte| byte.value()).collect();
    byte_values.map(|bytes| {
        bytes
            .into_iter()
            .rev()
            .fold(F::ZERO, |acc, byte| acc * F::from(1 << 8) + byte)
    })
}

/// Reconstruct a field `Value` from 64-bit `Blake2bWord`s (little-endian).
///
/// Words are stored least-significant-first. Reconstruction uses Horner's method
/// on the reversed array: `fold(0, |acc, word| acc * 2^64 + word)`.
fn field_value_from_words_64<F: PrimeField>(words: &[Blake2bWord<F>]) -> Value<F> {
    let word_values: Value<Vec<_>> = words.iter().map(|word| word.get_word().value()).collect();
    word_values.map(|words| {
        words
            .into_iter()
            .rev()
            .fold(F::ZERO, |acc, word| acc * F::from_u128(1u128 << 64) + word)
    })
}

// ----------------
// Helper gadgets
// ----------------

/// Place a prover-supplied (witness) value into an advice cell.
///
/// "Free" means no selector is enabled in this region — the cell is unconstrained
/// here and must be constrained elsewhere (typically via `copy_advice`).
pub fn assign_free_advice<F: PrimeField>(
    mut layouter: impl Layouter<F>,
    column: Column<Advice>,
    value: Value<F>,
) -> Result<AssignedCell<F, F>, Error> {
    layouter.assign_region(
        || "assign free advice",
        |mut region| region.assign_advice(|| "value", column, 0, || value),
    )
}

/// Place a public constant into an advice cell, pinned to the fixed column.
///
/// Unlike `assign_free_advice`, the verifier enforces the exact value.
/// Used for spec-defined constants (IV words, zero padding, etc.).
pub fn assign_free_constant<F: PrimeField>(
    mut layouter: impl Layouter<F>,
    column: Column<Advice>,
    constant: F,
) -> Result<AssignedCell<F, F>, Error> {
    layouter.assign_region(
        || "assign constant",
        |mut region| region.assign_advice_from_constant(|| "constant", column, 0, constant),
    )
}

// ----------------
// BLAKE2b CONSTANTS
// ----------------

// Initialization Vector (RFC 7693, Section 2.6).
// First 64 bits of the fractional parts of sqrt(2..9).
const IV: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

// Message word permutation schedule (RFC 7693, Section 2.7).
// Each of the 10 rows defines the message block order for one round.
// Rounds beyond 10 cycle back through the schedule (round i uses SIGMA[i % 10]).
const SIGMA: [[usize; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

// Rotation constants for BLAKE2b (RFC 7693, Section 2.1).
const R1: usize = 32;
const R2: usize = 24;
const R3: usize = 16;

// Number of rounds in the compression function.
const ROUNDS: usize = 12;

// ---- Chip, config, and type definitions ----

/// Constrained BLAKE2b-256 chip for Orchard compact action hashes (ZIP-244).
///
/// Delegates gate definitions to [`Blake2bConfig`] and provides synthesis methods
/// for hashing field elements and raw bytes through the BLAKE2b-256 compression
/// function.
#[derive(Clone, Debug)]
pub struct Blake2bChip<F: PrimeField> {
    config: Blake2bConfig<F>,
    _marker: PhantomData<F>,
}

/// Configuration for the BLAKE2b chip.
///
/// Uses a 16×16 nibble XOR lookup table and byte range checking. Each byte XOR
/// is split into two nibble XORs with decompose/recompose gates. Defines custom
/// gates for word decomposition, addition, result encoding, word-combine, and
/// 1-bit left shift (for R4=63 rotation).
#[derive(Clone, Debug)]
pub struct Blake2bConfig<F: PrimeField> {
    /// Advice columns used by the chip.
    pub advices: [Column<Advice>; 18],
    /// Selector for word decomposition gate: word = sum(byte[i] * 256^i).
    pub s_word_decompose: Selector,
    /// Selector for result encoding gate: field = word_1 + word_2 * 2^64.
    pub s_result_encode: Selector,
    /// Selector for field recomposition gate: field = words[0..1] + words[2..3] * 2^128.
    pub s_field_recompose: Selector,
    /// Selector for combining two 32-bit words into one 64-bit word.
    pub s_word_combine: Selector,
    /// Selector for 1-bit left shift gate (used for R4=63 rotation).
    pub s_left_shift_1: Selector,
    /// Selector for fused add-and-decompose gate: lhs + rhs = bytes + carry*2^64.
    pub s_fused_add_decompose: Selector,
    /// Selector for single-row word addition: lhs + rhs = result + carry*2^64.
    pub s_word_add_single: Selector,
    /// Selector for fused pack-add-decompose: pack input bytes + other_word = result bytes + carry*2^64.
    pub s_pack_add_decompose: Selector,
    /// Selector for fused pack-and-add: pack input bytes + other_word = result + carry*2^64.
    pub s_pack_add: Selector,
    /// Complex selector for dual nibble XOR (two byte XORs per row on A0-A8 + A9-A17).
    q_nibble_xor_dual: Selector,
    /// Complex selector for 8-byte range check lookup (checks A0..A7 against [0,255]).
    q_range_check_8: Selector,
    /// Lookup table columns for byte XOR.
    xor_table_lhs: TableColumn,
    xor_table_rhs: TableColumn,
    xor_table_out: TableColumn,
    /// Lookup table column for byte range [0, 255].
    range_table: TableColumn,
    _marker: PhantomData<F>,
}

/// A compact Orchard action with pre-assigned witness cells.
///
/// Used as input to [`Blake2bChip::process_precomputed_action_hash`] for the
/// 2-action hash per ZIP-244 `hashOrchardActions`.
///
/// Each action provides 148 bytes: nf(32B) + cmx(32B) + epk(32B) + enc_prefix(52B).
#[derive(Debug)]
pub struct CompactActionCells<F: PrimeField> {
    /// Nullifier (single field element).
    pub nf: AssignedCell<F, F>,
    /// Note commitment (single field element).
    pub cmx: AssignedCell<F, F>,
    /// Ephemeral key bytes (32 cells, each range-checked to [0, 255]).
    pub epk_bytes: [AssignedCell<F, F>; 32],
    /// First 52 bytes of the encrypted note ciphertext.
    pub enc_prefix: [AssignedCell<F, F>; 52],
}

/// A 64-bit word represented as both a packed field element and its 8 byte cells.
///
/// The dual representation exists because addition gates operate on packed words while
/// XOR and rotation gates operate on individual bytes. Both views stay in sync via
/// the `s_word_decompose` constraint.
#[derive(Clone, Debug)]
pub struct Blake2bWord<F: PrimeField> {
    word: AssignedCell<F, F>,
    bytes: [AssignedCell<F, F>; 8],
}

// ---- Gate definitions ----

impl<F: PrimeField> Blake2bConfig<F> {
    /// Configure the BLAKE2b chip.
    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        advices: [Column<Advice>; 18],
    ) -> Blake2bConfig<F> {
        let s_word_decompose = meta.selector();
        let s_result_encode = meta.selector();
        let s_field_recompose = meta.selector();
        let s_word_combine = meta.selector();
        let s_left_shift_1 = meta.selector();
        let s_fused_add_decompose = meta.selector();
        let s_word_add_single = meta.selector();
        let s_pack_add_decompose = meta.selector();
        let s_pack_add = meta.selector();

        // Complex selectors for use in lookup expressions
        let q_range_check_8 = meta.complex_selector();
        let q_nibble_xor_dual = meta.complex_selector();

        // Lookup table columns
        let xor_table_lhs = meta.lookup_table_column();
        let xor_table_rhs = meta.lookup_table_column();
        let xor_table_out = meta.lookup_table_column();
        let range_table = meta.lookup_table_column();

        // word = b1 + b2*2^8 + b3*2^16 + ... + b8*2^56
        meta.create_gate("decompose word to bytes", |meta| {
            let word = meta.query_advice(advices[A0], Rotation::next());
            let byte_1 = meta.query_advice(advices[A0], Rotation::cur());
            let byte_2 = meta.query_advice(advices[A1], Rotation::cur());
            let byte_3 = meta.query_advice(advices[A2], Rotation::cur());
            let byte_4 = meta.query_advice(advices[A3], Rotation::cur());
            let byte_5 = meta.query_advice(advices[A4], Rotation::cur());
            let byte_6 = meta.query_advice(advices[A5], Rotation::cur());
            let byte_7 = meta.query_advice(advices[A6], Rotation::cur());
            let byte_8 = meta.query_advice(advices[A7], Rotation::cur());
            let s_word_decompose = meta.query_selector(s_word_decompose);

            vec![
                s_word_decompose
                    * (byte_1
                        + byte_2 * F::from(1 << 8)
                        + byte_3 * F::from(1 << 16)
                        + byte_4 * F::from(1 << 24)
                        + byte_5 * F::from(1u64 << 32)
                        + byte_6 * F::from(1u64 << 40)
                        + byte_7 * F::from(1u64 << 48)
                        + byte_8 * F::from(1u64 << 56)
                        - word),
            ]
        });

        // field = word_1 + word_2 * 2^64  (16 bytes packed into one Pallas field element)
        meta.create_gate("encode two words to one field", |meta| {
            let field_element = meta.query_advice(advices[A0], Rotation::next());
            let word_1 = meta.query_advice(advices[A0], Rotation::cur());
            let word_2 = meta.query_advice(advices[A1], Rotation::cur());
            let s_result_encode = meta.query_selector(s_result_encode);

            vec![s_result_encode * (word_1 + word_2 * F::from_u128(1u128 << 64) - field_element)]
        });

        // field = sum_01 + sum_23 * 2^128  (4 words packed into one field element)
        // sum_01 = w0 + w1 * 2^64, sum_23 = w2 + w3 * 2^64 (from s_result_encode)
        meta.create_gate("recompose field from word-pair sums", |meta| {
            let field_element = meta.query_advice(advices[A0], Rotation::next());
            let sum_01 = meta.query_advice(advices[A0], Rotation::cur());
            let sum_23 = meta.query_advice(advices[A1], Rotation::cur());
            let s_field_recompose = meta.query_selector(s_field_recompose);
            let two_128 = F::from_u128(1u128 << 64) * F::from_u128(1u128 << 64);

            vec![s_field_recompose * (sum_01 + sum_23 * two_128 - field_element)]
        });

        // word_64 = word_32_lo + word_32_hi * 2^32
        meta.create_gate("combine two 32-bit words to 64-bit", |meta| {
            let s_word_combine = meta.query_selector(s_word_combine);

            let word_32_lo = meta.query_advice(advices[A0], Rotation::cur());
            let word_32_hi = meta.query_advice(advices[A1], Rotation::cur());
            let word_64 = meta.query_advice(advices[A0], Rotation::next());

            vec![s_word_combine * (word_32_lo + word_32_hi * F::from(1u64 << 32) - word_64)]
        });

        // Left-shift 1 bit gate (for R4=63 right-rotation = left-rotation by 1).
        // Layout:
        //   Row 0: byte_in[0..8] in A0..A7
        //   Row 1: byte_out[0..8] in A0..A7
        //   Row 2: carry[0..8] in A0..A7
        // Constraint per byte i:
        //   2 * byte_in[i] + carry[(i-1+8)%8] = byte_out[i] + 256 * carry[i]
        //   bool_check(carry[i])
        meta.create_gate("left shift 1 bit (R4=63)", |meta| {
            let s = meta.query_selector(s_left_shift_1);
            let mut constraints = Vec::with_capacity(16);

            for i in 0..8usize {
                let byte_in = meta.query_advice(advices[i], Rotation::cur());
                let byte_out = meta.query_advice(advices[i], Rotation::next());
                let carry_out = meta.query_advice(advices[i], Rotation(2));
                let carry_in = meta.query_advice(advices[(i + 7) % 8], Rotation(2));

                constraints.push(
                    byte_in * F::from(2) + carry_in - byte_out
                        - carry_out.clone() * F::from(256),
                );
                constraints.push(bool_check(carry_out));
            }

            Constraints::with_selector(s, constraints)
        });

        // Fused add-and-decompose gate:
        // Row 0: A0-A7 = result bytes, A8 = lhs, A9 = rhs
        // Row 1: A0 = carry, A1 = result_word
        // Constraints:
        //   lhs + rhs = result_word + carry * 2^64
        //   result_word = byte[0] + byte[1]*256 + ... + byte[7]*2^56
        //   bool_check(carry)
        // q_range_check_8 is enabled on row 0 externally (in the method).
        meta.create_gate("fused add and decompose", |meta| {
            let s = meta.query_selector(s_fused_add_decompose);
            let lhs = meta.query_advice(advices[A8], Rotation::cur());
            let rhs = meta.query_advice(advices[A9], Rotation::cur());
            let carry = meta.query_advice(advices[A0], Rotation::next());
            let result_word = meta.query_advice(advices[A1], Rotation::next());

            let mut byte_sum = meta.query_advice(advices[A0], Rotation::cur());
            for i in 1..8usize {
                byte_sum = byte_sum
                    + meta.query_advice(advices[i], Rotation::cur()) * F::from(1u64 << (8 * i));
            }

            Constraints::with_selector(
                s,
                [
                    ("addition", lhs + rhs - result_word.clone() - carry.clone() * F::from_u128(1u128 << 64)),
                    ("decompose", result_word - byte_sum),
                    ("carry bool", bool_check(carry)),
                ],
            )
        });

        // Single-row word addition gate:
        // Row 0: A0 = lhs, A1 = rhs, A2 = result, A3 = carry
        // Constraint: lhs + rhs = result + carry * 2^64, bool_check(carry)
        meta.create_gate("single-row word add", |meta| {
            let s = meta.query_selector(s_word_add_single);
            let lhs = meta.query_advice(advices[A0], Rotation::cur());
            let rhs = meta.query_advice(advices[A1], Rotation::cur());
            let result = meta.query_advice(advices[A2], Rotation::cur());
            let carry = meta.query_advice(advices[A3], Rotation::cur());

            Constraints::with_selector(
                s,
                [
                    ("add", lhs + rhs - result - carry.clone() * F::from_u128(1u128 << 64)),
                    ("carry bool", bool_check(carry)),
                ],
            )
        });

        // Fused pack-add-decompose gate:
        // Row 0: A0-A7 = input bytes (from XOR rotation), A8 = other_word, A9 = carry
        // Row 1: A0-A7 = result bytes, A8 = result_word
        // Constraints:
        //   other_word + pack(input_bytes) = result_word + carry * 2^64
        //   result_word = pack(result_bytes)
        //   bool_check(carry)
        // q_range_check_8 on row 1 (for result bytes) — enabled externally.
        meta.create_gate("fused pack-add-decompose", |meta| {
            let s = meta.query_selector(s_pack_add_decompose);

            let other_word = meta.query_advice(advices[A8], Rotation::cur());
            let carry = meta.query_advice(advices[A9], Rotation::cur());
            let result_word = meta.query_advice(advices[A8], Rotation::next());

            let mut input_sum = meta.query_advice(advices[A0], Rotation::cur());
            for i in 1..8usize {
                input_sum = input_sum
                    + meta.query_advice(advices[i], Rotation::cur()) * F::from(1u64 << (8 * i));
            }

            let mut result_sum = meta.query_advice(advices[A0], Rotation::next());
            for i in 1..8usize {
                result_sum = result_sum
                    + meta.query_advice(advices[i], Rotation::next()) * F::from(1u64 << (8 * i));
            }

            Constraints::with_selector(
                s,
                [
                    ("pack-add", other_word + input_sum - result_word.clone() - carry.clone() * F::from_u128(1u128 << 64)),
                    ("decompose", result_word - result_sum),
                    ("carry bool", bool_check(carry)),
                ],
            )
        });

        // Fused pack-and-add gate:
        // Row 0: A0-A7 = input bytes, A8 = other_word
        // Row 1: A0 = result, A1 = carry
        // Constraints:
        //   pack(input_bytes) + other_word = result + carry * 2^64
        //   bool_check(carry)
        meta.create_gate("fused pack-and-add", |meta| {
            let s = meta.query_selector(s_pack_add);

            let other_word = meta.query_advice(advices[A8], Rotation::cur());
            let result = meta.query_advice(advices[A0], Rotation::next());
            let carry = meta.query_advice(advices[A1], Rotation::next());

            let mut input_sum = meta.query_advice(advices[A0], Rotation::cur());
            for i in 1..8usize {
                input_sum = input_sum
                    + meta.query_advice(advices[i], Rotation::cur()) * F::from(1u64 << (8 * i));
            }

            Constraints::with_selector(
                s,
                [
                    ("pack-add", input_sum + other_word - result - carry.clone() * F::from_u128(1u128 << 64)),
                    ("carry bool", bool_check(carry)),
                ],
            )
        });

        // Byte range lookup: each of A0..A7 must be in [0, 255].
        // All 8 lookups share q_range_check_8, so enabling it on one row
        // simultaneously range-checks all 8 advice columns.
        for col in &advices[..8] {
            let col = *col;
            meta.lookup(|meta| {
                let q = meta.query_selector(q_range_check_8);
                let byte = meta.query_advice(col, Rotation::cur());
                vec![(q * byte, range_table)]
            });
        }

        // Dual nibble XOR: two byte XORs per row
        // Row layout: A0-A8 = first byte XOR, A9-A17 = second byte XOR
        meta.create_gate("dual nibble xor decompose", |meta| {
            let q = meta.query_selector(q_nibble_xor_dual);

            let byte_a1 = meta.query_advice(advices[A0], Rotation::cur());
            let lo_a1 = meta.query_advice(advices[A2], Rotation::cur());
            let hi_a1 = meta.query_advice(advices[A3], Rotation::cur());
            let byte_b1 = meta.query_advice(advices[A1], Rotation::cur());
            let lo_b1 = meta.query_advice(advices[A4], Rotation::cur());
            let hi_b1 = meta.query_advice(advices[A5], Rotation::cur());
            let lo_out1 = meta.query_advice(advices[A6], Rotation::cur());
            let hi_out1 = meta.query_advice(advices[A7], Rotation::cur());
            let out1 = meta.query_advice(advices[A8], Rotation::cur());

            let byte_a2 = meta.query_advice(advices[A9], Rotation::cur());
            let lo_a2 = meta.query_advice(advices[A11], Rotation::cur());
            let hi_a2 = meta.query_advice(advices[A12], Rotation::cur());
            let byte_b2 = meta.query_advice(advices[A10], Rotation::cur());
            let lo_b2 = meta.query_advice(advices[A13], Rotation::cur());
            let hi_b2 = meta.query_advice(advices[A14], Rotation::cur());
            let lo_out2 = meta.query_advice(advices[A15], Rotation::cur());
            let hi_out2 = meta.query_advice(advices[A16], Rotation::cur());
            let out2 = meta.query_advice(advices[A17], Rotation::cur());

            Constraints::with_selector(
                q,
                vec![
                    byte_a1 - lo_a1 - hi_a1 * F::from(16),
                    byte_b1 - lo_b1 - hi_b1 * F::from(16),
                    out1 - lo_out1 - hi_out1 * F::from(16),
                    byte_a2 - lo_a2 - hi_a2 * F::from(16),
                    byte_b2 - lo_b2 - hi_b2 * F::from(16),
                    out2 - lo_out2 - hi_out2 * F::from(16),
                ],
            )
        });

        // Dual XOR lookups — first byte (lo and hi nibbles)
        meta.lookup(|meta| {
            let q = meta.query_selector(q_nibble_xor_dual);
            vec![
                (q.clone() * meta.query_advice(advices[A2], Rotation::cur()), xor_table_lhs),
                (q.clone() * meta.query_advice(advices[A4], Rotation::cur()), xor_table_rhs),
                (q * meta.query_advice(advices[A6], Rotation::cur()), xor_table_out),
            ]
        });
        meta.lookup(|meta| {
            let q = meta.query_selector(q_nibble_xor_dual);
            vec![
                (q.clone() * meta.query_advice(advices[A3], Rotation::cur()), xor_table_lhs),
                (q.clone() * meta.query_advice(advices[A5], Rotation::cur()), xor_table_rhs),
                (q * meta.query_advice(advices[A7], Rotation::cur()), xor_table_out),
            ]
        });

        // Dual XOR lookups — second byte (lo and hi nibbles)
        meta.lookup(|meta| {
            let q = meta.query_selector(q_nibble_xor_dual);
            vec![
                (q.clone() * meta.query_advice(advices[A11], Rotation::cur()), xor_table_lhs),
                (q.clone() * meta.query_advice(advices[A13], Rotation::cur()), xor_table_rhs),
                (q * meta.query_advice(advices[A15], Rotation::cur()), xor_table_out),
            ]
        });
        meta.lookup(|meta| {
            let q = meta.query_selector(q_nibble_xor_dual);
            vec![
                (q.clone() * meta.query_advice(advices[A12], Rotation::cur()), xor_table_lhs),
                (q.clone() * meta.query_advice(advices[A14], Rotation::cur()), xor_table_rhs),
                (q * meta.query_advice(advices[A16], Rotation::cur()), xor_table_out),
            ]
        });

        Blake2bConfig {
            advices,
            s_word_decompose,
            s_result_encode,
            s_field_recompose,
            s_word_combine,
            s_left_shift_1,
            s_fused_add_decompose,
            s_word_add_single,
            s_pack_add_decompose,
            s_pack_add,
            q_nibble_xor_dual,
            q_range_check_8,
            xor_table_lhs,
            xor_table_rhs,
            xor_table_out,
            range_table,
            _marker: PhantomData,
        }
    }
}

// ---- Blake2bWord constructors and utilities ----

impl<F: PrimeField> Blake2bWord<F> {
    /// Create a `Blake2bWord` from a constant u64 WITHOUT decomposition gates.
    ///
    /// The word and each byte are pinned to the fixed column via
    /// `assign_advice_from_constant`. No decomposition or range-check gates are
    /// enabled — the fixed-column pinning alone guarantees correctness.
    ///
    /// Use this for spec-defined constants (IV words, zero padding, etc.) where
    /// the values are known at circuit compile time.
    pub fn from_constant_u64_unchecked(
        value: u64,
        layouter: &mut impl Layouter<F>,
        config: &Blake2bConfig<F>,
    ) -> Result<Self, Error> {
        let word = assign_free_constant(
            layouter.namespace(|| "constant word"),
            config.advices[A0],
            F::from(value),
        )?;
        let mut bytes = Vec::with_capacity(8);
        let mut tmp = value;
        for _ in 0..8 {
            let byte_val = (tmp & 0xFF) as u8;
            let byte_cell = assign_free_constant(
                layouter.namespace(|| "constant byte"),
                config.advices[A0],
                F::from(byte_val as u64),
            )?;
            bytes.push(byte_cell);
            tmp >>= 8;
        }
        Ok(Self {
            word,
            bytes: bytes.try_into().unwrap(),
        })
    }

    /// Byte-level right rotation (for R1, R2, R3 which are multiples of 8).
    ///
    /// Right-rotate by `by` bits where `by` is a multiple of 8.
    /// Returns the rotated byte array (zero-cost reordering).
    pub fn byte_rotate(bytes: &[AssignedCell<F, F>; 8], by: usize) -> [AssignedCell<F, F>; 8] {
        assert!(by % 8 == 0, "byte_rotate only supports multiples of 8");
        let shift = (by / 8) % 8;
        let mut result = Vec::with_capacity(8);
        for i in 0..8 {
            result.push(bytes[(i + shift) % 8].clone());
        }
        result.try_into().unwrap()
    }

    /// Get the packed word cell.
    pub fn get_word(&self) -> &AssignedCell<F, F> {
        &self.word
    }

    /// Get the 8 byte cells.
    pub fn get_bytes(&self) -> &[AssignedCell<F, F>; 8] {
        &self.bytes
    }

    /// Construct from an assigned 64-bit word cell by decomposing it into bytes.
    ///
    /// Uses `s_word_decompose` to constrain bytes sum to word, and
    /// `q_range_check_8` to range-check all 8 bytes to [0, 255].
    pub fn from_word(
        chip: &Blake2bChip<F>,
        mut layouter: impl Layouter<F>,
        word: AssignedCell<F, F>,
    ) -> Result<Self, Error> {
        let bytes = layouter.assign_region(
            || "word to bytes",
            |mut region| {
                chip.config.s_word_decompose.enable(&mut region, 0)?;
                chip.config.q_range_check_8.enable(&mut region, 0)?;

                let mut bytes = Vec::with_capacity(8);
                for i in 0..8 {
                    let byte_val = word.value().map(|v| {
                        F::from(v.to_repr().as_ref()[i] as u64)
                    });
                    let byte = region.assign_advice(
                        || format!("byte_{}", i),
                        chip.config.advices[i],
                        0,
                        || byte_val,
                    )?;
                    bytes.push(byte);
                }
                word.copy_advice(|| "word", &mut region, chip.config.advices[A0], 1)?;

                Ok(bytes)
            },
        )?;

        Ok(Self {
            word,
            bytes: bytes.try_into().unwrap(),
        })
    }

    /// Construct from 8 pre-range-checked byte cells by packing into a word.
    ///
    /// Uses `s_word_decompose` to constrain word = sum(byte[i] * 256^i).
    /// Does NOT enable `q_range_check_8` — the caller guarantees bytes are
    /// already range-checked (e.g. from XOR lookup output or left_rotate_1).
    pub fn from_bytes_unchecked(
        chip: &Blake2bChip<F>,
        mut layouter: impl Layouter<F>,
        bytes: [AssignedCell<F, F>; 8],
    ) -> Result<Self, Error> {
        let word = layouter.assign_region(
            || "pack bytes to word",
            |mut region| {
                chip.config.s_word_decompose.enable(&mut region, 0)?;

                for (i, byte) in bytes.iter().enumerate() {
                    byte.copy_advice(
                        || format!("byte_{}", i),
                        &mut region,
                        chip.config.advices[i],
                        0,
                    )?;
                }

                let word_val = word_value_from_bytes(&bytes);
                region.assign_advice(|| "word", chip.config.advices[A0], 1, || word_val)
            },
        )?;

        Ok(Self { word, bytes })
    }
}

impl<F: PrimeField> Blake2bChip<F> {
    /// Construct a new BLAKE2b chip from the given config.
    pub fn construct(config: Blake2bConfig<F>) -> Self {
        Self {
            config,
            _marker: PhantomData,
        }
    }

    // ---- Table loading ----

    /// Load the XOR and range lookup tables. Must be called once during synthesize.
    pub fn load_tables(
        config: &Blake2bConfig<F>,
        layouter: &mut impl Layouter<F>,
    ) -> Result<(), Error> {
        // Load nibble XOR table (16 × 16 = 256 entries)
        layouter.assign_table(
            || "XOR table",
            |mut table| {
                for a in 0u64..16 {
                    for b in 0u64..16 {
                        let offset = (a * 16 + b) as usize;
                        table.assign_cell(
                            || "lhs",
                            config.xor_table_lhs,
                            offset,
                            || Value::known(F::from(a)),
                        )?;
                        table.assign_cell(
                            || "rhs",
                            config.xor_table_rhs,
                            offset,
                            || Value::known(F::from(b)),
                        )?;
                        table.assign_cell(
                            || "out",
                            config.xor_table_out,
                            offset,
                            || Value::known(F::from(a ^ b)),
                        )?;
                    }
                }
                Ok(())
            },
        )?;

        // Load range table [0, 255]
        layouter.assign_table(
            || "range table",
            |mut table| {
                for i in 0..256 {
                    table.assign_cell(
                        || "range",
                        config.range_table,
                        i,
                        || Value::known(F::from(i as u64)),
                    )?;
                }
                Ok(())
            },
        )?;

        Ok(())
    }

    // ---- Low-level primitives ----

    /// XOR two pairs of bytes via nibble lookup table (dual: 2 byte XORs per row).
    ///
    /// Row layout: A0-A8 = first byte XOR, A9-A17 = second byte XOR.
    fn dual_byte_xor(
        &self,
        mut layouter: impl Layouter<F>,
        lhs1: &AssignedCell<F, F>,
        rhs1: &AssignedCell<F, F>,
        lhs2: &AssignedCell<F, F>,
        rhs2: &AssignedCell<F, F>,
    ) -> Result<(AssignedCell<F, F>, AssignedCell<F, F>), Error> {
        layouter.assign_region(
            || "dual nibble xor",
            |mut region| {
                self.config.q_nibble_xor_dual.enable(&mut region, 0)?;

                // First byte XOR (A0-A8)
                lhs1.copy_advice(|| "byte_a1", &mut region, self.config.advices[A0], 0)?;
                rhs1.copy_advice(|| "byte_b1", &mut region, self.config.advices[A1], 0)?;

                let vals1 = lhs1.value().zip(rhs1.value()).map(|(l, r)| {
                    let l_byte = f_to_u8_le(l);
                    let r_byte = f_to_u8_le(r);
                    let lo_a = l_byte & 0x0F;
                    let hi_a = l_byte >> 4;
                    let lo_b = r_byte & 0x0F;
                    let hi_b = r_byte >> 4;
                    let lo_out = lo_a ^ lo_b;
                    let hi_out = hi_a ^ hi_b;
                    let out_byte = lo_out | (hi_out << 4);
                    (lo_a, hi_a, lo_b, hi_b, lo_out, hi_out, out_byte)
                });

                region.assign_advice(|| "lo_a1", self.config.advices[A2], 0, || vals1.map(|v| F::from(v.0 as u64)))?;
                region.assign_advice(|| "hi_a1", self.config.advices[A3], 0, || vals1.map(|v| F::from(v.1 as u64)))?;
                region.assign_advice(|| "lo_b1", self.config.advices[A4], 0, || vals1.map(|v| F::from(v.2 as u64)))?;
                region.assign_advice(|| "hi_b1", self.config.advices[A5], 0, || vals1.map(|v| F::from(v.3 as u64)))?;
                region.assign_advice(|| "lo_out1", self.config.advices[A6], 0, || vals1.map(|v| F::from(v.4 as u64)))?;
                region.assign_advice(|| "hi_out1", self.config.advices[A7], 0, || vals1.map(|v| F::from(v.5 as u64)))?;
                let out1 = region.assign_advice(|| "out1", self.config.advices[A8], 0, || vals1.map(|v| F::from(v.6 as u64)))?;

                // Second byte XOR (A9-A17)
                lhs2.copy_advice(|| "byte_a2", &mut region, self.config.advices[A9], 0)?;
                rhs2.copy_advice(|| "byte_b2", &mut region, self.config.advices[A10], 0)?;

                let vals2 = lhs2.value().zip(rhs2.value()).map(|(l, r)| {
                    let l_byte = f_to_u8_le(l);
                    let r_byte = f_to_u8_le(r);
                    let lo_a = l_byte & 0x0F;
                    let hi_a = l_byte >> 4;
                    let lo_b = r_byte & 0x0F;
                    let hi_b = r_byte >> 4;
                    let lo_out = lo_a ^ lo_b;
                    let hi_out = hi_a ^ hi_b;
                    let out_byte = lo_out | (hi_out << 4);
                    (lo_a, hi_a, lo_b, hi_b, lo_out, hi_out, out_byte)
                });

                region.assign_advice(|| "lo_a2", self.config.advices[A11], 0, || vals2.map(|v| F::from(v.0 as u64)))?;
                region.assign_advice(|| "hi_a2", self.config.advices[A12], 0, || vals2.map(|v| F::from(v.1 as u64)))?;
                region.assign_advice(|| "lo_b2", self.config.advices[A13], 0, || vals2.map(|v| F::from(v.2 as u64)))?;
                region.assign_advice(|| "hi_b2", self.config.advices[A14], 0, || vals2.map(|v| F::from(v.3 as u64)))?;
                region.assign_advice(|| "lo_out2", self.config.advices[A15], 0, || vals2.map(|v| F::from(v.4 as u64)))?;
                region.assign_advice(|| "hi_out2", self.config.advices[A16], 0, || vals2.map(|v| F::from(v.5 as u64)))?;
                let out2 = region.assign_advice(|| "out2", self.config.advices[A17], 0, || vals2.map(|v| F::from(v.6 as u64)))?;

                Ok((out1, out2))
            },
        )
    }

    /// XOR two 64-bit words byte-by-byte via dual lookup table.
    ///
    /// Processes 2 bytes per row (4 rows instead of 8).
    /// Returns 8 result byte cells (range-checked by the XOR table).
    fn word_xor(
        &self,
        mut layouter: impl Layouter<F>,
        x: &[AssignedCell<F, F>; 8],
        y: &[AssignedCell<F, F>; 8],
    ) -> Result<[AssignedCell<F, F>; 8], Error> {
        let mut result = Vec::with_capacity(8);
        for i in (0..8).step_by(2) {
            let (r1, r2) = self.dual_byte_xor(
                layouter.namespace(|| format!("xor_bytes_{}_{}", i, i + 1)),
                &x[i],
                &y[i],
                &x[i + 1],
                &y[i + 1],
            )?;
            result.push(r1);
            result.push(r2);
        }
        Ok(result.try_into().unwrap())
    }

    /// Left-rotate a byte array by 1 bit (for R4=63 right-rotation).
    ///
    /// Uses the `s_left_shift_1` gate. Output bytes are range-checked by
    /// the gate constraint (proven in-range given in-range inputs + bool carries).
    fn left_rotate_1(
        &self,
        mut layouter: impl Layouter<F>,
        bytes_in: &[AssignedCell<F, F>; 8],
    ) -> Result<[AssignedCell<F, F>; 8], Error> {
        layouter.assign_region(
            || "left rotate 1 bit",
            |mut region| {
                self.config.s_left_shift_1.enable(&mut region, 0)?;

                // Row 0: byte_in
                for (i, byte) in bytes_in.iter().enumerate() {
                    byte.copy_advice(
                        || format!("byte_in_{}", i),
                        &mut region,
                        self.config.advices[i],
                        0,
                    )?;
                }

                // Pre-compute byte values
                let byte_vals: Vec<Value<u8>> = (0..8)
                    .map(|i| bytes_in[i].value().map(|v| f_to_u8_le(v)))
                    .collect();

                // Row 1: byte_out
                let mut bytes_out = Vec::with_capacity(8);
                for i in 0..8 {
                    let byte_out_val =
                        byte_vals[i]
                            .zip(byte_vals[(i + 7) % 8])
                            .map(|(b, prev_b)| {
                                let carry_in = prev_b >> 7;
                                F::from((((b as u16) * 2 + carry_in as u16) & 0xFF) as u64)
                            });
                    let byte_out = region.assign_advice(
                        || format!("byte_out_{}", i),
                        self.config.advices[i],
                        1,
                        || byte_out_val,
                    )?;
                    bytes_out.push(byte_out);
                }

                // Row 2: carry
                for (i, bv) in byte_vals.iter().enumerate().take(8) {
                    let carry_val = bv.map(|b| F::from((b >> 7) as u64));
                    region.assign_advice(
                        || format!("carry_{}", i),
                        self.config.advices[i],
                        2,
                        || carry_val,
                    )?;
                }

                Ok(bytes_out.try_into().unwrap())
            },
        )
    }

    /// Fused add-and-decompose: computes (lhs + rhs) mod 2^64 and decomposes into bytes.
    ///
    /// Row 0: A0-A7 = result bytes, A8 = lhs, A9 = rhs
    /// Row 1: A0 = carry, A1 = result_word
    /// Enables s_fused_add_decompose and q_range_check_8 on row 0.
    /// Single 2-row region produces both word and bytes.
    fn fused_add_decompose(
        &self,
        mut layouter: impl Layouter<F>,
        lhs: &AssignedCell<F, F>,
        rhs: &AssignedCell<F, F>,
    ) -> Result<Blake2bWord<F>, Error> {
        layouter.assign_region(
            || "fused add-decompose",
            |mut region| {
                self.config.s_fused_add_decompose.enable(&mut region, 0)?;
                self.config.q_range_check_8.enable(&mut region, 0)?;

                // Copy lhs and rhs
                lhs.copy_advice(|| "lhs", &mut region, self.config.advices[A8], 0)?;
                rhs.copy_advice(|| "rhs", &mut region, self.config.advices[A9], 0)?;

                // Compute result
                let sum_val = lhs.value().zip(rhs.value()).map(|(&l, &r)| {
                    let s = l + r;
                    let repr = s.to_repr();
                    let bytes_raw = repr.as_ref();
                    let carry = bytes_raw[8] as u64;
                    let mut result_bytes = [0u8; 8];
                    result_bytes.copy_from_slice(&bytes_raw[..8]);
                    let result_word = s - F::from(carry) * F::from_u128(1u128 << 64);
                    (result_bytes, result_word, carry)
                });

                // Assign bytes on row 0
                let mut bytes = Vec::with_capacity(8);
                for i in 0..8 {
                    let byte = region.assign_advice(
                        || format!("byte_{}", i),
                        self.config.advices[i],
                        0,
                        || sum_val.map(|(b, _, _)| F::from(b[i] as u64)),
                    )?;
                    bytes.push(byte);
                }

                // Assign carry on row 1, A0
                region.assign_advice(
                    || "carry",
                    self.config.advices[A0],
                    1,
                    || sum_val.map(|(_, _, c)| F::from(c)),
                )?;

                // Assign result_word on row 1, A1
                let word = region.assign_advice(
                    || "result_word",
                    self.config.advices[A1],
                    1,
                    || sum_val.map(|(_, w, _)| w),
                )?;

                Ok(Blake2bWord {
                    word,
                    bytes: bytes.try_into().unwrap(),
                })
            },
        )
    }

    /// Single-row 64-bit modular addition: (lhs + rhs) mod 2^64.
    ///
    /// Row 0: A0 = lhs, A1 = rhs, A2 = result, A3 = carry
    fn single_row_add(
        &self,
        mut layouter: impl Layouter<F>,
        lhs: &AssignedCell<F, F>,
        rhs: &AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, Error> {
        layouter.assign_region(
            || "single-row word add",
            |mut region| {
                self.config.s_word_add_single.enable(&mut region, 0)?;

                lhs.copy_advice(|| "lhs", &mut region, self.config.advices[A0], 0)?;
                rhs.copy_advice(|| "rhs", &mut region, self.config.advices[A1], 0)?;

                let sum = lhs.value().zip(rhs.value()).map(|(&x, &y)| {
                    let s = x + y;
                    let carry = F::from(s.to_repr().as_ref()[8] as u64);
                    let ret = s - carry * F::from_u128(1u128 << 64);
                    (ret, carry)
                });

                let ret = region.assign_advice(
                    || "result",
                    self.config.advices[A2],
                    0,
                    || sum.map(|s| s.0),
                )?;
                region.assign_advice(
                    || "carry",
                    self.config.advices[A3],
                    0,
                    || sum.map(|s| s.1),
                )?;

                Ok(ret)
            },
        )
    }

    /// Fused pack-add-decompose: packs input bytes into a word, adds other_word,
    /// and decomposes the result into bytes.
    ///
    /// Row 0: A0-A7 = input bytes, A8 = other_word, A9 = carry
    /// Row 1: A0-A7 = result bytes, A8 = result_word
    /// Enables q_range_check_8 on row 1 (result bytes need range check).
    fn pack_add_decompose(
        &self,
        mut layouter: impl Layouter<F>,
        input_bytes: &[AssignedCell<F, F>; 8],
        other_word: &AssignedCell<F, F>,
    ) -> Result<Blake2bWord<F>, Error> {
        layouter.assign_region(
            || "fused pack-add-decompose",
            |mut region| {
                self.config.s_pack_add_decompose.enable(&mut region, 0)?;
                self.config.q_range_check_8.enable(&mut region, 1)?;

                // Row 0: input bytes + other_word + carry
                for (i, byte) in input_bytes.iter().enumerate() {
                    byte.copy_advice(
                        || format!("in_byte_{}", i),
                        &mut region,
                        self.config.advices[i],
                        0,
                    )?;
                }
                other_word.copy_advice(
                    || "other_word",
                    &mut region,
                    self.config.advices[A8],
                    0,
                )?;

                // Compute: input_packed + other_word
                let input_packed = word_value_from_bytes(input_bytes);
                let sum_val = input_packed
                    .zip(other_word.value().copied())
                    .map(|(inp, ow)| {
                        let s = inp + ow;
                        let repr = s.to_repr();
                        let bytes_raw = repr.as_ref();
                        let carry = bytes_raw[8] as u64;
                        let mut result_bytes = [0u8; 8];
                        result_bytes.copy_from_slice(&bytes_raw[..8]);
                        let result_word = s - F::from(carry) * F::from_u128(1u128 << 64);
                        (result_bytes, result_word, carry)
                    });

                // Assign carry on row 0
                region.assign_advice(
                    || "carry",
                    self.config.advices[A9],
                    0,
                    || sum_val.map(|(_, _, c)| F::from(c)),
                )?;

                // Row 1: result bytes + result_word
                let mut result_bytes_cells = Vec::with_capacity(8);
                for i in 0..8 {
                    let byte = region.assign_advice(
                        || format!("res_byte_{}", i),
                        self.config.advices[i],
                        1,
                        || sum_val.map(|(b, _, _)| F::from(b[i] as u64)),
                    )?;
                    result_bytes_cells.push(byte);
                }
                let result_word = region.assign_advice(
                    || "result_word",
                    self.config.advices[A8],
                    1,
                    || sum_val.map(|(_, w, _)| w),
                )?;

                let bytes: [AssignedCell<F, F>; 8] =
                    result_bytes_cells.try_into().unwrap();
                Ok(Blake2bWord {
                    word: result_word,
                    bytes,
                })
            },
        )
    }

    /// Fused pack-and-add: packs input bytes into a word and adds other_word.
    /// Returns only the word result (no byte decomposition).
    ///
    /// Row 0: A0-A7 = input bytes, A8 = other_word
    /// Row 1: A0 = result, A1 = carry
    fn pack_add(
        &self,
        mut layouter: impl Layouter<F>,
        input_bytes: &[AssignedCell<F, F>; 8],
        other_word: &AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, Error> {
        layouter.assign_region(
            || "fused pack-and-add",
            |mut region| {
                self.config.s_pack_add.enable(&mut region, 0)?;

                // Row 0: input bytes + other_word
                for (i, byte) in input_bytes.iter().enumerate() {
                    byte.copy_advice(
                        || format!("in_byte_{}", i),
                        &mut region,
                        self.config.advices[i],
                        0,
                    )?;
                }
                other_word.copy_advice(
                    || "other_word",
                    &mut region,
                    self.config.advices[A8],
                    0,
                )?;

                // Compute: input_packed + other_word
                let input_packed = word_value_from_bytes(input_bytes);
                let sum_val = input_packed
                    .zip(other_word.value().copied())
                    .map(|(inp, ow)| {
                        let s = inp + ow;
                        let carry = F::from(s.to_repr().as_ref()[8] as u64);
                        let ret = s - carry * F::from_u128(1u128 << 64);
                        (ret, carry)
                    });

                // Row 1: result + carry
                let result = region.assign_advice(
                    || "result",
                    self.config.advices[A0],
                    1,
                    || sum_val.map(|(r, _)| r),
                )?;
                region.assign_advice(
                    || "carry",
                    self.config.advices[A1],
                    1,
                    || sum_val.map(|(_, c)| c),
                )?;

                Ok(result)
            },
        )
    }

    /// Decompose a 32-bit word into 4 bytes using the 8-byte word gate with upper bytes zeroed.
    fn word_decompose_32(
        &self,
        mut layouter: impl Layouter<F>,
        bytes: &[AssignedCell<F, F>],
        word: &AssignedCell<F, F>,
    ) -> Result<(), Error> {
        assert_eq!(bytes.len(), 4);
        layouter.assign_region(
            || "decompose 32-bit word to bytes",
            |mut region| {
                self.config.s_word_decompose.enable(&mut region, 0)?;
                for (i, byte) in bytes.iter().enumerate() {
                    byte.copy_advice(|| "byte", &mut region, self.config.advices[i], 0)?;
                }
                for i in 4..8 {
                    region.assign_advice_from_constant(
                        || format!("zero byte {}", i),
                        self.config.advices[i],
                        0,
                        F::ZERO,
                    )?;
                }
                word.copy_advice(|| "word", &mut region, self.config.advices[A0], 1)?;
                Ok(())
            },
        )
    }

    /// Assign a 32-bit word from 4 byte cells, constraining it via `word_decompose_32`.
    fn assign_word_32_from_bytes(
        &self,
        mut layouter: impl Layouter<F>,
        bytes: &[AssignedCell<F, F>],
    ) -> Result<AssignedCell<F, F>, Error> {
        let word_value = word_value_from_bytes(bytes);
        let word = assign_free_advice(
            layouter.namespace(|| "assign 32-bit word"),
            self.config.advices[A8],
            word_value,
        )?;
        self.word_decompose_32(layouter.namespace(|| "word decompose 32"), bytes, &word)?;
        Ok(word)
    }

    /// Combine two 32-bit words into one 64-bit word.
    fn word_combine(
        &self,
        mut layouter: impl Layouter<F>,
        word_32_lo: &AssignedCell<F, F>,
        word_32_hi: &AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, Error> {
        layouter.assign_region(
            || "combine two 32-bit words to 64-bit",
            |mut region| {
                self.config.s_word_combine.enable(&mut region, 0)?;
                word_32_lo.copy_advice(|| "word_32_lo", &mut region, self.config.advices[A0], 0)?;
                word_32_hi.copy_advice(|| "word_32_hi", &mut region, self.config.advices[A1], 0)?;
                let word_64_value = word_32_lo
                    .value()
                    .zip(word_32_hi.value())
                    .map(|(&lo, &hi)| lo + hi * F::from(1u64 << 32));
                region.assign_advice(|| "word_64", self.config.advices[A0], 1, || word_64_value)
            },
        )
    }

    /// Range-check 8 bytes via q_range_check_8 lookup.
    /// Copies the bytes to A0..A7 on a fresh row and enables q_range_check_8.
    fn range_check_bytes(
        &self,
        mut layouter: impl Layouter<F>,
        bytes: &[AssignedCell<F, F>],
    ) -> Result<(), Error> {
        assert!(bytes.len() <= 8);
        layouter.assign_region(
            || "range check bytes",
            |mut region| {
                self.config.q_range_check_8.enable(&mut region, 0)?;
                for (i, byte) in bytes.iter().enumerate() {
                    byte.copy_advice(|| "byte", &mut region, self.config.advices[i], 0)?;
                }
                // Pad remaining columns with zero (which is in [0, 255])
                for i in bytes.len()..8 {
                    region.assign_advice_from_constant(
                        || "zero pad",
                        self.config.advices[i],
                        0,
                        F::ZERO,
                    )?;
                }
                Ok(())
            },
        )
    }

    /// Read a u64 word from instance and decompose into bytes with range check.
    ///
    /// The `word_cell` should already be constrained to an instance cell via
    /// `layouter.constrain_instance()`. Decomposes into 8 bytes using
    /// `s_word_decompose` and range-checks via `q_range_check_8`. Uses 2 rows.
    pub fn word_from_instance(
        &self,
        layouter: impl Layouter<F>,
        word_cell: &AssignedCell<F, F>,
    ) -> Result<Blake2bWord<F>, Error> {
        Blake2bWord::from_word(self, layouter, word_cell.clone())
    }

    // ---- Field element to BLAKE2b words ----

    /// Convert a field element into 4 x 64-bit `Blake2bWord`s for BLAKE2b input.
    ///
    /// Witnesses 32 bytes from the field element, range-checks them, packs into
    /// 64-bit words, and verifies that the bytes reconstruct the original Fp via
    /// `s_result_encode` (word-pair sums) and `s_field_recompose` (full field).
    fn field_to_words(
        &self,
        layouter: &mut impl Layouter<F>,
        field_elem: &AssignedCell<F, F>,
    ) -> Result<Vec<Blake2bWord<F>>, Error> {
        // Decompose field element into 32 individual bytes
        let mut bytes = Vec::with_capacity(32);
        for i in 0..32 {
            let byte_value = field_elem.value().map(|f| {
                F::from(f.to_repr().as_ref()[i] as u64)
            });
            let byte = assign_free_advice(
                layouter.namespace(|| format!("byte_{}", i)),
                self.config.advices[A0],
                byte_value,
            )?;
            bytes.push(byte);
        }

        // Range-check all 32 bytes (4 batches of 8)
        for batch in 0..4 {
            self.range_check_bytes(
                layouter.namespace(|| format!("range_{}", batch)),
                &bytes[batch * 8..(batch + 1) * 8],
            )?;
        }

        // Pack bytes into 8 x 32-bit words
        let mut words_32 = Vec::with_capacity(8);
        for (j, chunk) in bytes.chunks(4).enumerate() {
            let word = self.assign_word_32_from_bytes(
                layouter.namespace(|| format!("w32_{}", j)),
                chunk,
            )?;
            words_32.push(word);
        }

        // Combine pairs of 32-bit words into 4 x 64-bit words
        let mut words_64 = Vec::with_capacity(4);
        for j in 0..4 {
            let word_64 = self.word_combine(
                layouter.namespace(|| format!("w64_{}", j)),
                &words_32[j * 2],
                &words_32[j * 2 + 1],
            )?;
            words_64.push(word_64);
        }

        // Recomposition check: sum_01 = w0 + w1 * 2^64
        let sum_01_val = words_64[0].value().zip(words_64[1].value())
            .map(|(&w0, &w1)| w0 + w1 * F::from_u128(1u128 << 64));
        let sum_01 = assign_free_advice(
            layouter.namespace(|| "sum_01"),
            self.config.advices[A0],
            sum_01_val,
        )?;
        layouter.assign_region(
            || "sum_01_recompose",
            |mut region| {
                self.config.s_result_encode.enable(&mut region, 0)?;
                words_64[0].copy_advice(|| "w0", &mut region, self.config.advices[A0], 0)?;
                words_64[1].copy_advice(|| "w1", &mut region, self.config.advices[A1], 0)?;
                sum_01.copy_advice(|| "sum_01", &mut region, self.config.advices[A0], 1)?;
                Ok(())
            },
        )?;

        // Recomposition check: sum_23 = w2 + w3 * 2^64
        let sum_23_val = words_64[2].value().zip(words_64[3].value())
            .map(|(&w0, &w1)| w0 + w1 * F::from_u128(1u128 << 64));
        let sum_23 = assign_free_advice(
            layouter.namespace(|| "sum_23"),
            self.config.advices[A0],
            sum_23_val,
        )?;
        layouter.assign_region(
            || "sum_23_recompose",
            |mut region| {
                self.config.s_result_encode.enable(&mut region, 0)?;
                words_64[2].copy_advice(|| "w0", &mut region, self.config.advices[A0], 0)?;
                words_64[3].copy_advice(|| "w1", &mut region, self.config.advices[A1], 0)?;
                sum_23.copy_advice(|| "sum_23", &mut region, self.config.advices[A0], 1)?;
                Ok(())
            },
        )?;

        // Recomposition check: field_elem = sum_01 + sum_23 * 2^128
        layouter.assign_region(
            || "field_recompose",
            |mut region| {
                self.config.s_field_recompose.enable(&mut region, 0)?;
                sum_01.copy_advice(|| "sum_01", &mut region, self.config.advices[A0], 0)?;
                sum_23.copy_advice(|| "sum_23", &mut region, self.config.advices[A1], 0)?;
                field_elem.copy_advice(|| "field", &mut region, self.config.advices[A0], 1)?;
                Ok(())
            },
        )?;

        // Build result Blake2bWords
        let result: Vec<Blake2bWord<F>> = (0..4).map(|j| {
            let word_bytes: [AssignedCell<F, F>; 8] = bytes[j * 8..(j + 1) * 8]
                .to_vec()
                .try_into()
                .unwrap();
            Blake2bWord {
                word: words_64[j].clone(),
                bytes: word_bytes,
            }
        }).collect();

        Ok(result)
    }

    // ---- Public API (hashing) ----

    /// Hash with precomputed block 1 state.
    ///
    /// `h_1 = compress(h_init_personalized, block1, 128, false)` — computed outside
    /// the circuit by the verifier. The circuit starts from h_1 and processes only
    /// blocks 2 and 3, eliminating one full compression call (4,800 rows).
    ///
    /// Instance layout: `[h_1[0..8], hash_output[0..2]]` (10 values total).
    ///
    /// Soundness: the verifier independently computes h_1 from public action data
    /// and passes it as public input. If the prover uses wrong h_1, the proof
    /// won't verify against the verifier's expected instance values.
    pub fn process_precomputed_action_hash(
        &self,
        layouter: &mut impl Layouter<F>,
        h_1: &[Blake2bWord<F>; 8],
        enc_1_tail: &[AssignedCell<F, F>; 20],
        action_2: &CompactActionCells<F>,
    ) -> Result<Vec<Blake2bWord<F>>, Error> {
        // Load lookup tables
        Blake2bChip::load_tables(&self.config, layouter)?;

        // Initialize state from precomputed h_1
        let mut h: Vec<Blake2bWord<F>> = h_1.to_vec();

        // Gather remaining bytes (168 = 20 + 148)
        let mut remaining_bytes: Vec<AssignedCell<F, F>> = Vec::with_capacity(168);

        // enc_1_tail: 20 bytes (enc_1[32..52]), range-check in 3 batches
        self.range_check_bytes(
            layouter.namespace(|| "enc1t_range_0"),
            &enc_1_tail[0..8],
        )?;
        self.range_check_bytes(
            layouter.namespace(|| "enc1t_range_1"),
            &enc_1_tail[8..16],
        )?;
        self.range_check_bytes(
            layouter.namespace(|| "enc1t_range_2"),
            &enc_1_tail[16..20],
        )?;
        remaining_bytes.extend_from_slice(enc_1_tail);

        // nf_2: field -> 4 words -> 32 bytes
        let nf_2_words = self.field_to_words(
            &mut layouter.namespace(|| "nf2_to_words"),
            &action_2.nf,
        )?;
        for w in &nf_2_words {
            remaining_bytes.extend_from_slice(w.get_bytes());
        }

        // cmx_2: field -> 4 words -> 32 bytes
        let cmx_2_words = self.field_to_words(
            &mut layouter.namespace(|| "cmx2_to_words"),
            &action_2.cmx,
        )?;
        for w in &cmx_2_words {
            remaining_bytes.extend_from_slice(w.get_bytes());
        }

        // epk_2: range-check 32 bytes (4 batches of 8)
        for chunk_idx in 0..4 {
            self.range_check_bytes(
                layouter.namespace(|| format!("epk2_range_{}", chunk_idx)),
                &action_2.epk_bytes[chunk_idx * 8..(chunk_idx + 1) * 8],
            )?;
        }
        remaining_bytes.extend_from_slice(&action_2.epk_bytes);

        // enc_2: range-check 52 bytes (6 batches of 8 + 1 batch of 4)
        for chunk_idx in 0..6 {
            self.range_check_bytes(
                layouter.namespace(|| format!("enc2_range_{}", chunk_idx)),
                &action_2.enc_prefix[chunk_idx * 8..(chunk_idx + 1) * 8],
            )?;
        }
        self.range_check_bytes(
            layouter.namespace(|| "enc2_range_last"),
            &action_2.enc_prefix[48..52],
        )?;
        remaining_bytes.extend_from_slice(&action_2.enc_prefix);

        assert_eq!(remaining_bytes.len(), 168);

        // Pack 168 bytes into 21 words
        let mut remaining_words: Vec<Blake2bWord<F>> = Vec::with_capacity(21);
        for (i, chunk) in remaining_bytes.chunks(8).enumerate() {
            let word_bytes: [AssignedCell<F, F>; 8] = chunk.to_vec().try_into().unwrap();
            let word = Blake2bWord::from_bytes_unchecked(
                self,
                layouter.namespace(|| format!("pack_rem_{}", i)),
                word_bytes,
            )?;
            remaining_words.push(word);
        }

        // Block 2: words 0..16 (128 bytes)
        let block_2: Vec<Blake2bWord<F>> = remaining_words[..16].to_vec();

        // Block 3: words 16..21 (40 bytes) + 11 zero-pad words (88 bytes)
        let mut block_3: Vec<Blake2bWord<F>> = remaining_words[16..].to_vec();
        while block_3.len() < 16 {
            block_3.push(Blake2bWord::from_constant_u64_unchecked(
                0, layouter, &self.config,
            )?);
        }

        // Compress block 2: t=256 (bytes 128-255), not last
        self.compress(layouter, &mut h, &block_2, 256u128, false)?;

        // Compress block 3: t=296 (total input bytes), last
        self.compress(layouter, &mut h, &block_3, 296u128, true)?;

        Ok(h[..4].to_vec())
    }

    /// Pack 4 x 8-byte result words into 2 field elements (16 bytes each).
    pub fn encode_result(
        &self,
        layouter: &mut impl Layouter<F>,
        ret: &[Blake2bWord<F>],
    ) -> Result<[AssignedCell<F, F>; 2], Error> {
        let mut fields = vec![];
        assert_eq!(ret.len(), 4);
        for words in ret.chunks(2) {
            let field = layouter.assign_region(
                || "encode two words to one field",
                |mut region| {
                    self.config.s_result_encode.enable(&mut region, 0)?;
                    for (i, word) in words.iter().enumerate() {
                        word.get_word().copy_advice(
                            || "word",
                            &mut region,
                            self.config.advices[i],
                            0,
                        )?;
                    }
                    let field_value = field_value_from_words_64(words);
                    region.assign_advice(
                        || "field",
                        self.config.advices[A0],
                        1,
                        || field_value,
                    )
                },
            )?;
            fields.push(field);
        }
        Ok(fields.try_into().unwrap())
    }

    // ---- Compression function ----

    /// BLAKE2b compression function F.
    fn compress(
        &self,
        layouter: &mut impl Layouter<F>,
        h: &mut [Blake2bWord<F>],
        m: &[Blake2bWord<F>],
        t: u128,
        f: bool,
    ) -> Result<(), Error> {
        let mut v = Vec::with_capacity(16);
        v.extend_from_slice(h);
        for iv in IV[0..4].iter() {
            let word = Blake2bWord::from_constant_u64_unchecked(*iv, layouter, &self.config)?;
            v.push(word);
        }
        let v_12 = Blake2bWord::from_constant_u64_unchecked(IV[4] ^ (t as u64), layouter, &self.config)?;
        v.push(v_12);
        let v_13 = Blake2bWord::from_constant_u64_unchecked(IV[5] ^ ((t >> 64) as u64), layouter, &self.config)?;
        v.push(v_13);
        let v_14 = if f {
            Blake2bWord::from_constant_u64_unchecked(IV[6] ^ u64::MAX, layouter, &self.config)?
        } else {
            Blake2bWord::from_constant_u64_unchecked(IV[6], layouter, &self.config)?
        };
        v.push(v_14);
        let v_15 = Blake2bWord::from_constant_u64_unchecked(IV[7], layouter, &self.config)?;
        v.push(v_15);
        assert_eq!(v.len(), 16);

        for i in 0..ROUNDS {
            let s = SIGMA[i % 10];
            self.g(
                layouter.namespace(|| format!("round_{}/mix_1", i)),
                &mut v,
                (0, 4, 8, 12),
                &m[s[0]],
                &m[s[1]],
            )?;
            self.g(
                layouter.namespace(|| format!("round_{}/mix_2", i)),
                &mut v,
                (1, 5, 9, 13),
                &m[s[2]],
                &m[s[3]],
            )?;
            self.g(
                layouter.namespace(|| format!("round_{}/mix_3", i)),
                &mut v,
                (2, 6, 10, 14),
                &m[s[4]],
                &m[s[5]],
            )?;
            self.g(
                layouter.namespace(|| format!("round_{}/mix_4", i)),
                &mut v,
                (3, 7, 11, 15),
                &m[s[6]],
                &m[s[7]],
            )?;

            self.g(
                layouter.namespace(|| format!("round_{}/mix_5", i)),
                &mut v,
                (0, 5, 10, 15),
                &m[s[8]],
                &m[s[9]],
            )?;
            self.g(
                layouter.namespace(|| format!("round_{}/mix_6", i)),
                &mut v,
                (1, 6, 11, 12),
                &m[s[10]],
                &m[s[11]],
            )?;
            self.g(
                layouter.namespace(|| format!("round_{}/mix_7", i)),
                &mut v,
                (2, 7, 8, 13),
                &m[s[12]],
                &m[s[13]],
            )?;
            self.g(
                layouter.namespace(|| format!("round_{}/mix_8", i)),
                &mut v,
                (3, 4, 9, 14),
                &m[s[14]],
                &m[s[15]],
            )?;
        }

        // Finalize: h[i] = h[i] ^ v[i] ^ v[i+8]
        for i in 0..8 {
            let xor_1 = self.word_xor(
                layouter.namespace(|| format!("final_xor_1_{}", i)),
                h[i].get_bytes(),
                v[i].get_bytes(),
            )?;
            let xor_2 = self.word_xor(
                layouter.namespace(|| format!("final_xor_2_{}", i)),
                &xor_1,
                v[i + 8].get_bytes(),
            )?;
            h[i] = Blake2bWord::from_bytes_unchecked(
                self,
                layouter.namespace(|| format!("final_word_{}", i)),
                xor_2,
            )?;
        }

        Ok(())
    }

    /// The G primitive function mixes two input words, "x" and "y", into
    /// four words indexed by "a", "b", "c", and "d" in the working vector
    /// v[0..15].
    ///
    /// Uses fused gates and dual byte XOR (34 rows per G):
    ///   1. single_row_add(v[a], v[b])             — 1 row
    ///   2. fused_add_decompose(sum, x)             — 2 rows
    ///   3. word_xor(v[d], v[a]) + rotate R1        — 4 rows
    ///   4. pack_add_decompose(d_bytes, v[c])       — 2 rows
    ///   5. word_xor(v[b], v[c]) + rotate R2        — 4 rows
    ///   6. pack_add(b_bytes, v[a])                  — 2 rows
    ///   7. fused_add_decompose(sum, y)             — 2 rows
    ///   8. word_xor(d_bytes, v[a]) + rotate R3     — 4 rows
    ///   9. pack_add_decompose(d_bytes2, v[c])      — 2 rows
    ///  10. word_xor(b_bytes, v[c])                  — 4 rows
    ///  11. left_rotate_1 (R4=63)                   — 3 rows
    ///  12. from_bytes_unchecked (v[b])              — 2 rows
    ///  13. from_bytes_unchecked (v[d])              — 2 rows
    ///                                        Total: 34 rows
    fn g(
        &self,
        mut layouter: impl Layouter<F>,
        v: &mut [Blake2bWord<F>],
        (a, b, c, d): (usize, usize, usize, usize),
        x: &Blake2bWord<F>,
        y: &Blake2bWord<F>,
    ) -> Result<(), Error> {
        // Step 1+2: v[a] := (v[a] + v[b] + x) mod 2**w
        v[a] = {
            let sum_a_b = self.single_row_add(
                layouter.namespace(|| "g/add_ab"),
                v[a].get_word(),
                v[b].get_word(),
            )?;
            self.fused_add_decompose(
                layouter.namespace(|| "g/fad_x"),
                &sum_a_b,
                x.get_word(),
            )?
        };

        // Step 3+4: v[d] := (v[d] ^ v[a]) >>> R1, v[c] := (v[c] + v[d]) mod 2**w
        // Keep d_bytes as local — v[d] word not needed until exit.
        let xor_da = self.word_xor(
            layouter.namespace(|| "g/xor_da"),
            v[d].get_bytes(),
            v[a].get_bytes(),
        )?;
        let d_bytes = Blake2bWord::byte_rotate(&xor_da, R1);
        v[c] = self.pack_add_decompose(
            layouter.namespace(|| "g/pad_cd"),
            &d_bytes,
            v[c].get_word(),
        )?;

        // Step 5+6: v[b] := (v[b] ^ v[c]) >>> R2, intermediate sum for v[a]
        // Keep b_bytes as local — v[b] word not needed until exit.
        let xor_bc = self.word_xor(
            layouter.namespace(|| "g/xor_bc"),
            v[b].get_bytes(),
            v[c].get_bytes(),
        )?;
        let b_bytes = Blake2bWord::byte_rotate(&xor_bc, R2);
        let sum_ab2 = self.pack_add(
            layouter.namespace(|| "g/pa_ab2"),
            &b_bytes,
            v[a].get_word(),
        )?;

        // Step 7: v[a] := (v[a] + v[b] + y) mod 2**w
        v[a] = self.fused_add_decompose(
            layouter.namespace(|| "g/fad_y"),
            &sum_ab2,
            y.get_word(),
        )?;

        // Step 8+9: v[d] := (v[d] ^ v[a]) >>> R3, v[c] := (v[c] + v[d]) mod 2**w
        let xor_da2 = self.word_xor(
            layouter.namespace(|| "g/xor_da_2"),
            &d_bytes,
            v[a].get_bytes(),
        )?;
        let d_bytes2 = Blake2bWord::byte_rotate(&xor_da2, R3);
        v[c] = self.pack_add_decompose(
            layouter.namespace(|| "g/pad_cd2"),
            &d_bytes2,
            v[c].get_word(),
        )?;

        // Step 10+11+12: v[b] := (v[b] ^ v[c]) >>> R4 (63 = left-rotate 1)
        v[b] = {
            let xor_bytes = self.word_xor(
                layouter.namespace(|| "g/xor_bc_2"),
                &b_bytes,
                v[c].get_bytes(),
            )?;
            let shifted = self.left_rotate_1(
                layouter.namespace(|| "g/rot_r4"),
                &xor_bytes,
            )?;
            Blake2bWord::from_bytes_unchecked(
                self,
                layouter.namespace(|| "g/word_from_shift"),
                shifted,
            )?
        };

        // Step 13: Pack v[d] final bytes (needed for next round's word operations)
        v[d] = Blake2bWord::from_bytes_unchecked(
            self,
            layouter.namespace(|| "g/pack_d"),
            d_bytes2,
        )?;

        Ok(())
    }
}

// ---- Reference (non-circuit) BLAKE2b helpers ----

fn g_ref(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(32);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(24);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(63);
}

fn compress_ref(h: &mut [u64; 8], m: &[u64; 16], t: u128, f: bool) {
    let mut v = [0u64; 16];
    v[..8].copy_from_slice(h);
    v[8..12].copy_from_slice(&IV[0..4]);
    v[12] = IV[4] ^ (t as u64);
    v[13] = IV[5] ^ ((t >> 64) as u64);
    v[14] = if f { IV[6] ^ u64::MAX } else { IV[6] };
    v[15] = IV[7];

    for i in 0..12 {
        let s = &SIGMA[i % 10];
        g_ref(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
        g_ref(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
        g_ref(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
        g_ref(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
        g_ref(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
        g_ref(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
        g_ref(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
        g_ref(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
    }

    for i in 0..8 {
        h[i] = h[i] ^ v[i] ^ v[i + 8];
    }
}

/// Compute the intermediate BLAKE2b state after compressing block 1.
///
/// Returns `h_1 = compress(h_init, block1, 128, false)` where block 1 is the
/// first 128 bytes of action 1: `nf(32) + cmx(32) + epk(32) + enc[0..32]`.
///
/// The verifier calls this to produce the `h_1` public input for the
/// precomputed circuit. This is a plain (non-circuit) BLAKE2b compress.
pub fn compute_h1(
    nf: &[u8; 32],
    cmx: &[u8; 32],
    epk: &[u8; 32],
    enc: &[u8; 52],
    personalization: &[u8; 16],
) -> [u64; 8] {
    let p0 = u64::from_le_bytes(personalization[0..8].try_into().unwrap());
    let p1 = u64::from_le_bytes(personalization[8..16].try_into().unwrap());
    let mut h = [
        IV[0] ^ 0x01010000 ^ 32,
        IV[1],
        IV[2],
        IV[3],
        IV[4],
        IV[5],
        IV[6] ^ p0,
        IV[7] ^ p1,
    ];

    let mut block1 = [0u8; 128];
    block1[0..32].copy_from_slice(nf);
    block1[32..64].copy_from_slice(cmx);
    block1[64..96].copy_from_slice(epk);
    block1[96..128].copy_from_slice(&enc[0..32]);

    let mut m = [0u64; 16];
    for (i, word_bytes) in block1.chunks(8).enumerate() {
        m[i] = u64::from_le_bytes(word_bytes.try_into().unwrap());
    }

    compress_ref(&mut h, &m, 128, false);
    h
}
