//! BLAKE2b-256 circuit implementation for Halo2.
//!
//! Provides a constrained BLAKE2b-256 hash function for use in Orchard compact
//! action hashes (ZIP-244).
//!
//! Field elements are first decomposed into 8 × 32-bit words. The 32-bit size
//! is chosen because: (a) all 8 words fit in one gate row (one per advice column),
//! and (b) the lower 4 words (128 bits) align directly with the Pallas modulus
//! structure needed for the canonicality check (proving the decomposition is < p,
//! so a malicious prover cannot use the non-canonical representation value + p).
//! These 32-bit words are then paired into 4 × 64-bit words — BLAKE2b's native
//! word size — via a `word_combine` gate, and processed through standard BLAKE2b
//! compression using bit-level gates.
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
use byteorder::{ByteOrder, LittleEndian};
use core::convert::TryInto;
use core::marker::PhantomData;
use group::ff::PrimeField;
use halo2_gadgets::utilities::bool_check;
use halo2_proofs::{
    circuit::{AssignedCell, Layouter, Value},
    plonk::{Advice, Column, ConstraintSystem, Constraints, Error, Selector, VirtualCells},
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

// ----------------
// Value helpers
// ----------------

/// Extract the least-significant byte from a field element's little-endian representation.
fn f_to_u8_le<F: PrimeField>(f: &F) -> u8 {
    let repr = f.to_repr();
    repr.as_ref()[0]
}

/// Extract the least-significant 32 bits from a field element's little-endian representation.
fn f_to_u32_le<F: PrimeField>(f: &F) -> u32 {
    let repr = f.to_repr();
    let bytes = repr.as_ref();
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Reconstruct a byte `Value` from assigned bit cells (MSB-first fold).
fn byte_value_from_bits<F: PrimeField>(bits: &[AssignedCell<F, F>]) -> Value<F> {
    let bit_values: Value<Vec<_>> = bits.iter().map(|bit| bit.value()).collect();
    bit_values.map(|bits| {
        bits.into_iter()
            .rev()
            .fold(F::ZERO, |acc, bit| acc * F::from(2) + bit)
    })
}

/// Reconstruct a word `Value` from assigned byte cells (MSB-first fold).
fn word_value_from_bytes<F: PrimeField>(bytes: &[AssignedCell<F, F>]) -> Value<F> {
    let byte_values: Value<Vec<_>> = bytes.iter().map(|byte| byte.value()).collect();
    byte_values.map(|bytes| {
        bytes
            .into_iter()
            .rev()
            .fold(F::ZERO, |acc, byte| acc * F::from(1 << 8) + byte)
    })
}

/// Reconstruct a field `Value` from 64-bit `Blake2bWord`s (MSB-first fold).
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

/// Witnesses the given value in a standalone region.
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

/// Assigns a constant value in a standalone region.
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

// Lower 128 bits of pallas base field modulus as a u128 for canonicality comparison.
// p_lower = 0x224698fc_094cf91b_992d30ed_00000001
const PALLAS_MODULUS_LOWER_128: u128 = 0x224698fc_094cf91b_992d30ed_00000001;

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

// G mixing function rotation constants (RFC 7693, Section 2.1): (32, 24, 16, 63).
const R1: usize = 32;
const R2: usize = 24;
const R3: usize = 16;
const R4: usize = 63;

// Number of rounds in the compression function.
const ROUNDS: usize = 12;

// ---------------

/// BLAKE2b chip for halo2.
#[derive(Clone, Debug)]
pub struct Blake2bChip<F: PrimeField> {
    config: Blake2bConfig<F>,
    _marker: PhantomData<F>,
}

/// Configuration for the BLAKE2b chip.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Blake2bConfig<F: PrimeField> {
    /// Advice columns used by the chip.
    pub advices: [Column<Advice>; 10],
    /// Selector for field decomposition gate.
    pub s_field_decompose: Selector,
    /// Selector for word decomposition gate.
    pub s_word_decompose: Selector,
    /// Selector for byte decomposition gate.
    pub s_byte_decompose: Selector,
    /// Selector for byte XOR gate.
    pub s_byte_xor: Selector,
    /// Selector for word addition gate.
    pub s_word_add: Selector,
    /// Selector for result encoding gate.
    pub s_result_encode: Selector,
    /// Selector for canonicality check gate (ensures field decomposition < p).
    pub s_canonicality: Selector,
    /// Selector for high bit zero check (bit_254 * bit = 0 for bits 128-253).
    pub s_high_bit_zero: Selector,
    /// Selector for combining two 32-bit words into one 64-bit word.
    pub s_word_combine: Selector,
    _marker: PhantomData<F>,
}

/// A 64-bit word represented as both a packed field element and its 64 individual bit cells.
#[derive(Clone, Debug)]
pub struct Blake2bWord<F: PrimeField> {
    word: AssignedCell<F, F>,
    bits: [AssignedCell<F, F>; 64],
}

/// One byte has 8 bits.
#[derive(Clone, Debug)]
struct Blake2bByte<F: PrimeField> {
    byte: AssignedCell<F, F>,
    bits: [AssignedCell<F, F>; 8],
}

impl<F: PrimeField> Blake2bByte<F> {
    pub fn get_byte(&self) -> AssignedCell<F, F> {
        self.byte.clone()
    }

    pub fn get_bits(&self) -> &[AssignedCell<F, F>; 8] {
        &self.bits
    }

    /// Decompose a private witness byte into 8 boolean-constrained bits.
    ///
    /// The prover supplies the byte value; the `s_byte_decompose` gate enforces
    /// that the bits are binary and reconstruct the byte.
    pub fn from_u8(
        value: Value<u8>,
        mut layouter: impl Layouter<F>,
        config: &Blake2bConfig<F>,
    ) -> Result<Self, Error> {
        layouter.assign_region(
            || "decompose bytes to bits",
            |mut region| {
                config.s_byte_decompose.enable(&mut region, 0)?;
                let mut byte = value;
                let mut bits = Vec::with_capacity(8);
                for i in 0..8 {
                    let bit = byte.map(|b| F::from((b & 1) as u64));
                    let bit_var = region.assign_advice(|| "bit", config.advices[i], 0, || bit)?;
                    bits.push(bit_var);
                    byte = byte.map(|b| b >> 1);
                }
                let byte = region.assign_advice(
                    || "byte",
                    config.advices[A0],
                    1,
                    || value.map(|v| F::from(v as u64)),
                )?;
                Ok(Self {
                    byte,
                    bits: bits.try_into().unwrap(),
                })
            },
        )
    }

    /// Decompose a public constant byte into 8 boolean-constrained bits.
    ///
    /// Unlike `from_u8`, each cell is pinned to the fixed column via
    /// `assign_advice_from_constant`, so the verifier enforces exact values.
    /// Used for spec-defined constants like IV words and zero padding.
    pub fn from_constant_u8(
        value: u8,
        layouter: &mut impl Layouter<F>,
        config: &Blake2bConfig<F>,
    ) -> Result<Self, Error> {
        layouter.assign_region(
            || "decompose bytes to bits",
            |mut region| {
                config.s_byte_decompose.enable(&mut region, 0)?;
                let mut byte = value;
                let mut bits = Vec::with_capacity(8);
                for i in 0..8 {
                    let bit = byte & 1;
                    let bit_var = region.assign_advice_from_constant(
                        || "bit",
                        config.advices[i],
                        0,
                        F::from(bit as u64),
                    )?;
                    bits.push(bit_var);
                    byte >>= 1;
                }
                let byte = region.assign_advice_from_constant(
                    || "byte",
                    config.advices[A0],
                    1,
                    F::from(value as u64),
                )?;
                Ok(Self {
                    byte,
                    bits: bits.try_into().unwrap(),
                })
            },
        )
    }
}

impl<F: PrimeField> Blake2bConfig<F> {
    /// Configure the BLAKE2b chip.
    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        advices: [Column<Advice>; 10],
    ) -> Blake2bConfig<F> {
        let s_field_decompose = meta.selector();
        let s_word_decompose = meta.selector();
        let s_byte_decompose = meta.selector();
        let s_byte_xor = meta.selector();
        let s_word_add = meta.selector();
        let s_result_encode = meta.selector();
        let s_canonicality = meta.selector();
        let s_high_bit_zero = meta.selector();
        let s_word_combine = meta.selector();

        // Gate layouts (rows = rotations):
        // - field_decompose: row 0 = word_1..word_8 (A0..A7), row 1 = field (A0)
        // - word_decompose: row 0 = byte_1..byte_8 (A0..A7), row 1 = word (A0)
        // - byte_decompose: row 0 = bit_1..bit_8 (A0..A7), row 1 = byte (A0)
        // - byte_xor: row 0 = lhs bits, row 1 = rhs bits, row 2 = out bits (A0..A7)
        // - word_add: row 0 = lhs (A0), rhs (A1), row 1 = out (A0), carry (A1)
        // - result_encode: row 0 = word_1 (A0), word_2 (A1), row 1 = field (A0)
        // - canonicality: see detailed layout below (2 rows)
        // - high_bit_zero: row 0 = bit_254 (A0), bit_to_check (A1)
        // - word_combine: row 0 = word_32_lo (A0), word_32_hi (A1), row 1 = word_64 (A0)

        meta.create_gate("decompose field to words", |meta| {
            let field_element = meta.query_advice(advices[A0], Rotation::next());
            let word_1 = meta.query_advice(advices[A0], Rotation::cur());
            let word_2 = meta.query_advice(advices[A1], Rotation::cur());
            let word_3 = meta.query_advice(advices[A2], Rotation::cur());
            let word_4 = meta.query_advice(advices[A3], Rotation::cur());
            let word_5 = meta.query_advice(advices[A4], Rotation::cur());
            let word_6 = meta.query_advice(advices[A5], Rotation::cur());
            let word_7 = meta.query_advice(advices[A6], Rotation::cur());
            let word_8 = meta.query_advice(advices[A7], Rotation::cur());
            let s_field_decompose = meta.query_selector(s_field_decompose);

            vec![
                s_field_decompose
                    * (word_1
                        + word_2 * F::from(1 << 32)
                        + word_3 * F::from_u128(1 << 64)
                        + word_4 * F::from_u128(1 << 96)
                        + word_5 * F::from_u128(1 << 64).square()
                        + word_6 * F::from_u128(1 << 80).square()
                        + word_7 * F::from_u128(1 << 96).square()
                        + word_8 * F::from_u128(1 << 112).square()
                        - field_element),
            ]
        });

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

        meta.create_gate("decompose byte to bits", |meta| {
            let byte = meta.query_advice(advices[A0], Rotation::next());
            let bit_1 = meta.query_advice(advices[A0], Rotation::cur());
            let bit_2 = meta.query_advice(advices[A1], Rotation::cur());
            let bit_3 = meta.query_advice(advices[A2], Rotation::cur());
            let bit_4 = meta.query_advice(advices[A3], Rotation::cur());
            let bit_5 = meta.query_advice(advices[A4], Rotation::cur());
            let bit_6 = meta.query_advice(advices[A5], Rotation::cur());
            let bit_7 = meta.query_advice(advices[A6], Rotation::cur());
            let bit_8 = meta.query_advice(advices[A7], Rotation::cur());
            let s_byte_decompose = meta.query_selector(s_byte_decompose);

            // Decomposition constraint: bits sum to byte
            let decomposition = bit_1.clone()
                + bit_2.clone() * F::from(1 << 1)
                + bit_3.clone() * F::from(1 << 2)
                + bit_4.clone() * F::from(1 << 3)
                + bit_5.clone() * F::from(1 << 4)
                + bit_6.clone() * F::from(1 << 5)
                + bit_7.clone() * F::from(1 << 6)
                + bit_8.clone() * F::from(1 << 7)
                - byte;

            // Each bit is boolean-constrained (b*(1-b) = 0) to prevent a
            // malicious prover from using non-binary values that satisfy
            // the decomposition equation.
            Constraints::with_selector(
                s_byte_decompose,
                [
                    ("decomposition", decomposition),
                    ("bit_1 bool", bool_check(bit_1)),
                    ("bit_2 bool", bool_check(bit_2)),
                    ("bit_3 bool", bool_check(bit_3)),
                    ("bit_4 bool", bool_check(bit_4)),
                    ("bit_5 bool", bool_check(bit_5)),
                    ("bit_6 bool", bool_check(bit_6)),
                    ("bit_7 bool", bool_check(bit_7)),
                    ("bit_8 bool", bool_check(bit_8)),
                ],
            )
        });

        meta.create_gate("byte xor", |meta| {
            let s_byte_xor = meta.query_selector(s_byte_xor);
            let bit_xor = |idx: usize, meta: &mut VirtualCells<F>| {
                let lhs_bit = meta.query_advice(advices[idx], Rotation::prev());
                let rhs_bit = meta.query_advice(advices[idx], Rotation::cur());
                let out_bit = meta.query_advice(advices[idx], Rotation::next());
                lhs_bit.clone() + rhs_bit.clone() - lhs_bit * rhs_bit * F::from(2) - out_bit
            };

            Constraints::with_selector(
                s_byte_xor,
                core::iter::empty()
                    .chain((0..8).map(|idx| bit_xor(idx, meta)))
                    .collect::<Vec<_>>(),
            )
        });

        // lhs + rhs = out + carry * 2^64, with carry boolean-constrained.
        meta.create_gate("word add", |meta| {
            let s_word_add = meta.query_selector(s_word_add);
            let lhs = meta.query_advice(advices[A0], Rotation::cur());
            let rhs = meta.query_advice(advices[A1], Rotation::cur());
            let out = meta.query_advice(advices[A0], Rotation::next());
            let carry = meta.query_advice(advices[A1], Rotation::next());
            let equal = lhs + rhs - carry.clone() * F::from_u128(1u128 << 64) - out;

            Constraints::with_selector(
                s_word_add,
                [
                    ("carry bool check", bool_check(carry)),
                    ("equal check", equal),
                ],
            )
        });

        // field = word_1 + word_2 * 2^64  (128 bits packed into one Pallas field element)
        meta.create_gate("encode two words to one field", |meta| {
            let field_element = meta.query_advice(advices[A0], Rotation::next());
            let word_1 = meta.query_advice(advices[A0], Rotation::cur());
            let word_2 = meta.query_advice(advices[A1], Rotation::cur());
            let s_result_encode = meta.query_selector(s_result_encode);

            vec![s_result_encode * (word_1 + word_2 * F::from_u128(1u128 << 64) - field_element)]
        });

        // CANONICALITY CHECK GATE (BIT-LEVEL)
        //
        // This gate ensures that the 256-bit decomposition of a field element is
        // strictly less than the pallas modulus p, using direct bit constraints.
        //
        // The pallas modulus is:
        // p = 0x40000000_00000000_00000000_00000000_224698fc_094cf91b_992d30ed_00000001
        //
        // In binary: bit 255 = 0, bit 254 = 1, bits 253-128 = 0, bits 127-0 = lower part
        //
        // For a canonical value x < p:
        // - Case 1: bit[255] = 0 AND bit[254] = 0 → x < 2^254 < p ✓
        // - Case 2: bit[255] = 0 AND bit[254] = 1 → x ∈ [2^254, 2^255)
        //   - If any of bits[253..128] = 1 → x >= 2^254 + 2^128 > p ✗
        //   - If bits[253..128] = 0 → need bits[127..0] < p's lower 128 bits
        //
        // This gate checks:
        // 1. bit[255] = 0 (always required)
        // 2. If bit[254] = 1: lower 128 bits must be < p_lower via diff decomposition
        //
        // The check for bits[253..128] = 0 when bit[254] = 1 is done separately
        // using s_high_bit_zero gate applied to each bit individually.
        //
        // Layout (2 rows):
        // Row 0: bit_255, bit_254, lower_128_diff, diff_w1, diff_w2, diff_w3, diff_w4, w1, w2, w3
        // Row 1: w4, lower_128
        meta.create_gate("canonicality check", |meta| {
            use halo2_proofs::plonk::Expression;

            let s_canonicality = meta.query_selector(s_canonicality);

            // Query the critical bits (copied from actual bit cells)
            let bit_255 = meta.query_advice(advices[A0], Rotation::cur());
            let bit_254 = meta.query_advice(advices[A1], Rotation::cur());

            // The diff value and its decomposition
            let lower_128_diff = meta.query_advice(advices[A2], Rotation::cur());
            let diff_word_1 = meta.query_advice(advices[A3], Rotation::cur());
            let diff_word_2 = meta.query_advice(advices[A4], Rotation::cur());
            let diff_word_3 = meta.query_advice(advices[A5], Rotation::cur());
            let diff_word_4 = meta.query_advice(advices[A6], Rotation::cur());

            // The 4 words that make up lower_128 (copied from actual word cells)
            let word_1 = meta.query_advice(advices[A7], Rotation::cur());
            let word_2 = meta.query_advice(advices[A8], Rotation::cur());
            let word_3 = meta.query_advice(advices[A9], Rotation::cur());
            let word_4 = meta.query_advice(advices[A0], Rotation::next());

            // The computed lower_128 value
            let lower_128 = meta.query_advice(advices[A1], Rotation::next());

            let one = Expression::Constant(F::ONE);
            let p_lower = Expression::Constant(F::from_u128(PALLAS_MODULUS_LOWER_128));
            let two_32 = Expression::Constant(F::from(1u64 << 32));

            // Constraint 1: bit_255 must be 0
            let bit_255_zero = bit_255;

            // Constraint 2: lower_128 must equal word_1 + word_2*2^32 + word_3*2^64 + word_4*2^96
            // This ensures lower_128 is properly constrained to the actual words
            let lower_128_decomposition = lower_128.clone()
                - word_1
                - word_2 * two_32.clone()
                - word_3 * Expression::Constant(F::from_u128(1u128 << 64))
                - word_4 * Expression::Constant(F::from_u128(1u128 << 96));

            // Constraint 3: When bit_254 = 1, we need lower_128 < p_lower
            // Verify using: lower_128_diff = p_lower - 1 - lower_128
            // When bit_254 = 0, this constraint is disabled (multiplied by 0)
            let diff_check =
                bit_254.clone() * (lower_128_diff.clone() - (p_lower - one - lower_128));

            // Constraint 4: Verify lower_128_diff decomposes correctly into 4 words
            // This ensures diff is in [0, 2^128 - 1], proving lower_128 < p_lower
            // The diff_words are range-checked via s_word_decompose elsewhere
            let diff_decomposition = bit_254
                * (lower_128_diff
                    - diff_word_1
                    - diff_word_2 * two_32
                    - diff_word_3 * Expression::Constant(F::from_u128(1u128 << 64))
                    - diff_word_4 * Expression::Constant(F::from_u128(1u128 << 96)));

            Constraints::with_selector(
                s_canonicality,
                [
                    ("bit_255 must be zero", bit_255_zero),
                    ("lower_128 decomposition", lower_128_decomposition),
                    ("diff equals p_lower - 1 - lower_128", diff_check),
                    ("diff decomposes to 4 words", diff_decomposition),
                ],
            )
        });

        // HIGH BIT ZERO CHECK GATE
        //
        // This gate constrains: bit_254 * bit = 0
        // Applied to each bit in [128..254] to ensure they're all 0 when bit_254 = 1.
        //
        // Layout (1 row):
        // Row 0: bit_254, bit_to_check
        meta.create_gate("high bit zero check", |meta| {
            let s_high_bit_zero = meta.query_selector(s_high_bit_zero);

            let bit_254 = meta.query_advice(advices[A0], Rotation::cur());
            let bit_to_check = meta.query_advice(advices[A1], Rotation::cur());

            // If bit_254 = 1, then bit_to_check must be 0
            // If bit_254 = 0, this constraint is satisfied for any bit value
            Constraints::with_selector(
                s_high_bit_zero,
                [("bit_254 * bit = 0", bit_254 * bit_to_check)],
            )
        });

        // WORD COMBINE GATE
        //
        // This gate constrains a 64-bit word to equal the combination of two 32-bit words:
        // word_64 = word_32_lo + word_32_hi * 2^32
        //
        // This is critical for soundness in field_decompose where we convert the
        // 8 x 32-bit words (used for canonicality check) into 4 x 64-bit words
        // (used for BLAKE2b operations).
        //
        // Layout (2 rows):
        // Row 0: word_32_lo, word_32_hi
        // Row 1: word_64
        meta.create_gate("combine two 32-bit words to 64-bit", |meta| {
            let s_word_combine = meta.query_selector(s_word_combine);

            let word_32_lo = meta.query_advice(advices[A0], Rotation::cur());
            let word_32_hi = meta.query_advice(advices[A1], Rotation::cur());
            let word_64 = meta.query_advice(advices[A0], Rotation::next());

            vec![s_word_combine * (word_32_lo + word_32_hi * F::from(1u64 << 32) - word_64)]
        });

        Blake2bConfig {
            advices,
            s_field_decompose,
            s_word_decompose,
            s_byte_decompose,
            s_byte_xor,
            s_word_add,
            s_result_encode,
            s_canonicality,
            s_high_bit_zero,
            s_word_combine,
            _marker: PhantomData,
        }
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

    /// Hash field-element inputs and return the BLAKE2b-256 result as 4 x 64-bit words.
    ///
    /// Each field element is decomposed into 4 x 64-bit words (with canonicality check).
    /// Words are packed into 128-byte blocks (16 words each), compressed through
    /// the standard BLAKE2b compression function, and the first 256 bits of the
    /// final state are returned.
    ///
    /// # Arguments
    /// * `layouter` - The circuit layouter
    /// * `inputs` - The input field elements (must be even length)
    /// * `personalization` - 16-byte personalization string
    pub fn process(
        &self,
        layouter: &mut impl Layouter<F>,
        inputs: &[AssignedCell<F, F>],
        personalization: &[u8],
    ) -> Result<Vec<Blake2bWord<F>>, Error> {
        assert_eq!(personalization.len(), 16);
        assert!(inputs.len() % 2 == 0);

        // Initialize state: h[0] = IV[0] XOR parameter block (0x01010000 | nn=32),
        // h[6..7] = IV[6..7] XOR personalization (RFC 7693, Section 2.5).
        let mut h = vec![
            Blake2bWord::from_constant_u64(IV[0] ^ 0x01010000 ^ 32, layouter, self)?,
            Blake2bWord::from_constant_u64(IV[1], layouter, self)?,
            Blake2bWord::from_constant_u64(IV[2], layouter, self)?,
            Blake2bWord::from_constant_u64(IV[3], layouter, self)?,
            Blake2bWord::from_constant_u64(IV[4], layouter, self)?,
            Blake2bWord::from_constant_u64(IV[5], layouter, self)?,
            Blake2bWord::from_constant_u64(
                IV[6] ^ LittleEndian::read_u64(&personalization[0..8]),
                layouter,
                self,
            )?,
            Blake2bWord::from_constant_u64(
                IV[7] ^ LittleEndian::read_u64(&personalization[8..16]),
                layouter,
                self,
            )?,
        ];

        // Convert field elements to 128-byte blocks.
        // Each field element (32 bytes) yields 4 x 64-bit words.
        // One BLAKE2b block = 128 bytes = 16 words = 4 field elements.
        let mut blocks = vec![];
        for block_fields in inputs.chunks(4) {
            let mut cur_block = Vec::with_capacity(16);
            for field in block_fields.iter() {
                let mut words = self.field_decompose(layouter, field)?;
                cur_block.append(&mut words);
            }
            // Pad with zeros if we don't have 16 words (partial last block)
            while cur_block.len() < 16 {
                cur_block.push(Blake2bWord::from_constant_u64(0, layouter, self)?);
            }
            blocks.push(cur_block);
        }

        if blocks.is_empty() {
            // Empty input - use zero padding block
            let zero_padding_block = (0..16)
                .map(|_| Blake2bWord::from_constant_u64(0, layouter, self).unwrap())
                .collect();
            blocks.push(zero_padding_block);
        }

        let block_len = blocks.len();

        // Compress intermediate blocks with cumulative byte counter
        for (i, block) in blocks[0..(block_len - 1)].iter().enumerate() {
            self.compress(layouter, &mut h, block, (i as u128 + 1) * 128, false)?;
        }

        // Compress final block with total byte count and finalization flag
        let total_bytes = inputs.len() as u128 * 32;
        self.compress(
            layouter,
            &mut h,
            &blocks[block_len - 1],
            total_bytes.max(128),
            true,
        )?;

        // Return first 4 words (256 bits) for BLAKE2b-256
        Ok(h[0..4].to_vec())
    }

    /// Process mixed field elements and raw bytes for compact action hash.
    ///
    /// This function implements a hybrid input system where:
    /// - **field_inputs**: Data that IS field elements (nullifier, cmx) - canonicality checked
    /// - **byte_inputs**: Arbitrary bytes (epk, enc[0..52]) - no canonicality check, just boolean constraints
    ///
    /// This distinction is critical for security:
    /// - Nullifier and cmx are Pallas field elements, so they must be < p
    /// - epk (curve point) and enc (ciphertext) can be arbitrary 32/52 bytes that may exceed p
    ///
    /// # Arguments
    /// * `layouter` - The circuit layouter
    /// * `field_inputs` - Field elements to hash (canonicality checked)
    /// * `byte_inputs` - Raw bytes to hash (each cell = 1 byte, boolean constrained only)
    /// * `personalization` - 16-byte personalization string
    ///
    /// # Returns
    /// The BLAKE2b-256 hash result as 4 x 64-bit words.
    pub fn process_hybrid(
        &self,
        layouter: &mut impl Layouter<F>,
        field_inputs: &[AssignedCell<F, F>],
        byte_inputs: &[AssignedCell<F, F>],
        personalization: &[u8; 16],
    ) -> Result<Vec<Blake2bWord<F>>, Error> {
        // Initialize BLAKE2b state with personalization
        let mut h = vec![
            Blake2bWord::from_constant_u64(IV[0] ^ 0x01010000 ^ 32, layouter, self)?,
            Blake2bWord::from_constant_u64(IV[1], layouter, self)?,
            Blake2bWord::from_constant_u64(IV[2], layouter, self)?,
            Blake2bWord::from_constant_u64(IV[3], layouter, self)?,
            Blake2bWord::from_constant_u64(IV[4], layouter, self)?,
            Blake2bWord::from_constant_u64(IV[5], layouter, self)?,
            Blake2bWord::from_constant_u64(
                IV[6] ^ LittleEndian::read_u64(&personalization[0..8]),
                layouter,
                self,
            )?,
            Blake2bWord::from_constant_u64(
                IV[7] ^ LittleEndian::read_u64(&personalization[8..16]),
                layouter,
                self,
            )?,
        ];

        // Convert field inputs to words (with canonicality check)
        let mut all_words = Vec::new();
        for (i, field) in field_inputs.iter().enumerate() {
            let words = self.field_decompose(
                &mut layouter.namespace(|| format!("field_decompose_{}", i)),
                field,
            )?;
            all_words.extend(words);
        }

        // Convert byte inputs to words (no canonicality check, just boolean constraints)
        let byte_words =
            self.bytes_to_words(&mut layouter.namespace(|| "bytes_to_words"), byte_inputs)?;
        all_words.extend(byte_words);

        // Calculate total input bytes for BLAKE2b counter
        // Field inputs: 32 bytes each, Byte inputs: 1 byte each
        let total_bytes = field_inputs.len() * 32 + byte_inputs.len();

        // Pack words into 128-byte blocks (16 x 64-bit words per block)
        let mut blocks = Vec::new();
        for block_words in all_words.chunks(16) {
            let mut cur_block = block_words.to_vec();
            // Pad with zeros if we don't have 16 words
            while cur_block.len() < 16 {
                cur_block.push(Blake2bWord::from_constant_u64(0, layouter, self)?);
            }
            blocks.push(cur_block);
        }

        if blocks.is_empty() {
            // Empty input - use zero padding block
            let zero_block = (0..16)
                .map(|_| Blake2bWord::from_constant_u64(0, layouter, self).unwrap())
                .collect();
            blocks.push(zero_block);
        }

        let block_len = blocks.len();

        // Compress all blocks except the last one
        for (i, block) in blocks[0..(block_len - 1)].iter().enumerate() {
            self.compress(layouter, &mut h, block, (i as u128 + 1) * 128, false)?;
        }

        // Compress final block with total byte count
        self.compress(
            layouter,
            &mut h,
            &blocks[block_len - 1],
            total_bytes.max(128) as u128,
            true,
        )?;

        // Return first 4 words (256 bits) for BLAKE2b-256
        Ok(h[0..4].to_vec())
    }

    /// Encode four 64-bit hash output words into two field elements (128 bits each).
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
                        || "result field",
                        self.config.advices[A0],
                        1,
                        || field_value,
                    )
                },
            )?;
            fields.push(field);
        }
        assert_eq!(fields.len(), 2);
        Ok(fields.try_into().unwrap())
    }

    /// Compression function F takes as an argument the state vector "h",
    /// message block vector "m" (last block is padded with zeros to full
    /// block size, if required), 2w-bit offset counter "t", and final block
    /// indicator flag "f".  Local vector v[0..15] is used in processing.  F
    /// returns a new state vector.  The number of rounds, "r", is 12 for
    /// BLAKE2b and 10 for BLAKE2s.  Rounds are numbered from 0 to r - 1.
    ///
    ///     FUNCTION F( h[0..7], m[0..15], t, f )
    ///     |
    ///     |      // Initialize local work vector v[0..15]
    ///     |      v[0..7] := h[0..7]              // First half from state.
    ///     |      v[8..15] := IV[0..7]            // Second half from IV.
    ///     |
    ///     |      v[12] := v[12] ^ (t mod 2**w)   // Low word of the offset.
    ///     |      v[13] := v[13] ^ (t >> w)       // High word.
    ///     |
    ///     |      IF f = TRUE THEN                // last block flag?
    ///     |      |   v[14] := v[14] ^ 0xFF..FF   // Invert all bits.
    ///     |      END IF.
    ///     |
    ///     |      // Cryptographic mixing
    ///     |      FOR i = 0 TO r - 1 DO           // Ten or twelve rounds.
    ///     |      |
    ///     |      |   // Message word selection permutation for this round.
    ///     |      |   s[0..15] := SIGMA[i mod 10][0..15]
    ///     |      |
    ///     |      |   v := G( v, 0, 4,  8, 12, m[s[ 0]], m[s[ 1]] )
    ///     |      |   v := G( v, 1, 5,  9, 13, m[s[ 2]], m[s[ 3]] )
    ///     |      |   v := G( v, 2, 6, 10, 14, m[s[ 4]], m[s[ 5]] )
    ///     |      |   v := G( v, 3, 7, 11, 15, m[s[ 6]], m[s[ 7]] )
    ///     |      |
    ///     |      |   v := G( v, 0, 5, 10, 15, m[s[ 8]], m[s[ 9]] )
    ///     |      |   v := G( v, 1, 6, 11, 12, m[s[10]], m[s[11]] )
    ///     |      |   v := G( v, 2, 7,  8, 13, m[s[12]], m[s[13]] )
    ///     |      |   v := G( v, 3, 4,  9, 14, m[s[14]], m[s[15]] )
    ///     |      |
    ///     |      END FOR
    ///     |
    ///     |      FOR i = 0 TO 7 DO               // XOR the two halves.
    ///     |      |   h[i] := h[i] ^ v[i] ^ v[i + 8]
    ///     |      END FOR.
    ///     |
    ///     |      RETURN h[0..7]                  // New state.
    ///     |
    ///     END FUNCTION.
    fn compress(
        &self,
        layouter: &mut impl Layouter<F>,
        h: &mut [Blake2bWord<F>], // current state
        m: &[Blake2bWord<F>],     // current block
        t: u128,                  // 128-bit offset counter (total bytes processed)
        f: bool,                  // final block flag
    ) -> Result<(), Error> {
        let mut v = Vec::with_capacity(16);
        v.extend_from_slice(h);
        for iv in IV[0..4].iter() {
            let word = Blake2bWord::from_constant_u64(*iv, layouter, self)?;
            v.push(word);
        }
        // v[12] := IV[4] ^ (t mod 2^64)  — low word of offset counter
        let v_12 = Blake2bWord::from_constant_u64(IV[4] ^ (t as u64), layouter, self)?;
        v.push(v_12);

        // v[13] := IV[5] ^ (t >> 64)  — high word of offset counter
        let v_13 = Blake2bWord::from_constant_u64(IV[5] ^ ((t >> 64) as u64), layouter, self)?;
        v.push(v_13);

        // v[14] := IV[6] ^ 0xFFFF_FFFF_FFFF_FFFF if final block, else IV[6]
        let v_14 = if f {
            Blake2bWord::from_constant_u64(IV[6] ^ u64::MAX, layouter, self)?
        } else {
            Blake2bWord::from_constant_u64(IV[6], layouter, self)?
        };
        v.push(v_14);

        // v_15
        let v_15 = Blake2bWord::from_constant_u64(IV[7], layouter, self)?;
        v.push(v_15);
        assert_eq!(v.len(), 16);

        for i in 0..ROUNDS {
            // SIGMA only has 10 permutations, so we cycle through them
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

        // Finalize the state
        for i in 0..8 {
            let h_i_bits = self.word_xor(
                layouter.namespace(|| "final_xor_1"),
                h[i].get_bits(),
                v[i].get_bits(),
            )?;
            let h_i_bits = self.word_xor(
                layouter.namespace(|| "final_xor_2"),
                &h_i_bits,
                v[i + 8].get_bits(),
            )?;
            h[i] = Blake2bWord::from_bits(
                self,
                layouter.namespace(|| "final_word_from_bits"),
                h_i_bits,
            )?;
        }

        Ok(())
    }

    /// The G primitive function mixes two input words, "x" and "y", into
    /// four words indexed by "a", "b", "c", and "d" in the working vector
    /// v[0..15].  The full modified vector is returned.  The rotation
    /// constants are (R1, R2, R3, R4) = (32, 24, 16, 63) for BLAKE2b.
    ///
    /// FUNCTION G( v[0..15], a, b, c, d, x, y )
    /// |
    /// |   v[a] := (v[a] + v[b] + x) mod 2**w
    /// |   v[d] := (v[d] ^ v[a]) >>> R1
    /// |   v[c] := (v[c] + v[d])     mod 2**w
    /// |   v[b] := (v[b] ^ v[c]) >>> R2
    /// |   v[a] := (v[a] + v[b] + y) mod 2**w
    /// |   v[d] := (v[d] ^ v[a]) >>> R3
    /// |   v[c] := (v[c] + v[d])     mod 2**w
    /// |   v[b] := (v[b] ^ v[c]) >>> R4
    /// |
    /// |   RETURN v[0..15]
    /// |
    /// END FUNCTION.
    fn g(
        &self,
        mut layouter: impl Layouter<F>,
        v: &mut [Blake2bWord<F>],
        (a, b, c, d): (usize, usize, usize, usize),
        x: &Blake2bWord<F>,
        y: &Blake2bWord<F>,
    ) -> Result<(), Error> {
        // v[a] := (v[a] + v[b] + x) mod 2**w
        v[a] = {
            let sum_a_b = self.add_mod_u64(
                layouter.namespace(|| "g/add_ab"),
                v[a].get_word(),
                v[b].get_word(),
            )?;
            let sum_a_b_x =
                self.add_mod_u64(layouter.namespace(|| "g/add_ab_x"), &sum_a_b, x.get_word())?;
            Blake2bWord::from_word(self, layouter.namespace(|| "g/word_from_sum"), sum_a_b_x)?
        };

        // v[d] := (v[d] ^ v[a]) >>> R1
        v[d] = {
            let d_xor_a = self.word_xor(
                layouter.namespace(|| "g/xor_da"),
                v[d].get_bits(),
                v[a].get_bits(),
            )?;
            let bits = Blake2bWord::word_rotate(&d_xor_a, R1);
            Blake2bWord::from_bits(self, layouter.namespace(|| "g/rot_r1"), bits)?
        };

        // v[c] := (v[c] + v[d])     mod 2**w
        v[c] = {
            let sum = self.add_mod_u64(
                layouter.namespace(|| "g/add_cd"),
                v[c].get_word(),
                v[d].get_word(),
            )?;
            Blake2bWord::from_word(self, layouter.namespace(|| "g/word_from_sum"), sum)?
        };

        // v[b] := (v[b] ^ v[c]) >>> R2
        v[b] = {
            let b_xor_c = self.word_xor(
                layouter.namespace(|| "g/xor_bc"),
                v[b].get_bits(),
                v[c].get_bits(),
            )?;
            let bits = Blake2bWord::word_rotate(&b_xor_c, R2);
            Blake2bWord::from_bits(self, layouter.namespace(|| "g/rot_r2"), bits)?
        };

        // v[a] := (v[a] + v[b] + y) mod 2**w
        v[a] = {
            let sum_a_b = self.add_mod_u64(
                layouter.namespace(|| "g/add_ab_2"),
                v[a].get_word(),
                v[b].get_word(),
            )?;
            let sum_a_b_y =
                self.add_mod_u64(layouter.namespace(|| "g/add_ab_y"), &sum_a_b, y.get_word())?;
            Blake2bWord::from_word(self, layouter.namespace(|| "g/word_from_sum"), sum_a_b_y)?
        };

        // v[d] := (v[d] ^ v[a]) >>> R3
        v[d] = {
            let d_xor_a = self.word_xor(
                layouter.namespace(|| "g/xor_da_2"),
                v[d].get_bits(),
                v[a].get_bits(),
            )?;
            let bits = Blake2bWord::word_rotate(&d_xor_a, R3);
            Blake2bWord::from_bits(self, layouter.namespace(|| "g/rot_r3"), bits)?
        };

        // v[c] := (v[c] + v[d])     mod 2**w
        v[c] = {
            let sum = self.add_mod_u64(
                layouter.namespace(|| "g/add_cd_2"),
                v[c].get_word(),
                v[d].get_word(),
            )?;
            Blake2bWord::from_word(self, layouter.namespace(|| "g/word_from_sum"), sum)?
        };

        // v[b] := (v[b] ^ v[c]) >>> R4
        v[b] = {
            let b_xor_c = self.word_xor(
                layouter.namespace(|| "g/xor_bc_2"),
                v[b].get_bits(),
                v[c].get_bits(),
            )?;
            let bits = Blake2bWord::word_rotate(&b_xor_c, R4);
            Blake2bWord::from_bits(self, layouter.namespace(|| "g/rot_r4"), bits)?
        };

        Ok(())
    }

    /// Decompose a field element into 4 x 64-bit `Blake2bWord`s.
    ///
    /// Pipeline: field → 32 bytes → 256 bits → 8 x 32-bit words → field
    /// decomposition check → canonicality check (value < p) → 4 x 64-bit
    /// words via `word_combine`.
    fn field_decompose(
        &self,
        layouter: &mut impl Layouter<F>,
        field: &AssignedCell<F, F>,
    ) -> Result<Vec<Blake2bWord<F>>, Error> {
        // the decomposition from bytes to bits
        let mut bits = vec![];
        let mut bytes = vec![];
        for i in 0..32 {
            let byte_value = field.value().map(|f| f.to_repr().as_ref()[i]);
            let byte =
                Blake2bByte::from_u8(byte_value, layouter.namespace(|| "from_u8"), &self.config)?;
            bits.append(&mut byte.get_bits().to_vec());
            bytes.push(byte.get_byte());
        }

        // Check the decomposition from 32-bit words to bytes
        // Note: We use 32-bit words here for field decomposition gate and canonicality check
        let mut words_32 = vec![];
        for bytes in bytes.chunks(4) {
            let word =
                self.assign_word_32_from_bytes(layouter.namespace(|| "assign word"), bytes)?;
            words_32.push(word);
        }

        // check the decomposition from field to 32-bit words
        layouter.assign_region(
            || "decompose field to words",
            |mut region| {
                self.config.s_field_decompose.enable(&mut region, 0)?;
                for (i, word) in words_32.iter().enumerate() {
                    word.copy_advice(|| "word", &mut region, self.config.advices[i], 0)?;
                }
                field.copy_advice(|| "field", &mut region, self.config.advices[A0], 1)?;
                Ok(())
            },
        )?;

        // Canonicality: ensure the 256-bit decomposition is strictly less than p.
        // Without this, a prover could use the non-canonical representation (value + p).
        self.check_canonicality(layouter, &bits, &words_32)?;

        // Combine pairs of 32-bit words into 64-bit words for BLAKE2b operations.
        // Each pair is constrained: word_64 = word_32_lo + word_32_hi * 2^32.
        let mut res = Vec::with_capacity(4);
        for (i, chunk) in bits.chunks(64).enumerate() {
            // Combine two adjacent 32-bit words into one 64-bit word with constraint
            let word_64 = self.word_combine(
                layouter.namespace(|| format!("combine 32-bit words to 64-bit word {}", i)),
                &words_32[i * 2],
                &words_32[i * 2 + 1],
            )?;
            res.push(Blake2bWord {
                word: word_64,
                bits: chunk.to_vec().try_into().unwrap(),
            });
        }

        Ok(res)
    }

    /// Check that the 256-bit decomposition is canonical (strictly less than p).
    ///
    /// This is a critical soundness check. Without it, a malicious prover could
    /// decompose a field element f as either f or f+p (both satisfy the mod-p
    /// constraint), leading to different BLAKE2b outputs for the "same" field value.
    ///
    /// The check uses the 256 bits directly:
    /// 1. bit[255] must be 0
    /// 2. If bit[254] = 1, bits[253..128] must all be 0
    /// 3. If bit[254] = 1, lower 128 bits must be < p's lower 128 bits
    ///
    /// All witness values are properly constrained via copy_advice to ensure
    /// they match the actual bit and word cells from field decomposition.
    ///
    /// The diff value (p_lower - 1 - lower_128) is decomposed into 4 words,
    /// and each word is range-checked via s_word_decompose to ensure diff >= 0.
    fn check_canonicality(
        &self,
        layouter: &mut impl Layouter<F>,
        bits: &[AssignedCell<F, F>],
        words: &[AssignedCell<F, F>],
    ) -> Result<(), Error> {
        assert_eq!(bits.len(), 256);
        assert_eq!(words.len(), 8);

        // Extract key bits
        let bit_255 = &bits[255];
        let bit_254 = &bits[254];

        let (lower_128, diff, diff_word_values) = self.compute_lower_128_and_diff(words);
        let diff_word_cells = self.assign_canonicality_region(
            layouter,
            bit_255,
            bit_254,
            words,
            lower_128,
            diff,
            &diff_word_values,
        )?;
        self.apply_high_bit_zero_checks(layouter, bit_254, bits)?;
        self.range_check_diff_words(layouter, &diff_word_values, &diff_word_cells)?;

        Ok(())
    }

    /// Decompose a 32-bit word into 4 bytes using the 8-byte word gate with upper bytes zeroed.
    /// Used for field decomposition and canonicality range checks.
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
                // Note: We use a subset of the s_word_decompose gate (first 4 bytes)
                // The gate supports up to 8 bytes but we only use 4 here
                self.config.s_word_decompose.enable(&mut region, 0)?;
                for (i, byte) in bytes.iter().enumerate() {
                    byte.copy_advice(|| "byte", &mut region, self.config.advices[i], 0)?;
                }
                // Zero out unused bytes (5-8) for the gate constraint
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

    /// Decompose a 64-bit word into 8 bytes.
    fn word_decompose(
        &self,
        mut layouter: impl Layouter<F>,
        bytes: &[AssignedCell<F, F>],
        word: &AssignedCell<F, F>,
    ) -> Result<(), Error> {
        assert_eq!(bytes.len(), 8);
        layouter.assign_region(
            || "decompose 64-bit word to bytes",
            |mut region| {
                self.config.s_word_decompose.enable(&mut region, 0)?;
                for (i, byte) in bytes.iter().enumerate() {
                    byte.copy_advice(|| "byte", &mut region, self.config.advices[i], 0)?;
                }
                word.copy_advice(|| "word", &mut region, self.config.advices[A0], 1)?;
                Ok(())
            },
        )
    }

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

    fn assign_word_64_from_bytes(
        &self,
        mut layouter: impl Layouter<F>,
        bytes: &[AssignedCell<F, F>],
    ) -> Result<AssignedCell<F, F>, Error> {
        let word_value = word_value_from_bytes(bytes);
        let word = assign_free_advice(
            layouter.namespace(|| "assign 64-bit word"),
            self.config.advices[A8],
            word_value,
        )?;
        self.word_decompose(layouter.namespace(|| "word decompose 64"), bytes, &word)?;
        Ok(word)
    }

    fn assign_byte_from_bits(
        &self,
        mut layouter: impl Layouter<F>,
        bits: &[AssignedCell<F, F>],
    ) -> Result<AssignedCell<F, F>, Error> {
        let byte_value = byte_value_from_bits(bits);
        let byte = assign_free_advice(
            layouter.namespace(|| "assign byte"),
            self.config.advices[A8],
            byte_value,
        )?;
        self.byte_decompose(layouter.namespace(|| "byte decompose"), bits, &byte)?;
        Ok(byte)
    }

    fn compute_lower_128_and_diff(
        &self,
        words: &[AssignedCell<F, F>],
    ) -> (Value<u128>, Value<u128>, [Value<u32>; 4]) {
        // Compute lower 128 bits as a field element
        // lower_128 = word_1 + word_2 * 2^32 + word_3 * 2^64 + word_4 * 2^96
        let lower_128: Value<u128> = words[0]
            .value()
            .zip(words[1].value())
            .zip(words[2].value())
            .zip(words[3].value())
            .map(|(((w1, w2), w3), w4)| {
                (f_to_u32_le(w1) as u128)
                    + ((f_to_u32_le(w2) as u128) << 32)
                    + ((f_to_u32_le(w3) as u128) << 64)
                    + ((f_to_u32_le(w4) as u128) << 96)
            });

        // Compute diff = p_lower - 1 - lower_128
        // If lower_128 < p_lower, diff is in [0, p_lower - 1]
        // If lower_128 >= p_lower, diff would be "negative" (wrap around)
        let p_lower = PALLAS_MODULUS_LOWER_128;
        let diff: Value<u128> = lower_128.map(|l| {
            if l < p_lower {
                p_lower - 1 - l
            } else {
                // This case should never happen for canonical values
                // Set to 0; the constraint will fail
                0
            }
        });

        // Decompose diff into 4 32-bit words for range checking
        let diff_word_values: [Value<u32>; 4] = [
            diff.map(|d| d as u32),
            diff.map(|d| (d >> 32) as u32),
            diff.map(|d| (d >> 64) as u32),
            diff.map(|d| (d >> 96) as u32),
        ];

        (lower_128, diff, diff_word_values)
    }

    fn assign_canonicality_region(
        &self,
        layouter: &mut impl Layouter<F>,
        bit_255: &AssignedCell<F, F>,
        bit_254: &AssignedCell<F, F>,
        words: &[AssignedCell<F, F>],
        lower_128: Value<u128>,
        diff: Value<u128>,
        diff_word_values: &[Value<u32>; 4],
    ) -> Result<Vec<AssignedCell<F, F>>, Error> {
        // Assign the canonicality check region and get back the diff_word cells
        // for range checking. All bits and words are copied via copy_advice.
        layouter.assign_region(
            || "canonicality check",
            |mut region| {
                self.config.s_canonicality.enable(&mut region, 0)?;

                // Row 0: bit_255, bit_254, lower_128_diff, diff_w1..diff_w4, w1, w2, w3
                // Use copy_advice to constrain these to the actual bit/word cells
                bit_255.copy_advice(|| "bit_255", &mut region, self.config.advices[A0], 0)?;
                bit_254.copy_advice(|| "bit_254", &mut region, self.config.advices[A1], 0)?;

                // Witness the diff value
                let diff_field = diff.map(|d| F::from_u128(d));
                region.assign_advice(
                    || "lower_128_diff",
                    self.config.advices[A2],
                    0,
                    || diff_field,
                )?;

                // Assign diff words and collect the cells for later range checking
                let mut diff_cells = Vec::with_capacity(4);
                for (i, dw) in diff_word_values.iter().enumerate() {
                    let cell = region.assign_advice(
                        || format!("diff_word_{}", i + 1),
                        self.config.advices[A3 + i],
                        0,
                        || dw.map(|w| F::from(w as u64)),
                    )?;
                    diff_cells.push(cell);
                }

                // Copy words[0..4] using copy_advice to constrain lower_128
                words[0].copy_advice(|| "word_1", &mut region, self.config.advices[A7], 0)?;
                words[1].copy_advice(|| "word_2", &mut region, self.config.advices[A8], 0)?;
                words[2].copy_advice(|| "word_3", &mut region, self.config.advices[A9], 0)?;

                // Row 1: word_4, lower_128
                words[3].copy_advice(|| "word_4", &mut region, self.config.advices[A0], 1)?;

                // Assign lower_128 - this is constrained by the gate to equal
                // word_1 + word_2*2^32 + word_3*2^64 + word_4*2^96
                let lower_128_field = lower_128.map(|l| F::from_u128(l));
                region.assign_advice(
                    || "lower_128",
                    self.config.advices[A1],
                    1,
                    || lower_128_field,
                )?;

                Ok(diff_cells)
            },
        )
    }

    fn apply_high_bit_zero_checks(
        &self,
        layouter: &mut impl Layouter<F>,
        bit_254: &AssignedCell<F, F>,
        bits: &[AssignedCell<F, F>],
    ) -> Result<(), Error> {
        // Apply s_high_bit_zero gate to each bit in [128..254]
        // This constrains: bit_254 * bit[i] = 0 for each bit
        // Using copy_advice ensures we're checking the actual bits
        for (i, bit) in bits[128..254].iter().enumerate() {
            layouter.assign_region(
                || format!("high bit zero check {}", i),
                |mut region| {
                    self.config.s_high_bit_zero.enable(&mut region, 0)?;
                    bit_254.copy_advice(|| "bit_254", &mut region, self.config.advices[A0], 0)?;
                    bit.copy_advice(|| "bit_to_check", &mut region, self.config.advices[A1], 0)?;
                    Ok(())
                },
            )?;
        }
        Ok(())
    }

    fn range_check_diff_words(
        &self,
        layouter: &mut impl Layouter<F>,
        diff_word_values: &[Value<u32>; 4],
        diff_word_cells: &[AssignedCell<F, F>],
    ) -> Result<(), Error> {
        // Range-check each diff word by decomposing it to bytes
        // This ensures diff is in [0, 2^128 - 1], proving lower_128 < p_lower
        // The diff_word_cells are the SAME cells from the canonicality region
        for (i, (dw_val, dw_cell)) in diff_word_values
            .iter()
            .zip(diff_word_cells.iter())
            .enumerate()
        {
            // Decompose each diff word into 4 bytes, then each byte into bits
            // The bit boolean constraints will ensure each word is in [0, 2^32 - 1]
            let mut diff_bytes = Vec::with_capacity(4);
            for j in 0..4 {
                let byte_val = dw_val.map(|w| ((w >> (j * 8)) & 0xFF) as u8);
                let byte = Blake2bByte::from_u8(
                    byte_val,
                    layouter.namespace(|| format!("diff_word_{}_byte_{}", i, j)),
                    &self.config,
                )?;
                diff_bytes.push(byte.get_byte());
            }

            // Use the SAME diff_word cell from the canonicality region
            // This ensures the range-checked word is the same as the one in the constraint
            // Note: diff_words are 32-bit for canonicality check
            self.word_decompose_32(
                layouter.namespace(|| format!("diff_word_{}_decompose", i)),
                &diff_bytes,
                dw_cell,
            )?;
        }
        Ok(())
    }

    /// Combine two 32-bit words into one 64-bit word.
    ///
    /// Constrains: `word_64 = word_32_lo + word_32_hi * 2^32`.
    ///
    /// This bridges the 32-bit words (verified by the canonicality check) to the
    /// 64-bit words used in BLAKE2b compression.
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

                // Copy the 32-bit words to row 0
                word_32_lo.copy_advice(|| "word_32_lo", &mut region, self.config.advices[A0], 0)?;
                word_32_hi.copy_advice(|| "word_32_hi", &mut region, self.config.advices[A1], 0)?;

                // Compute and assign the 64-bit word to row 1
                let word_64_value = word_32_lo
                    .value()
                    .zip(word_32_hi.value())
                    .map(|(&lo, &hi)| lo + hi * F::from(1u64 << 32));

                region.assign_advice(|| "word_64", self.config.advices[A0], 1, || word_64_value)
            },
        )
    }

    /// Decompose raw bytes to Blake2bWords without field interpretation.
    ///
    /// Unlike field_decompose(), this function does NOT perform canonicality checks
    /// because the input bytes may represent arbitrary data (like curve points or
    /// ciphertext) that can exceed the field modulus.
    ///
    /// Each byte is decomposed to 8 bits with boolean constraints, ensuring the
    /// bytes are well-formed even without canonicality.
    ///
    /// # Arguments
    /// * `layouter` - The circuit layouter
    /// * `bytes` - The input bytes (each cell holds one byte value 0-255)
    ///
    /// # Returns
    /// A vector of Blake2bWords constructed from the input bytes.
    /// The bytes are packed into 64-bit words in little-endian order.
    pub fn bytes_to_words(
        &self,
        layouter: &mut impl Layouter<F>,
        bytes: &[AssignedCell<F, F>],
    ) -> Result<Vec<Blake2bWord<F>>, Error> {
        let mut all_bits = Vec::with_capacity((bytes.len() + 7) / 8 * 64);

        // Decompose each byte to 8 bits with boolean constraints
        for (i, byte_cell) in bytes.iter().enumerate() {
            // Get the byte value from the cell
            let byte_value = byte_cell.value().map(|f| f_to_u8_le(f));

            // Create bits for this byte
            let mut byte_bits = Vec::with_capacity(8);
            for j in 0..8 {
                let bit_value = byte_value.map(|b| F::from(((b >> j) & 1) as u64));
                let bit = assign_free_advice(
                    layouter.namespace(|| format!("byte_{}_bit_{}", i, j)),
                    self.config.advices[A0],
                    bit_value,
                )?;
                byte_bits.push(bit);
            }

            // Constrain: byte = sum of bits * 2^i, and each bit is boolean
            // Uses the s_byte_decompose gate
            self.byte_decompose(
                layouter.namespace(|| format!("decompose_byte_{}", i)),
                &byte_bits,
                byte_cell,
            )?;

            all_bits.extend(byte_bits);
        }

        // Pad with zero bits to reach a multiple of 64
        let padding_needed = (64 - (all_bits.len() % 64)) % 64;
        for i in 0..padding_needed {
            let zero_bit = assign_free_constant(
                layouter.namespace(|| format!("zero_padding_bit_{}", i)),
                self.config.advices[A0],
                F::ZERO,
            )?;
            all_bits.push(zero_bit);
        }

        // Convert bits to 64-bit words
        let mut words = Vec::with_capacity(all_bits.len() / 64);
        for (i, chunk) in all_bits.chunks(64).enumerate() {
            let word = Blake2bWord::from_bits(
                self,
                layouter.namespace(|| format!("word_from_bytes_{}", i)),
                chunk.to_vec(),
            )?;
            words.push(word);
        }

        Ok(words)
    }

    /// Decompose a byte to eight bits.
    fn byte_decompose(
        &self,
        mut layouter: impl Layouter<F>,
        bits: &[AssignedCell<F, F>],
        byte: &AssignedCell<F, F>,
    ) -> Result<(), Error> {
        assert_eq!(bits.len(), 8);
        layouter.assign_region(
            || "decompose byte to bits",
            |mut region| {
                self.config.s_byte_decompose.enable(&mut region, 0)?;
                for (i, bit) in bits.iter().enumerate() {
                    bit.copy_advice(|| "bit", &mut region, self.config.advices[i], 0)?;
                }
                byte.copy_advice(|| "byte", &mut region, self.config.advices[A0], 1)?;
                Ok(())
            },
        )
    }

    fn byte_xor(
        &self,
        mut layouter: impl Layouter<F>,
        x: &[AssignedCell<F, F>],
        y: &[AssignedCell<F, F>],
    ) -> Result<Vec<AssignedCell<F, F>>, Error> {
        assert_eq!(x.len(), 8);
        assert_eq!(y.len(), 8);
        layouter.assign_region(
            || "byte xor",
            |mut region| {
                self.config.s_byte_xor.enable(&mut region, 1)?;
                let xor = |x: &F, y: &F| -> F {
                    F::from(((x.is_odd()) ^ (y.is_odd())).unwrap_u8() as u64)
                };
                let mut byte_ret = Vec::with_capacity(8);
                for i in 0..8 {
                    x[i].copy_advice(|| "xor bit x", &mut region, self.config.advices[i], 0)?;
                    y[i].copy_advice(|| "xor bit y", &mut region, self.config.advices[i], 1)?;
                    let result_bits = x[i]
                        .value()
                        .zip(y[i].value())
                        .map(|(x_bit, y_bit)| xor(x_bit, y_bit));
                    let ret = region.assign_advice(
                        || "xor bit result",
                        self.config.advices[i],
                        2,
                        || result_bits,
                    )?;
                    byte_ret.push(ret);
                }

                Ok(byte_ret)
            },
        )
    }

    /// XOR two 64-bit words bit-by-bit (8 bytes x 8 bits each).
    fn word_xor(
        &self,
        mut layouter: impl Layouter<F>,
        x: &[AssignedCell<F, F>],
        y: &[AssignedCell<F, F>],
    ) -> Result<Vec<AssignedCell<F, F>>, Error> {
        assert_eq!(x.len(), 64);
        assert_eq!(y.len(), 64);
        let mut bits = Vec::with_capacity(64);
        for (x_byte, y_byte) in x.chunks(8).zip(y.chunks(8)) {
            let mut ret = self.byte_xor(layouter.namespace(|| "byte xor"), x_byte, y_byte)?;
            bits.append(&mut ret);
        }

        Ok(bits)
    }

    /// 64-bit modular addition: (x + y) mod 2^64.
    ///
    /// Carry is detected by checking byte index 8 of the field sum.
    fn add_mod_u64(
        &self,
        mut layouter: impl Layouter<F>,
        x: &AssignedCell<F, F>,
        y: &AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, Error> {
        layouter.assign_region(
            || "64-bit word add",
            |mut region| {
                self.config.s_word_add.enable(&mut region, 0)?;
                x.copy_advice(|| "word_add x", &mut region, self.config.advices[A0], 0)?;
                y.copy_advice(|| "word_add y", &mut region, self.config.advices[A1], 0)?;
                let sum = x.value().zip(y.value()).map(|(&x, &y)| {
                    let sum = x + y;
                    let carry = F::from(sum.to_repr().as_ref()[8] as u64);
                    let ret = sum - carry * F::from_u128(1u128 << 64);
                    (ret, carry)
                });
                let ret = region.assign_advice(
                    || "word_add ret",
                    self.config.advices[A0],
                    1,
                    || sum.map(|sum| sum.0),
                )?;
                region.assign_advice(
                    || "word_add carry",
                    self.config.advices[A1],
                    1,
                    || sum.map(|sum| sum.1),
                )?;
                Ok(ret)
            },
        )
    }
}

