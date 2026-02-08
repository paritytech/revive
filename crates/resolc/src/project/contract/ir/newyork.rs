//! The contract compiled via newyork IR.
//!
//! This module integrates the newyork IR pipeline:
//! 1. Parse Yul source to Yul AST
//! 2. Translate Yul AST to newyork IR
//! 3. Run heap optimization analysis
//! 4. Generate LLVM IR from newyork IR

use std::collections::BTreeSet;

use inkwell::debug_info::AsDIScope;
use revive_llvm_context::PolkaVMCodeType;
use revive_newyork::{LlvmCodegen, TranslationResult};
use revive_yul::lexer::Lexer;
use revive_yul::parser::statement::object::Object as YulObject;
use serde::{Deserialize, Serialize};

/// The contract compiled via newyork IR.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NewYork {
    /// The Yul AST object (source).
    pub yul_object: YulObject,
}

impl NewYork {
    /// Transforms the `solc` standard JSON output contract into a Yul object
    /// for subsequent translation to newyork IR.
    pub fn try_from_source(source_code: &str) -> anyhow::Result<Option<Self>> {
        if source_code.is_empty() {
            return Ok(None);
        };

        let mut lexer = Lexer::new(source_code.to_owned());
        let object = YulObject::parse(&mut lexer, None)
            .map_err(|error| anyhow::anyhow!("Yul parsing: {error:?}"))?;

        Ok(Some(Self { yul_object: object }))
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> BTreeSet<String> {
        self.yul_object.get_missing_libraries()
    }

    /// Translate the Yul AST to newyork IR with heap optimization analysis.
    fn translate_to_ir(&self) -> anyhow::Result<TranslationResult> {
        let result = revive_newyork::translate_yul_object(&self.yul_object)
            .map_err(|e| anyhow::anyhow!("newyork IR translation: {e}"))?;

        // Debug: dump IR if RESOLC_DEBUG_IR is set
        if std::env::var("RESOLC_DEBUG_IR").is_ok() {
            use std::io::Write;
            let ir_text = revive_newyork::print_object(&result.object);
            let _ = writeln!(
                std::io::stderr(),
                "=== newyork IR for {} ===\n{}",
                result.object.name,
                ir_text
            );
            let _ = std::io::stderr().flush();
        }

        Ok(result)
    }
}

impl revive_llvm_context::PolkaVMWriteLLVM for NewYork {
    fn declare(&mut self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        // Delegate to the Yul object's declare to set up all runtime functions.
        // This ensures all the necessary runtime support (heap, storage, events, etc.)
        // is available for the newyork IR codegen.
        self.yul_object.declare(context)?;

        // Declare keccak256 two-words helper for deduplicating mapping hash patterns
        revive_llvm_context::PolkaVMKeccak256TwoWordsFunction.declare(context)?;

        // Declare outlined callvalue function for deduplicating non-payable checks
        revive_llvm_context::PolkaVMCallValueFunction.declare(context)?;

        // Declare outlined callvalue nonzero check for boolean-only callvalue usage
        revive_llvm_context::PolkaVMCallValueNonzeroFunction.declare(context)?;

        // Declare outlined calldataload function for deduplicating ABI decoding
        revive_llvm_context::PolkaVMCallDataLoadFunction.declare(context)?;

        // Declare outlined caller function for deduplicating msg.sender checks
        revive_llvm_context::PolkaVMCallerFunction.declare(context)?;

        // Declare outlined revert functions for deduplicating revert(0, K) patterns
        revive_llvm_context::PolkaVMRevertEmptyFunction.declare(context)?;
        revive_llvm_context::PolkaVMRevertFunction.declare(context)?;

        Ok(())
    }

    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        // Translate Yul AST to newyork IR with optimization analysis
        let translation_result = self.translate_to_ir()?;
        let ir_object = translation_result.object;

        // Debug: dump IR if RESOLC_DEBUG_IR is set
        if std::env::var("RESOLC_DEBUG_IR").is_ok() {
            use std::io::Write;
            let ir_text = revive_newyork::print_object(&ir_object);
            let debug_file = format!("/tmp/newyork_ir_{}.txt", ir_object.name.replace('/', "_"));
            if let Ok(mut f) = std::fs::File::create(&debug_file) {
                let _ = writeln!(f, "{}", ir_text);
            }
        }
        let heap_opt = translation_result.heap_opt;
        let type_info = translation_result.type_info;
        let inline_decisions: std::collections::BTreeMap<u32, revive_newyork::InlineDecision> =
            translation_result
                .inline_results
                .decisions
                .into_iter()
                .map(|(fid, decision)| (fid.0, decision))
                .collect();

        // Count heap operations to decide whether __sbrk_internal should be outlined.
        // For large contracts with many heap operations (MLoad/MStore/MCopy), outlining
        // sbrk saves significant code because the sbrk body (~7 basic blocks) is deduplicated.
        // For small contracts, the function call overhead outweighs the savings.
        let heap_op_count = ir_object.count_heap_operations();
        const SBRK_NOINLINE_THRESHOLD: usize = 20;
        if heap_op_count > SBRK_NOINLINE_THRESHOLD {
            if let Some(sbrk_func) = context.get_function("__sbrk_internal", false) {
                revive_llvm_context::PolkaVMFunction::set_attributes(
                    context.llvm(),
                    sbrk_func.borrow().declaration(),
                    &[revive_llvm_context::PolkaVMAttribute::NoInline],
                    true,
                );
            }
        }

