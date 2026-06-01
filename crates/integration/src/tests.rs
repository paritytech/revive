use std::str::FromStr;

use alloy_primitives::*;
use alloy_sol_types::SolCall;
use resolc::test_utils::build_yul;
use revive_runner::*;
use SpecsAction::*;

use crate::cases::Contract;
use crate::cases::{DivConst, ModConst, SdivConst, SmodConst};

/// Parameters:
/// - The function name of the test
/// - The contract name to fill in empty code based on the file path
/// - The contract source file
macro_rules! test_spec {
    ($test_name:ident, $contract_name:literal, $source_file:literal) => {
        #[test]
        fn $test_name() {
            let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("should always exist");
            let path = format!("{manifest_dir}/../integration/contracts/{}", $source_file);
            Specs::from_comment($contract_name, &path).remove(0).run();
        }
    };
}

test_spec!(baseline, "Baseline", "Baseline.sol");
test_spec!(flipper, "Flipper", "flipper.sol");
test_spec!(fibonacci_recursive, "FibonacciRecursive", "Fibonacci.sol");
test_spec!(fibonacci_iterative, "FibonacciIterative", "Fibonacci.sol");
test_spec!(fibonacci_binet, "FibonacciBinet", "Fibonacci.sol");
test_spec!(hash_keccak_256, "TestSha3", "Crypto.sol");
test_spec!(erc20, "ERC20", "ERC20.sol");
test_spec!(computation, "Computation", "Computation.sol");
test_spec!(msize, "MSize", "MSize.sol");
test_spec!(sha1, "SHA1", "SHA1.sol");
test_spec!(block, "Block", "Block.sol");
test_spec!(mcopy, "MCopy", "MCopy.sol");
test_spec!(mcopy_overlap, "MCopyOverlap", "MCopyOverlap.sol");
test_spec!(events, "Events", "Events.sol");
test_spec!(storage, "Storage", "Storage.sol");
test_spec!(mstore8, "MStore8", "MStore8.sol");
test_spec!(address, "Context", "Context.sol");
test_spec!(value, "Value", "Value.sol");
test_spec!(create, "CreateB", "Create.sol");
test_spec!(call, "Caller", "Call.sol");
test_spec!(balance, "Balance", "Balance.sol");
test_spec!(return_data_oob, "ReturnDataOob", "ReturnDataOob.sol");
test_spec!(revert_data_oob, "RevertDataOob", "RevertDataOob.sol");
test_spec!(immutables, "Immutables", "Immutables.sol");
test_spec!(transaction, "Transaction", "Transaction.sol");
test_spec!(block_hash, "BlockHash", "BlockHash.sol");
test_spec!(delegate, "Delegate", "Delegate.sol");
test_spec!(gas_price, "GasPrice", "GasPrice.sol");
test_spec!(gas_left, "GasLeft", "GasLeft.sol");
test_spec!(gas_limit, "GasLimit", "GasLimit.sol");
test_spec!(base_fee, "BaseFee", "BaseFee.sol");
test_spec!(coinbase, "Coinbase", "Coinbase.sol");
test_spec!(create2, "CreateB", "Create2.sol");
test_spec!(transfer, "Transfer", "Transfer.sol");
test_spec!(send, "Send", "Send.sol");
test_spec!(function_pointer, "FunctionPointer", "FunctionPointer.sol");
test_spec!(mload, "MLoad", "MLoad.sol");
test_spec!(delegate_no_contract, "DelegateCaller", "DelegateCaller.sol");
test_spec!(function_type, "FunctionType", "FunctionType.sol");
test_spec!(layout_at, "LayoutAt", "LayoutAt.sol");
test_spec!(shift_arithmetic_right, "SAR", "SAR.sol");
test_spec!(add_mod_mul_mod, "AddModMulModTester", "AddModMulMod.sol");
test_spec!(ulongrem, "UlongRemTester", "UlongRem.sol");
test_spec!(memory_bounds, "MemoryBounds", "MemoryBounds.sol");
test_spec!(selfdestruct, "Selfdestruct", "Selfdestruct.sol");
test_spec!(clz, "CountLeadingZeros", "CountLeadingZeros.sol");
test_spec!(erc7201, "ERC7201", "ERC7201.sol");
test_spec!(call_gas, "CallGas", "CallGas.sol");
test_spec!(linker_symbol, "Linked", "Linked.sol");
test_spec!(
    struct_delete_storage,
    "StructDeleteStorage",
    "StructDeleteStorage.sol"
);
test_spec!(internal_fn, "InternalFn", "InternalFn.sol");
test_spec!(
    sub_type_validation,
    "SubTypeValidation",
    "SubTypeValidation.sol"
);
test_spec!(factorial, "Factorial", "Factorial.sol");
test_spec!(
    uint128_arithmetic,
    "Uint128Arithmetic",
    "Uint128Arithmetic.sol"
);
test_spec!(
    sub_underflow_zext,
    "SubUnderflowZext",
    "SubUnderflowZext.sol"
);

fn instantiate(path: &str, contract: &str) -> Vec<SpecsAction> {
    vec![Instantiate {
        origin: TestAddress::Alice,
        value: 0,
        gas_limit: Some(GAS_LIMIT),
        storage_deposit_limit: None,
        code: Code::Solidity {
            path: Some(path.into()),
            contract: contract.to_string(),
            solc_optimizer: None,
            libraries: Default::default(),
        },
        data: vec![],
        salt: OptionalHex::default(),
    }]
}

fn run_differential(actions: Vec<SpecsAction>) {
    Specs {
        differential: true,
        actions,
        ..Default::default()
    }
    .run();
}

/// Rust-driven fuzz against the stdlib `__ulongrem` slow path. Each iteration
/// computes (a, b, m) deterministically from the index and dispatches a single
/// `bigMulMod` call via the differential runner — there is no in-contract loop,
/// so PVM never sees more than one mulmod per call dispatch. Any divergence
/// shows up as a returndata mismatch on the specific failing iteration.
#[test]
fn ulongrem_fuzz() {
    use alloy_primitives::keccak256;

    let mut actions = instantiate("contracts/UlongRem.sol", "UlongRem");

    for i in 0u64..256 {
        let derive = |tag: &[u8]| -> U256 {
            let mut buf = Vec::with_capacity(8 + tag.len());
            buf.extend_from_slice(&i.to_be_bytes());
            buf.extend_from_slice(tag);
            U256::from_be_bytes::<32>(keccak256(&buf).0)
        };
        let a = derive(b"a");
        let b = derive(b"b");
        // Force modulus into the slow path (m >= 2^255).
        let m = derive(b"m") | (U256::from(1u64) << 255);

        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: Contract::ulongrem_big_mulmod(a, b, m).calldata,
        });
    }

    run_differential(actions);
}

