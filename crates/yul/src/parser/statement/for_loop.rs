//! The for-loop statement.

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

use crate::error::Error;
use crate::lexer::token::location::Location;
use crate::lexer::token::Token;
use crate::lexer::Lexer;
use crate::parser::statement::block::Block;
use crate::parser::statement::expression::Expression;

/// The Yul for-loop statement.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ForLoop {
    /// The location.
    pub location: Location,
    /// The index variables initialization block.
    pub initializer: Block,
    /// The continue condition block.
    pub condition: Expression,
    /// The index variables mutating block.
    pub finalizer: Block,
    /// The loop body.
    pub body: Block,
}

impl ForLoop {
    /// The element parser.
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let token = crate::parser::take_or_next(initial, lexer)?;
        let location = token.location;

        let initializer = Block::parse(lexer, Some(token))?;

        let condition = Expression::parse(lexer, None)?;

        let finalizer = Block::parse(lexer, None)?;

        let body = Block::parse(lexer, None)?;

        Ok(Self {
            location,
            initializer,
            condition,
            finalizer,
            body,
        })
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        let mut libraries = self.initializer.get_missing_libraries();
        libraries.extend(self.condition.get_missing_libraries());
        libraries.extend(self.finalizer.get_missing_libraries());
        libraries.extend(self.body.get_missing_libraries());
        libraries
    }
}

impl<D> revive_llvm_context::PolkaVMWriteLLVM<D> for ForLoop
where
    D: revive_llvm_context::PolkaVMDependency + Clone,
{
    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext<D>) -> anyhow::Result<()> {
        self.initializer.into_llvm(context)?;

        let condition_block = context.append_basic_block("for_condition");
        let body_block = context.append_basic_block("for_body");
        let increment_block = context.append_basic_block("for_increment");
        let join_block = context.append_basic_block("for_join");

        context.set_debug_location(self.location.line, self.location.column, None)?;
        context.build_unconditional_branch(condition_block);
        context.set_basic_block(condition_block);
        let condition = self
            .condition
            .into_llvm(context)?
            .expect("Always exists")
            .access(context)?
            .into_int_value();
        context.set_debug_location(self.location.line, self.location.column, None)?;
        let condition = context.builder().build_int_z_extend_or_bit_cast(
            condition,
            context.word_type(),
            "for_condition_extended",
        )?;
        let condition = context.builder().build_int_compare(
            inkwell::IntPredicate::NE,
            condition,
            context.word_const(0),
            "for_condition_compared",
        )?;
        context.build_conditional_branch(condition, body_block, join_block)?;

        context.push_loop(body_block, increment_block, join_block);

        context.set_basic_block(body_block);
        self.body.into_llvm(context)?;
        context.build_unconditional_branch(increment_block);
        context.set_debug_location(self.location.line, self.location.column, None)?;

        context.set_basic_block(increment_block);
        self.finalizer.into_llvm(context)?;
        context.build_unconditional_branch(condition_block);
        context.set_debug_location(self.location.line, self.location.column, None)?;

        context.pop_loop();
        context.set_basic_block(join_block);

        Ok(())
    }
}