impl<F: PrimeField> Blake2bWord<F> {
    /// Create a `Blake2bWord` from a constant u64 value.
    pub fn from_constant_u64(
        value: u64,
        layouter: &mut impl Layouter<F>,
        chip: &Blake2bChip<F>,
    ) -> Result<Self, Error> {
        let mut bytes = Vec::with_capacity(8);
        let mut word_bits = Vec::with_capacity(64);
        let mut tmp = value;
        for _ in 0..8 {
            let input_byte = tmp as u8;
            let byte = Blake2bByte::from_constant_u8(input_byte, layouter, &chip.config)?;
            bytes.push(byte.get_byte());
            word_bits.append(&mut byte.get_bits().to_vec());
            tmp >>= 8;
        }
        let word = assign_free_constant(
            layouter.namespace(|| "constant word"),
            chip.config.advices[A0],
            F::from(value),
        )?;
        chip.word_decompose(layouter.namespace(|| "word decompose"), &bytes, &word)?;
        Ok(Self {
            word,
            bits: word_bits.try_into().unwrap(),
        })
    }

    /// Rotate 64 bits right by the given number of positions.
    pub fn word_rotate(bits: &[AssignedCell<F, F>], by: usize) -> Vec<AssignedCell<F, F>> {
        assert!(bits.len() == 64);
        let by = by % 64;
        bits.iter()
            .skip(by)
            .chain(bits.iter())
            .take(64)
            .cloned()
            .collect()
    }