#[test]
fn bitwise_byte() {
    let mut actions = instantiate("contracts/Bitwise.sol", "Bitwise");

    let de_bruijn_sequence =
        hex::decode("4060503824160d0784426150b864361d0f88c4a27148ac5a2f198d46e391d8f4").unwrap();
    let value = U256::from_be_bytes::<32>(de_bruijn_sequence.clone().try_into().unwrap());
    for input in de_bruijn_sequence
        .iter()
        .enumerate()
        .map(|(index, _)| Contract::bitwise_byte(U256::from(index), value).calldata)
        .chain([
            Contract::bitwise_byte(U256::ZERO, U256::ZERO).calldata,
            Contract::bitwise_byte(U256::ZERO, U256::MAX).calldata,
            Contract::bitwise_byte(U256::MAX, U256::ZERO).calldata,
            Contract::bitwise_byte(U256::from_str("18446744073709551619").unwrap(), U256::MAX)
                .calldata,
        ])
    {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: input,
        })
    }

    run_differential(actions);
}

#[test]
fn unsigned_division() {
    let mut actions = instantiate("contracts/DivisionArithmetics.sol", "DivisionArithmetics");

    let one = U256::from(1);
    let two = U256::from(2);
    let five = U256::from(5);
    for (n, d) in [
        (five, five),
        (five, one),
        (U256::ZERO, U256::MAX),
        (five, two),
        (one, U256::ZERO),
    ] {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: Contract::division_arithmetics_div(n, d).calldata,
        })
    }

    run_differential(actions);
}

#[test]
fn signed_division() {
    let mut actions = instantiate("contracts/DivisionArithmetics.sol", "DivisionArithmetics");

    let one = I256::try_from(1).unwrap();
    let two = I256::try_from(2).unwrap();
    let minus_two = I256::try_from(-2).unwrap();
    let five = I256::try_from(5).unwrap();
    let minus_five = I256::try_from(-5).unwrap();
    for (n, d) in [
        (five, five),
        (five, one),
        (I256::ZERO, I256::MAX),
        (I256::ZERO, I256::MINUS_ONE),
        (five, two),
        (five, I256::MINUS_ONE),
        (I256::MINUS_ONE, minus_two),
        (minus_five, minus_five),
        (minus_five, two),
        (I256::MINUS_ONE, I256::MIN),
        (one, I256::ZERO),
        (I256::MIN, I256::MINUS_ONE),
        (I256::MIN + I256::ONE, I256::MINUS_ONE),
    ] {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: Contract::division_arithmetics_sdiv(n, d).calldata,
        })
    }

    run_differential(actions);
}

#[test]
fn unsigned_remainder() {
    let mut actions = instantiate("contracts/DivisionArithmetics.sol", "DivisionArithmetics");

    let one = U256::from(1);
    let two = U256::from(2);
    let five = U256::from(5);
    for (n, d) in [
        (five, five),
        (five, one),
        (U256::ZERO, U256::MAX),
        (U256::MAX, U256::MAX),
        (five, two),
        (two, five),
        (U256::MAX, U256::ZERO),
    ] {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: Contract::division_arithmetics_mod(n, d).calldata,
        })
    }

    run_differential(actions);
}

#[test]
fn signed_remainder() {
    let mut actions = instantiate("contracts/DivisionArithmetics.sol", "DivisionArithmetics");

    let one = I256::try_from(1).unwrap();
    let two = I256::try_from(2).unwrap();
    let minus_two = I256::try_from(-2).unwrap();
    let five = I256::try_from(5).unwrap();
    let minus_five = I256::try_from(-5).unwrap();
    for (n, d) in [
        (five, five),
        (five, one),
        (I256::ZERO, I256::MAX),
        (I256::MAX, I256::MAX),
        (five, two),
        (two, five),
        (five, minus_five),
        (five, I256::MINUS_ONE),
        (five, minus_two),
        (minus_five, two),
        (minus_two, five),
        (minus_five, minus_five),
        (minus_five, I256::MINUS_ONE),
        (minus_five, minus_two),
        (minus_two, minus_five),
        (I256::MIN, I256::MINUS_ONE),
        (I256::ZERO, I256::ZERO),
    ] {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: Contract::division_arithmetics_smod(n, d).calldata,
        })
    }

    run_differential(actions);
}

/// Regression: `div(x, x)` (and `sdiv(x, x)`) must return 0 when `x == 0`.
///
/// EVM defines `DIV` / `SDIV` to return 0 when the denominator is zero, so
/// `div(0, 0) == 0`. The newyork simplifier in `simplify.rs` previously
/// folded `div(x, x) → 1` and `sdiv(x, x) → 1` based on operand-identity
/// alone, which is only correct when `x != 0`. For `x == 0`, EVM returns 0
/// but PVM returned 1 — a semantic divergence visible to any contract that
/// passes 0 (intentionally or via uninitialised state) to such an
/// expression in inline assembly.
///
/// `mod(x, x)` is sound (mod by zero in EVM is also 0).
#[test]
fn div_self_zero_returns_zero() {
    let mut actions = instantiate("contracts/DivisionArithmetics.sol", "DivisionArithmetics");
    for x in [U256::ZERO, U256::from(1u64), U256::from(5u64), U256::MAX] {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: Contract::division_arithmetics_div_self(x).calldata,
        });
    }
    run_differential(actions);
}

#[test]
fn sdiv_self_zero_returns_zero() {
    let mut actions = instantiate("contracts/DivisionArithmetics.sol", "DivisionArithmetics");
    for x in [
        I256::ZERO,
        I256::try_from(1).unwrap(),
        I256::MINUS_ONE,
        I256::MIN,
        I256::MAX,
    ] {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: Contract::division_arithmetics_sdiv_self(x).calldata,
        });
    }
    run_differential(actions);
}

#[test]
fn mod_self_zero_returns_zero() {
    let mut actions = instantiate("contracts/DivisionArithmetics.sol", "DivisionArithmetics");
    for x in [U256::ZERO, U256::from(7u64), U256::MAX] {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: Contract::division_arithmetics_mod_self(x).calldata,
        });
    }
    run_differential(actions);
}

/// Regression: `narrow_function_params` narrows a function parameter that is
/// only used as a `MemoryOffset` (e.g. as `mload` offset) from `i256` to
/// `i64`. The call site emits a bare `trunc i256 → i64`, so an i256
/// argument with bits above 64 (e.g. `2^64`) silently aliases to `0`. The
/// use-site `safe_truncate_int_to_xlen` (i64 → i32) only sees the already-
/// truncated value and can't observe the discarded high bits.
///
/// Commit ccca38df fixed the same class of issue for `try_narrow_let_binding`
/// (let-binding demand narrowing) but parameter narrowing still uses
/// `constraint.max_width` directly, which `narrow_from_use(offset.id, I64)`
/// pulls down to I64. Differential mode catches the mismatch: EVM OOGs on
/// memory expansion, PVM returns 0 from the zero-initialised scratch slot.
#[test]
fn param_mload_huge_offset_traps() {
    for shift in [64u32, 128, 200] {
        let huge = U256::from(1u64) << shift;
        let mut actions = instantiate("contracts/ParamMload.sol", "ParamMload");
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: Contract::param_mload_try_fetch(huge).calldata,
        });
        Specs {
            actions,
            differential: true,
            ..Default::default()
        }
        .run();
    }
}

