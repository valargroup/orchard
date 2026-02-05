//! Integration tests for the BLAKE2s circuit.

#![cfg(feature = "circuit")]

use halo2_proofs::{
    circuit::{floor_planner, Layouter, Value},
    dev::MockProver,
    plonk::{Circuit, ConstraintSystem, Error},
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
