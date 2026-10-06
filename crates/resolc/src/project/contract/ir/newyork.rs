//! The contract compiled via newyork IR.
//!
//! This module integrates the newyork IR pipeline:
//! 1. Parse Yul source to Yul AST
//! 2. Translate Yul AST to newyork IR
//! 3. Run heap optimization analysis
//! 4. Generate LLVM IR from newyork IR

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use inkwell::debug_info::AsDIScope;
use revive_common::Keccak256;
use revive_llvm_context::DebugConfig;
use revive_llvm_context::PolkaVMCodeType;
use revive_newyork::{CapturedStage, LlvmCodegen, StageSink, TranslationResult};
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
    ///
    /// `sink` receives the IR between optimization passes. `heap_size` is the
    /// configured EVM heap size in bytes.
    fn translate_to_ir(
        &self,
        sink: &mut StageSink<'_>,
        heap_size: u64,
    ) -> anyhow::Result<TranslationResult> {
        revive_newyork::translate_yul_object(&self.yul_object, sink, heap_size)
            .map_err(|e| anyhow::anyhow!("newyork IR translation: {e}"))
    }

    /// Builds the sink that writes one artifact per optimization pass.
    ///
    /// Returns a sink that serializes nothing unless stage export was requested.
    /// The sink owns a clone of the debug configuration so that it does not
    /// borrow the context, which codegen needs mutably afterwards.
    fn stage_sink(
        debug_config: &DebugConfig,
        stages: &Rc<RefCell<Vec<ManifestStage>>>,
    ) -> StageSink<'static> {
        if !debug_config.emit_ir_stages {
            return StageSink::disabled();
        }

        let debug_config = debug_config.clone();
        let stages = Rc::clone(stages);
        StageSink::new(Box::new(move |stage: CapturedStage<'_>| {
            let Some(file) = debug_config.dump_newyork_stage(stage.stage_id, stage.json)? else {
                return Ok(());
            };
            stages.borrow_mut().push(ManifestStage {
                id: stage.stage_id.to_owned(),
                pass: stage.pass.to_owned(),
                object: stage.object.to_owned(),
                ir_hash: stage.ir_hash.to_owned(),
                file,
            });

            Ok(())
        }))
    }

    /// Writes the Yul AST and the manifest indexing the stage artifacts.
    ///
    /// Also checks that the last exported stage is the IR codegen is about to
    /// receive. Nothing between translation and codegen mutates the IR today;
    /// the check is what keeps that true, since a chain whose final stage is not
    /// what was compiled describes a compilation that did not happen.
    ///
    /// A no-op when stage export is off.
    fn write_stage_manifest(
        &self,
        debug_config: &DebugConfig,
        stages: &Rc<RefCell<Vec<ManifestStage>>>,
        ir_object: &revive_newyork::Object,
        heap_size: u64,
    ) -> anyhow::Result<()> {
        if !debug_config.emit_ir_stages {
            return Ok(());
        }

        let stages = stages.borrow();
        if let Some(last) = stages.last() {
            let serialized = serde_json::to_string(ir_object)
                .map_err(|error| anyhow::anyhow!("serializing the final newyork IR: {error}"))?;
            let compiled = Keccak256::from_slice(serialized.as_bytes()).to_string();
            anyhow::ensure!(
                last.ir_hash == compiled,
                "ICE: the newyork IR changed after the last exported stage `{}`; \
                 the stage artifacts do not describe the compiled code",
                last.id,
            );
        }

        let yul_ast = serde_json::to_string(&self.yul_object)
            .map_err(|error| anyhow::anyhow!("serializing the Yul AST: {error}"))?;
        let manifest = StageManifest {
            schema: STAGE_MANIFEST_SCHEMA,
            contract: self.yul_object.identifier.as_str(),
            heap_size,
            yul_ast: debug_config.dump_yul_ast(&yul_ast)?,
            stages: stages.as_slice(),
        };
        let manifest = serde_json::to_string_pretty(&manifest)
            .map_err(|error| anyhow::anyhow!("serializing the stage manifest: {error}"))?;
        debug_config.dump_newyork_manifest(&manifest)?;

        Ok(())
    }
}

/// The stage manifest format version, raised whenever the shape below changes.
const STAGE_MANIFEST_SCHEMA: u32 = 1;

/// One entry in the stage manifest.
#[derive(Serialize)]
struct ManifestStage {
    /// The stage identifier, which is also the infix of the artifact's file name.
    id: String,
    /// The pass that produced this IR. Stable across pipeline edits that shift
    /// ordinals, so a consumer should key on this rather than on `id`.
    pass: String,
    /// The Yul object the stage belongs to. A contract's deploy and runtime
    /// halves run the same passes, and this is what separates them.
    object: String,
    /// Hash of the serialized IR alone. Equal hashes across adjacent stages of
    /// one object mean the pass left the IR untouched.
    ir_hash: String,
    /// The artifact's file name, beside the manifest.
    file: String,
}