/// Regression: the panic-pattern outliner in `simplify.rs` extracts a
/// panic code via `to_u64_digits().first()` (lowest u64 digit), then
/// gates on `<= 0xFF`. A 256-bit code value with non-zero high bits but a
/// low byte `<= 0xFF` passes the check; codegen for the synthesised
/// `Statement::PanicRevert { code }` emits Solidity's canonical 36-byte
/// panic encoding with `code` zero-padded — the original high bits of the
/// mstored value are silently dropped from the revert data. EVM emits the
/// original bytes.
#[test]
fn panic_code_high_bits_preserved() {
    let mut actions = instantiate("contracts/PanicCodeBug.sol", "PanicCodeBug");
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::panic_code_bug_trigger().calldata,
    });
    run_differential(actions);
}

/// Regression: `mem_opt::optimize_statements` records each `mstore` at the
/// *aligned* word offset (`static_offset / 32 * 32`) but doesn't invalidate
/// adjacent words that an *unaligned* store partially overwrites. An
/// `mstore(0x70, v)` writes bytes [0x70, 0x90) — the lower half of the
/// word at 0x80 — yet the entry tracked at `memory_state[0x80]` (from a
/// previous aligned `mstore(0x80, …)`) survives untouched. A later
/// `mload(0x80)` matches that stale entry by exact-offset comparison and
/// is forwarded to the pre-overwrite value, dropping the bytes from the
/// unaligned write. EVM reads the actual mixed memory.
#[test]
fn unaligned_mstore_forwarding() {
    let mut actions = instantiate("contracts/UnalignedMStoreBug.sol", "UnalignedMStoreBug");
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::unaligned_mstore_bug().calldata,
    });
    run_differential(actions);
}

/// Regression: `to_llvm.rs::get_or_create_return_block` (and revert / Stop
/// siblings) build the constant offset/length via
/// `context.xlen_type().const_int(const_offset, false)`, which silently
/// truncates a `u64` to `i32`. A constant offset like `0x100000000000000`
/// (`2^56`) fits in `u64` so `try_extract_const_u64` returns `Some`, the
/// shared block path is taken, and the truncated offset (`0`) is fed to
/// `emit_exit_unchecked` — the return reads from `heap[0]` instead of
/// trapping on the out-of-bounds offset. EVM OOGs on memory expansion.
#[test]
fn const_return_offset_overflow_traps() {
    let mut actions = instantiate(
        "contracts/ConstReturnOverflowBug.sol",
        "ConstReturnOverflowBug",
    );
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::const_return_overflow_bug().calldata,
    });
    run_differential(actions);
}

/// Regression: `simplify.rs::find_panic_pattern_backwards` walks the
/// statement list backwards from `revert(0, 0x24)` and accepts an
/// `mstore` at offsets ≠ 0 / 4 as "safe filler" — it skips it in the
/// backward search and the `only_safe` gate only restricts statement
/// kinds, not memory regions touched. An intermediate `mstore(p, v)`
/// with `p` inside the panic encoding region (e.g. `p = 7`) partially
/// overwrites the EVM revert bytes EVM caller sees, but the
/// simplifier still matches the canonical panic shape and replaces
/// the whole sequence with `Statement::PanicRevert { code }` —
/// codegen then emits Solidity's canonical panic encoding without
/// the corrupting mstore. The caller observes different revert data
/// on PVM vs EVM.
#[test]
fn panic_pattern_intervening_mstore_preserved() {
    let mut actions = instantiate("contracts/PanicInterveneBug.sol", "PanicInterveneBug");
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::panic_intervene_bug().calldata,
    });
    run_differential(actions);
}

/// Regression: `heap_opt::fmp_native_safe()` does not check
/// `tainted_regions` or `escaping_regions`. A static `revert(0, 96)`
/// marks the FMP word (0x40) as escaping via `mark_escaping_range`,
/// but `Statement::Revert` (unlike `Statement::Return`) doesn't set
/// `has_return_covering_fmp`. The four flags `fmp_native_safe()` reads
/// stay false, so it returns true and `mstore(0x40, …)` is encoded as
/// a 4-byte i32 LE native store. The revert data bytes [0x40..0x60)
/// then carry the LE-encoded i32 followed by 28 stale bytes instead
/// of the BE 32-byte word EVM emits.
#[test]
fn fmp_revert_native_mode_corrupts_revert_data() {
    let mut actions = instantiate("contracts/FmpRevertBug.sol", "FmpRevertBug");
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::fmp_revert_bug().calldata,
    });
    run_differential(actions);
}

/// Bug #15a regression test: `to_llvm.rs::Expression::MLoad` applies
/// a range-proof truncation on any FMP-slot mload, assuming
/// `FMP < heap_size`. Sound for Solidity-convention FMP updates via
/// sbrk-style allocations but unsound for inline asm that puts an
/// arbitrary i256 at memory[0x40..0x60]. Fixed by gating on
/// `!heap_opt.fmp_could_be_unbounded()`, a precise static detector
/// that only trips when an `mstore(0x40, _)` writes from a source
/// `is_trusted_fmp_source` cannot prove sbrk-bounded. See
/// `project_round3_fmp_native_bugs.md` Bug #15a.
#[test]
fn fmp_load_range_proof_corrupts_non_bounded_value() {
    let mut actions = instantiate("contracts/FmpRangeProofBug.sol", "FmpRangeProofBug");
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::fmp_range_proof_bug().calldata,
    });
    run_differential(actions);
}

/// Bug #15b regression test: when `fmp_native_safe()` returns true,
/// the InlineNative-mode MStore at `to_llvm.rs` previously truncated
/// any `mstore(0x40, _)` value to i32 unconditionally. Fixed by
/// gating the i32 truncation on `!heap_opt.fmp_could_be_unbounded()`,
/// the same precise static gate used for Bug #15a's range proof. See
/// `project_round3_fmp_native_bugs.md` Bug #15b.
#[test]
fn fmp_native_store_truncates_non_bounded_value() {
    let mut actions = instantiate("contracts/FmpNativeStoreBug.sol", "FmpNativeStoreBug");
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::fmp_native_store_bug().calldata,
    });
    run_differential(actions);
}

/// Cross-subobject ValueId collision regression test: `HeapAnalysis`
/// shares its `value_expressions` map across deploy and deployed
/// objects, but `from_yul.rs` constructs a fresh `SsaBuilder` per
/// subobject so ValueIds restart at 0 inside each subobject. Before
/// the fix, the deploy code's `Let v0 := 0x80` (trusted Literal)
/// poisoned `value_expressions[0]`; the deployed object's recursive
/// `fun_rec(v0, v1)` then reused id 0 for its first parameter, and
/// `is_trusted_fmp_source(0)` returned the parent's trusted entry —
/// silently disabling the `fmp_could_be_unbounded` flag for the
/// `mstore(0x40, param)` inside the function. Fixed by clearing
/// `value_expressions` alongside `offset_values` when recursing into
/// subobjects in `HeapAnalysis::analyze_object`.
#[test]
fn fmp_cross_object_value_id_collision_corrupts_fmp() {
    let mut actions = instantiate("contracts/FmpCrossObjectBug.sol", "FmpCrossObjectBug");
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::fmp_cross_object_bug().calldata,
    });
    run_differential(actions);
}

