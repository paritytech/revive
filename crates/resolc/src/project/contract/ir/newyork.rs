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

        if std::env::var(crate::RESOLC_DEBUG_IR_ENV).is_ok() {
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

/// Code-size threshold for emitting the outlined single-word keccak256 helper.
///
/// The helper body costs ~150 bytes; each call site it replaces saves ~20 bytes through
/// deduplication. Fewer than this many sites and the helper costs more than it returns.
const KECCAK_SINGLE_THRESHOLD: usize = 8;

/// Heap-operation threshold above which `__sbrk_internal` is marked NoInline.
///
/// `__sbrk_internal` has five basic blocks of bounds-checking; inlining it at too many
/// sites bloats the binary beyond the call-overhead savings on PolkaVM.
const SBRK_NOINLINE_THRESHOLD: usize = 30;

/// File name of the heap-analysis log appended inside the debug output directory
/// when [`crate::RESOLC_DEBUG_HEAP_ENV`] is set.
const HEAP_DEBUG_LOG_FILE: &str = "resolc_heap_debug.log";

impl revive_llvm_context::PolkaVMWriteLLVM for NewYork {
    fn declare(&mut self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        self.yul_object.declare(context)?;

        revive_llvm_context::PolkaVMKeccak256TwoWordsFunction.declare(context)?;
        revive_llvm_context::PolkaVMCallValueFunction.declare(context)?;
        revive_llvm_context::PolkaVMCallValueNonzeroFunction.declare(context)?;
        revive_llvm_context::PolkaVMCallDataLoadFunction.declare(context)?;
        revive_llvm_context::PolkaVMCallerFunction.declare(context)?;
        revive_llvm_context::PolkaVMRevertEmptyFunction.declare(context)?;
        revive_llvm_context::PolkaVMRevertFunction.declare(context)?;
        revive_llvm_context::PolkaVMRevertPanicFunction.declare(context)?;

        Ok(())
    }

    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        let translation_result = self.translate_to_ir()?;
        let ir_object = translation_result.object;

        if std::env::var(crate::RESOLC_DEBUG_IR_ENV).is_ok() {
            if let Some(output_directory) = context.debug_config().output_directory.as_ref() {
                use std::io::Write;
                let ir_text = revive_newyork::print_object(&ir_object);
                let mut file_path = output_directory.to_owned();
                file_path.push(format!("{}.newyork.txt", ir_object.name.replace('/', "_")));
                if let Ok(mut f) = std::fs::File::create(&file_path) {
                    let _ = writeln!(f, "{}", ir_text);
                }
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

        let heap_op_count = ir_object.count_heap_operations();
        let keccak_single_count = ir_object.count_keccak256_single();
        let has_keccak_single = keccak_single_count >= KECCAK_SINGLE_THRESHOLD;
        let use_native_heap = heap_opt.all_native();

        if std::env::var(crate::RESOLC_DEBUG_HEAP_ENV).is_ok() {
            if let Some(output_directory) = context.debug_config().output_directory.as_ref() {
                use std::io::Write;
                let mut log_path = output_directory.to_owned();
                log_path.push(HEAP_DEBUG_LOG_FILE);
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                {
                    let _ = writeln!(
                        file,
                        "HEAP_OPT [{}]: all_native={}, total={}, unknown={}, tainted={}, escaping={}, native_regions={:?}, native_offsets={:?}, dynamic_escapes={}, dynamic_accesses={}",
                        ir_object.name,
                        use_native_heap,
                        heap_opt.total_accesses,
                        heap_opt.unknown_accesses,
                        heap_opt.tainted_count,
                        heap_opt.escaping_count,
                        heap_opt.native_safe_regions,
                        heap_opt.native_safe_offsets,
                        heap_opt.has_dynamic_escapes,
                        heap_opt.has_dynamic_accesses,
                    );
                }
            }
        }

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
            context.set_code_type(PolkaVMCodeType::Runtime);

            let mut codegen = LlvmCodegen::new(
                heap_opt.clone(),
                type_info.clone(),
                inline_decisions.clone(),
            );
            codegen
                .generate_object(&ir_object, context)
                .map_err(|e| anyhow::anyhow!("newyork LLVM codegen: {e}"))?;
        } else {
            context.set_code_type(PolkaVMCodeType::Deploy);

            revive_llvm_context::PolkaVMEntryFunction::default().into_llvm(context)?;

            revive_llvm_context::PolkaVMLoadImmutableDataFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMStoreImmutableDataFunction.into_llvm(context)?;

            if use_native_heap {
                revive_llvm_context::PolkaVMLoadHeapWordNativeFunction.into_llvm(context)?;
                revive_llvm_context::PolkaVMStoreHeapWordNativeFunction.into_llvm(context)?;
            } else {
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
            if has_keccak_single {
                revive_llvm_context::PolkaVMKeccak256OneWordFunction.declare(context)?;
                revive_llvm_context::PolkaVMKeccak256OneWordFunction.into_llvm(context)?;
            }
            revive_llvm_context::PolkaVMCallValueFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMCallValueNonzeroFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMCallDataLoadFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMCallerFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMRevertEmptyFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMRevertFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMRevertPanicFunction.into_llvm(context)?;

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
