use crate::cli_utils::{
    assert_command_success, execute_resolc, RESOLC_YUL_FLAG, YUL_CONTRACT_PATH,
};

#[test]
fn llvm_arguments_work_with_yul_input() {
    let output_with_argument = execute_resolc(&[
        RESOLC_YUL_FLAG,
        YUL_CONTRACT_PATH,
        "--llvm-arg=-riscv-soften-spills'",
        "--bin",
    ]);
    assert_command_success(&output_with_argument, "Providing LLVM arguments");
    assert!(output_with_argument.success);
}
