//! Integration tests for the BLAKE2b circuit.

#![cfg(feature = "circuit")]

use ff::{Field, PrimeField};
use halo2_proofs::{
    circuit::{floor_planner, Layouter, Value},
    dev::MockProver,
    plonk::{Circuit, Column, ConstraintSystem, Error, Instance},
};
use orchard::circuit::blake2b::{
    assign_free_advice, compute_h1, Blake2bChip, Blake2bConfig, Blake2bWord, CompactActionCells,
};
use pasta_curves::pallas;

/// Test vector data from Zcash test vectors for compact action hash testing.
mod compact_test_data {
    // Action 1 test vectors (from orchard_note_encryption)
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

    pub const C_ENC_PREFIX: [u8; 52] = [
        0x1a, 0x9a, 0xdb, 0x14, 0x24, 0x98, 0xe3, 0xdc, 0xc7, 0x6f, 0xed, 0x77, 0x86, 0x14, 0xdd,
        0x31, 0x6c, 0x02, 0xfb, 0xb8, 0xba, 0x92, 0x44, 0xae, 0x4c, 0x2e, 0x32, 0xa0, 0x7d, 0xae,
        0xec, 0xa4, 0x12, 0x26, 0xb9, 0x8b, 0xfe, 0x74, 0xf9, 0xfc, 0xb2, 0x28, 0xcf, 0xc1, 0x00,
        0xf3, 0x18, 0x0f, 0x57, 0x75, 0xec, 0xe3,
    ];

    // Action 2 test vectors (distinct from action 1)
    pub const NF_OLD_2: [u8; 32] = [
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
        0x0d, 0x0e, 0x0f, 0x10,
    ];

    pub const CMX_2: [u8; 32] = [
        0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45,
        0x67, 0x89, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0,
        0xd0, 0xe0, 0xf0, 0x00,
    ];

    pub const EPHEMERAL_KEY_2: [u8; 32] = [
        0xca, 0xfe, 0xba, 0xbe, 0xde, 0xad, 0xbe, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
        0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0x11, 0x22, 0x33, 0x44,
        0x55, 0x66, 0x77, 0x88,
    ];

    pub const C_ENC_PREFIX_2: [u8; 52] = [
        0xa1, 0xb2, 0xc3, 0xd4, 0xe5, 0xf6, 0x07, 0x18, 0x29, 0x3a, 0x4b, 0x5c, 0x6d, 0x7e,
        0x8f, 0x90, 0xa0, 0xb1, 0xc2, 0xd3, 0xe4, 0xf5, 0x06, 0x17, 0x28, 0x39, 0x4a, 0x5b,
        0x6c, 0x7d, 0x8e, 0x9f, 0x10, 0x21, 0x32, 0x43, 0x54, 0x65, 0x76, 0x87, 0x98, 0xa9,
        0xba, 0xcb, 0xdc, 0xed, 0xfe, 0x0f, 0x20, 0x31, 0x42, 0x53,
    ];
}


// ---- Helpers ----

struct ActionData {
    nf: [u8; 32],
    cmx: [u8; 32],
    epk: [u8; 32],
    enc: [u8; 52],
}

fn expected_hash_from_blake2b_simd(a1: &ActionData, a2: &ActionData, personalization: &[u8; 16]) -> Vec<pallas::Base> {
    let expected_hash = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(personalization)
        .to_state()
        .update(&a1.nf)
        .update(&a1.cmx)
        .update(&a1.epk)
        .update(&a1.enc)
        .update(&a2.nf)
        .update(&a2.cmx)
        .update(&a2.epk)
        .update(&a2.enc)
        .finalize();

    let hash_bytes = expected_hash.as_bytes();
    (0..2)
        .map(|i| {
            let w0 = u64::from_le_bytes(hash_bytes[i * 16..i * 16 + 8].try_into().unwrap());
            let w1 = u64::from_le_bytes(hash_bytes[i * 16 + 8..i * 16 + 16].try_into().unwrap());
            pallas::Base::from(w0) + pallas::Base::from(w1) * pallas::Base::from_u128(1u128 << 64)
        })
        .collect()
}

