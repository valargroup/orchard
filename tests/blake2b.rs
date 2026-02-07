//! Integration tests for the BLAKE2b circuit.

#![cfg(feature = "circuit")]

use ff::PrimeField;
use halo2_proofs::{
    circuit::{floor_planner, Layouter, Value},
    dev::MockProver,
    plonk::{Circuit, Column, ConstraintSystem, Error, Instance},
};
use orchard::circuit::blake2b::{
    assign_free_advice, Blake2bChip, Blake2bConfig, CompactActionCells,
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

/// Reference BLAKE2b implementation for testing.
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
        v[12] = IV[4] ^ (t as u64);
        v[13] = IV[5] ^ ((t >> 64) as u64);
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

    /// Compute BLAKE2b-256 hash from raw input bytes with personalization.
    pub fn blake2b_hash(input: &[u8], personalization: &[u8; 16]) -> [u64; 4] {
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

        let total_input_bytes = input.len();

        // Pad to multiple of 128 bytes (BLAKE2b block size)
        let mut all_bytes = input.to_vec();
        if all_bytes.is_empty() {
            all_bytes.resize(128, 0);
        } else if all_bytes.len() % 128 != 0 {
            let padding = 128 - (all_bytes.len() % 128);
            all_bytes.resize(all_bytes.len() + padding, 0);
        }

        let num_blocks = all_bytes.len() / 128;

        for (block_idx, chunk) in all_bytes.chunks(128).enumerate() {
            let mut m = [0u64; 16];
            for (i, word_bytes) in chunk.chunks(8).enumerate() {
                m[i] = LittleEndian::read_u64(word_bytes);
            }

            let is_last = block_idx == num_blocks - 1;
            let t = if is_last {
                total_input_bytes as u128
            } else {
                ((block_idx + 1) * 128) as u128
            };
            compress(&mut h, &m, t, is_last);
        }

        [h[0], h[1], h[2], h[3]]
    }
}

// ---- Shared circuit definition for two-action tests ----

/// Circuit hashing 2 compact actions via `process_compact_action_hash`.
struct TwoActionHashCircuit {
    nf_1: Value<pallas::Base>,
    cmx_1: Value<pallas::Base>,
    epk_1: Value<[u8; 32]>,
    enc_1: Value<[u8; 52]>,
    nf_2: Value<pallas::Base>,
    cmx_2: Value<pallas::Base>,
    epk_2: Value<[u8; 32]>,
    enc_2: Value<[u8; 52]>,
    personalization: [u8; 16],
}

#[derive(Clone)]
struct TwoActionConfig {
    blake2b_config: Blake2bConfig<pallas::Base>,
    instance: Column<Instance>,
}

impl Circuit<pallas::Base> for TwoActionHashCircuit {
    type Config = TwoActionConfig;
    type FloorPlanner = floor_planner::V1;