    /// Shift 64 bits right by the given number of positions, filling with zeros.
    pub fn shift(
        &self,
        by: usize,
        mut layouter: impl Layouter<F>,
        advice: Column<Advice>,
    ) -> Result<Vec<AssignedCell<F, F>>, Error> {
        let by = by % 64;
        let padding_zero = assign_free_constant(layouter.namespace(|| "zero"), advice, F::from(0))?;
        let old_bits = self.get_bits();
        Ok(old_bits
            .iter()
            .skip(by)
            .chain(Some(&padding_zero).into_iter().cycle())
            .take(64)
            .cloned()
            .collect())
    }

    /// Get the 64 bit cells.
    pub fn get_bits(&self) -> &[AssignedCell<F, F>; 64] {
        &self.bits
    }

    /// Get the word value.
    pub fn get_word(&self) -> &AssignedCell<F, F> {
        &self.word
    }

    /// Construct from 64 assigned bit cells.
    pub fn from_bits(
        chip: &Blake2bChip<F>,
        mut layouter: impl Layouter<F>,
        bits: Vec<AssignedCell<F, F>>,
    ) -> Result<Self, Error> {
        assert!(bits.len() == 64);
        let mut bytes = Vec::with_capacity(8);
        for bits in bits.chunks(8) {
            let byte = chip.assign_byte_from_bits(layouter.namespace(|| "byte from bits"), bits)?;
            bytes.push(byte);
        }
        let word = chip.assign_word_64_from_bytes(layouter.namespace(|| "word from bytes"), &bytes)?;
        Ok(Self {
            word,
            bits: bits.try_into().unwrap(),
        })
    }

    /// Construct from an assigned 64-bit word cell by decomposing it into bytes and bits.
    pub fn from_word(
        chip: &Blake2bChip<F>,
        mut layouter: impl Layouter<F>,
        word: AssignedCell<F, F>,
    ) -> Result<Self, Error> {
        let mut bytes = Vec::with_capacity(8);
        let mut bits = Vec::with_capacity(64);
        for i in 0..8 {
            let byte_value = word.value().map(|v| v.to_repr().as_ref()[i]);
            let byte =
                Blake2bByte::from_u8(byte_value, layouter.namespace(|| "from_u8"), &chip.config)?;
            bits.append(&mut byte.get_bits().to_vec());
            bytes.push(byte.get_byte());
        }

        chip.word_decompose(layouter.namespace(|| "word decompose"), &bytes, &word)?;
        Ok(Self {
            word,
            bits: bits.try_into().unwrap(),
        })
    }
}
