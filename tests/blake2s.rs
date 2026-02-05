//! Integration tests for the BLAKE2s circuit.

#![cfg(feature = "circuit")]

use ff::{Field, PrimeField};
use halo2_proofs::{
    circuit::{floor_planner, Layouter, Value},
    dev::MockProver,
    plonk::{Circuit, Column, ConstraintSystem, Error, Instance},
};
use orchard::circuit::blake2s::{assign_free_advice, Blake2sChip, Blake2sConfig};
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

#[test]
fn test_blake2s_empty_input() {
    // Test with empty input (zero padding)
    #[derive(Default)]
    struct EmptyInputCircuit;

    impl Circuit<pallas::Base> for EmptyInputCircuit {
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
            let blake2s_chip = Blake2sChip::construct(config);
            // Empty input - will use zero padding block
            let _result = blake2s_chip.process(&mut layouter, &[], b"EmptyTst")?;

            Ok(())
        }
    }

    let circuit = EmptyInputCircuit;
    let k = 14;
    let prover = MockProver::run(k, &circuit, vec![]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
}

/// Reference BLAKE2s implementation for testing
mod reference {
    use byteorder::{ByteOrder, LittleEndian};

    const IV: [u32; 8] = [
        0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A,
        0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
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

    fn g(v: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize, x: u32, y: u32) {
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
        v[d] = (v[d] ^ v[a]).rotate_right(16);
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(12);
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
        v[d] = (v[d] ^ v[a]).rotate_right(8);
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(7);
    }

    fn compress(h: &mut [u32; 8], m: &[u32; 16], t: u64, f: bool) {
        let mut v = [0u32; 16];
        v[..8].copy_from_slice(h);
        v[8..12].copy_from_slice(&IV[0..4]);
        v[12] = IV[4] ^ (t as u32);
        v[13] = IV[5] ^ ((t >> 32) as u32);
        v[14] = if f { IV[6] ^ u32::MAX } else { IV[6] };
        v[15] = IV[7];

        for i in 0..10 {
            let s = &SIGMA[i];
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

    /// Compute BLAKE2s hash matching the circuit's input format
    /// Input: field element bytes (32 bytes each), personalization (8 bytes)
    pub fn blake2s_hash(inputs: &[&[u8; 32]], personalization: &[u8; 8]) -> [u32; 8] {
        // Initialize state with personalization
        let mut h = [
            IV[0] ^ 0x01010000 ^ 32,
            IV[1],
            IV[2],
            IV[3],
            IV[4],
            IV[5],
            IV[6] ^ LittleEndian::read_u32(&personalization[0..4]),
            IV[7] ^ LittleEndian::read_u32(&personalization[4..8]),
        ];

        // Convert inputs to message blocks (16 words = 64 bytes per block)
        let mut all_bytes = Vec::new();
        for input in inputs {
            all_bytes.extend_from_slice(*input);
        }

        // Pad to multiple of 64 bytes
        if all_bytes.is_empty() {
            all_bytes.resize(64, 0);
        } else if all_bytes.len() % 64 != 0 {
            let padding = 64 - (all_bytes.len() % 64);
            all_bytes.resize(all_bytes.len() + padding, 0);
        }

        let num_blocks = all_bytes.len() / 64;

        for (block_idx, chunk) in all_bytes.chunks(64).enumerate() {
            let mut m = [0u32; 16];
            for (i, word_bytes) in chunk.chunks(4).enumerate() {
                m[i] = LittleEndian::read_u32(word_bytes);
            }
            let t = ((block_idx + 1) * 64) as u64;
            let is_last = block_idx == num_blocks - 1;
            compress(&mut h, &m, t, is_last);
        }

        h
    }
}

/// Test that verifies the circuit output matches a reference implementation.
#[test]
fn test_blake2s_against_reference() {
    /// Circuit that exposes hash output as public inputs for verification
    struct Blake2sVerifyCircuit {
        input1: Value<pallas::Base>,
        input2: Value<pallas::Base>,
        personalization: [u8; 8],
    }

    #[derive(Clone)]
    struct VerifyConfig {
        blake2s_config: Blake2sConfig<pallas::Base>,
        instance: Column<Instance>,
    }

    impl Circuit<pallas::Base> for Blake2sVerifyCircuit {
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
                blake2s_config: Blake2sConfig::configure(meta, advices),
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
                config.blake2s_config.advices[0],
                self.input1,
            )?;

            let input2 = assign_free_advice(
                layouter.namespace(|| "input2"),
                config.blake2s_config.advices[0],
                self.input2,
            )?;

            let blake2s_chip = Blake2sChip::construct(config.blake2s_config.clone());
            let result = blake2s_chip.process(
                &mut layouter,
                &[input1, input2],
                &self.personalization,
            )?;

            // Expose all 8 words as public inputs
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
    let personalization = *b"TestPers";

    // Get input bytes in the same format as the circuit
    let input1_bytes: [u8; 32] = input1.to_repr().as_ref().try_into().unwrap();
    let input2_bytes: [u8; 32] = input2.to_repr().as_ref().try_into().unwrap();

    // Compute reference hash
    let expected_hash = reference::blake2s_hash(&[&input1_bytes, &input2_bytes], &personalization);
    let expected_words: Vec<pallas::Base> = expected_hash
        .iter()
        .map(|&w| pallas::Base::from(w as u64))
        .collect();

    // Create and run circuit
    let circuit = Blake2sVerifyCircuit {
        input1: Value::known(input1),
        input2: Value::known(input2),
        personalization,
    };

    let k = 14;
    let prover = MockProver::run(k, &circuit, vec![expected_words]).unwrap();
    assert_eq!(prover.verify(), Ok(()), "Circuit output doesn't match reference BLAKE2s");
}

/// Test with zero inputs
#[test]
fn test_blake2s_zeros_against_reference() {
    /// Circuit for hashing zero-filled input
    struct Blake2sZerosCircuit {
        input1: Value<pallas::Base>,
        input2: Value<pallas::Base>,
    }

    #[derive(Clone)]
    struct ZerosConfig {
        blake2s_config: Blake2sConfig<pallas::Base>,
        instance: Column<Instance>,
    }

    impl Circuit<pallas::Base> for Blake2sZerosCircuit {
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
                blake2s_config: Blake2sConfig::configure(meta, advices),
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
                config.blake2s_config.advices[0],
                self.input1,
            )?;

            let input2 = assign_free_advice(
                layouter.namespace(|| "input2"),
                config.blake2s_config.advices[0],
                self.input2,
            )?;

            let blake2s_chip = Blake2sChip::construct(config.blake2s_config.clone());
            let result = blake2s_chip.process(
                &mut layouter,
                &[input1, input2],
                &[0u8; 8],
            )?;

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

    let expected_hash = reference::blake2s_hash(&[&input1_bytes, &input2_bytes], &[0u8; 8]);
    let expected_words: Vec<pallas::Base> = expected_hash
        .iter()
        .map(|&w| pallas::Base::from(w as u64))
        .collect();

    let circuit = Blake2sZerosCircuit {
        input1: Value::known(input1),
        input2: Value::known(input2),
    };

    let k = 14;
    let prover = MockProver::run(k, &circuit, vec![expected_words]).unwrap();
    assert_eq!(prover.verify(), Ok(()), "BLAKE2s zeros test failed");
}
