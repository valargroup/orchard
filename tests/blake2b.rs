//! Integration tests for the BLAKE2b circuit.

#![cfg(feature = "circuit")]

use ff::{Field, PrimeField};
use halo2_proofs::{
    circuit::{floor_planner, Layouter, Value},
    dev::MockProver,
    plonk::{Circuit, Column, ConstraintSystem, Error, Instance},
};
use orchard::circuit::blake2b::{assign_free_advice, Blake2bChip, Blake2bConfig};
use pasta_curves::pallas;

/// Test vector data from Zcash test vectors for compact action hash testing.
/// Source: orchard_note_encryption test vectors
mod compact_test_data {
    // Test vector 0 from note_encryption.rs
    pub const NF_OLD: [u8; 32] = [
        0xc5, 0x96, 0xfb, 0xd3, 0x2e, 0xbb, 0xcb, 0xad, 0xae, 0x60, 0xd2, 0x85, 0xc7, 0xd7, 0x5f,
        0xa8, 0x36, 0xf9, 0xd2, 0xfa, 0x86, 0x10, 0x0a, 0xb8, 0x58, 0xea, 0x2d, 0xe1, 0xf1, 0x1c,
        0x83, 0x06,
    ];

    pub const CMX: [u8; 32] = [
        0xa5, 0x70, 0x6f, 0x3d, 0x1b, 0x68, 0x8e, 0x9d, 0xc6, 0x34, 0xee, 0xe4, 0xe6, 0x5b, 0x02,
        0x8a, 0x43, 0xee, 0xae, 0xd2, 0x43, 0x5b, 0xea, 0x2a, 0xe3, 0xd5, 0x16, 0x05, 0x75, 0xc1,
        0x1a, 0x3b,
    ];

    pub const EPHEMERAL_KEY: [u8; 32] = [
        0xad, 0xdb, 0x47, 0xb6, 0xac, 0x5d, 0xfc, 0x16, 0x55, 0x89, 0x23, 0xd3, 0xa8, 0xf3, 0x76,
        0x09, 0x5c, 0x69, 0x5c, 0x04, 0x7c, 0x4e, 0x32, 0x66, 0xae, 0x67, 0x69, 0x87, 0xf7, 0xe3,
        0x13, 0x81,
    ];

    // First 52 bytes of c_enc
    pub const C_ENC_PREFIX: [u8; 52] = [
        0x1a, 0x9a, 0xdb, 0x14, 0x24, 0x98, 0xe3, 0xdc, 0xc7, 0x6f, 0xed, 0x77, 0x86, 0x14, 0xdd,
        0x31, 0x6c, 0x02, 0xfb, 0xb8, 0xba, 0x92, 0x44, 0xae, 0x4c, 0x2e, 0x32, 0xa0, 0x7d, 0xae,
        0xec, 0xa4, 0x12, 0x26, 0xb9, 0x8b, 0xfe, 0x74, 0xf9, 0xfc, 0xb2, 0x28, 0xcf, 0xc1, 0x00,
        0xf3, 0x18, 0x0f, 0x57, 0x75, 0xec, 0xe3,
    ];
}

#[derive(Default)]
struct Blake2bTestCircuit {
    input1: Value<pallas::Base>,
    input2: Value<pallas::Base>,
}

impl Circuit<pallas::Base> for Blake2bTestCircuit {
    type Config = Blake2bConfig<pallas::Base>;
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
        Blake2bConfig::configure(meta, advices)
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

        let blake2b_chip = Blake2bChip::construct(config);
        let _result =
            blake2b_chip.process(&mut layouter, &[input1, input2], b"ZcshBlake2bTest!")?;

        Ok(())
    }
}

