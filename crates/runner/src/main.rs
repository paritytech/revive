use std::path::PathBuf;

use clap::Parser;

use revive_runner::{Code, OptionalHex, Specs, SpecsAction::*, TestAddress};

/// Execute revive PolkaVM contracts locally.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Arguments {
    /// The hex encoded calldata for the contract call.
    #[arg(short, long)]
    calldata: Option<String>,

    /// The hex encoded calldata for the contract deployment.
    #[arg(short, long)]
    deploy_calldata: Option<String>,

    /// The hex encoded contract code blob to instantiate and execute.
    #[arg(short, long)]
    blob: Option<String>,

    /// The contract code to instantiate and execute.
    #[arg(short, long)]
    file: Option<PathBuf>,

    /// The origin account used to initiate the deploy and call transactions.
    #[arg(short, long)]
    origin: Option<TestAddress>,

    /// The value the call transaction is endowed with.
    #[arg(short, long)]
    value: Option<u128>,

    /// The value the deploy transaction is endowed with.
    #[arg(long)]
    deploy_value: Option<u128>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let arguments = Arguments::parse();

    let code = match (arguments.blob, arguments.file) {
        (Some(blob), None) => hex::decode(blob)
            .map_err(|error| anyhow::anyhow!("expected hex encoded PVM blob: {error}"))?,
        (None, Some(file)) => std::fs::read(&file).map_err(|error| {
            anyhow::anyhow!("unable to read PVM file {}: {error}", file.display())
        })?,
        _ => anyhow::bail!("should either provide a PVM blob or a PVM file"),
    };
    let calldata = match arguments.calldata {
        Some(calldata) => hex::decode(calldata)
            .map_err(|error| anyhow::anyhow!("expected hex encoded calldata: {error}"))?,
        None => vec![],
    };
    let deploy_calldata = match arguments.deploy_calldata {
        Some(calldata) => hex::decode(calldata)
            .map_err(|error| anyhow::anyhow!("expected hex encoded calldata: {error}"))?,
        None => vec![],
    };
    let origin = arguments.origin.unwrap_or(TestAddress::Alice);

    let actions = vec![
        Instantiate {
            origin: origin.clone(),
            value: arguments.deploy_value.unwrap_or(0),
            gas_limit: None,
            storage_deposit_limit: None,
            code: Code::Bytes(code),
            data: deploy_calldata,
            salt: OptionalHex::default(),
        },
        Call {
            origin,
            dest: TestAddress::Instantiated(0),
            value: arguments.value.unwrap_or(0),
            gas_limit: None,
            storage_deposit_limit: None,
            data: calldata,
        },
    ];

    Specs {
        actions,
        differential: false,
        ..Default::default()
    }
    .run();

    Ok(())
}
