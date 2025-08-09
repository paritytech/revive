//! The location mapper utility maps YUL source locations to AST statements.

use std::{collections::HashMap, path::Path};

use revive_yul::{
    lexer::{token::location::Location, Lexer},
    parser::{
        identifier::Identifier,
        statement::{
            assignment::Assignment,
            block::Block,
            expression::{function_call::FunctionCall, literal::Literal},
            for_loop::ForLoop,
            function_definition::FunctionDefinition,
            if_conditional::IfConditional,
            object::Object,
            switch::Switch,
            variable_declaration::VariableDeclaration,
        },
    },
    visitor::{AstNode, AstVisitor},
};

/// Code attributed to an unknown location.
pub const OTHER: &str = "other";
/// Code attributed to a compiler internal location.
pub const INTERNAL: &str = "internal";
/// Code attributed to a block.
pub const BLOCK: &str = "block";
/// Code attributed to a function call.
pub const FUNCTION_CALL: &str = "function_call";
/// Code attributed to a for loop.
pub const FOR: &str = "for";
/// Code attributed to an if statement.
pub const IF: &str = "if";
/// Code attributed to a switch statement.
pub const SWITCH: &str = "switch";
/// Code attributed to a variable declaration.
pub const DECLARATION: &str = "let";
/// Code attributed to a variable assignement.
pub const ASSIGNMENT: &str = "assignment";
/// Code attributed to a function definition.
pub const FUNCTION_DEFINITION: &str = "function_definition";
/// Code attributed to an identifier.
pub const IDENTIFIER: &str = "identifier";
/// Code attributed to a literal.
pub const LITERAL: &str = "identifier";

/// The location to statements mapper.
pub struct LocationMapper(HashMap<Location, String>);

impl LocationMapper {
    /// Construct a [LocationMap] from the given YUL `source` file.
    pub fn map_locations(source: &Path) -> anyhow::Result<HashMap<Location, String>> {
        let mut lexer = Lexer::new(std::fs::read_to_string(source)?);
        let ast = Object::parse(&mut lexer, None).map_err(|error| {
            anyhow::anyhow!("Contract `{}` parsing error: {:?}", source.display(), error)
        })?;

        let mut location_map = Self(Default::default());
        ast.accept(&mut location_map);
        location_map.0.insert(Location::new(0, 0), OTHER.into());
        location_map.0.insert(Location::new(1, 0), INTERNAL.into());

        Ok(location_map.0)
    }
}

impl AstVisitor for LocationMapper {
    fn visit(&mut self, node: &impl AstNode) {
        node.visit_children(self);
    }

    fn visit_block(&mut self, node: &Block) {
        node.visit_children(self);
        self.0.insert(node.location, BLOCK.into());
    }

    fn visit_assignment(&mut self, node: &Assignment) {
        node.visit_children(self);
        self.0.insert(node.location, ASSIGNMENT.into());
    }

    fn visit_if_conditional(&mut self, node: &IfConditional) {
        node.visit_children(self);
        self.0.insert(node.location, IF.into());
    }

    fn visit_variable_declaration(&mut self, node: &VariableDeclaration) {
        node.visit_children(self);
        self.0.insert(node.location, DECLARATION.into());
    }

    fn visit_function_call(&mut self, node: &FunctionCall) {
        node.visit_children(self);
        self.0.insert(node.location, node.name.to_string());
    }

    fn visit_function_definition(&mut self, node: &FunctionDefinition) {
        node.visit_children(self);
        self.0.insert(node.location, FUNCTION_DEFINITION.into());
    }

    fn visit_identifier(&mut self, node: &Identifier) {
        node.visit_children(self);
        self.0.insert(node.location, IDENTIFIER.into());
    }

    fn visit_literal(&mut self, node: &Literal) {
        node.visit_children(self);
        self.0.insert(node.location, LITERAL.into());
    }

    fn visit_for_loop(&mut self, node: &ForLoop) {
        node.visit_children(self);
        self.0.insert(node.location, FOR.into());
    }

    fn visit_switch(&mut self, node: &Switch) {
        node.visit_children(self);
        self.0.insert(node.location, SWITCH.into());
    }
}
