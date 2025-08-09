//! The if-conditional statement.

use std::collections::HashSet;

use serde::Deserialize;
use serde::Serialize;

use crate::error::Error;
use crate::lexer::token::location::Location;
use crate::lexer::token::Token;
use crate::lexer::Lexer;
use crate::parser::statement::block::Block;
use crate::parser::statement::expression::Expression;
use crate::visitor::AstNode;

/// The Yul if-conditional statement.
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
    /// The element parser.
    pub fn parse(lexer: &mut Lexer, initial: Option<Token>) -> Result<Self, Error> {
        let token = crate::parser::take_or_next(initial, lexer)?;
        let location = token.location;

        let condition = Expression::parse(lexer, Some(token))?;

        let block = Block::parse(lexer, None)?;

        Ok(Self {
            location,
            condition,
            block,
        })
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> HashSet<String> {
        let mut libraries = self.condition.get_missing_libraries();
        libraries.extend(self.block.get_missing_libraries());
        libraries
    }
}

impl<D> revive_llvm_context::PolkaVMWriteLLVM<D> for IfConditional
where
    D: revive_llvm_context::PolkaVMDependency + Clone,
{
    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext<D>) -> anyhow::Result<()> {
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
            "if_condition_extended",
        )?;
        let condition = context.builder().build_int_compare(
            inkwell::IntPredicate::NE,
            condition,
            context.word_const(0),
            "if_condition_compared",
        )?;
        let main_block = context.append_basic_block("if_main");
        let join_block = context.append_basic_block("if_join");
        context.build_conditional_branch(condition, main_block, join_block)?;
        context.set_basic_block(main_block);
        self.block.into_llvm(context)?;
        context.build_unconditional_branch(join_block);
        context.set_basic_block(join_block);

        Ok(())
    }
}

impl AstNode for IfConditional {
    fn accept(&self, ast_visitor: &mut impl crate::visitor::AstVisitor) {
        ast_visitor.visit_if_conditional(self);
    }

    fn visit_children(&self, ast_visitor: &mut impl crate::visitor::AstVisitor) {
        self.condition.accept(ast_visitor);
        self.block.accept(ast_visitor);
    }

    fn location(&self) -> Location {
        self.location
    }
}
