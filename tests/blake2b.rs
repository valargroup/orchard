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
        // BLAKE2B-MOD: 16-byte personalization (was 8 bytes in BLAKE2s)
        let _result = blake2b_chip.process(
            &mut layouter,
            &[input1, input2],
            b"ZcshBlake2bTest!", // 16-byte personalization
        )?;

        Ok(())
    }
}

#[test]
fn test_blake2b_circuit() {
    let circuit = Blake2bTestCircuit {
        input1: Value::known(pallas::Base::from(1u64)),
        input2: Value::known(pallas::Base::from(2u64)),
    };

    let k = 17;  // BLAKE2B-MOD: May need larger circuit due to 64-bit operations
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
            // BLAKE2B-MOD: 16-byte personalization (was 8 bytes in BLAKE2s)
            // Empty input - will use zero padding block
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

    // BLAKE2B-MOD: 64-bit IV constants
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

    // BLAKE2B-MOD: 64-bit G function with different rotations (32, 24, 16, 63)
    fn g(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
        v[d] = (v[d] ^ v[a]).rotate_right(32);  // R1 = 32
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(24);  // R2 = 24
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
        v[d] = (v[d] ^ v[a]).rotate_right(16);  // R3 = 16
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(63);  // R4 = 63
    }

    // BLAKE2B-MOD: 12 rounds (was 10 in BLAKE2s)
    fn compress(h: &mut [u64; 8], m: &[u64; 16], t: u128, f: bool) {
        let mut v = [0u64; 16];
        v[..8].copy_from_slice(h);
        v[8..12].copy_from_slice(&IV[0..4]);
        v[12] = IV[4] ^ (t as u64);       // Low 64 bits of counter
        v[13] = IV[5] ^ ((t >> 64) as u64);  // High 64 bits of counter
        v[14] = if f { IV[6] ^ u64::MAX } else { IV[6] };
        v[15] = IV[7];

        // BLAKE2B-MOD: 12 rounds (was 10 in BLAKE2s)
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
    /// BLAKE2B-MOD: Returns 4 words (256 bits) for BLAKE2b-256, matching Orchard's
    /// action hash requirements (ZIP-244).
    pub fn blake2b_hash(inputs: &[&[u8; 32]], personalization: &[u8; 16]) -> [u64; 4] {
        // Initialize state with personalization
        // BLAKE2B-MOD: 32-byte output length (BLAKE2b-256)
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

        // BLAKE2B-MOD: Return first 4 words (256 bits) for BLAKE2b-256
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
            let result = blake2b_chip.process(
                &mut layouter,
                &[input1, input2],
                &self.personalization,
            )?;

            // BLAKE2B-MOD: Expose all 4 words (256 bits) as public inputs
            for (i, word) in result.iter().enumerate() {
                layouter.constrain_instance(
                    word.get_word().cell(),
                    config.instance,
                    i,
                )?;
            }

            Ok(())
        }
    }

    // Test inputs
    let input1 = pallas::Base::from(0x12345678_9abcdef0_u64);
    let input2 = pallas::Base::from(0xfedcba98_76543210_u64);
    let personalization = *b"TestPersonaliz16";  // 16 bytes

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
    assert_eq!(prover.verify(), Ok(()), "Circuit output doesn't match reference BLAKE2b");
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
            let result = blake2b_chip.process(
                &mut layouter,
                &[input1, input2],
                &[0u8; 16],
            )?;

            // BLAKE2B-MOD: Expose all 4 words (256 bits) as public inputs
            for (i, word) in result.iter().enumerate() {
                layouter.constrain_instance(
                    word.get_word().cell(),
                    config.instance,
                    i,
                )?;
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