#[test]
fn fmp_propagation_misses_dynamic_offset_mstore() {
    let mut actions = instantiate("contracts/FmpDynStoreBug.sol", "FmpDynStoreBug");
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::fmp_dyn_store_bug().calldata,
    });
    run_differential(actions);
}

#[test]
fn keccak_fuse_preserves_scratch_observable_via_mload() {
    let mut actions = instantiate("contracts/KeccakFuseBug.sol", "KeccakFuseBug");
    let seeds = [
        U256::from(0xaa01u64),
        U256::from(0xaa02u64),
        U256::from(0xaa03u64),
        U256::from(0xaa04u64),
        U256::from(0xaa05u64),
        U256::from(0xaa06u64),
        U256::from(0xaa07u64),
        U256::from(0xaa08u64),
    ];
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::keccak_fuse_bug_probe(seeds).calldata,
    });
    run_differential(actions);
}

/// Regression: same FMP-native-mode mismatch as
/// `fmp_revert_native_mode_corrupts_revert_data`, but the revert
/// uses *dynamic* offset and length read from calldata. The
/// `(None, _)` arm of `mark_escaping_range` only flips
/// `has_dynamic_escapes`; without lowering `min_dynamic_escape_start`
/// the FMP-native guard misses this case too. Fixed by
/// `note_fmp_coverage` lowering `min_dynamic_escape_start` to 0
/// for fully-dynamic escapes.
#[test]
fn fmp_dyn_revert_native_mode_corrupts_revert_data() {
    let mut actions = instantiate("contracts/FmpDynRevertBug.sol", "FmpDynRevertBug");
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::fmp_dyn_revert_bug().calldata,
    });
    run_differential(actions);
}

/// Regression: same shape as `unaligned_mstore_forwarding` (Bug #6),
/// but the second store is `mstore8` instead of `mstore`. mem_opt's
/// MStore8 handler removes only `memory_state[word(offset)]` and
/// doesn't invalidate the overlapping tracked entry from an earlier
/// *unaligned* `mstore` whose 32-byte write range covers the single
/// byte. A later `mload` matching the stale tracked offset gets
/// forwarded to the pre-overwrite value, dropping the byte overwrite.
#[test]
fn unaligned_mstore8_forwarding() {
    let mut actions = instantiate("contracts/UnalignedMStore8Bug.sol", "UnalignedMStore8Bug");
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::unaligned_mstore8_bug().calldata,
    });
    run_differential(actions);
}

/// Regression: `mem_opt::optimize_statements` `*Copy` handler
/// (CodeCopy / ExtCodeCopy / ReturnDataCopy / DataCopy / CallDataCopy)
/// removes only `memory_state[word(dest)]` when `dest` is statically
/// known. It ignores the copy's *length*, so additional words inside
/// `[dest, dest + length)` keep their stale tracked entries. A
/// subsequent `mload` matching the stale tracked offset is forwarded
/// to the pre-overwrite value while EVM reads the bytes that the copy
/// actually wrote.
///
/// The dynamic `length` argument defeats solc's dead-store
/// elimination of the sentinel mstore — solc can't prove that
/// `calldatacopy(0xc0, 0, length)` overwrites the sentinel at
/// `mstore(0xe0, …)` without knowing `length >= 0x40`.
#[test]
fn copy_length_invalidates_tracked_overlap() {
    use alloy_primitives::U256;
    let mut actions = instantiate("contracts/CopyOverlapBug.sol", "CopyOverlapBug");
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::copy_overlap_bug(U256::from(64u64)).calldata,
    });
    run_differential(actions);
}

/// Build calldata for the both-const Yul fixtures. Each fixture reads the case
/// index from `calldataload(0)` and a 32-byte "tag" from `calldataload(32)`
/// that gets XORed into the returned result. A non-zero tag turns silent
/// poison/undef from a UB-triggering const-fold into an observable divergence
/// from EVM (otherwise the buggy path coincidentally returns 0, which matches
/// EVM SMOD/SDIV's defined result for INT_MIN op -1 and masks the bug).
fn yul_which_calldata(which: u8) -> Vec<u8> {
    let mut data = vec![0u8; 64];
    data[31] = which;
    // tag: 0xdeadbeef padded into the second 32-byte word
    data[60..64].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    data
}

fn instantiate_yul(path: &str, contract: &str) -> Vec<SpecsAction> {
    vec![Instantiate {
        origin: TestAddress::Alice,
        value: 0,
        gas_limit: Some(GAS_LIMIT),
        storage_deposit_limit: None,
        code: Code::Yul {
            path: path.into(),
            contract: contract.to_string(),
        },
        data: vec![],
        salt: OptionalHex::default(),
    }]
}

fn unsigned_const_set() -> (U256, U256, U256, U256) {
    (U256::from(1), U256::from(2), U256::from(5), U256::MAX)
}

fn signed_const_set() -> (I256, I256, I256, I256, I256, I256, I256, I256) {
    (
        I256::try_from(1).unwrap(),
        I256::try_from(2).unwrap(),
        I256::try_from(-2).unwrap(),
        I256::try_from(5).unwrap(),
        I256::try_from(-5).unwrap(),
        I256::MIN,
        I256::MIN + I256::ONE,
        I256::MAX,
    )
}

fn div_rhs_const_data(n: U256, d: U256) -> Vec<u8> {
    let (one, two, five, max) = unsigned_const_set();
    if d == U256::ZERO {
        DivConst::divRhsZeroCall::new((n,)).abi_encode()
    } else if d == one {
        DivConst::divRhsOneCall::new((n,)).abi_encode()
    } else if d == two {
        DivConst::divRhsTwoCall::new((n,)).abi_encode()
    } else if d == five {
        DivConst::divRhsFiveCall::new((n,)).abi_encode()
    } else if d == max {
        DivConst::divRhsMaxCall::new((n,)).abi_encode()
    } else {
        panic!("no divRhsConst variant for d={d}")
    }
}

fn div_lhs_const_data(n: U256, d: U256) -> Vec<u8> {
    let (one, two, five, max) = unsigned_const_set();
    if n == U256::ZERO {
        DivConst::divLhsZeroCall::new((d,)).abi_encode()
    } else if n == one {
        DivConst::divLhsOneCall::new((d,)).abi_encode()
    } else if n == two {
        DivConst::divLhsTwoCall::new((d,)).abi_encode()
    } else if n == five {
        DivConst::divLhsFiveCall::new((d,)).abi_encode()
    } else if n == max {
        DivConst::divLhsMaxCall::new((d,)).abi_encode()
    } else {
        panic!("no divLhsConst variant for n={n}")
    }
}

fn mod_rhs_const_data(n: U256, d: U256) -> Vec<u8> {
    let (one, two, five, max) = unsigned_const_set();
    if d == U256::ZERO {
        ModConst::modRhsZeroCall::new((n,)).abi_encode()
    } else if d == one {
        ModConst::modRhsOneCall::new((n,)).abi_encode()
    } else if d == two {
        ModConst::modRhsTwoCall::new((n,)).abi_encode()
    } else if d == five {
        ModConst::modRhsFiveCall::new((n,)).abi_encode()
    } else if d == max {
        ModConst::modRhsMaxCall::new((n,)).abi_encode()
    } else {
        panic!("no modRhsConst variant for d={d}")
    }
}

