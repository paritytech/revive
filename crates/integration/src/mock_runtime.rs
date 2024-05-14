//! Mock environment used for integration tests.
use std::collections::HashMap;

use alloy_primitives::{keccak256, Address, Keccak256, B256, U256};
use polkavm::{
    Caller, Config, Engine, ExportIndex, GasMeteringKind, Instance, Linker, Module, ModuleConfig,
    ProgramBlob, Trap,
};
use revive_llvm_context::polkavm_const::runtime_api;

/// The mocked blockchain account.
#[derive(Debug, Default, Clone)]
struct Account {
    value: U256,
    contract: Option<U256>,
    storage: HashMap<U256, U256>,
}

/// Emitted event data.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Event {
    pub address: Address,
    pub data: Vec<u8>,
    pub topics: Vec<U256>,
}

/// The result of the contract call.
#[derive(Debug, Default, Clone)]
pub struct CallOutput {
    /// The return flags.
    pub flags: ReturnFlags,
    /// The contract call output.
    pub data: Vec<u8>,
    /// The emitted events.
    pub events: Vec<Event>,
}

/// The contract blob export to be called.
#[derive(Clone, Debug, Default)]
enum Export {
    #[default]
    Call,
    Deploy(U256),
}

/// Possible contract call return flags.
#[derive(Debug, Default, Clone, PartialEq)]
#[repr(u32)]
pub enum ReturnFlags {
    /// The contract execution returned normally.
    Success = 0,
    /// The contract execution returned normally but state changes should be reverted.
    Revert = 1,
    /// The contract trapped unexpectedly during execution.
    #[default]
    Trap = u32::MAX,
}

impl From<u32> for ReturnFlags {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::Success,
            1 => Self::Revert,
            u32::MAX => Self::Trap,
            _ => panic!("invalid return flag: {value}"),
        }
    }
}

/// The local context inside the call stack.
#[derive(Debug, Clone)]
struct Frame {
    /// The account that is being executed.
    callee: Address,
    /// The caller account.
    caller: Address,
    /// The value transferred with this transaction.
    callvalue: U256,
    /// The calldata for the contract execution.
    input: Vec<u8>,
    // The contract call output.
    output: CallOutput,
    /// The export to call.
    export: Export,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            callee: Transaction::default_address(),
            caller: Transaction::default_address(),
            callvalue: Default::default(),
            input: Default::default(),
            output: Default::default(),
            export: Default::default(),
        }
    }
}

/// The transaction can modify the state by calling contracts.
///
/// Use the [TransactionBuilder] to create new transactions.
#[derive(Default, Clone, Debug)]
pub struct Transaction {
    state: State,
    stack: Vec<Frame>,
}

impl Transaction {
    pub fn default_address() -> Address {
        Address::default().create2(B256::default(), keccak256([]).0)
    }

    fn top_frame(&self) -> &Frame {
        self.stack.last().expect("transactions should have a frame")
    }

    fn top_frame_mut(&mut self) -> &mut Frame {
        self.stack
            .last_mut()
            .expect("transactions should have a frame")
    }

    fn top_account_mut(&mut self) -> &mut Account {
        let account = self.top_frame_mut().callee;
        self.state
            .accounts
            .get_mut(&account)
            .unwrap_or_else(|| panic!("callee has no associated account: {account}"))
    }

    fn create2(&self, salt: U256, blob_hash: U256) -> Address {
        self.top_frame()
            .callee
            .create2(salt.to_be_bytes(), blob_hash.to_be_bytes())
    }
}

/// Helper to create valid transactions.
#[derive(Default, Clone, Debug)]
pub struct TransactionBuilder {
    context: Transaction,
    state_before: State,
}

impl TransactionBuilder {
    /// Set the caller account.
    pub fn caller(mut self, account: Address) -> Self {
        self.context.top_frame_mut().caller = account;
        self
    }

    /// Set the callee account.
    pub fn callee(mut self, account: Address) -> Self {
        self.context.top_frame_mut().callee = account;
        self
    }

    /// Set the transferred callvalue.
    pub fn callvalue(mut self, amount: U256) -> Self {
        self.context.top_frame_mut().callvalue = amount;
        self
    }

    /// Set the calldata.
    pub fn calldata(mut self, data: Vec<u8>) -> Self {
        self.context.top_frame_mut().input = data;
        self
    }

