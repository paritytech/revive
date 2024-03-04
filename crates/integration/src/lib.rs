pub mod polkavm;

#[cfg(test)]
mod tests {
    #[test]
    fn flipper() {
        // TODO this is apparently not possible, so need to
        // factor out the helpers into this crate

        //use std::collections::BTreeMap;
        //let source = r#"contract Flipper {
        //    bool coin;
        //    function flip() public payable { coin = !coin; }
        //}"#;

        //let mut sources = BTreeMap::new();
        //sources.insert("flipper.sol".to_owned(), source.to_owned());

        //revive_solidity::tests::build_solidity(
        //    sources,
        //    BTreeMap::new(),
        //    None,
        //    revive_solidity::SolcPipeline::Yul,
        //    era_compiler_llvm_context::OptimizerSettings::cycles(),
        //)
        //.unwrap();
    }
    #[test]
    fn it_works() {
        let input = 0xcde4efa9u32.to_be_bytes().to_vec();
        let code = include_bytes!("/tmp/out.pvm");
        let (state, instance, export) =
            crate::polkavm::prepare(code, input, polkavm::BackendKind::Interpreter);
        let state = crate::polkavm::call(state, instance, export);
        dbg!(state);
    }
}