    fn without_witnesses(&self) -> Self {
        Self {
            nf_1: Value::unknown(),
            cmx_1: Value::unknown(),
            epk_1: Value::unknown(),
            enc_1: Value::unknown(),
            nf_2: Value::unknown(),
            cmx_2: Value::unknown(),
            epk_2: Value::unknown(),
            enc_2: Value::unknown(),
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

        TwoActionConfig {
            blake2b_config: Blake2bConfig::configure(meta, advices),
            instance,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
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

        let col = config.blake2b_config.advices[0];

        // Action 1
        let nf_1 = assign_free_advice(layouter.namespace(|| "nf_1"), col, self.nf_1)?;
        let cmx_1 = assign_free_advice(layouter.namespace(|| "cmx_1"), col, self.cmx_1)?;
        let epk_1_cells = assign_bytes(
            &mut layouter, col, "epk_1",
            self.epk_1.as_ref().map(|b| b.as_ref()), 32,
        )?;
        let enc_1_cells = assign_bytes(
            &mut layouter, col, "enc_1",
            self.enc_1.as_ref().map(|b| b.as_ref()), 52,
        )?;

        // Action 2
        let nf_2 = assign_free_advice(layouter.namespace(|| "nf_2"), col, self.nf_2)?;
        let cmx_2 = assign_free_advice(layouter.namespace(|| "cmx_2"), col, self.cmx_2)?;
        let epk_2_cells = assign_bytes(
            &mut layouter, col, "epk_2",
            self.epk_2.as_ref().map(|b| b.as_ref()), 32,
        )?;
        let enc_2_cells = assign_bytes(
            &mut layouter, col, "enc_2",
            self.enc_2.as_ref().map(|b| b.as_ref()), 52,
        )?;

        let action_1 = CompactActionCells {
            nf: nf_1,
            cmx: cmx_1,
            epk_bytes: epk_1_cells.try_into().unwrap(),
            enc_prefix: enc_1_cells.try_into().unwrap(),
        };

        let action_2 = CompactActionCells {
            nf: nf_2,
            cmx: cmx_2,
            epk_bytes: epk_2_cells.try_into().unwrap(),
            enc_prefix: enc_2_cells.try_into().unwrap(),
        };

        let blake2b_chip = Blake2bChip::construct(config.blake2b_config.clone());
        let result = blake2b_chip.process_compact_action_hash(
            &mut layouter,
            &action_1,
            &action_2,
            &self.personalization,
        )?;

        let encoded = blake2b_chip.encode_result(&mut layouter, &result)?;
        for (i, field) in encoded.iter().enumerate() {
            layouter.constrain_instance(field.cell(), config.instance, i)?;
        }

        Ok(())
    }
}

// ---- Helper to build circuit and expected hash from raw action bytes ----

struct ActionData {
    nf: [u8; 32],
    cmx: [u8; 32],
    epk: [u8; 32],
    enc: [u8; 52],
}

fn build_circuit(a1: &ActionData, a2: &ActionData, personalization: &[u8; 16]) -> TwoActionHashCircuit {
    let nf_1 = pallas::Base::from_repr(a1.nf).expect("valid field element");
    let cmx_1 = pallas::Base::from_repr(a1.cmx).expect("valid field element");
    let nf_2 = pallas::Base::from_repr(a2.nf).expect("valid field element");
    let cmx_2 = pallas::Base::from_repr(a2.cmx).expect("valid field element");

    TwoActionHashCircuit {
        nf_1: Value::known(nf_1),
        cmx_1: Value::known(cmx_1),
        epk_1: Value::known(a1.epk),
        enc_1: Value::known(a1.enc),
        nf_2: Value::known(nf_2),
        cmx_2: Value::known(cmx_2),
        epk_2: Value::known(a2.epk),
        enc_2: Value::known(a2.enc),
        personalization: *personalization,
    }
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

// ---- Tests ----

/// Test that the circuit matches our reference BLAKE2b implementation.
#[test]
fn test_two_action_against_reference() {
    use compact_test_data::*;

    let a1 = ActionData { nf: NF_OLD, cmx: CMX, epk: EPHEMERAL_KEY, enc: C_ENC_PREFIX };
    let a2 = ActionData { nf: NF_OLD_2, cmx: CMX_2, epk: EPHEMERAL_KEY_2, enc: C_ENC_PREFIX_2 };
    let personalization = b"ZTxIdOrcActCHash";

    // Build input bytes in message order
    let mut input = Vec::with_capacity(296);
    for a in [&a1, &a2] {
        input.extend_from_slice(&a.nf);
        input.extend_from_slice(&a.cmx);
        input.extend_from_slice(&a.epk);
        input.extend_from_slice(&a.enc);
    }

    let ref_hash = reference::blake2b_hash(&input, personalization);
    let expected_words: Vec<pallas::Base> = ref_hash
        .chunks(2)
        .map(|pair| {
            pallas::Base::from(pair[0])
                + pallas::Base::from(pair[1]) * pallas::Base::from_u128(1u128 << 64)
        })
        .collect();

    let circuit = build_circuit(&a1, &a2, personalization);

    let k = 15;
    let prover = MockProver::run(k, &circuit, vec![expected_words]).unwrap();
    assert_eq!(
        prover.verify(),
        Ok(()),
        "Circuit output doesn't match reference BLAKE2b"
    );
}

/// Test with zero-filled actions against blake2b_simd.
#[test]
fn test_two_action_zeros() {
    let a1 = ActionData { nf: [0u8; 32], cmx: [0u8; 32], epk: [0u8; 32], enc: [0u8; 52] };
    let a2 = ActionData { nf: [0u8; 32], cmx: [0u8; 32], epk: [0u8; 32], enc: [0u8; 52] };
    let personalization = b"ZTxIdOrcActCHash";

    let expected_words = expected_hash_from_blake2b_simd(&a1, &a2, personalization);
    let circuit = build_circuit(&a1, &a2, personalization);

    let k = 15;
    let prover = MockProver::run(k, &circuit, vec![expected_words]).unwrap();
    assert_eq!(prover.verify(), Ok(()), "Two-action zeros test failed");
}

/// Test with non-trivial test vectors against blake2b_simd.
///
/// Uses distinct data for both actions to verify correct interleaving
/// and the 148-byte action boundary (word 18 spans both actions).
#[test]
fn test_two_action_compact_hash() {
    use compact_test_data::*;

    let a1 = ActionData { nf: NF_OLD, cmx: CMX, epk: EPHEMERAL_KEY, enc: C_ENC_PREFIX };
    let a2 = ActionData { nf: NF_OLD_2, cmx: CMX_2, epk: EPHEMERAL_KEY_2, enc: C_ENC_PREFIX_2 };
    let personalization = b"ZTxIdOrcActCHash";

    let expected_words = expected_hash_from_blake2b_simd(&a1, &a2, personalization);
    let circuit = build_circuit(&a1, &a2, personalization);

    let k = 15;
    let prover = MockProver::run(k, &circuit, vec![expected_words]).unwrap();
    assert_eq!(
        prover.verify(),
        Ok(()),
        "Two-action compact hash should match blake2b_simd reference"
    );
}