#[test]
fn test_blake2b_circuit() {
    let circuit = Blake2bTestCircuit {
        input1: Value::known(pallas::Base::from(1u64)),
        input2: Value::known(pallas::Base::from(2u64)),
    };

    let k = 17;
    let prover = MockProver::run(k, &circuit, vec![]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
}

#[test]
fn test_blake2b_empty_input() {
    // Test with empty input (zero padding)
    #[derive(Default)]
    struct EmptyInputCircuit;

    impl Circuit<pallas::Base> for EmptyInputCircuit {
        type Config = Blake2bConfig<pallas::Base>;
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
            Blake2bConfig::configure(meta, advices)
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<pallas::Base>,
        ) -> Result<(), Error> {
            let blake2b_chip = Blake2bChip::construct(config);
            let _result = blake2b_chip.process(&mut layouter, &[], b"EmptyTestBlake2b")?;

            Ok(())
        }
    }

    let circuit = EmptyInputCircuit;
    let k = 17;
    let prover = MockProver::run(k, &circuit, vec![]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
}

/// Reference BLAKE2b implementation for testing
mod reference {
    use byteorder::{ByteOrder, LittleEndian};

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

    fn g(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
        v[d] = (v[d] ^ v[a]).rotate_right(32);
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(24);
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
        v[d] = (v[d] ^ v[a]).rotate_right(16);
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(63);
    }

    fn compress(h: &mut [u64; 8], m: &[u64; 16], t: u128, f: bool) {
        let mut v = [0u64; 16];
        v[..8].copy_from_slice(h);
        v[8..12].copy_from_slice(&IV[0..4]);
        v[12] = IV[4] ^ (t as u64); // Low 64 bits of counter
        v[13] = IV[5] ^ ((t >> 64) as u64); // High 64 bits of counter
        v[14] = if f { IV[6] ^ u64::MAX } else { IV[6] };
        v[15] = IV[7];

        for i in 0..12 {
            let s = &SIGMA[i % 10];
            g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
            g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
            g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
            g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
            g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
            g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
            g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
            g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
        }

        for i in 0..8 {
            h[i] = h[i] ^ v[i] ^ v[i + 8];
        }
    }

    /// Compute BLAKE2b-256 hash matching the circuit's input format
    /// Input: field element bytes (32 bytes each), personalization (16 bytes)
    ///
    /// BLAKE2b uses 128-byte blocks. The circuit processes inputs in chunks of 4 field
    /// elements (4 * 32 = 128 bytes) per block. Each field element provides 4 x 64-bit
    /// words, so one block = 4 fields = 16 x 64-bit words = 128 bytes.
    ///
    pub fn blake2b_hash(inputs: &[&[u8; 32]], personalization: &[u8; 16]) -> [u64; 4] {
        let mut h = [
            IV[0] ^ 0x01010000 ^ 32,
            IV[1],
            IV[2],
            IV[3],
            IV[4],
            IV[5],
            IV[6] ^ LittleEndian::read_u64(&personalization[0..8]),
            IV[7] ^ LittleEndian::read_u64(&personalization[8..16]),
        ];

        // Convert inputs to message blocks
        // BLAKE2b block = 128 bytes = 16 x 64-bit words = 4 field elements
        let mut all_bytes = Vec::new();
        for input in inputs {
            all_bytes.extend_from_slice(*input);
        }

        // Total input bytes (used for final block counter)
        let total_input_bytes = all_bytes.len();

        // Pad to multiple of 128 bytes (BLAKE2b block size)
        if all_bytes.is_empty() {
            all_bytes.resize(128, 0);
        } else if all_bytes.len() % 128 != 0 {
            let padding = 128 - (all_bytes.len() % 128);
            all_bytes.resize(all_bytes.len() + padding, 0);
        }

        let num_blocks = all_bytes.len() / 128;

        for (block_idx, chunk) in all_bytes.chunks(128).enumerate() {
            // Read 16 x 64-bit words from 128-byte block
            let mut m = [0u64; 16];
            for (i, word_bytes) in chunk.chunks(8).enumerate() {
                if word_bytes.len() == 8 {
                    m[i] = LittleEndian::read_u64(word_bytes);
                } else {
                    // Handle partial chunks
                    let mut padded = [0u8; 8];
                    padded[..word_bytes.len()].copy_from_slice(word_bytes);
                    m[i] = LittleEndian::read_u64(&padded);
                }
            }

            let is_last = block_idx == num_blocks - 1;
            // Counter: bytes processed so far
            // - Intermediate blocks: (block_idx + 1) * 128
            // - Final block: total_input_bytes.max(128) to match circuit's handling
            let t = if is_last {
                (total_input_bytes.max(128)) as u128
            } else {
                ((block_idx + 1) * 128) as u128
            };
            compress(&mut h, &m, t, is_last);
        }

        [h[0], h[1], h[2], h[3]]
    }
}

/// Test that verifies the circuit output matches a reference implementation.
#[test]
fn test_blake2b_against_reference() {
    /// Circuit that exposes hash output as public inputs for verification
    struct Blake2bVerifyCircuit {
        input1: Value<pallas::Base>,
        input2: Value<pallas::Base>,
        personalization: [u8; 16],
    }

    #[derive(Clone)]
    struct VerifyConfig {
        blake2b_config: Blake2bConfig<pallas::Base>,
        instance: Column<Instance>,
    }

    impl Circuit<pallas::Base> for Blake2bVerifyCircuit {
        type Config = VerifyConfig;
        type FloorPlanner = floor_planner::V1;

        fn without_witnesses(&self) -> Self {
            Self {
                input1: Value::unknown(),
                input2: Value::unknown(),
                personalization: self.personalization,
            }
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

            let instance = meta.instance_column();
            meta.enable_equality(instance);

            let constants = meta.fixed_column();
            meta.enable_constant(constants);

            VerifyConfig {
                blake2b_config: Blake2bConfig::configure(meta, advices),
                instance,
            }
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<pallas::Base>,
        ) -> Result<(), Error> {
            let input1 = assign_free_advice(
                layouter.namespace(|| "input1"),
                config.blake2b_config.advices[0],
                self.input1,
            )?;

            let input2 = assign_free_advice(
                layouter.namespace(|| "input2"),
                config.blake2b_config.advices[0],
                self.input2,
            )?;

            let blake2b_chip = Blake2bChip::construct(config.blake2b_config.clone());
            let result =
                blake2b_chip.process(&mut layouter, &[input1, input2], &self.personalization)?;

            // Expose hash output words as public inputs
            for (i, word) in result.iter().enumerate() {
                layouter.constrain_instance(word.get_word().cell(), config.instance, i)?;
            }

            Ok(())
        }
    }

    // Test inputs
    let input1 = pallas::Base::from(0x12345678_9abcdef0_u64);
    let input2 = pallas::Base::from(0xfedcba98_76543210_u64);
    let personalization = *b"TestPersonaliz16"; // 16 bytes

    // Get input bytes in the same format as the circuit
    let input1_bytes: [u8; 32] = input1.to_repr().as_ref().try_into().unwrap();
    let input2_bytes: [u8; 32] = input2.to_repr().as_ref().try_into().unwrap();

    // Compute reference hash
    let expected_hash = reference::blake2b_hash(&[&input1_bytes, &input2_bytes], &personalization);
    let expected_words: Vec<pallas::Base> = expected_hash
        .iter()
        .map(|&w| pallas::Base::from(w))
        .collect();

    // Create and run circuit
    let circuit = Blake2bVerifyCircuit {
        input1: Value::known(input1),
        input2: Value::known(input2),
        personalization,
    };

    let k = 17;
    let prover = MockProver::run(k, &circuit, vec![expected_words]).unwrap();
    assert_eq!(
        prover.verify(),
        Ok(()),
        "Circuit output doesn't match reference BLAKE2b"
    );
}

/// Test with zero inputs
#[test]
fn test_blake2b_zeros_against_reference() {
    /// Circuit for hashing zero-filled input
    struct Blake2bZerosCircuit {
        input1: Value<pallas::Base>,
        input2: Value<pallas::Base>,
    }

    #[derive(Clone)]
    struct ZerosConfig {
        blake2b_config: Blake2bConfig<pallas::Base>,
        instance: Column<Instance>,
    }

    impl Circuit<pallas::Base> for Blake2bZerosCircuit {
        type Config = ZerosConfig;
        type FloorPlanner = floor_planner::V1;

        fn without_witnesses(&self) -> Self {
            Self {
                input1: Value::unknown(),
                input2: Value::unknown(),
            }
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

            let instance = meta.instance_column();
            meta.enable_equality(instance);

            let constants = meta.fixed_column();
            meta.enable_constant(constants);

            ZerosConfig {
                blake2b_config: Blake2bConfig::configure(meta, advices),
                instance,
            }
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<pallas::Base>,
        ) -> Result<(), Error> {
            let input1 = assign_free_advice(
                layouter.namespace(|| "input1"),
                config.blake2b_config.advices[0],
                self.input1,
            )?;

            let input2 = assign_free_advice(
                layouter.namespace(|| "input2"),
                config.blake2b_config.advices[0],
                self.input2,
            )?;

            let blake2b_chip = Blake2bChip::construct(config.blake2b_config.clone());
            // 16-byte zero personalization
            let result = blake2b_chip.process(&mut layouter, &[input1, input2], &[0u8; 16])?;

            // Expose hash output words as public inputs
            for (i, word) in result.iter().enumerate() {
                layouter.constrain_instance(word.get_word().cell(), config.instance, i)?;
            }

            Ok(())
        }
    }

    let input1 = pallas::Base::ZERO;
    let input2 = pallas::Base::ZERO;

    let input1_bytes: [u8; 32] = input1.to_repr().as_ref().try_into().unwrap();
    let input2_bytes: [u8; 32] = input2.to_repr().as_ref().try_into().unwrap();

    let expected_hash = reference::blake2b_hash(&[&input1_bytes, &input2_bytes], &[0u8; 16]);
    let expected_words: Vec<pallas::Base> = expected_hash
        .iter()
        .map(|&w| pallas::Base::from(w))
        .collect();

    let circuit = Blake2bZerosCircuit {
        input1: Value::known(input1),
        input2: Value::known(input2),
    };

    let k = 17;
    let prover = MockProver::run(k, &circuit, vec![expected_words]).unwrap();
    assert_eq!(prover.verify(), Ok(()), "BLAKE2b zeros test failed");
}

/// Test: Compact Action Hash with Nullifier as Private Input
///
/// Proves: "I know nullifier N such that
/// BLAKE2b-256("ZTxIdOrcActCHash", N || cmx || epk || enc[0..52]) = expected_hash"
///
/// This test demonstrates the hybrid input system where:
/// - nullifier and cmx are field elements (canonicality checked)
/// - epk and enc[0..52] are raw bytes (boolean constrained only)
#[test]
fn test_compact_hash_nullifier_proof() {
    use compact_test_data::*;

    /// Circuit proving knowledge of a nullifier for compact action hash
    struct CompactHashCircuit {
        // PRIVATE witness - the secret we're proving knowledge of
        nullifier: Value<pallas::Base>,

        // PUBLIC inputs (known to verifier)
        cmx: Value<pallas::Base>,
        epk_bytes: Value<[u8; 32]>,
        enc_prefix: Value<[u8; 52]>,
    }

    #[derive(Clone)]
    struct CompactHashConfig {
        blake2b_config: Blake2bConfig<pallas::Base>,
        instance: Column<Instance>,
    }

    impl Circuit<pallas::Base> for CompactHashCircuit {
        type Config = CompactHashConfig;
        type FloorPlanner = floor_planner::V1;

        fn without_witnesses(&self) -> Self {
            Self {
                nullifier: Value::unknown(),
                cmx: Value::unknown(),
                epk_bytes: Value::unknown(),
                enc_prefix: Value::unknown(),
            }
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

            let instance = meta.instance_column();
            meta.enable_equality(instance);

            let constants = meta.fixed_column();
            meta.enable_constant(constants);

            CompactHashConfig {
                blake2b_config: Blake2bConfig::configure(meta, advices),
                instance,
            }
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<pallas::Base>,
        ) -> Result<(), Error> {
            // Assign field inputs (nullifier is private, cmx is public)
            let nullifier = assign_free_advice(
                layouter.namespace(|| "nullifier"),
                config.blake2b_config.advices[0],
                self.nullifier,
            )?;

            let cmx = assign_free_advice(
                layouter.namespace(|| "cmx"),
                config.blake2b_config.advices[0],
                self.cmx,
            )?;

            // Assign byte inputs (epk and enc_prefix)
            let mut byte_cells = Vec::with_capacity(32 + 52);

            // epk bytes
            for i in 0..32 {
                let byte_val = self
                    .epk_bytes
                    .map(|bytes| pallas::Base::from(bytes[i] as u64));
                let byte_cell = assign_free_advice(
                    layouter.namespace(|| format!("epk_byte_{}", i)),
                    config.blake2b_config.advices[0],
                    byte_val,
                )?;
                byte_cells.push(byte_cell);
            }

            // enc_prefix bytes
            for i in 0..52 {
                let byte_val = self
                    .enc_prefix
                    .map(|bytes| pallas::Base::from(bytes[i] as u64));
                let byte_cell = assign_free_advice(
                    layouter.namespace(|| format!("enc_byte_{}", i)),
                    config.blake2b_config.advices[0],
                    byte_val,
                )?;
                byte_cells.push(byte_cell);
            }

            // Compute compact action hash using hybrid processing
            let blake2b_chip = Blake2bChip::construct(config.blake2b_config.clone());
            let result = blake2b_chip.process_hybrid(
                &mut layouter,
                &[nullifier, cmx],   // Field inputs (canonicality checked)
                &byte_cells,         // Byte inputs (boolean constrained only)
                b"ZTxIdOrcActCHash", // ZIP-244 personalization for compact action hash
            )?;

            // Expose hash output as public inputs for verification
            for (i, word) in result.iter().enumerate() {
                layouter.constrain_instance(word.get_word().cell(), config.instance, i)?;
            }

            Ok(())
        }
    }

    // Compute expected hash using blake2b_simd reference implementation
    let expected_hash = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"ZTxIdOrcActCHash")
        .to_state()
        .update(&NF_OLD)
        .update(&CMX)
        .update(&EPHEMERAL_KEY)
        .update(&C_ENC_PREFIX)
        .finalize();

    // Convert expected hash to 4 x 64-bit words (little-endian)
    let hash_bytes = expected_hash.as_bytes();
    let expected_words: Vec<pallas::Base> = (0..4)
        .map(|i| {
            let start = i * 8;
            let word = u64::from_le_bytes(hash_bytes[start..start + 8].try_into().unwrap());
            pallas::Base::from(word)
        })
        .collect();

    // Convert field element bytes to pallas::Base
    // Note: These should be valid field elements (< p)
    let nullifier =
        pallas::Base::from_repr(NF_OLD.into()).expect("nullifier should be valid field element");
    let cmx = pallas::Base::from_repr(CMX.into()).expect("cmx should be valid field element");

    let circuit = CompactHashCircuit {
        nullifier: Value::known(nullifier),
        cmx: Value::known(cmx),
        epk_bytes: Value::known(EPHEMERAL_KEY),
        enc_prefix: Value::known(C_ENC_PREFIX),
    };

    let k = 17;
    let prover = MockProver::run(k, &circuit, vec![expected_words]).unwrap();
    assert_eq!(
        prover.verify(),
        Ok(()),
        "Compact action hash nullifier proof failed"
    );

    println!("SUCCESS: Proved knowledge of nullifier for compact action hash");
    println!("Expected hash: {}", hex::encode(hash_bytes));
}