fn mod_lhs_const_data(n: U256, d: U256) -> Vec<u8> {
    let (one, two, five, max) = unsigned_const_set();
    if n == U256::ZERO {
        ModConst::modLhsZeroCall::new((d,)).abi_encode()
    } else if n == one {
        ModConst::modLhsOneCall::new((d,)).abi_encode()
    } else if n == two {
        ModConst::modLhsTwoCall::new((d,)).abi_encode()
    } else if n == five {
        ModConst::modLhsFiveCall::new((d,)).abi_encode()
    } else if n == max {
        ModConst::modLhsMaxCall::new((d,)).abi_encode()
    } else {
        panic!("no modLhsConst variant for n={n}")
    }
}

fn sdiv_rhs_const_data(n: I256, d: I256) -> Vec<u8> {
    let (one, two, neg_two, five, neg_five, min, min_p1, max) = signed_const_set();
    if d == I256::ZERO {
        SdivConst::sdivRhsZeroCall::new((n,)).abi_encode()
    } else if d == one {
        SdivConst::sdivRhsOneCall::new((n,)).abi_encode()
    } else if d == I256::MINUS_ONE {
        SdivConst::sdivRhsNegOneCall::new((n,)).abi_encode()
    } else if d == two {
        SdivConst::sdivRhsTwoCall::new((n,)).abi_encode()
    } else if d == neg_two {
        SdivConst::sdivRhsNegTwoCall::new((n,)).abi_encode()
    } else if d == five {
        SdivConst::sdivRhsFiveCall::new((n,)).abi_encode()
    } else if d == neg_five {
        SdivConst::sdivRhsNegFiveCall::new((n,)).abi_encode()
    } else if d == min {
        SdivConst::sdivRhsMinCall::new((n,)).abi_encode()
    } else if d == min_p1 {
        SdivConst::sdivRhsMinPlusOneCall::new((n,)).abi_encode()
    } else if d == max {
        SdivConst::sdivRhsMaxCall::new((n,)).abi_encode()
    } else {
        panic!("no sdivRhsConst variant for d={d}")
    }
}

fn sdiv_lhs_const_data(n: I256, d: I256) -> Vec<u8> {
    let (one, two, neg_two, five, neg_five, min, min_p1, max) = signed_const_set();
    if n == I256::ZERO {
        SdivConst::sdivLhsZeroCall::new((d,)).abi_encode()
    } else if n == one {
        SdivConst::sdivLhsOneCall::new((d,)).abi_encode()
    } else if n == I256::MINUS_ONE {
        SdivConst::sdivLhsNegOneCall::new((d,)).abi_encode()
    } else if n == two {
        SdivConst::sdivLhsTwoCall::new((d,)).abi_encode()
    } else if n == neg_two {
        SdivConst::sdivLhsNegTwoCall::new((d,)).abi_encode()
    } else if n == five {
        SdivConst::sdivLhsFiveCall::new((d,)).abi_encode()
    } else if n == neg_five {
        SdivConst::sdivLhsNegFiveCall::new((d,)).abi_encode()
    } else if n == min {
        SdivConst::sdivLhsMinCall::new((d,)).abi_encode()
    } else if n == min_p1 {
        SdivConst::sdivLhsMinPlusOneCall::new((d,)).abi_encode()
    } else if n == max {
        SdivConst::sdivLhsMaxCall::new((d,)).abi_encode()
    } else {
        panic!("no sdivLhsConst variant for n={n}")
    }
}

fn smod_rhs_const_data(n: I256, d: I256) -> Vec<u8> {
    let (one, two, neg_two, five, neg_five, min, _min_p1, max) = signed_const_set();
    if d == I256::ZERO {
        SmodConst::smodRhsZeroCall::new((n,)).abi_encode()
    } else if d == one {
        SmodConst::smodRhsOneCall::new((n,)).abi_encode()
    } else if d == I256::MINUS_ONE {
        SmodConst::smodRhsNegOneCall::new((n,)).abi_encode()
    } else if d == two {
        SmodConst::smodRhsTwoCall::new((n,)).abi_encode()
    } else if d == neg_two {
        SmodConst::smodRhsNegTwoCall::new((n,)).abi_encode()
    } else if d == five {
        SmodConst::smodRhsFiveCall::new((n,)).abi_encode()
    } else if d == neg_five {
        SmodConst::smodRhsNegFiveCall::new((n,)).abi_encode()
    } else if d == min {
        SmodConst::smodRhsMinCall::new((n,)).abi_encode()
    } else if d == max {
        SmodConst::smodRhsMaxCall::new((n,)).abi_encode()
    } else {
        panic!("no smodRhsConst variant for d={d}")
    }
}

fn smod_lhs_const_data(n: I256, d: I256) -> Vec<u8> {
    let (one, two, neg_two, five, neg_five, min, _min_p1, max) = signed_const_set();
    if n == I256::ZERO {
        SmodConst::smodLhsZeroCall::new((d,)).abi_encode()
    } else if n == one {
        SmodConst::smodLhsOneCall::new((d,)).abi_encode()
    } else if n == I256::MINUS_ONE {
        SmodConst::smodLhsNegOneCall::new((d,)).abi_encode()
    } else if n == two {
        SmodConst::smodLhsTwoCall::new((d,)).abi_encode()
    } else if n == neg_two {
        SmodConst::smodLhsNegTwoCall::new((d,)).abi_encode()
    } else if n == five {
        SmodConst::smodLhsFiveCall::new((d,)).abi_encode()
    } else if n == neg_five {
        SmodConst::smodLhsNegFiveCall::new((d,)).abi_encode()
    } else if n == min {
        SmodConst::smodLhsMinCall::new((d,)).abi_encode()
    } else if n == max {
        SmodConst::smodLhsMaxCall::new((d,)).abi_encode()
    } else {
        panic!("no smodLhsConst variant for n={n}")
    }
}

fn push_call(actions: &mut Vec<SpecsAction>, dest: TestAddress, data: Vec<u8>) {
    actions.push(Call {
        origin: TestAddress::Alice,
        dest,
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data,
    });
}

#[test]
fn unsigned_division_half_const() {
    let mut actions = instantiate("contracts/DivisionArithmeticsConst.sol", "DivConst");
    let (one, two, five, _max) = unsigned_const_set();
    let pairs = [
        (five, five),
        (five, one),
        (U256::ZERO, U256::MAX),
        (five, two),
        (one, U256::ZERO),
    ];
    for (n, d) in pairs {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            div_rhs_const_data(n, d),
        );
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            div_lhs_const_data(n, d),
        );
    }
    run_differential(actions);
}

#[test]
fn unsigned_division_both_const() {
    let mut actions = instantiate_yul("contracts/DivBothConst.yul", "DivBothConst");
    let pair_count = 5;
    for i in 0..pair_count {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            yul_which_calldata(i),
        );
    }
    run_differential(actions);
}

