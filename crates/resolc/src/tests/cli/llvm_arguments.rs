use crate::tests::cli::utils::{
    assert_command_success, execute_resolc, RESOLC_YUL_FLAG, YUL_CONTRACT_PATH,
};

#[test]
fn llvm_arguments_work_with_yul_input() {
    let output_with_argument = execute_resolc(&[
        RESOLC_YUL_FLAG,
        YUL_CONTRACT_PATH,
        "--llvm-arg=-riscv-asm-relax-branches=false",
        "--bin",
    ]);
    assert_command_success(&output_with_argument, "Providing LLVM arguments");
    assert!(output_with_argument.success);

    let output_no_argument = execute_resolc(&[RESOLC_YUL_FLAG, YUL_CONTRACT_PATH, "--bin"]);
    assert_command_success(&output_no_argument, "Providing LLVM arguments");
    assert!(output_no_argument.success);

    assert_ne!(output_with_argument.stdout, output_no_argument.stdout);
}