// ---- Circuit definition ----

/// Circuit with precomputed block 1 state (h_1).
///
/// Instance layout: `[h_1[0..8], hash_output[0..2]]` (10 values total).
/// - instance[0..8]: h_1 as 8 raw u64 words (each a field element)
/// - instance[8..10]: hash output as 2 × 128-bit packed fields
struct PrecomputedCircuit {
    h_1_words: [Value<pallas::Base>; 8],
    enc_1_tail: Value<[u8; 20]>,
    nf_2: Value<pallas::Base>,
    cmx_2: Value<pallas::Base>,
    epk_2: Value<[u8; 32]>,
    enc_2: Value<[u8; 52]>,
}

#[derive(Clone)]
struct PrecomputedConfig {
    blake2b_config: Blake2bConfig<pallas::Base>,
    instance: Column<Instance>,
}

impl Circuit<pallas::Base> for PrecomputedCircuit {
    type Config = PrecomputedConfig;
    type FloorPlanner = floor_planner::V1;

    fn without_witnesses(&self) -> Self {
        Self {
            h_1_words: [
                Value::unknown(), Value::unknown(), Value::unknown(), Value::unknown(),
                Value::unknown(), Value::unknown(), Value::unknown(), Value::unknown(),
            ],
            enc_1_tail: Value::unknown(),
            nf_2: Value::unknown(),
            cmx_2: Value::unknown(),
            epk_2: Value::unknown(),
            enc_2: Value::unknown(),
        }
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
        let advices: [_; 18] = core::array::from_fn(|_| meta.advice_column());

        for advice in advices.iter() {
            meta.enable_equality(*advice);
        }

        let instance = meta.instance_column();
        meta.enable_equality(instance);

        let constants = meta.fixed_column();
        meta.enable_constant(constants);

        PrecomputedConfig {
            blake2b_config: Blake2bConfig::configure(meta, advices),
            instance,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        let col = config.blake2b_config.advices[0];
        let chip = Blake2bChip::construct(config.blake2b_config.clone());

        // Copy h_1 from instance: assign advice, constrain to instance, decompose
        let mut h_1_vec: Vec<Blake2bWord<pallas::Base>> = Vec::with_capacity(8);
        for i in 0..8 {
            let word_cell = assign_free_advice(
                layouter.namespace(|| format!("h1_{}", i)),
                col,
                self.h_1_words[i],
            )?;
            layouter.constrain_instance(word_cell.cell(), config.instance, i)?;
            let word = chip.word_from_instance(
                layouter.namespace(|| format!("h1_decomp_{}", i)),
                &word_cell,
            )?;
            h_1_vec.push(word);
        }
        let h_1: [Blake2bWord<pallas::Base>; 8] = h_1_vec.try_into().unwrap();

        // Assign enc_1_tail (20 bytes)
        let mut enc_1_tail_cells = Vec::with_capacity(20);
        for i in 0..20 {
            let val = self.enc_1_tail.map(|bytes| pallas::Base::from(bytes[i] as u64));
            enc_1_tail_cells.push(assign_free_advice(
                layouter.namespace(|| format!("enc1t_{}", i)),
                col,
                val,
            )?);
        }
        let enc_1_tail: [halo2_proofs::circuit::AssignedCell<pallas::Base, pallas::Base>; 20] =
            enc_1_tail_cells.try_into().unwrap();

        // Assign action_2
        let nf_2 = assign_free_advice(layouter.namespace(|| "nf_2"), col, self.nf_2)?;
        let cmx_2 = assign_free_advice(layouter.namespace(|| "cmx_2"), col, self.cmx_2)?;

        fn assign_bytes(
            layouter: &mut impl Layouter<pallas::Base>,
            col: halo2_proofs::plonk::Column<halo2_proofs::plonk::Advice>,
            prefix: &str,
            byte_vals: Value<&[u8]>,
            count: usize,
        ) -> Result<Vec<halo2_proofs::circuit::AssignedCell<pallas::Base, pallas::Base>>, Error>
        {
            let mut cells = Vec::with_capacity(count);
            for i in 0..count {
                let val = byte_vals.map(|bytes| pallas::Base::from(bytes[i] as u64));
                cells.push(assign_free_advice(
                    layouter.namespace(|| format!("{}_{}", prefix, i)),
                    col,
                    val,
                )?);
            }
            Ok(cells)
        }

        let epk_2_cells = assign_bytes(
            &mut layouter, col, "epk_2",
            self.epk_2.as_ref().map(|b| b.as_ref()), 32,
        )?;
        let enc_2_cells = assign_bytes(
            &mut layouter, col, "enc_2",
            self.enc_2.as_ref().map(|b| b.as_ref()), 52,
        )?;

        let action_2 = CompactActionCells {
            nf: nf_2,
            cmx: cmx_2,
            epk_bytes: epk_2_cells.try_into().unwrap(),
            enc_prefix: enc_2_cells.try_into().unwrap(),
        };

        let result = chip.process_precomputed_action_hash(
            &mut layouter,
            &h_1,
            &enc_1_tail,
            &action_2,
        )?;

        let encoded = chip.encode_result(&mut layouter, &result)?;
        for (i, field) in encoded.iter().enumerate() {
            layouter.constrain_instance(field.cell(), config.instance, 8 + i)?;
        }

        Ok(())
    }
}

// ---- Test helpers ----

/// Build a PrecomputedCircuit and instance vector from two actions.
/// The circuit witness uses `a2_witness` data while the instance hash is computed from `a2_instance`.
/// For positive tests, pass the same ActionData for both.
fn build_test(
    a1: &ActionData,
    a2_instance: &ActionData,
    a2_witness: &ActionData,
    personalization: &[u8; 16],
) -> (PrecomputedCircuit, Vec<pallas::Base>) {
    let h_1 = compute_h1(&a1.nf, &a1.cmx, &a1.epk, &a1.enc, personalization);

    let mut instance: Vec<pallas::Base> = Vec::with_capacity(10);
    for &word in &h_1 {
        instance.push(pallas::Base::from(word));
    }
    let expected_hash_fields = expected_hash_from_blake2b_simd(a1, a2_instance, personalization);
    instance.extend_from_slice(&expected_hash_fields);

    let mut enc_1_tail = [0u8; 20];
    enc_1_tail.copy_from_slice(&a1.enc[32..52]);

    let nf_2 = pallas::Base::from_repr(a2_witness.nf).expect("valid field element");
    let cmx_2 = pallas::Base::from_repr(a2_witness.cmx).expect("valid field element");

    let h_1_words = h_1.map(|w| Value::known(pallas::Base::from(w)));

    let circuit = PrecomputedCircuit {
        h_1_words,
        enc_1_tail: Value::known(enc_1_tail),
        nf_2: Value::known(nf_2),
        cmx_2: Value::known(cmx_2),
        epk_2: Value::known(a2_witness.epk),
        enc_2: Value::known(a2_witness.enc),
    };

    (circuit, instance)
}

/// Build and verify a circuit where witness matches instance (positive test).
fn assert_circuit_verifies(a1: &ActionData, a2: &ActionData, personalization: &[u8; 16], msg: &str) {
    let (circuit, instance) = build_test(a1, a2, a2, personalization);
    let prover = MockProver::run(13, &circuit, vec![instance]).unwrap();
    assert_eq!(prover.verify(), Ok(()), "{}", msg);
}

/// Generate deterministic action data from a seed byte.
fn generate_action_data(seed: u8) -> ActionData {
    let hash = |tag: &[u8]| -> [u8; 32] {
        let h = blake2b_simd::Params::new()
            .hash_length(32)
            .personal(b"test_vector_gen!")
            .to_state()
            .update(&[seed])
            .update(tag)
            .finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&h.as_bytes()[..32]);
        out
    };

