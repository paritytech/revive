//! Mock environment used for integration tests.
//! TODO: Switch to drink! once RISCV is ready in polkadot-sdk
use std::collections::HashMap;

use parity_scale_codec::Encode;
use polkavm::{
    BackendKind, Caller, Config, Engine, ExecutionConfig, Gas, GasMeteringKind, InstancePre,
    Linker, Module, ModuleConfig, ProgramBlob, Trap,
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

                caller.write_memory(out_ptr, &state.input)?;
                caller.write_memory(out_len_ptr, &(state.input.len() as u32).encode())?;

                Ok(())
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
            "seal_return",
            |caller: Caller<State>, flags: u32, data_ptr: u32, data_len: u32| -> Result<(), Trap> {
                let (caller, state) = caller.split();

                state.output.0 = flags;
                state.output.1 = caller.read_memory_into_new_vec(data_ptr, data_len)?;

                Err(Default::default())
            },
        )
        .unwrap();

    linker
        .func_wrap(
            "debug_message",
            |caller: Caller<State>, str_ptr: u32, str_len: u32| -> Result<u32, Trap> {
                let (caller, _) = caller.split();

                let data = caller.read_memory_into_new_vec(str_ptr, str_len)?;
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

                let key = caller.read_memory_into_new_vec(key_ptr, key_len)?;
                let value = caller.read_memory_into_new_vec(value_ptr, value_len)?;

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

                let key = caller.read_memory_into_new_vec(key_ptr, key_len)?;
                let out_len = u32::from_le_bytes(
                    caller
                        .read_memory_into_new_vec(out_len_ptr, 4)?
                        .try_into()
                        .unwrap(),
                );
                assert!(out_len >= 32);

                let mut value = vec![0u8; 32];

                if let Some(storage_value) = state.storage.get(&U256::from_big_endian(&key[..])) {
                    storage_value.to_big_endian(&mut value)
                }

                caller.write_memory(out_ptr, &value[..])?;
                caller.write_memory(out_len_ptr, &32u32.to_le_bytes())?;

                Ok(0)
            },
        )
        .unwrap();

    linker
}

pub fn prepare(code: &[u8], input: Vec<u8>, backend: BackendKind) -> (State, InstancePre<State>) {
    let blob = ProgramBlob::parse(code).unwrap();

    let mut config = Config::new();
    config.set_allow_insecure(false);
    config.set_backend(Some(backend));

    let engine = Engine::new(&config).unwrap();

    let mut module_config = ModuleConfig::new();
    module_config.set_gas_metering(Some(GasMeteringKind::Async));

    let module = Module::from_blob(&engine, &module_config, &blob).unwrap();

    let func = link_host_functions(&engine)
        .instantiate_pre(&module)
        .unwrap();

    let state = State {
        input,
        ..Default::default()
    };

    (state, func)
}

pub fn call(mut state: State, on: InstancePre<State>) -> State {
    let mut config = ExecutionConfig::default();
    config.set_gas(Gas::MAX);

    on.instantiate()
        .unwrap()
        .get_func("call")
        .unwrap()
        .call_ex(&mut state, &[], &mut [], config)
        .unwrap_err();

    state
}