use evm_interpreter::{
    interpreter::{EtableInterpreter, RunInterpreter},
    trap::CallCreateTrap,
    Context, Etable, ExitError, Log, Machine, RuntimeBackend, RuntimeBaseBackend,
    RuntimeEnvironment, RuntimeState, TransactionContext, Valids,
};
use primitive_types::{H160, H256, U256};

static RUNTIME_ETABLE: Etable<RuntimeState, UnimplementedHandler, CallCreateTrap> =
    Etable::runtime();

pub struct UnimplementedHandler;

impl RuntimeEnvironment for UnimplementedHandler {
    fn block_hash(&self, _number: U256) -> H256 {
        unimplemented!()
    }
    fn block_number(&self) -> U256 {
        U256::from(123)
    }
    fn block_coinbase(&self) -> H160 {
        unimplemented!()
    }
    fn block_timestamp(&self) -> U256 {
        U256::from(456)
    }
    fn block_difficulty(&self) -> U256 {
        unimplemented!()
    }
    fn block_randomness(&self) -> Option<H256> {
        unimplemented!()
    }
    fn block_gas_limit(&self) -> U256 {
        unimplemented!()
    }
    fn block_base_fee_per_gas(&self) -> U256 {
        unimplemented!()
    }
    fn chain_id(&self) -> U256 {
        unimplemented!()
    }
}

impl RuntimeBaseBackend for UnimplementedHandler {
    fn balance(&self, _address: H160) -> U256 {
        unimplemented!()
    }
    fn code_size(&self, _address: H160) -> U256 {
        unimplemented!()
    }
    fn code_hash(&self, _address: H160) -> H256 {
        unimplemented!()
    }
    fn code(&self, _address: H160) -> Vec<u8> {
        unimplemented!()
    }
    fn storage(&self, _address: H160, _index: H256) -> H256 {
        unimplemented!()
    }

    fn exists(&self, _address: H160) -> bool {
        unimplemented!()
    }

    fn nonce(&self, _address: H160) -> U256 {
        unimplemented!()
    }
}

impl RuntimeBackend for UnimplementedHandler {
    fn original_storage(&self, _address: H160, _index: H256) -> H256 {
        unimplemented!()
    }

    fn deleted(&self, _address: H160) -> bool {
        unimplemented!()
    }
    fn is_cold(&self, _address: H160, _index: Option<H256>) -> bool {
        unimplemented!()
    }

    fn mark_hot(&mut self, _address: H160, _index: Option<H256>) {
        unimplemented!()
    }

    fn set_storage(&mut self, _address: H160, _index: H256, _value: H256) -> Result<(), ExitError> {
        unimplemented!()
    }
    fn log(&mut self, _log: Log) -> Result<(), ExitError> {
        unimplemented!()
    }
    fn mark_delete(&mut self, _address: H160) {
        unimplemented!()
    }

    fn reset_storage(&mut self, _address: H160) {
        unimplemented!()
    }

    fn set_code(&mut self, _address: H160, _code: Vec<u8>) -> Result<(), ExitError> {
        unimplemented!()
    }
    fn reset_balance(&mut self, _address: H160) {
        unimplemented!()
    }

    fn deposit(&mut self, _address: H160, _value: U256) {
        unimplemented!()
    }
    fn withdrawal(&mut self, _address: H160, _value: U256) -> Result<(), ExitError> {
        unimplemented!()
    }

    fn inc_nonce(&mut self, _address: H160) -> Result<(), ExitError> {
        unimplemented!()
    }
}

#[derive(Clone)]
pub struct PreparedEvm {
    pub valids: Valids,
    pub vm: Machine<RuntimeState>,
}

pub fn prepare(code: Vec<u8>, data: Vec<u8>) -> PreparedEvm {
    let state = RuntimeState {
        context: Context {
            address: H160::default(),
            caller: H160::default(),
            apparent_value: U256::default(),
        },
        transaction_context: TransactionContext {
            gas_price: U256::default(),
            origin: H160::default(),
        }
        .into(),
        retbuf: Vec::new(),
    };

    PreparedEvm {
        valids: Valids::new(&code[..]),
        vm: evm_interpreter::Machine::new(code.into(), data.to_vec().into(), 1024, 0xFFFF, state),
    }
}

pub fn execute(pre: PreparedEvm) -> Vec<u8> {
    let mut vm = EtableInterpreter::new_valid(pre.vm, &RUNTIME_ETABLE, pre.valids);
    vm.run(&mut UnimplementedHandler {})
        .exit()
        .unwrap()
        .unwrap();

    vm.retval.clone()
}
