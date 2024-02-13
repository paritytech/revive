//!
//! The if-conditional statement.
//!

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

use crate::yul::error::Error;
use crate::yul::lexer::token::location::Location;
use crate::yul::lexer::token::Token;
use crate::yul::lexer::Lexer;
use crate::yul::parser::statement::block::Block;
use crate::yul::parser::statement::expression::Expression;

///
/// The Yul if-conditional statement.
///
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct IfConditional {
    /// The location.
    pub location: Location,
    /// The condition expression.
    pub condition: Expression,
    /// The conditional block.
    pub block: Block,
}

impl IfConditional {
    ///
    /// The element parser.
    ///
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let token = crate::yul::parser::take_or_next(initial, lexer)?;
        let location = token.location;

        let condition = Expression::parse(lexer, Some(token))?;

        let block = Block::parse(lexer, None)?;

        Ok(Self {
            location,
            condition,
            block,
        })
    }

    ///
    /// Get the list of missing deployable libraries.
    ///
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        let mut libraries = self.condition.get_missing_libraries();
        libraries.extend(self.block.get_missing_libraries());
        libraries
    }
}

impl<D> era_compiler_llvm_context::EraVMWriteLLVM<D> for IfConditional
where
    D: era_compiler_llvm_context::EraVMDependency + Clone,
{
    fn into_llvm(
        self,
        context: &mut era_compiler_llvm_context::EraVMContext<D>,
    ) -> anyhow::Result<()> {
        let condition = self
            .condition
            .into_llvm(context)?
            .expect("Always exists")
            .to_llvm()
            .into_int_value();
        let condition = context.builder().build_int_z_extend_or_bit_cast(
            condition,
            context.field_type(),
            "if_condition_extended",
        )?;
        let condition = context.builder().build_int_compare(
            inkwell::IntPredicate::NE,
            condition,
            context.field_const(0),
            "if_condition_compared",
        )?;
        let main_block = context.append_basic_block("if_main");
        let join_block = context.append_basic_block("if_join");
        context.build_conditional_branch(condition, main_block, join_block);
        context.set_basic_block(main_block);
        self.block.into_llvm(context)?;
        context.build_unconditional_branch(join_block);
        context.set_basic_block(join_block);

        Ok(())
    }
}
