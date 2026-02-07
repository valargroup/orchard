#[macro_use]
extern crate criterion;

use criterion::Criterion;

#[cfg(unix)]
use pprof::criterion::{Output, PProfProfiler};

use ff::PrimeField;
use halo2_proofs::{
    circuit::{floor_planner, Layouter, Value},
    plonk::{self, Circuit, Column, ConstraintSystem, Error, Instance, SingleVerifier},
    poly::commitment::Params,
    transcript::{Blake2bRead, Blake2bWrite},
};
use orchard::circuit::blake2b::{
    assign_free_advice, Blake2bChip, Blake2bConfig, CompactActionCells,
};
use pasta_curves::{pallas, vesta};
use rand::rngs::OsRng;

// Action 1 test vectors
const NF_OLD: [u8; 32] = [
    0xc5, 0x96, 0xfb, 0xd3, 0x2e, 0xbb, 0xcb, 0xad, 0xae, 0x60, 0xd2, 0x85, 0xc7, 0xd7, 0x5f, 0xa8,
    0x36, 0xf9, 0xd2, 0xfa, 0x86, 0x10, 0x0a, 0xb8, 0x58, 0xea, 0x2d, 0xe1, 0xf1, 0x1c, 0x83, 0x06,
];

const CMX: [u8; 32] = [
    0xa5, 0x70, 0x6f, 0x3d, 0x1b, 0x68, 0x8e, 0x9d, 0xc6, 0x34, 0xee, 0xe4, 0xe6, 0x5b, 0x02, 0x8a,
    0x43, 0xee, 0xae, 0xd2, 0x43, 0x5b, 0xea, 0x2a, 0xe3, 0xd5, 0x16, 0x05, 0x75, 0xc1, 0x1a, 0x3b,
];

const EPHEMERAL_KEY: [u8; 32] = [
    0xad, 0xdb, 0x47, 0xb6, 0xac, 0x5d, 0xfc, 0x16, 0x55, 0x89, 0x23, 0xd3, 0xa8, 0xf3, 0x76, 0x09,
    0x5c, 0x69, 0x5c, 0x04, 0x7c, 0x4e, 0x32, 0x66, 0xae, 0x67, 0x69, 0x87, 0xf7, 0xe3, 0x13, 0x81,
];

const C_ENC_PREFIX: [u8; 52] = [
    0x1a, 0x9a, 0xdb, 0x14, 0x24, 0x98, 0xe3, 0xdc, 0xc7, 0x6f, 0xed, 0x77, 0x86, 0x14, 0xdd, 0x31,
    0x6c, 0x02, 0xfb, 0xb8, 0xba, 0x92, 0x44, 0xae, 0x4c, 0x2e, 0x32, 0xa0, 0x7d, 0xae, 0xec, 0xa4,
    0x12, 0x26, 0xb9, 0x8b, 0xfe, 0x74, 0xf9, 0xfc, 0xb2, 0x28, 0xcf, 0xc1, 0x00, 0xf3, 0x18, 0x0f,
    0x57, 0x75, 0xec, 0xe3,
];

// Action 2 test vectors
const NF_OLD_2: [u8; 32] = [
    0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00,
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
];

const CMX_2: [u8; 32] = [
    0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89,
    0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0, 0xd0, 0xe0, 0xf0, 0x00,
];

const EPHEMERAL_KEY_2: [u8; 32] = [
    0xca, 0xfe, 0xba, 0xbe, 0xde, 0xad, 0xbe, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
    0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
];

const C_ENC_PREFIX_2: [u8; 52] = [
    0xa1, 0xb2, 0xc3, 0xd4, 0xe5, 0xf6, 0x07, 0x18, 0x29, 0x3a, 0x4b, 0x5c, 0x6d, 0x7e, 0x8f, 0x90,
    0xa0, 0xb1, 0xc2, 0xd3, 0xe4, 0xf5, 0x06, 0x17, 0x28, 0x39, 0x4a, 0x5b, 0x6c, 0x7d, 0x8e, 0x9f,
    0x10, 0x21, 0x32, 0x43, 0x54, 0x65, 0x76, 0x87, 0x98, 0xa9, 0xba, 0xcb, 0xdc, 0xed, 0xfe, 0x0f,
    0x20, 0x31, 0x42, 0x53,
];

const K: u32 = 14;

/// Circuit hashing 2 compact actions via `process_compact_action_hash`.
#[derive(Clone)]
struct TwoActionBenchCircuit {
    nf_1: Value<pallas::Base>,
    cmx_1: Value<pallas::Base>,
    epk_1: Value<[u8; 32]>,
    enc_1: Value<[u8; 52]>,
    nf_2: Value<pallas::Base>,
    cmx_2: Value<pallas::Base>,
    epk_2: Value<[u8; 32]>,
    enc_2: Value<[u8; 52]>,
}

