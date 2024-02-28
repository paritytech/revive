pub mod polkavm;

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    #[test]
    fn flipper() {
        let source = r#"contract Flipper {
            bool coin;
            function flip() public payable { coin = !coin; }
        }"#;

        let mut sources = BTreeMap::new();
        sources.insert("flipper.sol".to_owned(), source.to_owned());

        revive_solidity::tests::build_solidity(
            sources,
            BTreeMap::new(),
            None,
            revive_solidity::SolcPipeline::Yul,
            era_compiler_llvm_context::OptimizerSettings::cycles(),
        )
        .unwrap();
    }
}