/// Indexes the stage artifacts emitted for one contract.
///
/// The codegen dumps — LLVM IR, assembly, the object — are written after this
/// manifest and share its file-name stem, so they are not listed here.
#[derive(Serialize)]
struct StageManifest<'a> {
    /// See [`STAGE_MANIFEST_SCHEMA`].
    schema: u32,
    /// The Yul object identifier the chain was compiled from.
    contract: &'a str,
    /// The EVM heap size the pipeline was configured with, which the heap
    /// analysis depends on.
    heap_size: u64,
    /// The serialized Yul AST the chain starts from, if it was written.
    yul_ast: Option<String>,
    /// Every stage, in pipeline order.
    stages: &'a [ManifestStage],
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
        let heap_size = context
            .heap_size()
            .get_zero_extended_constant()
            .expect("the heap size is a compile time constant");
        let debug_config = context.debug_config().clone();
        let stages = Rc::new(RefCell::new(Vec::new()));
        let mut sink = Self::stage_sink(&debug_config, &stages);

        let translation_result = self.translate_to_ir(&mut sink, heap_size)?;
        if let Some(error) = sink.take_error() {
            return Err(error.context("writing newyork IR stage artifacts"));
        }
        let ir_object = translation_result.object;
        let heap_opt = translation_result.heap_opt;
        let type_info = translation_result.type_info;
        let mem_opt = translation_result.mem_opt;

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

        // Dump the newyork debug artifacts into the debug output directory, when configured.
        // The final IR is annotated with the inferred type widths so the narrow pass's effect is
        // visible.
        context.debug_config().dump_newyork(
            None,
            &revive_newyork::print_object_with_types(&ir_object, &type_info),
        )?;
        context.debug_config().dump_newyork(
            Some("heap"),
            &format!(
                "all_native={}, total={}, unknown={}, tainted={}, escaping={}, native_regions={:?}, native_offsets={:?}, dynamic_escapes={}, dynamic_accesses={}",
                use_native_heap,
                heap_opt.total_accesses,
                heap_opt.unknown_accesses,
                heap_opt.tainted_count,
                heap_opt.escaping_count,
                heap_opt.native_safe_regions,
                heap_opt.native_safe_offsets,
                heap_opt.has_dynamic_escapes,
                heap_opt.has_dynamic_accesses,
            ),
        )?;
        context.debug_config().dump_newyork(
            Some("mem"),
            &format!(
                "loads_eliminated={}, stores_eliminated={}, values_tracked={}, keccak_pairs_fused={}, keccak_singles_fused={}, fmp_loads_eliminated={}",
                mem_opt.loads_eliminated,
                mem_opt.stores_eliminated,
                mem_opt.values_tracked,
                mem_opt.keccak_pairs_fused,
                mem_opt.keccak_singles_fused,
                mem_opt.fmp_loads_eliminated,
            ),
        )?;

        self.write_stage_manifest(&debug_config, &stages, &ir_object, heap_size)?;

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

        // `NewYork` always wraps the top-level (deploy) object; the runtime
        // `_deployed` object is emitted as a subobject by `generate_object`,
        // which sets the runtime code type itself.
        assert!(
            !self.yul_object.identifier.ends_with("_deployed"),
            "ICE: newyork into_llvm expected the top-level object, got `{}`",
            self.yul_object.identifier,
        );

        context.set_code_type(PolkaVMCodeType::Deploy);

        revive_llvm_context::PolkaVMEntryFunction::default().into_llvm(context)?;

        revive_llvm_context::PolkaVMLoadImmutableDataFunction.into_llvm(context)?;
        revive_llvm_context::PolkaVMStoreImmutableDataFunction.into_llvm(context)?;

        if use_native_heap {
            revive_llvm_context::PolkaVMLoadHeapWordNativeFunction.declare(context)?;
            revive_llvm_context::PolkaVMStoreHeapWordNativeFunction.declare(context)?;
            revive_llvm_context::PolkaVMLoadHeapWordNativeFunction.into_llvm(context)?;
            revive_llvm_context::PolkaVMStoreHeapWordNativeFunction.into_llvm(context)?;
        }
        revive_llvm_context::PolkaVMLoadHeapWordFunction.into_llvm(context)?;
        revive_llvm_context::PolkaVMStoreHeapWordFunction.into_llvm(context)?;

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

        context.set_debug_location(
            self.yul_object.location.line,
            self.yul_object.location.column,
            None,
        )?;

        context.pop_debug_scope();

        Ok(())
    }
}
