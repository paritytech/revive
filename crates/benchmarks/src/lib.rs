use polkavm::{BackendKind, Config, Engine, ExportIndex, Instance, SandboxKind};
use revive_integration::mock_runtime::{self, TransactionBuilder};
use revive_integration::mock_runtime::{State, Transaction};

pub fn prepare_pvm(
    code: &[u8],
    input: Vec<u8>,
    backend: BackendKind,
) -> (TransactionBuilder, Instance<Transaction>, ExportIndex) {
    let mut config = Config::new();
    config.set_backend(Some(backend));
    config.set_sandbox(Some(SandboxKind::Linux));

    let (instance, export_index) = mock_runtime::prepare(code, Some(config));
    let transaction = State::default()
        .transaction()
        .with_default_account(code)
        .calldata(input);

    (transaction, instance, export_index)
}

pub fn instantiate_engine(backend: BackendKind) -> Engine {
    let mut config = Config::new();
    config.set_backend(Some(backend));
    config.set_sandbox(Some(SandboxKind::Linux));
    mock_runtime::setup(Some(config))
}
