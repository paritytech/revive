//! The contract compiled via newyork IR.
//!
//! This module integrates the newyork IR pipeline:
//! 1. Parse Yul source to Yul AST
//! 2. Translate Yul AST to newyork IR
//! 3. Generate LLVM IR from newyork IR

use std::collections::BTreeSet;

use inkwell::debug_info::AsDIScope;
use revive_llvm_context::PolkaVMCodeType;
use revive_newyork::{LlvmCodegen, Object as NewYorkObject};
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

    /// Translate the Yul AST to newyork IR.
    fn translate_to_ir(&self) -> anyhow::Result<NewYorkObject> {
        revive_newyork::translate_yul_object(&self.yul_object)
            .map_err(|e| anyhow::anyhow!("newyork IR translation: {e}"))
    }
}

impl revive_llvm_context::PolkaVMWriteLLVM for NewYork {
    fn declare(&mut self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        // Delegate to the Yul object's declare to set up all runtime functions.
        // This ensures all the necessary runtime support (heap, storage, events, etc.)
        // is available for the newyork IR codegen.
        self.yul_object.declare(context)
    }

    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext) -> anyhow::Result<()> {
        // Translate Yul AST to newyork IR
        let ir_object = self.translate_to_ir()?;

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
            let mut codegen = LlvmCodegen::new();
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

            // Generate the deploy code using newyork IR
            // Note: generate_object handles subobjects (inner_object) internally
            let mut codegen = LlvmCodegen::new();
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