#[derive(Clone)]
struct TwoActionBenchConfig {
    blake2b_config: Blake2bConfig<pallas::Base>,
    instance: Column<Instance>,
}

impl Circuit<pallas::Base> for TwoActionBenchCircuit {
    type Config = TwoActionBenchConfig;
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

        TwoActionBenchConfig {
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

        let blake2b_chip = Blake2bChip::construct(config.blake2b_config);
        let result = blake2b_chip.process_compact_action_hash(
            &mut layouter,
            &action_1,
            &action_2,
            b"ZTxIdOrcActCHash",
        )?;

        let encoded = blake2b_chip.encode_result(&mut layouter, &result)?;
        for (i, field) in encoded.iter().enumerate() {
            layouter.constrain_instance(field.cell(), config.instance, i)?;
        }

        Ok(())
    }
}

fn compute_expected_words() -> Vec<pallas::Base> {
    let expected_hash = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"ZTxIdOrcActCHash")
        .to_state()
        .update(&NF_OLD)
        .update(&CMX)
        .update(&EPHEMERAL_KEY)
        .update(&C_ENC_PREFIX)
        .update(&NF_OLD_2)
        .update(&CMX_2)
        .update(&EPHEMERAL_KEY_2)
        .update(&C_ENC_PREFIX_2)
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

fn build_circuit() -> TwoActionBenchCircuit {
    let nf_1 = pallas::Base::from_repr(NF_OLD).expect("valid field element");
    let cmx_1 = pallas::Base::from_repr(CMX).expect("valid field element");
    let nf_2 = pallas::Base::from_repr(NF_OLD_2).expect("valid field element");
    let cmx_2 = pallas::Base::from_repr(CMX_2).expect("valid field element");

    TwoActionBenchCircuit {
        nf_1: Value::known(nf_1),
        cmx_1: Value::known(cmx_1),
        epk_1: Value::known(EPHEMERAL_KEY),
        enc_1: Value::known(C_ENC_PREFIX),
        nf_2: Value::known(nf_2),
        cmx_2: Value::known(cmx_2),
        epk_2: Value::known(EPHEMERAL_KEY_2),
        enc_2: Value::known(C_ENC_PREFIX_2),
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let circuit = build_circuit();
    let expected_words = compute_expected_words();
    let instance_col: &[pallas::Base] = &expected_words;
    let instance_row: &[&[pallas::Base]] = &[instance_col];
    let instances: &[&[&[pallas::Base]]] = &[instance_row];

    // Build params/keys once for proving and verifying benchmarks
    let params: Params<vesta::Affine> = Params::new(K);
    let vk = plonk::keygen_vk(&params, &circuit).expect("keygen_vk should not fail");
    let pk = plonk::keygen_pk(&params, vk.clone(), &circuit).expect("keygen_pk should not fail");

    // Proving benchmark
    {
        let mut group = c.benchmark_group("blake2b_proving");
        group.sample_size(10);
        group.measurement_time(std::time::Duration::from_secs(1));
        group.bench_function("prove", |b| {
            b.iter(|| {
                let mut transcript = Blake2bWrite::<_, vesta::Affine, _>::init(vec![]);
                plonk::create_proof(
                    &params,
                    &pk,
                    &[circuit.clone()],
                    instances,
                    OsRng,
                    &mut transcript,
                )
                .expect("proof generation should not fail");
                transcript.finalize()
            });
        });
    }

    // Generate a proof once for verification benchmarks
    let proof = {
        let mut transcript = Blake2bWrite::<_, vesta::Affine, _>::init(vec![]);
        plonk::create_proof(&params, &pk, &[circuit], instances, OsRng, &mut transcript)
            .expect("proof generation should not fail");
        transcript.finalize()
    };

    // Verifying benchmark
    {
        let mut group = c.benchmark_group("blake2b_verifying");
        group.sample_size(10);
        group.measurement_time(std::time::Duration::from_secs(1));
        group.bench_function("verify", |b| {
            b.iter(|| {
                let strategy = SingleVerifier::new(&params);
                let mut transcript = Blake2bRead::init(&proof[..]);
                plonk::verify_proof(&params, &vk, strategy, instances, &mut transcript)
                    .expect("proof verification should not fail");
            });
        });
    }
}

#[cfg(unix)]
criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets = criterion_benchmark
}
#[cfg(windows)]
criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = criterion_benchmark
}
criterion_main!(benches);