    // nf, cmx must be valid field elements: clear MSB to ensure < p
    let mut nf = hash(b"nf");
    nf[31] &= 0x3F;
    let mut cmx = hash(b"cmx");
    cmx[31] &= 0x3F;

    let epk = hash(b"epk");

    let enc_part1 = hash(b"enc1");
    let enc_part2 = hash(b"enc2");
    let mut enc = [0u8; 52];
    enc[0..32].copy_from_slice(&enc_part1);
    enc[32..52].copy_from_slice(&enc_part2[0..20]);

    ActionData { nf, cmx, epk, enc }
}

// ---- Positive tests ----

/// Test precomputed action hash produces same result as full 3-block hash.
#[test]
fn test_precomputed_action_hash() {
    use compact_test_data::*;

    let a1 = ActionData { nf: NF_OLD, cmx: CMX, epk: EPHEMERAL_KEY, enc: C_ENC_PREFIX };
    let a2 = ActionData { nf: NF_OLD_2, cmx: CMX_2, epk: EPHEMERAL_KEY_2, enc: C_ENC_PREFIX_2 };
    assert_circuit_verifies(&a1, &a2, b"ZTxIdOrcActCHash", "original test vectors");
}

/// All-zero inputs exercise zero-carry paths and zero-XOR in G.
#[test]
fn test_all_zeros() {
    let a1 = ActionData { nf: [0u8; 32], cmx: [0u8; 32], epk: [0u8; 32], enc: [0u8; 52] };
    let a2 = ActionData { nf: [0u8; 32], cmx: [0u8; 32], epk: [0u8; 32], enc: [0u8; 52] };
    assert_circuit_verifies(&a1, &a2, b"ZTxIdOrcActCHash", "all-zero inputs");
}