    /// Set the transaction to deploy code.
    pub fn deploy(mut self, code: &[u8]) -> Self {
        let blob_hash = self.context.state.upload_code(code);
        self.context.top_frame_mut().export = Export::Deploy(blob_hash);
        self
    }

    /// Set the account at [Transaction::default_address] to the given `code`.
    ///
    /// Useful helper to spare the deploy transaction.
    pub fn with_default_account(mut self, code: &[u8]) -> Self {
        self.context.state.upload_code(code);
        self.context.state.create_account(
            Transaction::default_address(),
            Default::default(),
            keccak256(code).into(),
        );
        self
    }

    /// Execute the transaction with a default config backend.
    ///
    /// Reverts any state changes if the contract reverts or the exuection traps.
    pub fn call(mut self) -> (State, CallOutput) {
        let blob_hash = match self.context.top_frame().export {
            Export::Call => self.context.top_account_mut().contract.unwrap_or_else(|| {
                panic!(
                    "not a contract account: {}",
                    self.context.top_frame().callee
                )
            }),
            Export::Deploy(blob_hash) => blob_hash,
        };
        let code = self
            .context
            .state
            .blobs
            .get(&blob_hash)
            .unwrap_or_else(|| panic!("contract code not found: {blob_hash}"));
        let (mut instance, export) = prepare(code, None);
        self.call_on(&mut instance, export)
    }

    /// Execute the transaction on a given instance and export.
    /// The `instance` and `export` are expected to match that of the `Transaction`.

    /// Reverts any state changes if the contract reverts or the exuection traps.
    pub fn call_on(
        mut self,
        instance: &mut Instance<Transaction>,
        export: ExportIndex,
    ) -> (State, CallOutput) {
        let mut state_args = polkavm::StateArgs::default();
        state_args.set_gas(polkavm::Gas::MAX);

        if let Export::Deploy(blob_hash) = self.context.top_frame().export {
            self.context.state.create_account(
                self.context.create2(Default::default(), blob_hash),
                self.context.top_frame().callvalue,
                blob_hash,
            );
        }

        let callvalue = self.context.top_frame().callvalue;
        self.context.top_account_mut().value += callvalue;

        let call_args = polkavm::CallArgs::new(&mut self.context, export);

        init_logs();

        match instance.call(state_args, call_args) {
            Err(polkavm::ExecutionError::Trap(_)) => self.finalize(),
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("unexpected return"),
        }
    }

    /// Commits or reverts the state changes based on the call flags.
    fn finalize(mut self) -> (State, CallOutput) {
        let state = match self.context.top_frame().output.flags {
            ReturnFlags::Success => self.context.state,
            _ => self.state_before,
        };
        let output = self.context.stack.pop().unwrap().output;
        (state, output)
    }
}

impl From<State> for TransactionBuilder {
    fn from(state: State) -> Self {
        TransactionBuilder {
            state_before: state.clone(),
            context: Transaction {
                state,
                stack: Default::default(),
            },
        }
    }
}

/// The mocked blockchain state.
#[derive(Default, Clone, Debug)]
pub struct State {
    blobs: HashMap<U256, Vec<u8>>,
    accounts: HashMap<Address, Account>,
}

impl State {
    pub const BLOCK_NUMBER: u64 = 123;
    pub const BLOCK_TIMESTAMP: u64 = 456;

    pub fn transaction(self) -> TransactionBuilder {
        TransactionBuilder {
            state_before: self.clone(),
            context: Transaction {
                state: self,
                stack: vec![Default::default()],
            },
        }
    }

    pub fn upload_code(&mut self, code: &[u8]) -> U256 {
        let blob_hash = keccak256(code).into();
        self.blobs.insert(blob_hash, code.to_vec());
        blob_hash
    }

    pub fn assert_storage_key(&self, account: Address, key: U256, expected: U256) {
        assert_eq!(
            self.accounts
                .get(&account)
                .unwrap_or_else(|| panic!("unknown account: {account}"))
                .storage
                .get(&key)
                .copied()
                .unwrap_or_default(),
            expected
        );
    }

    pub fn create_account(&mut self, address: Address, value: U256, blob_hash: U256) {
        self.accounts.insert(
            address,
            Account {
                value,
                contract: Some(blob_hash),
                storage: HashMap::new(),
            },
        );
    }
}