#[test]
fn signed_division_half_const() {
    let mut actions = instantiate("contracts/DivisionArithmeticsConst.sol", "SdivConst");
    let (one, two, neg_two, five, neg_five, _min, _min_p1, _max) = signed_const_set();
    let pairs = [
        (five, five),
        (five, one),
        (I256::ZERO, I256::MAX),
        (I256::ZERO, I256::MINUS_ONE),
        (five, two),
        (five, I256::MINUS_ONE),
        (I256::MINUS_ONE, neg_two),
        (neg_five, neg_five),
        (neg_five, two),
        (I256::MINUS_ONE, I256::MIN),
        (one, I256::ZERO),
        (I256::MIN, I256::MINUS_ONE),
        (I256::MIN + I256::ONE, I256::MINUS_ONE),
    ];
    for (n, d) in pairs {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            sdiv_rhs_const_data(n, d),
        );
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            sdiv_lhs_const_data(n, d),
        );
    }
    run_differential(actions);
}

#[test]
fn signed_division_both_const() {
    let mut actions = instantiate_yul("contracts/SdivBothConst.yul", "SdivBothConst");
    let pair_count = 13;
    for i in 0..pair_count {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            yul_which_calldata(i),
        );
    }
    run_differential(actions);
}

#[test]
fn unsigned_remainder_half_const() {
    let mut actions = instantiate("contracts/DivisionArithmeticsConst.sol", "ModConst");
    let (one, two, five, _max) = unsigned_const_set();
    let pairs = [
        (five, five),
        (five, one),
        (U256::ZERO, U256::MAX),
        (U256::MAX, U256::MAX),
        (five, two),
        (two, five),
        (U256::MAX, U256::ZERO),
    ];
    for (n, d) in pairs {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            mod_rhs_const_data(n, d),
        );
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            mod_lhs_const_data(n, d),
        );
    }
    run_differential(actions);
}

#[test]
fn unsigned_remainder_both_const() {
    let mut actions = instantiate_yul("contracts/ModBothConst.yul", "ModBothConst");
    let pair_count = 7;
    for i in 0..pair_count {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            yul_which_calldata(i),
        );
    }
    run_differential(actions);
}

#[test]
fn signed_remainder_half_const() {
    let mut actions = instantiate("contracts/DivisionArithmeticsConst.sol", "SmodConst");
    let (one, two, neg_two, five, neg_five, _min, _min_p1, _max) = signed_const_set();
    let pairs = [
        (five, five),
        (five, one),
        (I256::ZERO, I256::MAX),
        (I256::MAX, I256::MAX),
        (five, two),
        (two, five),
        (five, neg_five),
        (five, I256::MINUS_ONE),
        (five, neg_two),
        (neg_five, two),
        (neg_two, five),
        (neg_five, neg_five),
        (neg_five, I256::MINUS_ONE),
        (neg_five, neg_two),
        (neg_two, neg_five),
        (I256::MIN, I256::MINUS_ONE),
        (I256::ZERO, I256::ZERO),
    ];
    for (n, d) in pairs {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            smod_rhs_const_data(n, d),
        );
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            smod_lhs_const_data(n, d),
        );
    }
    run_differential(actions);
}

#[test]
fn signed_remainder_both_const() {
    let mut actions = instantiate_yul("contracts/SmodBothConst.yul", "SmodBothConst");
    let pair_count = 17;
    for i in 0..pair_count {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            yul_which_calldata(i),
        );
    }
    run_differential(actions);
}

/// Surfaces the `smod(INT_MIN, -1)` LLVM-UB const-fold bug from
/// paritytech/revive#524. See `SmodIntMinNegOneBug.yul` for why this specific
/// fixture is shaped the way it is. Expected to FAIL until the bug is fixed.
#[test]
fn signed_remainder_int_min_neg_one_bug() {
    let mut actions = instantiate_yul("contracts/SmodIntMinNegOneBug.yul", "SmodIntMinNegOneBug");
    let mut tag = vec![0u8; 32];
    tag[28..32].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    push_call(&mut actions, TestAddress::Instantiated(0), tag);
    run_differential(actions);
}

/// Sibling to `signed_remainder_int_min_neg_one_bug`. Per the issue, sdiv is
/// claimed to be guarded; this test pins that guard so regressions surface.
#[test]
fn signed_division_int_min_neg_one_bug() {
    let mut actions = instantiate_yul("contracts/SdivIntMinNegOneBug.yul", "SdivIntMinNegOneBug");
    let mut tag = vec![0u8; 32];
    tag[28..32].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    push_call(&mut actions, TestAddress::Instantiated(0), tag);
    run_differential(actions);
}

/// Build the standard "tag plus extra calldata words" payload used by the
/// single-case probe fixtures.
fn probe_calldata(extra_words: &[U256]) -> Vec<u8> {
    let mut data = vec![0u8; 32 * (1 + extra_words.len())];
    data[28..32].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    for (i, word) in extra_words.iter().enumerate() {
        let off = 32 * (i + 1);
        data[off..off + 32].copy_from_slice(&word.to_be_bytes::<32>());
    }
    data
}

fn run_probe(path: &str, contract: &str, extra: &[U256]) {
    let mut actions = instantiate_yul(path, contract);
    push_call(
        &mut actions,
        TestAddress::Instantiated(0),
        probe_calldata(extra),
    );
    run_differential(actions);
}

#[test]
fn probe_shl_overflow() {
    run_probe(
        "contracts/ShlOverflowProbe.yul",
        "ShlOverflowProbe",
        &[U256::from(0x123456789abcdef_u64)],
    );
}

#[test]
fn probe_shr_overflow() {
    run_probe(
        "contracts/ShrOverflowProbe.yul",
        "ShrOverflowProbe",
        &[U256::from(0x123456789abcdef_u64)],
    );
}

#[test]
fn probe_sar_overflow() {
    run_probe("contracts/SarOverflowProbe.yul", "SarOverflowProbe", &[]);
}

#[test]
fn probe_addmod_zero() {
    run_probe(
        "contracts/AddModZeroProbe.yul",
        "AddModZeroProbe",
        &[U256::from(42), U256::from(99)],
    );
}

#[test]
fn probe_mulmod_zero() {
    run_probe(
        "contracts/MulModZeroProbe.yul",
        "MulModZeroProbe",
        &[U256::from(42), U256::from(99)],
    );
}

#[test]
fn probe_signextend_oob() {
    run_probe(
        "contracts/SignExtendOobProbe.yul",
        "SignExtendOobProbe",
        &[U256::MAX],
    );
}

#[test]
fn probe_byte_oob() {
    run_probe("contracts/ByteOobProbe.yul", "ByteOobProbe", &[U256::MAX]);
}

#[test]
fn probe_exp_zero_zero() {
    run_probe("contracts/ExpZeroZeroProbe.yul", "ExpZeroZeroProbe", &[]);
}

