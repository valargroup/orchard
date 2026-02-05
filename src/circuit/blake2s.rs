//! BLAKE2s circuit implementation for Halo2.
//!
//! This is a 1:1 port of Anoma's BLAKE2s circuit from:
//! https://github.com/anoma/taiga/blob/main/taiga_halo2/src/circuit/blake2s.rs
//!
//! BLAKE2s parameters:
//!               | BLAKE2s          |
//! --------------+------------------+
//!  Bits in word | w = 32           |
//!  Rounds in F  | r = 10           |
//!  Block bytes  | bb = 64          |
//!  Hash bytes   | 1 <= nn <= 32    |
//!  Key bytes    | 0 <= kk <= 32    |
//!  Input bytes  | 0 <= ll < 2**64  |
//! --------------+------------------+
//!  G Rotation   | (R1, R2, R3, R4) |
//!   constants = | (16, 12,  8,  7) |
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
// BLAKE2 CONSTANTS
// ----------------

// Initialisation Vector (IV)
const IV: [u32; 8] = [
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
];

// Pallas modulus words (little-endian, word_1 is least significant):
// p = 0x40000000_00000000_00000000_00000000_224698fc_094cf91b_992d30ed_00000001
// This is used for canonicality checks to ensure field decomposition is unique.
const PALLAS_MODULUS_WORDS: [u32; 8] = [
    0x00000001, // word_1 (bits 0-31)
    0x992d30ed, // word_2 (bits 32-63)
    0x094cf91b, // word_3 (bits 64-95)
    0x224698fc, // word_4 (bits 96-127)
    0x00000000, // word_5 (bits 128-159)
    0x00000000, // word_6 (bits 160-191)
    0x00000000, // word_7 (bits 192-223)
    0x40000000, // word_8 (bits 224-255)
];

// Lower 128 bits of pallas modulus as a u128 for comparison
// p_lower = 0x224698fc_094cf91b_992d30ed_00000001
const PALLAS_MODULUS_LOWER_128: u128 = 0x224698fc_094cf91b_992d30ed_00000001;

// The SIGMA constant in Blake2s is a 10x16 array that defines the message permutations in the
// algorithm. Each of the 10 rows corresponds to a round of the hashing process, and each of the
// 16 elements in the row determines the message block order.
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

// G Rotation constants
const R1: usize = 16;
const R2: usize = 12;
const R3: usize = 8;
const R4: usize = 7;

const ROUNDS: usize = 10;

// ---------------

/// BLAKE2s chip for halo2.
#[derive(Clone, Debug)]
pub struct Blake2sChip<F: PrimeField> {
    config: Blake2sConfig<F>,
    _marker: PhantomData<F>,
}

/// Configuration for the BLAKE2s chip.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Blake2sConfig<F: PrimeField> {
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
    _marker: PhantomData<F>,
}

/// One blockword has 4 bytes (32 bits).
#[derive(Clone, Debug)]
pub struct Blake2sWord<F: PrimeField> {
    word: AssignedCell<F, F>,
    bits: [AssignedCell<F, F>; 32],
}

/// One byte has 8 bits.
#[derive(Clone, Debug)]
struct Blake2sByte<F: PrimeField> {
    byte: AssignedCell<F, F>,
    bits: [AssignedCell<F, F>; 8],
}

impl<F: PrimeField> Blake2sByte<F> {
    pub fn get_byte(&self) -> AssignedCell<F, F> {
        self.byte.clone()
    }

    pub fn get_bits(&self) -> &[AssignedCell<F, F>; 8] {
        &self.bits
    }

