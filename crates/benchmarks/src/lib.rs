#[cfg(feature = "bench-pvm-interpreter")]
pub fn create_specs(contract: &revive_integration::cases::Contract) -> revive_runner::Specs {
    use revive_runner::*;
    use SpecsAction::*;
    Specs {
        differential: false,
        actions: vec![
            Instantiate {
                code: Code::Bytes(contract.pvm_runtime.to_vec()),
                origin: TestAddress::Alice,
                data: Default::default(),
                value: Default::default(),
                gas_limit: Default::default(),
                storage_deposit_limit: Default::default(),
                salt: Default::default(),
            },
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                data: contract.calldata.to_vec(),
                value: Default::default(),
                gas_limit: Default::default(),
                storage_deposit_limit: Default::default(),
            },
        ],
        ..Default::default()
    }
}

#[cfg(feature = "bench-pvm-interpreter")]
pub fn measure_pvm(specs: &revive_runner::Specs, iters: u64) -> std::time::Duration {
    use revive_runner::*;
    let mut total_time = std::time::Duration::default();

    for _ in 0..iters {
        let results = specs.clone().run();

        let CallResult::Exec { result, wall_time } =
            results.get(1).expect("contract should have been called")
        else {
            panic!("expected a execution result");
        };
        let ret = result.result.as_ref().unwrap();
        assert!(!ret.did_revert());

        total_time += *wall_time;
    }

    total_time
}

#[cfg(feature = "bench-evm")]
pub fn measure_evm(code: &[u8], input: &[u8], iters: u64) -> std::time::Duration {
    let mut total_time = std::time::Duration::default();

    let code = hex::encode(code);

    for _ in 0..iters {
        let log = revive_differential::Evm::default()
            .code_blob(code.as_bytes().to_vec())
            .input(input.to_vec().into())
            .bench(true)
            .run();
        assert!(log.output.run_success(), "evm run failed: {log:?}");

        total_time += log.execution_time().unwrap();
    }

    total_time
}