        // NOTE: __revive_store_heap_word / __revive_load_heap_word NoInline was tested but
        // had ZERO effect on OZ contracts. LLVM's -Oz already keeps these as function calls.

        // NOTE: __revive_exit NoInline was tested but REGRESSED all OZ contracts by 2-4%.
        // When exit is not inlined, LLVM can't propagate range proofs (FMP, etc.) into the
        // exit function, forcing it to keep all overflow checks in safe_truncate_int_to_xlen.
        // The exit function is best left as AlwaysInline.

        // Check if we can use native-only heap mode (no byte-swapping needed)
        let use_native_heap = heap_opt.all_native();

        // Log heap analysis results (output to file for subprocess visibility)
        if std::env::var("RESOLC_DEBUG_HEAP").is_ok() {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/resolc_heap_debug.log")
            {
                let _ = writeln!(
                    file,
                    "HEAP_OPT [{}]: all_native={}, total={}, unknown={}, tainted={}, escaping={}, native_regions={}",
                    ir_object.name,
                    use_native_heap,
                    heap_opt.total_accesses,
                    heap_opt.unknown_accesses,
                    heap_opt.tainted_count,
                    heap_opt.escaping_count,
                    heap_opt.native_safe_regions.len(),
                );
            }
        }

        // Set up debug info scope if available
        if let Some(debug_info) = context.debug_info() {
            let di_builder = debug_info.builder();
            let object_name: &str = self.yul_object.identifier.as_str();
            let di_parent_scope = debug_info
                .top_scope()
                .expect("expected an existing debug-info scope");
            let object_scope = di_builder.create_namespace(di_parent_scope, object_name, true);
            context.push_debug_scope(object_scope.as_debug_info_scope());
        }

        context.set_debug_location(
            self.yul_object.location.line,
            self.yul_object.location.column,
            None,
        )?;

        if self.yul_object.identifier.ends_with("_deployed") {
            // Runtime code path
            context.set_code_type(PolkaVMCodeType::Runtime);

            // Generate the runtime code using newyork IR
            let mut codegen = LlvmCodegen::new(
                heap_opt.clone(),
                type_info.clone(),
                inline_decisions.clone(),
            );
            codegen
                .generate_object(&ir_object, context)
                .map_err(|e| anyhow::anyhow!("newyork LLVM codegen: {e}"))?;
        } else {
            // Deploy code path
            context.set_code_type(PolkaVMCodeType::Deploy);

            // Generate entry function
            revive_llvm_context::PolkaVMEntryFunction::default().into_llvm(context)?;

            // Generate runtime helper function bodies
            revive_llvm_context::PolkaVMLoadImmutableDataFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMStoreImmutableDataFunction.into_llvm(context)?;

            // Emit heap functions: either native-only OR byte-swapping versions
            // Using native-only saves ~200 bytes of code when all accesses are aligned
            if use_native_heap {
                // Native heap mode: no byte-swapping, use direct RISC-V load/store
                revive_llvm_context::PolkaVMLoadHeapWordNativeFunction.into_llvm(context)?;
                revive_llvm_context::PolkaVMStoreHeapWordNativeFunction.into_llvm(context)?;
            } else {
                // Standard mode: EVM-compatible big-endian byte order
                revive_llvm_context::PolkaVMLoadHeapWordFunction.into_llvm(context)?;
                revive_llvm_context::PolkaVMStoreHeapWordFunction.into_llvm(context)?;
            }

            revive_llvm_context::PolkaVMLoadStorageWordFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMStoreStorageWordFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMLoadTransientStorageWordFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMStoreTransientStorageWordFunction.into_llvm(context)?;

            revive_llvm_context::PolkaVMWordToPointerFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMExitFunction.into_llvm(context)?;

            revive_llvm_context::PolkaVMEventLogFunction::<0>.into_llvm(context)?;
            revive_llvm_context::PolkaVMEventLogFunction::<1>.into_llvm(context)?;
            revive_llvm_context::PolkaVMEventLogFunction::<2>.into_llvm(context)?;
            revive_llvm_context::PolkaVMEventLogFunction::<3>.into_llvm(context)?;
            revive_llvm_context::PolkaVMEventLogFunction::<4>.into_llvm(context)?;

            revive_llvm_context::PolkaVMDivisionFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMSignedDivisionFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMRemainderFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMSignedRemainderFunction.into_llvm(context)?;

            revive_llvm_context::PolkaVMSbrkFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMKeccak256TwoWordsFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMCallValueFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMCallValueNonzeroFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMCallDataLoadFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMCallerFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMRevertEmptyFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMRevertFunction.into_llvm(context)?;

            // Generate the deploy code using newyork IR
            // Note: generate_object handles subobjects (inner_object) internally
            let mut codegen = LlvmCodegen::new(heap_opt, type_info, inline_decisions);
            codegen
                .generate_object(&ir_object, context)
                .map_err(|e| anyhow::anyhow!("newyork LLVM codegen: {e}"))?;
        }

        context.set_debug_location(
            self.yul_object.location.line,
            self.yul_object.location.column,
            None,
        )?;

        context.pop_debug_scope();

        Ok(())
    }
}
