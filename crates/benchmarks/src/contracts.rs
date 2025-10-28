use std::{
    fs::{self, File},
    io::{Result, Write},
    path::Path,
};

const GENERATED_CONTRACTS_DIRECTORY: &str = "crates/benchmarks/generated/contracts";

#[derive(Clone)]
pub struct Contract {
    pub name: String,
    pub path: String,
}

impl Contract {
    pub fn build(name: String, path: String) -> Self {
        Self { name, path }
    }

    /// Builds and returns a contract which stores a `uint256`
    /// in the same memory location `n` times.
    pub fn store_uint256_n_times(n: u16) -> Self {
        let name = "StoreUint256";
        let mut code = format!(
            r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

contract {name} {{
    constructor() {{
        storeAndOverwrite();
    }}

    function f() external pure {{
        storeAndOverwrite();
    }}

    function storeAndOverwrite() internal pure {{
        uint256[1] memory zeros;
"#,
        );

        for _i in 0..n {
            code.push_str(
                r#"
        zeros[0] = 0;"#,
            );
        }

        code.push_str(
            r#"
    }
}
"#,
        );

        let contract_path = format!("{GENERATED_CONTRACTS_DIRECTORY}/{name}.sol");
        Contract::create_and_write_file(&contract_path, &code).expect("writing contract failed");

        Contract::build(name.to_string(), contract_path)
    }

    /// Creates a file at `path` and writes the `content` to it.
    fn create_and_write_file(path: &str, content: &str) -> Result<()> {
        let parent_dir = Path::new(path).parent().unwrap();
        fs::create_dir_all(parent_dir)?;
        let mut file = File::create(path)?;
        file.write_all(content.as_bytes())?;

        Ok(())
    }
}