#[test]
fn ext_code_hash() {
    let mut actions = instantiate("contracts/ExtCode.sol", "ExtCode");

    // First do contract instantiation to figure out address and code hash
    let results = Specs {
        actions: actions.clone(),
        ..Default::default()
    }
    .run();
    let (addr, code_hash) = match results.first().cloned() {
        Some(CallResult::Instantiate {
            result, code_hash, ..
        }) => (result.result.unwrap().addr, code_hash),
        _ => panic!("instantiate contract failed"),
    };

    // code hash of itself
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::code_hash().calldata,
    });
    actions.push(VerifyCall(VerifyCallExpectation {
        success: true,
        output: OptionalHex::from(code_hash.as_bytes().to_vec()),
        gas_consumed: None,
    }));

    // code hash for a given contract address
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::ext_code_hash(Address::from(addr.to_fixed_bytes())).calldata,
    });
    actions.push(VerifyCall(VerifyCallExpectation {
        success: true,
        output: OptionalHex::from(code_hash.as_bytes().to_vec()),
        gas_consumed: None,
    }));

    // EOA returns fixed hash
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::ext_code_hash(Address::from(CHARLIE.to_fixed_bytes())).calldata,
    });
    actions.push(VerifyCall(VerifyCallExpectation {
        success: true,
        output: OptionalHex::from(
            hex!("c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470").to_vec(),
        ),
        gas_consumed: None,
    }));

    // non-existing account
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::ext_code_hash(Address::from([8u8; 20])).calldata,
    });
    actions.push(VerifyCall(VerifyCallExpectation {
        success: true,
        output: OptionalHex::from([0u8; 32].to_vec()),
        gas_consumed: None,
    }));

    Specs {
        actions,
        ..Default::default()
    }
    .run();
}

#[test]
fn ext_code_size() {
    let alice = Address::from(ALICE.0);
    let own_address = alice.create(0);
    let baseline_address = alice.create2([0u8; 32], keccak256(Contract::baseline().pvm_runtime));

    let own_code_size = U256::from(
        Contract::ext_code_size(Default::default())
            .pvm_runtime
            .len(),
    );
    let baseline_code_size = U256::from(Contract::baseline().pvm_runtime.len());

    Specs {
        actions: vec![
            // Instantiate the test contract
            instantiate("contracts/ExtCode.sol", "ExtCode").remove(0),
            // Instantiate the baseline contract
            Instantiate {
                origin: TestAddress::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: None,
                code: Code::Solidity {
                    path: Some("contracts/Baseline.sol".into()),
                    contract: "Baseline".to_string(),
                    solc_optimizer: None,
                    libraries: Default::default(),
                },
                data: vec![],
                salt: OptionalHex::from([0; 32]),
            },
            // Alice is not a contract and returns a code size of 0
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: 0,
                gas_limit: None,
                storage_deposit_limit: None,
                data: Contract::ext_code_size(alice).calldata,
            },
            VerifyCall(VerifyCallExpectation {
                success: true,
                output: OptionalHex::from([0u8; 32].to_vec()),
                gas_consumed: None,
            }),
            // Unknown address returns a code size of 0
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: 0,
                gas_limit: None,
                storage_deposit_limit: None,
                data: Contract::ext_code_size(Address::from([0xff; 20])).calldata,
            },
            VerifyCall(VerifyCallExpectation {
                success: true,
                output: OptionalHex::from([0u8; 32].to_vec()),
                gas_consumed: None,
            }),
            // Own address via extcodesize returns own code size
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: 0,
                gas_limit: None,
                storage_deposit_limit: None,
                data: Contract::ext_code_size(own_address).calldata,
            },
            VerifyCall(VerifyCallExpectation {
                success: true,
                output: OptionalHex::from(own_code_size.to_be_bytes::<32>().to_vec()),
                gas_consumed: None,
            }),
            // Own address via codesize returns own code size
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: 0,
                gas_limit: None,
                storage_deposit_limit: None,
                data: Contract::code_size().calldata,
            },
            VerifyCall(VerifyCallExpectation {
                success: true,
                output: OptionalHex::from(own_code_size.to_be_bytes::<32>().to_vec()),
                gas_consumed: None,
            }),
            // Baseline address returns the baseline code size
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: 0,
                gas_limit: None,
                storage_deposit_limit: None,
                data: Contract::ext_code_size(baseline_address).calldata,
            },
            VerifyCall(VerifyCallExpectation {
                success: true,
                output: OptionalHex::from(baseline_code_size.to_be_bytes::<32>().to_vec()),
                gas_consumed: None,
            }),
        ],
        ..Default::default()
    }
    .run();
}

#[test]
fn create2_salt() {
    let salt = U256::from(777);
    let predicted = Contract::predicted_constructor(salt).pvm_runtime;
    let predictor = Contract::address_predictor_constructor(salt, predicted.clone().into());
    Specs {
        actions: vec![
            Upload {
                origin: TestAddress::Alice,
                code: Code::Bytes(predicted),
                storage_deposit_limit: None,
            },
            Instantiate {
                origin: TestAddress::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: None,
                code: Code::Bytes(predictor.pvm_runtime),
                data: predictor.calldata,
                salt: OptionalHex::default(),
            },
        ],
        differential: false,
        ..Default::default()
    }
    .run();
}

#[test]
fn code_block_stops() {
    let code = &build_yul(&[(
        "poc.yul",
        r#"object "Test"{
  code {
    tstore(0x7fd9d641,0x7b1e022)
    returndatacopy(0x0,0x0,returndatasize())
  }
  object "Test_deployed" { code{} }
}"#,
    )])
    .unwrap()["poc.yul:Test"];

    Specs {
        actions: vec![
            Instantiate {
                origin: TestAddress::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: None,
                code: Code::Bytes(code.to_vec()),
                data: Default::default(),
                salt: OptionalHex::default(),
            },
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: Default::default(),
                gas_limit: None,
                storage_deposit_limit: None,
                data: Default::default(),
            },
            VerifyCall(Default::default()),
        ],
        differential: false,
        ..Default::default()
    }
    .run();
}

#[test]
fn code_block_with_nested_object_stops() {
    let code = &build_yul(&[(
        "poc.yul",
        r#"object "Test" {
    code {
        function allocate(size) -> ptr {
            ptr := mload(0x40)
            if iszero(ptr) { ptr := 0x60 }
            mstore(0x40, add(ptr, size))
        }
        let size := datasize("Test_deployed")
        let offset := allocate(size)
        datacopy(offset, dataoffset("Test_deployed"), size)
        return(offset, size)
    }
    object "Test_deployed" {
        code {
            sstore(0, 100)
	 }
        object "Test" {
            code {
	    revert(0,0)
            }
        }
    }
}"#,
    )])
    .unwrap()["poc.yul:Test"];

    Specs {
        actions: vec![
            Instantiate {
                origin: TestAddress::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: None,
                code: Code::Bytes(code.to_vec()),
                data: Default::default(),
                salt: OptionalHex::default(),
            },
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: Default::default(),
                gas_limit: None,
                storage_deposit_limit: None,
                data: Default::default(),
            },
            VerifyCall(Default::default()),
        ],
        differential: false,
        ..Default::default()
    }
    .run();
}

