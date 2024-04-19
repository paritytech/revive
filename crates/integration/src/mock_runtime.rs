//! Mock environment used for integration tests.
//! TODO: Switch to drink! once RISCV is ready in polkadot-sdk
use std::collections::HashMap;

use alloy_primitives::{Keccak256, U256};
use polkavm::{
    Caller, Config, Engine, ExportIndex, GasMeteringKind, Instance, Linker, Module, ModuleConfig,
    ProgramBlob, Trap,
};

#[derive(Default, Clone, Debug)]
pub struct State {
    pub input: Vec<u8>,
    pub output: CallOutput,
    pub value: u128,
    pub storage: HashMap<U256, U256>,
}

#[derive(Clone, Debug)]
pub struct CallOutput {
    pub flags: u32,
    pub data: Vec<u8>,
}

impl Default for CallOutput {
    fn default() -> Self {
        Self {
            flags: u32::MAX,
            data: Vec::new(),
        }
    }
}

impl State {
    pub fn new(input: Vec<u8>) -> Self {
        Self {
            input,
            ..Default::default()
        }
    }

    pub fn reset_output(&mut self) {
        self.output = Default::default();
    }

    pub fn assert_storage_key(&self, at: U256, expect: U256) {
        assert_eq!(self.storage[&at], expect);
    }
}

fn link_host_functions(engine: &Engine) -> Linker<State> {
    let mut linker = Linker::new(engine);

    linker
        .func_wrap(
            "input",
            |caller: Caller<State>, out_ptr: u32, out_len_ptr: u32| -> Result<(), Trap> {
                let (mut caller, state) = caller.split();

                assert!(state.input.len() <= caller.read_u32(out_len_ptr).unwrap() as usize);

                caller.write_memory(out_ptr, &state.input)?;
                caller.write_memory(out_len_ptr, &(state.input.len() as u32).to_le_bytes())?;

                Ok(())
            },
        )
        .unwrap();

    linker
        .func_wrap(
            "seal_return",
            |caller: Caller<State>, flags: u32, data_ptr: u32, data_len: u32| -> Result<(), Trap> {
                let (caller, state) = caller.split();

                state.output.flags = flags;
                state.output.data = caller.read_memory_into_vec(data_ptr, data_len)?;

                Err(Default::default())
            },
        )
        .unwrap();

    linker
        .func_wrap(
            "value_transferred",
            |caller: Caller<State>, out_ptr: u32, out_len_ptr: u32| -> Result<(), Trap> {
                let (mut caller, state) = caller.split();

                let value = state.value.to_le_bytes();

                caller.write_memory(out_ptr, &value)?;
                caller.write_memory(out_len_ptr, &(value.len() as u32).to_le_bytes())?;

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

                assert_eq!(key_len, 32, "storage key must be 32 bytes");
                assert_eq!(value_len, 32, "storage value must be 32 bytes");

                let key = caller.read_memory_into_vec(key_ptr, key_len)?;
                let value = caller.read_memory_into_vec(value_ptr, value_len)?;

                state.storage.insert(
                    U256::from_be_bytes::<32>(key.try_into().unwrap()),
                    U256::from_be_bytes::<32>(value.try_into().unwrap()),
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

                let value = state
                    .storage
                    .get(&U256::from_be_bytes::<32>(key.try_into().unwrap()))
                    .map(U256::to_be_bytes::<32>)
                    .unwrap_or_default();

                caller.write_memory(out_ptr, &value[..])?;
                caller.write_memory(out_len_ptr, &32u32.to_le_bytes())?;

                Ok(0)
            },
        )
        .unwrap();

    linker
        .func_wrap(
            "hash_keccak_256",
            |caller: Caller<State>,
             input_ptr: u32,
             input_len: u32,
             out_ptr: u32|
             -> Result<(), Trap> {
                let (mut caller, _) = caller.split();

                let pre = caller.read_memory_into_vec(input_ptr, input_len)?;

                let mut hasher = Keccak256::new();
                hasher.update(&pre);
                caller.write_memory(out_ptr, &hasher.finalize()[..])?;

                Ok(())
            },
        )
        .unwrap();

    linker
}

pub fn setup(config: Option<Config>) -> Engine {
    Engine::new(&config.unwrap_or_default()).unwrap()
}

pub fn recompile_code(code: &[u8], engine: &Engine) -> Module {
    let mut module_config = ModuleConfig::new();
    module_config.set_gas_metering(Some(GasMeteringKind::Sync));

    Module::new(engine, &module_config, code).unwrap()
}

pub fn instantiate_module(module: &Module, engine: &Engine) -> (Instance<State>, ExportIndex) {
    let export = module.lookup_export("call").unwrap();
    let func = link_host_functions(engine).instantiate_pre(module).unwrap();
    let instance = func.instantiate().unwrap();

    (instance, export)
}

pub fn prepare(code: &[u8], config: Option<Config>) -> (Instance<State>, ExportIndex) {
    let blob = ProgramBlob::parse(code).unwrap();

    let engine = Engine::new(&config.unwrap_or_default()).unwrap();

    let mut module_config = ModuleConfig::new();
    module_config.set_gas_metering(Some(GasMeteringKind::Sync));

    let module = Module::from_blob(&engine, &module_config, &blob).unwrap();
    let export = module.lookup_export("call").unwrap();
    let func = link_host_functions(&engine)
        .instantiate_pre(&module)
        .unwrap();
    let instance = func.instantiate().unwrap();

    (instance, export)
}

pub fn call(mut state: State, on: &mut Instance<State>, export: ExportIndex) -> State {
    state.reset_output();

    let mut state_args = polkavm::StateArgs::default();
    state_args.set_gas(polkavm::Gas::MAX);

    let call_args = polkavm::CallArgs::new(&mut state, export);

    init_logs();

    match on.call(state_args, call_args) {
        Err(polkavm::ExecutionError::Trap(_)) => state,
        Err(other) => panic!("unexpected error: {other}"),
        Ok(_) => panic!("unexpected return"),
    }
}

fn init_logs() {
    if std::env::var("RUST_LOG").is_ok() {
        #[cfg(test)]
        let test = true;
        #[cfg(not(test))]
        let test = false;
        let _ = env_logger::builder().is_test(test).try_init();
    }
}