/// High-value inputs exercise max-carry wrapping_add overflow and all-ones XOR.
/// nf/cmx use 0x3FFF..FF (largest value with MSB < 0x40), epk/enc are all 0xFF.
#[test]
fn test_high_carry_inputs() {
    let mut high_field = [0xFFu8; 32];
    high_field[31] = 0x3F; // ensure < p

    let a1 = ActionData { nf: high_field, cmx: high_field, epk: [0xFF; 32], enc: [0xFF; 52] };
    let a2 = ActionData { nf: high_field, cmx: high_field, epk: [0xFF; 32], enc: [0xFF; 52] };
    assert_circuit_verifies(&a1, &a2, b"ZTxIdOrcActCHash", "high-carry 0xFF inputs");
}

/// Deterministic generated vectors provide a third independent input pattern.
#[test]
fn test_deterministic_generated_vectors() {
    let a1 = generate_action_data(0x42);
    let a2 = generate_action_data(0xAB);
    assert_circuit_verifies(&a1, &a2, b"ZTxIdOrcActCHash", "deterministic generated vectors");
}

/// Test with nf_2 = p-1 (largest valid Pallas field element).
/// Exercises field_to_words decomposition at the field boundary.
#[test]
fn test_field_boundary_p_minus_1() {
    use compact_test_data::*;

    let a1 = ActionData { nf: NF_OLD, cmx: CMX, epk: EPHEMERAL_KEY, enc: C_ENC_PREFIX };

    // Construct p-1 as bytes
    let neg_one = -pallas::Base::ONE;
    let repr = neg_one.to_repr();
    let mut p_minus_1 = [0u8; 32];
    p_minus_1.copy_from_slice(repr.as_ref());

    // Verify it round-trips
    assert!(bool::from(pallas::Base::from_repr(p_minus_1).is_some()), "p-1 must be a valid field repr");

    let a2 = ActionData { nf: p_minus_1, cmx: p_minus_1, epk: EPHEMERAL_KEY_2, enc: C_ENC_PREFIX_2 };
    assert_circuit_verifies(&a1, &a2, b"ZTxIdOrcActCHash", "nf_2/cmx_2 = p-1 field boundary");
}

// ---- Negative tests ----