    pub fn from_u8(
        value: Value<u8>,
        mut layouter: impl Layouter<F>,
        config: &Blake2sConfig<F>,
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
                    config.advices[0],
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

    pub fn from_constant_u8(
        value: u8,
        layouter: &mut impl Layouter<F>,
        config: &Blake2sConfig<F>,
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
                    config.advices[0],
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

impl<F: PrimeField> Blake2sConfig<F> {
    /// Configure the BLAKE2s chip.
    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        advices: [Column<Advice>; 10],
    ) -> Blake2sConfig<F> {
        let s_field_decompose = meta.selector();
        let s_word_decompose = meta.selector();
        let s_byte_decompose = meta.selector();
        let s_byte_xor = meta.selector();
        let s_word_add = meta.selector();
        let s_result_encode = meta.selector();
        let s_canonicality = meta.selector();
        let s_high_bit_zero = meta.selector();

        meta.create_gate("decompose field to words", |meta| {
            let field_element = meta.query_advice(advices[0], Rotation::next());
            let word_1 = meta.query_advice(advices[0], Rotation::cur());
            let word_2 = meta.query_advice(advices[1], Rotation::cur());
            let word_3 = meta.query_advice(advices[2], Rotation::cur());
            let word_4 = meta.query_advice(advices[3], Rotation::cur());
            let word_5 = meta.query_advice(advices[4], Rotation::cur());
            let word_6 = meta.query_advice(advices[5], Rotation::cur());
            let word_7 = meta.query_advice(advices[6], Rotation::cur());
            let word_8 = meta.query_advice(advices[7], Rotation::cur());
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

        meta.create_gate("decompose word to bytes", |meta| {
            let word = meta.query_advice(advices[0], Rotation::next());
            let byte_1 = meta.query_advice(advices[0], Rotation::cur());
            let byte_2 = meta.query_advice(advices[1], Rotation::cur());
            let byte_3 = meta.query_advice(advices[2], Rotation::cur());
            let byte_4 = meta.query_advice(advices[3], Rotation::cur());
            let s_word_decompose = meta.query_selector(s_word_decompose);

            vec![
                s_word_decompose
                    * (byte_1
                        + byte_2 * F::from(1 << 8)
                        + byte_3 * F::from(1 << 16)
                        + byte_4 * F::from(1 << 24)
                        - word),
            ]
        });

        meta.create_gate("decompose byte to bits", |meta| {
            let byte = meta.query_advice(advices[0], Rotation::next());
            let bit_1 = meta.query_advice(advices[0], Rotation::cur());
            let bit_2 = meta.query_advice(advices[1], Rotation::cur());
            let bit_3 = meta.query_advice(advices[2], Rotation::cur());
            let bit_4 = meta.query_advice(advices[3], Rotation::cur());
            let bit_5 = meta.query_advice(advices[4], Rotation::cur());
            let bit_6 = meta.query_advice(advices[5], Rotation::cur());
            let bit_7 = meta.query_advice(advices[6], Rotation::cur());
            let bit_8 = meta.query_advice(advices[7], Rotation::cur());
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

            // SOUNDNESS FIX: Each bit must be boolean (0 or 1)
            // Without these constraints, a malicious prover could use invalid
            // values that still satisfy the decomposition equation.
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

        meta.create_gate("word add", |meta| {
            let s_word_add = meta.query_selector(s_word_add);
            let lhs = meta.query_advice(advices[0], Rotation::cur());
            let rhs = meta.query_advice(advices[1], Rotation::cur());
            let out = meta.query_advice(advices[0], Rotation::next());
            let carry = meta.query_advice(advices[1], Rotation::next());
            let equal = lhs + rhs - carry.clone() * F::from(1 << 32) - out;

            Constraints::with_selector(
                s_word_add,
                [
                    ("carry bool check", bool_check(carry)),
                    ("equal check", equal),
                ],
            )
        });

        meta.create_gate("encode four words to one field", |meta| {
            let field_element = meta.query_advice(advices[0], Rotation::next());
            let word_1 = meta.query_advice(advices[0], Rotation::cur());
            let word_2 = meta.query_advice(advices[1], Rotation::cur());
            let word_3 = meta.query_advice(advices[2], Rotation::cur());
            let word_4 = meta.query_advice(advices[3], Rotation::cur());
            let s_result_encode = meta.query_selector(s_result_encode);

            vec![
                s_result_encode
                    * (word_1
                        + word_2 * F::from(1 << 32)
                        + word_3 * F::from_u128(1 << 64)
                        + word_4 * F::from_u128(1 << 96)
                        - field_element),
            ]
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
            let bit_255 = meta.query_advice(advices[0], Rotation::cur());
            let bit_254 = meta.query_advice(advices[1], Rotation::cur());

            // The diff value and its decomposition
            let lower_128_diff = meta.query_advice(advices[2], Rotation::cur());
            let diff_word_1 = meta.query_advice(advices[3], Rotation::cur());
            let diff_word_2 = meta.query_advice(advices[4], Rotation::cur());
            let diff_word_3 = meta.query_advice(advices[5], Rotation::cur());
            let diff_word_4 = meta.query_advice(advices[6], Rotation::cur());

            // The 4 words that make up lower_128 (copied from actual word cells)
            let word_1 = meta.query_advice(advices[7], Rotation::cur());
            let word_2 = meta.query_advice(advices[8], Rotation::cur());
            let word_3 = meta.query_advice(advices[9], Rotation::cur());
            let word_4 = meta.query_advice(advices[0], Rotation::next());

            // The computed lower_128 value
            let lower_128 = meta.query_advice(advices[1], Rotation::next());

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
            let diff_check = bit_254.clone()
                * (lower_128_diff.clone() - (p_lower - one - lower_128));

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

            let bit_254 = meta.query_advice(advices[0], Rotation::cur());
            let bit_to_check = meta.query_advice(advices[1], Rotation::cur());

            // If bit_254 = 1, then bit_to_check must be 0
            // If bit_254 = 0, this constraint is satisfied for any bit value
            Constraints::with_selector(s_high_bit_zero, [("bit_254 * bit = 0", bit_254 * bit_to_check)])
        });

        Blake2sConfig {
            advices,
            s_field_decompose,
            s_word_decompose,
            s_byte_decompose,
            s_byte_xor,
            s_word_add,
            s_result_encode,
            s_canonicality,
            s_high_bit_zero,
            _marker: PhantomData,
        }
    }
}

impl<F: PrimeField> Blake2sChip<F> {
    /// Construct a new BLAKE2s chip from the given config.
    pub fn construct(config: Blake2sConfig<F>) -> Self {
        Self {
            config,
            _marker: PhantomData,
        }
    }

    /// Process the inputs and return the hash result as 8 words.
    ///
    /// # Arguments
    /// * `layouter` - The circuit layouter
    /// * `inputs` - The input field elements (must be even length)
    /// * `personalization` - 8-byte personalization string
    pub fn process(
        &self,
        layouter: &mut impl Layouter<F>,
        inputs: &[AssignedCell<F, F>],
        personalization: &[u8],
    ) -> Result<Vec<Blake2sWord<F>>, Error> {
        assert_eq!(personalization.len(), 8);
        assert!(inputs.len() % 2 == 0);

        // Init
        let mut h = vec![
            Blake2sWord::from_constant_u32(IV[0] ^ 0x01010000 ^ 32, layouter, self)?,
            Blake2sWord::from_constant_u32(IV[1], layouter, self)?,
            Blake2sWord::from_constant_u32(IV[2], layouter, self)?,
            Blake2sWord::from_constant_u32(IV[3], layouter, self)?,
            Blake2sWord::from_constant_u32(IV[4], layouter, self)?,
            Blake2sWord::from_constant_u32(IV[5], layouter, self)?,
            Blake2sWord::from_constant_u32(
                IV[6] ^ LittleEndian::read_u32(&personalization[0..4]),
                layouter,
                self,
            )?,
            Blake2sWord::from_constant_u32(
                IV[7] ^ LittleEndian::read_u32(&personalization[4..8]),
                layouter,
                self,
            )?,
        ];

        // Handle message: convert field message to blocks.
        let mut blocks = vec![];
        for block in inputs.chunks(2) {
            let mut cur_block = Vec::with_capacity(16);
            for field in block.iter() {
                let mut words = self.field_decompose(layouter, field)?;
                cur_block.append(&mut words);
            }
            blocks.push(cur_block);
        }

        if blocks.is_empty() {
            let zero_padding_block = (0..16)
                .map(|_| Blake2sWord::from_constant_u32(0, layouter, self).unwrap())
                .collect();
            blocks.push(zero_padding_block);
        }

        let block_len = blocks.len();

        for (i, block) in blocks[0..(block_len - 1)].iter().enumerate() {
            self.compress(layouter, &mut h, block, (i as u64 + 1) * 64, false)?;
        }

        // Compress(Final block)
        self.compress(
            layouter,
            &mut h,
            &blocks[block_len - 1],
            (block_len as u64) * 64,
            true,
        )?;

        Ok(h)
    }

    /// Encode the eight words to two field elements.
    pub fn encode_result(
        &self,
        layouter: &mut impl Layouter<F>,
        ret: &[Blake2sWord<F>],
    ) -> Result<[AssignedCell<F, F>; 2], Error> {
        let mut fields = vec![];
        assert_eq!(ret.len(), 8);
        for words in ret.chunks(4) {
            let field = layouter.assign_region(
                || "encode four words to one field",
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
                    let word_values: Value<Vec<_>> =
                        words.iter().map(|word| word.get_word().value()).collect();
                    let field_value = word_values.map(|words| {
                        words
                            .into_iter()
                            .rev()
                            .fold(F::ZERO, |acc, byte| acc * F::from(1 << 32) + byte)
                    });
                    region.assign_advice(
                        || "result field",
                        self.config.advices[0],
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
        h: &mut [Blake2sWord<F>], // current state
        m: &[Blake2sWord<F>],     // current block
        t: u64,                   // offset counter
        f: bool,                  // final flag
    ) -> Result<(), Error> {
        let mut v = Vec::with_capacity(16);
        v.extend_from_slice(h);
        for iv in IV[0..4].iter() {
            let word = Blake2sWord::from_constant_u32(*iv, layouter, self)?;
            v.push(word);
        }
        // v[12] := v[12] ^ (t mod 2**w)
        let v_12 = Blake2sWord::from_constant_u32(IV[4] ^ (t as u32), layouter, self)?;
        v.push(v_12);

        // v[13] := v[13] ^ (t >> w)
        let v_13 = Blake2sWord::from_constant_u32(IV[5] ^ ((t >> 32) as u32), layouter, self)?;
        v.push(v_13);

        // IF f = TRUE THEN                // last block flag?
        // |   v[14] := v[14] ^ 0xFF..FF   // Invert all bits.
        // END IF.
        let v_14 = if f {
            Blake2sWord::from_constant_u32(IV[6] ^ u32::MAX, layouter, self)?
        } else {
            Blake2sWord::from_constant_u32(IV[6], layouter, self)?
        };
        v.push(v_14);

        // v_15
        let v_15 = Blake2sWord::from_constant_u32(IV[7], layouter, self)?;
        v.push(v_15);
        assert_eq!(v.len(), 16);

        for i in 0..ROUNDS {
            let s = SIGMA[i % ROUNDS];
            self.g(
                layouter.namespace(|| "mixing 1"),
                &mut v,
                (0, 4, 8, 12),
                &m[s[0]],
                &m[s[1]],
            )?;
            self.g(
                layouter.namespace(|| "mixing 2"),
                &mut v,
                (1, 5, 9, 13),
                &m[s[2]],
                &m[s[3]],
            )?;
            self.g(
                layouter.namespace(|| "mixing 3"),
                &mut v,
                (2, 6, 10, 14),
                &m[s[4]],
                &m[s[5]],
            )?;
            self.g(
                layouter.namespace(|| "mixing 4"),
                &mut v,
                (3, 7, 11, 15),
                &m[s[6]],
                &m[s[7]],
            )?;

            self.g(
                layouter.namespace(|| "mixing 5"),
                &mut v,
                (0, 5, 10, 15),
                &m[s[8]],
                &m[s[9]],
            )?;
            self.g(
                layouter.namespace(|| "mixing 6"),
                &mut v,
                (1, 6, 11, 12),
                &m[s[10]],
                &m[s[11]],
            )?;
            self.g(
                layouter.namespace(|| "mixing 7"),
                &mut v,
                (2, 7, 8, 13),
                &m[s[12]],
                &m[s[13]],
            )?;
            self.g(
                layouter.namespace(|| "mixing 8"),
                &mut v,
                (3, 4, 9, 14),
                &m[s[14]],
                &m[s[15]],
            )?;
        }

        // Finalize the state
        for i in 0..8 {
            let h_i_bits = self.word_xor(
                layouter.namespace(|| "final first xor"),
                h[i].get_bits(),
                v[i].get_bits(),
            )?;
            let h_i_bits = self.word_xor(
                layouter.namespace(|| "final second xor"),
                &h_i_bits,
                v[i + 8].get_bits(),
            )?;
            h[i] = Blake2sWord::from_bits(
                self,
                layouter.namespace(|| "construct word from bits"),
                h_i_bits,
            )?;
        }

        Ok(())
    }

    /// The G primitive function mixes two input words, "x" and "y", into
    /// four words indexed by "a", "b", "c", and "d" in the working vector
    /// v[0..15].  The full modified vector is returned.  The rotation
    /// constants are (R1, R2, R3, R4) = (16, 12, 8, 7) for BLAKE2s.
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
        v: &mut [Blake2sWord<F>],
        (a, b, c, d): (usize, usize, usize, usize),
        x: &Blake2sWord<F>,
        y: &Blake2sWord<F>,
    ) -> Result<(), Error> {
        // v[a] := (v[a] + v[b] + x) mod 2**w
        v[a] = {
            let sum_a_b = self.add_mod_u32(
                layouter.namespace(|| "add_mod_u32"),
                v[a].get_word(),
                v[b].get_word(),
            )?;
            let sum_a_b_x =
                self.add_mod_u32(layouter.namespace(|| "add_mod_u32"), &sum_a_b, x.get_word())?;
            Blake2sWord::from_word(self, layouter.namespace(|| "from word"), sum_a_b_x)?
        };

        // v[d] := (v[d] ^ v[a]) >>> R1
        v[d] = {
            let d_xor_a = self.word_xor(
                layouter.namespace(|| "xor"),
                v[d].get_bits(),
                v[a].get_bits(),
            )?;
            let bits = Blake2sWord::word_rotate(&d_xor_a, R1);
            Blake2sWord::from_bits(self, layouter.namespace(|| "from bits"), bits)?
        };

        // v[c] := (v[c] + v[d])     mod 2**w
        v[c] = {
            let sum = self.add_mod_u32(
                layouter.namespace(|| "add_mod_u32"),
                v[c].get_word(),
                v[d].get_word(),
            )?;
            Blake2sWord::from_word(self, layouter.namespace(|| "from word"), sum)?
        };

        // v[b] := (v[b] ^ v[c]) >>> R2
        v[b] = {
            let b_xor_c = self.word_xor(
                layouter.namespace(|| "xor"),
                v[b].get_bits(),
                v[c].get_bits(),
            )?;
            let bits = Blake2sWord::word_rotate(&b_xor_c, R2);
            Blake2sWord::from_bits(self, layouter.namespace(|| "from bits"), bits)?
        };

        // v[a] := (v[a] + v[b] + y) mod 2**w
        v[a] = {
            let sum_a_b = self.add_mod_u32(
                layouter.namespace(|| "add_mod_u32"),
                v[a].get_word(),
                v[b].get_word(),
            )?;
            let sum_a_b_y =
                self.add_mod_u32(layouter.namespace(|| "add_mod_u32"), &sum_a_b, y.get_word())?;
            Blake2sWord::from_word(self, layouter.namespace(|| "from word"), sum_a_b_y)?
        };

        // v[d] := (v[d] ^ v[a]) >>> R3
        v[d] = {
            let d_xor_a = self.word_xor(
                layouter.namespace(|| "xor"),
                v[d].get_bits(),
                v[a].get_bits(),
            )?;
            let bits = Blake2sWord::word_rotate(&d_xor_a, R3);
            Blake2sWord::from_bits(self, layouter.namespace(|| "from bits"), bits)?
        };

        // v[c] := (v[c] + v[d])     mod 2**w
        v[c] = {
            let sum = self.add_mod_u32(
                layouter.namespace(|| "add_mod_u32"),
                v[c].get_word(),
                v[d].get_word(),
            )?;
            Blake2sWord::from_word(self, layouter.namespace(|| "from word"), sum)?
        };

        // v[b] := (v[b] ^ v[c]) >>> R4
        v[b] = {
            let b_xor_c = self.word_xor(
                layouter.namespace(|| "xor"),
                v[b].get_bits(),
                v[c].get_bits(),
            )?;
            let bits = Blake2sWord::word_rotate(&b_xor_c, R4);
            Blake2sWord::from_bits(self, layouter.namespace(|| "from bits"), bits)?
        };

        Ok(())
    }

    /// Decompose a field element to words.
    fn field_decompose(
        &self,
        layouter: &mut impl Layouter<F>,
        field: &AssignedCell<F, F>,
    ) -> Result<Vec<Blake2sWord<F>>, Error> {
        // the decomposition from bytes to bits
        let mut bits = vec![];
        let mut bytes = vec![];
        for i in 0..32 {
            let byte_value = field.value().map(|f| f.to_repr().as_ref()[i]);
            let byte =
                Blake2sByte::from_u8(byte_value, layouter.namespace(|| "from_u8"), &self.config)?;
            bits.append(&mut byte.get_bits().to_vec());
            bytes.push(byte.get_byte());
        }

        // Check the decomposition from words to bytes
        let mut words = vec![];
        for bytes in bytes.chunks(4) {
            let word = {
                let byte_values: Value<Vec<_>> = bytes.iter().map(|byte| byte.value()).collect();
                let word_value = byte_values.map(|bytes| {
                    bytes
                        .into_iter()
                        .rev()
                        .fold(F::ZERO, |acc, byte| acc * F::from(1 << 8) + byte)
                });
                assign_free_advice(
                    layouter.namespace(|| "assign word"),
                    self.config.advices[8],
                    word_value,
                )?
            };
            self.word_decompose(layouter.namespace(|| "word decompose"), bytes, &word)?;
            words.push(word);
        }

        // check the decomposition from field to words
        layouter.assign_region(
            || "decompose field to words",
            |mut region| {
                self.config.s_field_decompose.enable(&mut region, 0)?;
                for (i, word) in words.iter().enumerate() {
                    word.copy_advice(|| "word", &mut region, self.config.advices[i], 0)?;
                }
                field.copy_advice(|| "field", &mut region, self.config.advices[0], 1)?;
                Ok(())
            },
        )?;

        // SOUNDNESS FIX: Canonicality check
        // Ensure the 256-bit decomposition represents a value strictly less than p.
        // Without this, a prover could use the non-canonical representation (value + p).
        // We pass both the bits (for direct bit constraints) and words (for lower_128 computation).
        self.check_canonicality(layouter, &bits, &words)?;

        let res = bits
            .chunks(32)
            .zip(words)
            .map(|(bits, word)| Blake2sWord {
                word,
                bits: bits.to_vec().try_into().unwrap(),
            })
            .collect::<Vec<_>>();

        Ok(res)
    }

    /// Check that the 256-bit decomposition is canonical (strictly less than p).
    ///
    /// This is a critical soundness check. Without it, a malicious prover could
    /// decompose a field element f as either f or f+p (both satisfy the mod-p
    /// constraint), leading to different BLAKE2s outputs for the "same" field value.
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

        // Compute lower 128 bits as a field element
        // lower_128 = word_1 + word_2 * 2^32 + word_3 * 2^64 + word_4 * 2^96
        let lower_128: Value<u128> = words[0]
            .value()
            .zip(words[1].value())
            .zip(words[2].value())
            .zip(words[3].value())
            .map(|(((w1, w2), w3), w4)| {
                let to_u32 = |f: &F| -> u32 {
                    let repr = f.to_repr();
                    let bytes = repr.as_ref();
                    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
                };
                (to_u32(w1) as u128)
                    + ((to_u32(w2) as u128) << 32)
                    + ((to_u32(w3) as u128) << 64)
                    + ((to_u32(w4) as u128) << 96)
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

        // Assign the canonicality check region and get back the diff_word cells
        // for range checking. All bits and words are copied via copy_advice.
        let diff_word_cells = layouter.assign_region(
            || "canonicality check",
            |mut region| {
                self.config.s_canonicality.enable(&mut region, 0)?;

                // Row 0: bit_255, bit_254, lower_128_diff, diff_w1..diff_w4, w1, w2, w3
                // Use copy_advice to constrain these to the actual bit/word cells
                bit_255.copy_advice(|| "bit_255", &mut region, self.config.advices[0], 0)?;
                bit_254.copy_advice(|| "bit_254", &mut region, self.config.advices[1], 0)?;

                // Witness the diff value
                let diff_field = diff.map(|d| F::from_u128(d));
                region.assign_advice(|| "lower_128_diff", self.config.advices[2], 0, || diff_field)?;

                // Assign diff words and collect the cells for later range checking
                let mut diff_cells = Vec::with_capacity(4);
                for (i, dw) in diff_word_values.iter().enumerate() {
                    let cell = region.assign_advice(
                        || format!("diff_word_{}", i + 1),
                        self.config.advices[3 + i],
                        0,
                        || dw.map(|w| F::from(w as u64)),
                    )?;
                    diff_cells.push(cell);
                }

                // Copy words[0..4] using copy_advice to constrain lower_128
                words[0].copy_advice(|| "word_1", &mut region, self.config.advices[7], 0)?;
                words[1].copy_advice(|| "word_2", &mut region, self.config.advices[8], 0)?;
                words[2].copy_advice(|| "word_3", &mut region, self.config.advices[9], 0)?;

                // Row 1: word_4, lower_128
                words[3].copy_advice(|| "word_4", &mut region, self.config.advices[0], 1)?;

                // Assign lower_128 - this is constrained by the gate to equal
                // word_1 + word_2*2^32 + word_3*2^64 + word_4*2^96
                let lower_128_field = lower_128.map(|l| F::from_u128(l));
                region.assign_advice(|| "lower_128", self.config.advices[1], 1, || lower_128_field)?;

                Ok(diff_cells)
            },
        )?;

        // Apply s_high_bit_zero gate to each bit in [128..254]
        // This constrains: bit_254 * bit[i] = 0 for each bit
        // Using copy_advice ensures we're checking the actual bits
        for (i, bit) in bits[128..254].iter().enumerate() {
            layouter.assign_region(
                || format!("high bit zero check {}", i),
                |mut region| {
                    self.config.s_high_bit_zero.enable(&mut region, 0)?;
                    bit_254.copy_advice(|| "bit_254", &mut region, self.config.advices[0], 0)?;
                    bit.copy_advice(|| "bit_to_check", &mut region, self.config.advices[1], 0)?;
                    Ok(())
                },
            )?;
        }

        // Range-check each diff word by decomposing it to bytes
        // This ensures diff is in [0, 2^128 - 1], proving lower_128 < p_lower
        // The diff_word_cells are the SAME cells from the canonicality region
        for (i, (dw_val, dw_cell)) in diff_word_values.iter().zip(diff_word_cells.iter()).enumerate()
        {
            // Decompose each diff word into 4 bytes, then each byte into bits
            // The bit boolean constraints will ensure each word is in [0, 2^32 - 1]
            let mut diff_bytes = Vec::with_capacity(4);
            for j in 0..4 {
                let byte_val = dw_val.map(|w| ((w >> (j * 8)) & 0xFF) as u8);
                let byte = Blake2sByte::from_u8(
                    byte_val,
                    layouter.namespace(|| format!("diff_word_{}_byte_{}", i, j)),
                    &self.config,
                )?;
                diff_bytes.push(byte.get_byte());
            }

            // Use the SAME diff_word cell from the canonicality region
            // This ensures the range-checked word is the same as the one in the constraint
            self.word_decompose(
                layouter.namespace(|| format!("diff_word_{}_decompose", i)),
                &diff_bytes,
                dw_cell,
            )?;
        }

        Ok(())
    }

    /// Decompose a word to four bytes.
    fn word_decompose(
        &self,
        mut layouter: impl Layouter<F>,
        bytes: &[AssignedCell<F, F>],
        word: &AssignedCell<F, F>,
    ) -> Result<(), Error> {
        assert_eq!(bytes.len(), 4);
        layouter.assign_region(
            || "decompose word to bytes",
            |mut region| {
                self.config.s_word_decompose.enable(&mut region, 0)?;
                for (i, byte) in bytes.iter().enumerate() {
                    byte.copy_advice(|| "byte", &mut region, self.config.advices[i], 0)?;
                }
                word.copy_advice(|| "word", &mut region, self.config.advices[0], 1)?;
                Ok(())
            },
        )
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
                byte.copy_advice(|| "byte", &mut region, self.config.advices[0], 1)?;
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

    fn word_xor(
        &self,
        mut layouter: impl Layouter<F>,
        x: &[AssignedCell<F, F>],
        y: &[AssignedCell<F, F>],
    ) -> Result<Vec<AssignedCell<F, F>>, Error> {
        assert_eq!(x.len(), 32);
        assert_eq!(y.len(), 32);
        let mut bits = Vec::with_capacity(32);
        for (x_byte, y_byte) in x.chunks(8).zip(y.chunks(8)) {
            let mut ret = self.byte_xor(layouter.namespace(|| "byte xor"), x_byte, y_byte)?;
            bits.append(&mut ret);
        }

        Ok(bits)
    }

    fn add_mod_u32(
        &self,
        mut layouter: impl Layouter<F>,
        // x and y must be a word variable
        x: &AssignedCell<F, F>,
        y: &AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, Error> {
        layouter.assign_region(
            || "decompose bytes to bits",
            |mut region| {
                self.config.s_word_add.enable(&mut region, 0)?;
                x.copy_advice(|| "word_add x", &mut region, self.config.advices[0], 0)?;
                y.copy_advice(|| "word_add y", &mut region, self.config.advices[1], 0)?;
                let sum = x.value().zip(y.value()).map(|(&x, &y)| {
                    let sum = x + y;
                    let carry = F::from(sum.to_repr().as_ref()[4] as u64);
                    let ret = sum - carry * F::from(1 << 32);
                    (ret, carry)
                });
                let ret = region.assign_advice(
                    || "word_add ret",
                    self.config.advices[0],
                    1,
                    || sum.map(|sum| sum.0),
                )?;
                region.assign_advice(
                    || "word_add carry",
                    self.config.advices[1],
                    1,
                    || sum.map(|sum| sum.1),
                )?;
                Ok(ret)
            },
        )
    }
}

impl<F: PrimeField> Blake2sWord<F> {
    /// Create a Blake2sWord from a constant u32 value.
    pub fn from_constant_u32(
        value: u32,
        layouter: &mut impl Layouter<F>,
        chip: &Blake2sChip<F>,
    ) -> Result<Self, Error> {
        let mut bytes = Vec::with_capacity(4);
        let mut word_bits = Vec::with_capacity(32);
        let mut tmp = value;
        for _ in 0..4 {
            let input_byte = tmp as u8;
            let byte = Blake2sByte::from_constant_u8(input_byte, layouter, &chip.config)?;
            bytes.push(byte.get_byte());
            word_bits.append(&mut byte.get_bits().to_vec());
            tmp >>= 8;
        }
        let word = assign_free_constant(
            layouter.namespace(|| "constant word"),
            chip.config.advices[0],
            F::from(value as u64),
        )?;
        chip.word_decompose(layouter.namespace(|| "word decompose"), &bytes, &word)?;
        Ok(Self {
            word,
            bits: word_bits.try_into().unwrap(),
        })
    }

    /// Rotate word bits to the right by `by` positions.
    pub fn word_rotate(bits: &[AssignedCell<F, F>], by: usize) -> Vec<AssignedCell<F, F>> {
        assert!(bits.len() == 32);
        let by = by % 32;
        bits.iter()
            .skip(by)
            .chain(bits.iter())
            .take(32)
            .cloned()
            .collect()
    }

    /// Shift word bits to the right by `by` positions (with zero fill).
    pub fn shift(
        &self,
        by: usize,
        mut layouter: impl Layouter<F>,
        advice: Column<Advice>,
    ) -> Result<Vec<AssignedCell<F, F>>, Error> {
        let by = by % 32;
        let padding_zero = assign_free_constant(layouter.namespace(|| "zero"), advice, F::from(0))?;
        let old_bits = self.get_bits();
        Ok(old_bits
            .iter()
            .skip(by)
            .chain(Some(&padding_zero).into_iter().cycle())
            .take(32)
            .cloned()
            .collect())
    }

    /// Get the bits of this word.
    pub fn get_bits(&self) -> &[AssignedCell<F, F>; 32] {
        &self.bits
    }

    /// Get the word value.
    pub fn get_word(&self) -> &AssignedCell<F, F> {
        &self.word
    }

    /// Create a Blake2sWord from bits.
    pub fn from_bits(
        chip: &Blake2sChip<F>,
        mut layouter: impl Layouter<F>,
        bits: Vec<AssignedCell<F, F>>,
    ) -> Result<Self, Error> {
        assert!(bits.len() == 32);
        let mut bytes = Vec::with_capacity(4);
        for bits in bits.chunks(8) {
            let bit_values: Value<Vec<_>> = bits.iter().map(|bit| bit.value()).collect();
            let byte_value = bit_values.map(|bits| {
                bits.into_iter()
                    .rev()
                    .fold(F::ZERO, |acc, bit| acc * F::from(2) + bit)
            });
            let byte = assign_free_advice(
                layouter.namespace(|| "assign byte"),
                chip.config.advices[8],
                byte_value,
            )?;
            chip.byte_decompose(layouter.namespace(|| "byte decompose"), bits, &byte)?;
            bytes.push(byte);
        }
        let word = {
            let byte_values: Value<Vec<_>> = bytes.iter().map(|byte| byte.value()).collect();
            let word_value = byte_values.map(|bytes| {
                bytes
                    .into_iter()
                    .rev()
                    .fold(F::ZERO, |acc, byte| acc * F::from(1 << 8) + byte)
            });
            assign_free_advice(
                layouter.namespace(|| "assign word"),
                chip.config.advices[8],
                word_value,
            )?
        };
        chip.word_decompose(layouter.namespace(|| "word decompose"), &bytes, &word)?;
        Ok(Self {
            word,
            bits: bits.try_into().unwrap(),
        })
    }

    /// Create a Blake2sWord from an assigned word.
    pub fn from_word(
        chip: &Blake2sChip<F>,
        mut layouter: impl Layouter<F>,
        word: AssignedCell<F, F>,
    ) -> Result<Self, Error> {
        let mut bytes = Vec::with_capacity(4);
        let mut bits = Vec::with_capacity(32);
        for i in 0..4 {
            let byte_value = word.value().map(|v| v.to_repr().as_ref()[i]);
            let byte =
                Blake2sByte::from_u8(byte_value, layouter.namespace(|| "from_u8"), &chip.config)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use halo2_proofs::{
        circuit::{floor_planner, Layouter, Value},
        dev::MockProver,
        plonk::{Circuit, ConstraintSystem, Error},
    };
    use pasta_curves::pallas;

    #[derive(Default)]
    struct Blake2sTestCircuit {
        input1: Value<pallas::Base>,
        input2: Value<pallas::Base>,
    }

    impl Circuit<pallas::Base> for Blake2sTestCircuit {
        type Config = Blake2sConfig<pallas::Base>;
        type FloorPlanner = floor_planner::V1;

        fn without_witnesses(&self) -> Self {
            Self::default()
        }

        fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
            let advices = [
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
            ];

            for advice in advices.iter() {
                meta.enable_equality(*advice);
            }

            let constants = meta.fixed_column();
            meta.enable_constant(constants);
            Blake2sConfig::configure(meta, advices)
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<pallas::Base>,
        ) -> Result<(), Error> {
            let input1 = assign_free_advice(
                layouter.namespace(|| "input1"),
                config.advices[0],
                self.input1,
            )?;

            let input2 = assign_free_advice(
                layouter.namespace(|| "input2"),
                config.advices[0],
                self.input2,
            )?;

            let blake2s_chip = Blake2sChip::construct(config);
            let _result = blake2s_chip.process(
                &mut layouter,
                &[input1, input2],
                b"ZcshTest", // 8-byte personalization
            )?;

            Ok(())
        }
    }

    #[test]
    fn test_blake2s_circuit() {
        let circuit = Blake2sTestCircuit {
            input1: Value::known(pallas::Base::from(1u64)),
            input2: Value::known(pallas::Base::from(2u64)),
        };

        let k = 14;
        let prover = MockProver::run(k, &circuit, vec![]).unwrap();
        assert_eq!(prover.verify(), Ok(()));
    }
}
