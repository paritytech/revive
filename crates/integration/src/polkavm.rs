//! Mock environment used for integration tests.
//! TODO: Switch to drink! once RISCV is ready in polkadot-sdk
use std::collections::HashMap;

use parity_scale_codec::Encode;
use polkavm::{
    BackendKind, Caller, Config, Engine, ExportIndex, GasMeteringKind, InstancePre, Linker, Module,
    ModuleConfig, ProgramBlob, Trap,
};
use primitive_types::U256;

#[derive(Default, Clone, Debug)]
pub struct State {
    pub input: Vec<u8>,
    pub output: (u32, Vec<u8>),
    pub value: u128,
    pub storage: HashMap<U256, U256>,
}

fn link_host_functions(engine: &Engine) -> Linker<State> {
    let mut linker = Linker::new(engine);

    linker
        .func_wrap(
            "input",
            |caller: Caller<State>, out_ptr: u32, out_len_ptr: u32| -> Result<(), Trap> {
                let (mut caller, state) = caller.split();

                assert_ne!(0, caller.read_u32(out_len_ptr)?);

                caller.write_memory(out_ptr, &state.input)?;
                caller.write_memory(out_len_ptr, &(state.input.len() as u32).encode())?;

                Ok(())
            },
        )
        .unwrap();

    linker
        .func_wrap(
            "seal_return",
            |caller: Caller<State>, flags: u32, data_ptr: u32, data_len: u32| -> Result<(), Trap> {
                let (caller, state) = caller.split();

                state.output.0 = flags;
                state.output.1 = caller.read_memory_into_vec(data_ptr, data_len)?;

                Err(Default::default())
            },
        )
        .unwrap();

    linker
        .func_wrap(
            "value_transferred",
            |caller: Caller<State>, out_ptr: u32, out_len_ptr: u32| -> Result<(), Trap> {
                let (mut caller, state) = caller.split();

                let value = state.value.encode();

                caller.write_memory(out_ptr, &value)?;
                caller.write_memory(out_len_ptr, &(value.len() as u32).encode())?;

                Ok(())
            },
        )
        .unwrap();

    linker
        .func_wrap(
            "debug_message",
            |caller: Caller<State>, str_ptr: u32, str_len: u32| -> Result<u32, Trap> {
                let (caller, _) = caller.split();

                let data = caller.read_memory_into_vec(str_ptr, str_len)?;
                print!("debug_message: {}", String::from_utf8(data).unwrap());

                Ok(0)
            },
        )
        .unwrap();

    linker
        .func_wrap(
            "set_storage",
            |caller: Caller<State>,
             key_ptr: u32,
             key_len: u32,
             value_ptr: u32,
             value_len: u32|
             -> Result<u32, Trap> {
                let (caller, state) = caller.split();

                let key = caller.read_memory_into_vec(key_ptr, key_len)?;
                let value = caller.read_memory_into_vec(value_ptr, value_len)?;

                state.storage.insert(
                    U256::from_big_endian(&key[..]),
                    U256::from_big_endian(&value[..]),
                );

                Ok(0)
            },
        )
        .unwrap();

    linker
        .func_wrap(
            "get_storage",
            |caller: Caller<State>,
             key_ptr: u32,
             key_len: u32,
             out_ptr: u32,
             out_len_ptr: u32|
             -> Result<u32, Trap> {
                let (mut caller, state) = caller.split();

                let key = caller.read_memory_into_vec(key_ptr, key_len)?;
                let out_len = caller.read_u32(out_len_ptr)?;
                assert!(out_len >= 32);

                let mut value = vec![0u8; 32];

                state
                    .storage
                    .get(&U256::from_big_endian(&key[..]))
                    .map(|storage_value| storage_value.to_big_endian(&mut value));

                caller.write_memory(out_ptr, &value[..])?;
                caller.write_memory(out_len_ptr, &32u32.to_le_bytes())?;

                Ok(0)
            },
        )
        .unwrap();

    linker
}

pub fn prepare(
    code: &[u8],
    input: Vec<u8>,
    backend: BackendKind,
) -> (State, InstancePre<State>, ExportIndex) {
    let blob = ProgramBlob::parse(code).unwrap();

    let mut config = Config::new();
    config.set_allow_insecure(true);
    config.set_backend(Some(backend));
    config.set_trace_execution(true);

    let engine = Engine::new(&config).unwrap();

    let mut module_config = ModuleConfig::new();
    module_config.set_gas_metering(Some(GasMeteringKind::Sync));

    let module = Module::from_blob(&engine, &module_config, &blob).unwrap();
    let export = module.lookup_export("call").unwrap();
    let func = link_host_functions(&engine)
        .instantiate_pre(&module)
        .unwrap();

    let state = State {
        input,
        ..Default::default()
    };

    (state, func, export)
}

pub fn call(mut state: State, on: InstancePre<State>, export: ExportIndex) -> State {
    let mut state_args = polkavm::StateArgs::default();
    state_args.set_gas(polkavm::Gas::MAX);

    let call_args = polkavm::CallArgs::new(&mut state, export);

    match on.instantiate().unwrap().call(state_args, call_args) {
        Err(polkavm::ExecutionError::Trap(_)) => state,
        Err(other) => panic!("unexpected error: {other}"),
        Ok(_) => panic!("unexpected return"),
    }
}