/// Wrong h_1 in instance: flipping a bit in h_1[0] causes instance constraint failure.
#[test]
fn test_wrong_h1_rejected() {
    use compact_test_data::*;

    let a1 = ActionData { nf: NF_OLD, cmx: CMX, epk: EPHEMERAL_KEY, enc: C_ENC_PREFIX };
    let a2 = ActionData { nf: NF_OLD_2, cmx: CMX_2, epk: EPHEMERAL_KEY_2, enc: C_ENC_PREFIX_2 };
    let (circuit, mut instance) = build_test(&a1, &a2, &a2, b"ZTxIdOrcActCHash");

    // Flip a bit in h_1[0] instance value (circuit witness still has the correct h_1)
    instance[0] += pallas::Base::ONE;

    let prover = MockProver::run(13, &circuit, vec![instance]).unwrap();
    assert!(
        prover.verify().is_err(),
        "Wrong h_1 instance should cause constraint failure"
    );
}

/// Wrong hash output in instance: correct h_1 but wrong expected hash.
#[test]
fn test_wrong_hash_output_rejected() {
    use compact_test_data::*;

    let a1 = ActionData { nf: NF_OLD, cmx: CMX, epk: EPHEMERAL_KEY, enc: C_ENC_PREFIX };
    let a2 = ActionData { nf: NF_OLD_2, cmx: CMX_2, epk: EPHEMERAL_KEY_2, enc: C_ENC_PREFIX_2 };
    let (circuit, mut instance) = build_test(&a1, &a2, &a2, b"ZTxIdOrcActCHash");

    // Corrupt the hash output instance (index 8)
    instance[8] += pallas::Base::ONE;

    let prover = MockProver::run(13, &circuit, vec![instance]).unwrap();
    assert!(
        prover.verify().is_err(),
        "Wrong hash output instance should cause constraint failure"
    );
}

/// Wrong nf_2 witness: instance expects hash(real_nf_2) but circuit witnesses a different nf_2.
/// The circuit hashes wrong data, producing output that doesn't match instance.
#[test]
fn test_wrong_nf2_witness_rejected() {
    use compact_test_data::*;

    let a1 = ActionData { nf: NF_OLD, cmx: CMX, epk: EPHEMERAL_KEY, enc: C_ENC_PREFIX };
    let a2_real = ActionData { nf: NF_OLD_2, cmx: CMX_2, epk: EPHEMERAL_KEY_2, enc: C_ENC_PREFIX_2 };

    // Create a2_fake with a different nf
    let mut fake_nf = NF_OLD_2;
    fake_nf[0] ^= 0x01; // flip one bit
    let a2_fake = ActionData { nf: fake_nf, cmx: CMX_2, epk: EPHEMERAL_KEY_2, enc: C_ENC_PREFIX_2 };

    // Instance hash is computed from a2_real, but circuit witness uses a2_fake
    let (circuit, instance) = build_test(&a1, &a2_real, &a2_fake, b"ZTxIdOrcActCHash");

    let prover = MockProver::run(13, &circuit, vec![instance]).unwrap();
    assert!(
        prover.verify().is_err(),
        "Wrong nf_2 witness should cause hash mismatch and constraint failure"
    );
}

/// Flipping a single byte in enc_2 witness causes the hash to change, failing verification.
#[test]
fn test_single_byte_flip_detected() {
    use compact_test_data::*;

    let a1 = ActionData { nf: NF_OLD, cmx: CMX, epk: EPHEMERAL_KEY, enc: C_ENC_PREFIX };
    let a2_real = ActionData { nf: NF_OLD_2, cmx: CMX_2, epk: EPHEMERAL_KEY_2, enc: C_ENC_PREFIX_2 };

    // Flip one byte in enc_2
    let mut flipped_enc = C_ENC_PREFIX_2;
    flipped_enc[25] ^= 0x80;
    let a2_flipped = ActionData { nf: NF_OLD_2, cmx: CMX_2, epk: EPHEMERAL_KEY_2, enc: flipped_enc };

    // Instance hash from real data, witness from flipped data
    let (circuit, instance) = build_test(&a1, &a2_real, &a2_flipped, b"ZTxIdOrcActCHash");

    let prover = MockProver::run(13, &circuit, vec![instance]).unwrap();
    assert!(
        prover.verify().is_err(),
        "Single byte flip in enc_2 should cause constraint failure"
    );
}