fn link_host_functions(engine: &Engine) -> Linker<Transaction> {
    let mut linker = Linker::new(engine);

    linker
        .func_wrap(
            runtime_api::INPUT,
            |caller: Caller<Transaction>, out_ptr: u32, out_len_ptr: u32| -> Result<(), Trap> {
                let (mut caller, transaction) = caller.split();

                let input = &transaction.top_frame().input;
                assert!(input.len() <= caller.read_u32(out_len_ptr).unwrap() as usize);

                caller.write_memory(out_ptr, input)?;
                caller.write_memory(out_len_ptr, &(input.len() as u32).to_le_bytes())?;

                Ok(())
            },
        )
        .unwrap();

    linker
        .func_wrap(
            runtime_api::RETURN,
            |caller: Caller<Transaction>,
             flags: u32,
             data_ptr: u32,
             data_len: u32|
             -> Result<(), Trap> {
                let (caller, transaction) = caller.split();

                let frame = transaction.top_frame_mut();
                frame.output.flags = flags.into();
                frame.output.data = caller.read_memory_into_vec(data_ptr, data_len)?;

                Err(Default::default())
            },
        )
        .unwrap();

    linker
        .func_wrap(
            runtime_api::VALUE_TRANSFERRED,
            |caller: Caller<Transaction>, out_ptr: u32, out_len_ptr: u32| -> Result<(), Trap> {
                let (mut caller, transaction) = caller.split();

                let out_len = caller.read_u32(out_len_ptr)? as usize;
                assert_eq!(
                    out_len,
                    revive_common::BYTE_LENGTH_VALUE,
                    "spurious output buffer size: {out_len}"
                );

                let value = transaction.top_frame().callvalue.as_le_bytes();
                caller.write_memory(out_ptr, &value)?;
                caller.write_memory(out_len_ptr, &(value.len() as u32).to_le_bytes())?;

                Ok(())
            },
        )
        .unwrap();

    linker
        .func_wrap(
            "debug_message",
            |caller: Caller<Transaction>, str_ptr: u32, str_len: u32| -> Result<u32, Trap> {
                let (caller, _) = caller.split();

                let data = caller.read_memory_into_vec(str_ptr, str_len)?;
                print!("debug_message: {}", String::from_utf8(data).unwrap());

                Ok(0)
            },
        )
        .unwrap();

    linker
        .func_wrap(
            runtime_api::SET_STORAGE,
            |caller: Caller<Transaction>,
             key_ptr: u32,
             key_len: u32,
             value_ptr: u32,
             value_len: u32|
             -> Result<u32, Trap> {
                let (caller, transaction) = caller.split();

                assert_eq!(
                    key_len as usize,
                    revive_common::BYTE_LENGTH_WORD,
                    "storage key must be 32 bytes"
                );
                assert_eq!(
                    value_len as usize,
                    revive_common::BYTE_LENGTH_WORD,
                    "storage value must be 32 bytes"
                );

                let key = caller.read_memory_into_vec(key_ptr, key_len)?;
                let value = caller.read_memory_into_vec(value_ptr, value_len)?;

                transaction.top_account_mut().storage.insert(
                    U256::from_be_bytes::<32>(key.try_into().unwrap()),
                    U256::from_be_bytes::<32>(value.try_into().unwrap()),
                );

                Ok(0)
            },
        )
        .unwrap();

    linker
        .func_wrap(
            runtime_api::GET_STORAGE,
            |caller: Caller<Transaction>,
             key_ptr: u32,
             key_len: u32,
             out_ptr: u32,
             out_len_ptr: u32|
             -> Result<u32, Trap> {
                let (mut caller, transaction) = caller.split();

                let key = caller.read_memory_into_vec(key_ptr, key_len)?;
                let out_len = caller.read_u32(out_len_ptr)? as usize;
                assert_eq!(
                    out_len,
                    revive_common::BYTE_LENGTH_WORD,
                    "spurious output buffer size: {out_len}"
                );

                let value = transaction
                    .top_account_mut()
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
            runtime_api::HASH_KECCAK_256,
            |caller: Caller<Transaction>,
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
        .func_wrap(
            runtime_api::NOW,
            |caller: Caller<Transaction>, out_ptr: u32, out_len_ptr: u32| {
                let (mut caller, _) = caller.split();

                let out_len = caller.read_u32(out_len_ptr)? as usize;
                assert_eq!(
                    out_len,
                    revive_common::BYTE_LENGTH_BLOCK_TIMESTAMP,
                    "spurious output buffer size: {out_len}"
                );

                caller.write_memory(out_ptr, &State::BLOCK_TIMESTAMP.to_le_bytes())?;
                caller.write_memory(out_len_ptr, &64u32.to_le_bytes())?;

                Ok(())
            },
        )
        .unwrap();

    linker
        .func_wrap(
            runtime_api::BLOCK_NUMBER,
            |caller: Caller<Transaction>, out_ptr: u32, out_len_ptr: u32| {
                let (mut caller, _) = caller.split();

                let out_len = caller.read_u32(out_len_ptr)? as usize;
                assert_eq!(
                    out_len,
                    revive_common::BYTE_LENGTH_BLOCK_NUMBER,
                    "spurious output buffer size: {out_len}"
                );

                caller.write_memory(out_ptr, &State::BLOCK_NUMBER.to_le_bytes())?;
                caller.write_memory(out_len_ptr, &64u32.to_le_bytes())?;

                Ok(())
            },
        )
        .unwrap();

    linker
        .func_wrap(
            runtime_api::ADDRESS,
            |caller: Caller<Transaction>, out_ptr: u32, out_len_ptr: u32| {
                let (mut caller, transaction) = caller.split();

                let out_len = caller.read_u32(out_len_ptr)? as usize;
                assert_eq!(
                    out_len,
                    revive_common::BYTE_LENGTH_ETH_ADDRESS,
                    "spurious output buffer size: {out_len}"
                );

                let address = transaction.top_frame().callee.as_slice();
                caller.write_memory(out_ptr, address)?;
                caller.write_memory(out_len_ptr, &(address.len() as u32).to_le_bytes())?;

                Ok(())
            },
        )
        .unwrap();

    linker
        .func_wrap(
            runtime_api::CALLER,
            |caller: Caller<Transaction>, out_ptr: u32, out_len_ptr: u32| {
                let (mut caller, transaction) = caller.split();

                let out_len = caller.read_u32(out_len_ptr)? as usize;
                assert_eq!(
                    out_len,
                    revive_common::BYTE_LENGTH_ETH_ADDRESS,
                    "spurious output buffer size: {out_len}"
                );

                let address = transaction.top_frame().caller.as_slice();
                caller.write_memory(out_ptr, address)?;
                caller.write_memory(out_len_ptr, &(address.len() as u32).to_le_bytes())?;

                Ok(())
            },
        )
        .unwrap();

    linker
        .func_wrap(
            runtime_api::DEPOSIT_EVENT,
            |caller: Caller<Transaction>,
             topics_ptr: u32,
             topics_len: u32,
             data_ptr: u32,
             data_len: u32| {
                let (caller, transaction) = caller.split();

                let address = transaction.top_frame().callee;
                let data = if data_len != 0 {
                    caller.read_memory_into_vec(data_ptr, data_len)?
                } else {
                    Default::default()
                };
                let topics = if topics_len != 0 {
                    caller
                        .read_memory_into_vec(topics_ptr, topics_len)?
                        .chunks(32)
                        .map(|chunk| U256::from_be_slice(chunk))
                        .collect()
                } else {
                    Default::default()
                };

                transaction.top_frame_mut().output.events.push(Event {
                    address,
                    data,
                    topics,
                });

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

pub fn instantiate_module(
    module: &Module,
    engine: &Engine,
) -> (Instance<Transaction>, ExportIndex) {
    let export = module.lookup_export(runtime_api::CALL).unwrap();
    let func = link_host_functions(engine).instantiate_pre(module).unwrap();
    let instance = func.instantiate().unwrap();

    (instance, export)
}

pub fn prepare(code: &[u8], config: Option<Config>) -> (Instance<Transaction>, ExportIndex) {
    let blob = ProgramBlob::parse(code).unwrap();

    let engine = Engine::new(&config.unwrap_or_default()).unwrap();

    let mut module_config = ModuleConfig::new();
    module_config.set_gas_metering(Some(GasMeteringKind::Sync));

    let module = Module::from_blob(&engine, &module_config, &blob).unwrap();
    let export = module.lookup_export(runtime_api::CALL).unwrap();
    let func = link_host_functions(&engine)
        .instantiate_pre(&module)
        .unwrap();
    let instance = func.instantiate().unwrap();

    (instance, export)
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