#[test]
fn sbrk_bounds_checks() {
    let code = &build_yul(&[(
        "poc.yul",
        r#"object "Test" {
    code {
        return(0x4, 0xffffffff)
        stop()
    }
    object "Test_deployed" {
        code {
            stop()
        }
    }
}"#,
    )])
    .unwrap()["poc.yul:Test"];

    let results = Specs {
        actions: vec![
            Instantiate {
                origin: TestAddress::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: None,
                code: Code::Bytes(code.to_vec()),
                data: Default::default(),
                salt: OptionalHex::default(),
            },
            VerifyCall(VerifyCallExpectation {
                success: false,
                ..Default::default()
            }),
        ],
        differential: false,
        ..Default::default()
    }
    .run();

    let CallResult::Instantiate { result, .. } = results.last().unwrap() else {
        unreachable!()
    };

    assert!(
        format!("{result:?}").contains("ContractTrapped"),
        "not seeing a trap means the contract did not catch the OOB"
    );
}

#[test]
fn invalid_opcode_works() {
    let code = &build_yul(&[(
        "invalid.yul",
        r#"object "Test" {
    code {
        invalid()
    }
    object "Test_deployed" {
        code {
            invalid()
        }
    }
}"#,
    )])
    .unwrap()["invalid.yul:Test"];

    let results = Specs {
        actions: vec![
            Instantiate {
                origin: TestAddress::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: None,
                code: Code::Bytes(code.to_vec()),
                data: Default::default(),
                salt: OptionalHex::default(),
            },
            VerifyCall(VerifyCallExpectation {
                success: false,
                ..Default::default()
            }),
        ],
        differential: false,
        ..Default::default()
    }
    .run();

    let CallResult::Instantiate { result, .. } = results.last().unwrap() else {
        unreachable!()
    };

    assert_eq!(result.weight_consumed, GAS_LIMIT);
}

/// Guard: `sdiv` of AND-masked i256 operands must agree with EVM. Today this
/// works only because LLVM gets there first — `__revive_signed_division`
/// holds the only `sdiv i256` we emit, and InstCombine rewrites it to
/// `udiv i256` (then to `udiv i64`) once it sees the AND-masks prove the
/// operands non-negative. The latent risk: revive's
/// `narrow_divrem_instructions` (crates/llvm-context/src/polkavm/context/mod.rs)
/// would, given a surviving `sdiv i256 (and a, M), (and b, M)`, rewrite it
/// to `sext (sdiv i64 (trunc..), (trunc..))`, which flips sign for any
/// operand whose bit (n-1) is set. If LLVM ever stops folding sdiv → udiv
/// before that pass, this differential test fails on `(high_bit, 1)` etc.
#[test]
fn sdiv_narrow_masked() {
    let mut actions = instantiate("contracts/SDivNarrowBug.sol", "SDivNarrowBug");

    let u64_max = U256::from(u64::MAX);
    let one = U256::from(1u64);
    let two = U256::from(2u64);
    let high_bit = U256::from(1u64) << 63;

    for (a, b) in [
        (u64_max, one),
        (high_bit, one),
        (high_bit + one, one),
        (u64_max, two),
    ] {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: Contract::sdiv_narrow_bug_masked(a, b).calldata,
        });
    }

    run_differential(actions);
}

/// Regression: `__revive_caller` must not share `@__address_spill_buffer`
/// with `tx.origin` / `address(this)`. The helper carries
/// `memory(inaccessiblemem: read)` so LLVM treats a `__revive_caller()` call
/// as having no `Other` (heap/global) effect. The previous body wrote the
/// caller into the shared spill buffer global — which `origin()` and
/// `build_address` also write — making the attribute a contract violation
/// from LLVM's point of view. No current Solidity emission pattern triggers
/// a miscompile, but any optimizer pass that hoisted a load of the spill
/// buffer past a `__revive_caller()` call would have corrupted the
/// surrounding `tx.origin` / `address(this)` result. Running the four
/// interleaved patterns through the differential harness asserts the
/// PVM-side values match EVM, guarding against future pipeline changes that
/// could expose the wrong attribute.
#[test]
fn caller_origin_aliasing() {
    let mut actions = instantiate("contracts/CallerOriginAliasing.sol", "CallerOriginAliasing");

    for calldata in [
        Contract::caller_origin_aliasing_caller_then_origin().calldata,
        Contract::caller_origin_aliasing_origin_then_caller().calldata,
        Contract::caller_origin_aliasing_caller_address_origin().calldata,
        Contract::caller_origin_aliasing_repeated_caller().calldata,
    ] {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: calldata,
        });
    }

    run_differential(actions);
}

/// Regression: `mload(huge_offset)` must trap, not silently return zero.
///
/// With the newyork backend, the calldata-loaded `_offset` was demand-narrowed
/// to i64 because its only use is a memory-offset (max-width I64). The
/// narrowing was a bare i256→i64 truncate with no overflow check, so values
/// like 2^128 (high bit in the upper i128 half) and 2^255 (high bit in the
/// top word) silently aliased to 0 and `mload(0)` returned the zero-initialized
/// scratch slot successfully. EVM correctly OOGs on the memory expansion.
/// Differential mode catches the mismatch.
#[test]
fn mload_huge_offset_traps() {
    for shift in [128u32, 255] {
        let huge = U256::from(1u64) << shift;
        let data = Contract::load_at(huge).calldata;
        let mut actions = instantiate("contracts/MLoad.sol", "MLoad");
        actions.append(&mut vec![
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: 0,
                gas_limit: None,
                storage_deposit_limit: None,
                data,
            },
            VerifyCall(VerifyCallExpectation {
                success: false,
                ..Default::default()
            }),
        ]);

        Specs {
            actions,
            differential: true,
            ..Default::default()
        }
        .run();
    }
}

/// Regression: `mload(0x40)` on a contract that only does inline-assembly
/// `mload(dynamic)` must return Solidity's free-memory pointer (0x80).
/// Fuzzer found divergence on offsets near 0x40 under newyork; suspected
/// heap-optimization native-mode / byteswap mismatch.
#[test]
fn mload_at_fmp_slot() {
    for &offset in &[0x40u64, 0x21, 0x3f, 0x42] {
        let data = Contract::load_at(Uint::from(offset)).calldata;
        let mut actions = instantiate("contracts/MLoad.sol", "MLoad");
        actions.append(&mut vec![Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data,
        }]);
        Specs {
            actions,
            differential: true,
            ..Default::default()
        }
        .run();
    }
}

/// Load from heap memory using an out of bounds offset and expect the
/// contract to hit the `invalid` syscall to use all gas (like on EVM).
///
/// The offset is picked such that a regular truncate would be in bounds.
#[test]
fn safe_truncate_int_to_xlen_works() {
    let offset = 0x10000000_00000000u64;
    let data = Contract::load_at(Uint::from(offset)).calldata;
    let mut actions = instantiate("contracts/MLoad.sol", "MLoad");
    actions.append(&mut vec![
        Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data,
        },
        VerifyCall(VerifyCallExpectation {
            success: false,
            ..Default::default()
        }),
    ]);

    let results = Specs {
        actions,
        differential: true,
        ..Default::default()
    }
    .run();

    let CallResult::Exec { result, .. } = results.last().unwrap() else {
        unreachable!()
    };

    assert_eq!(result.weight_consumed, GAS_LIMIT);
}
