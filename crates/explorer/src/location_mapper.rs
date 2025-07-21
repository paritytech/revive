//! The location mapper utility maps YUL source locations to AST statements.
//!
//! TODO: Refactor when the AST visitor is implemented.

use std::{collections::HashMap, path::Path};

use revive_yul::{
    lexer::{token::location::Location, Lexer},
    parser::statement::{
        block::Block,
        expression::{function_call::name::Name, Expression},
        object::Object,
        Statement,
    },
};

/// Code attributed to an unknown location.
pub const OTHER: &str = "other";
/// Code attributed to a compiler internal location.
pub const INTERNAL: &str = "internal";
/// Code attributed to a
pub const BLOCK: &str = "block";
pub const FUNCTION_CALL: &str = "function_call";
pub const FOR: &str = "for";
pub const IF: &str = "if";
pub const CONTINUE: &str = "continue";
pub const BREAK: &str = "break";
pub const LEAVE: &str = "leave";
pub const SWITCH: &str = "switch";
pub const DECLARATION: &str = "let";
pub const ASSIGNMENT: &str = "assignment";
pub const FUNCTION_DEFINITION: &str = "function_definition";

/// The location to statements map type alias.
pub type LocationMap = HashMap<Location, String>;

/// Construct a [LocationMap] from the given YUL `source` file.
pub fn map_locations(source: &Path) -> anyhow::Result<LocationMap> {
    let mut lexer = Lexer::new(std::fs::read_to_string(source)?);
    let ast = Object::parse(&mut lexer, None).map_err(|error| {
        anyhow::anyhow!("Contract `{}` parsing error: {:?}", source.display(), error)
    })?;

    let mut location_map = HashMap::with_capacity(1024);
    crate::location_mapper::object_mapper(&mut location_map, &ast);
    location_map.insert(Location::new(0, 0), OTHER.to_string());
    location_map.insert(Location::new(1, 0), INTERNAL.to_string());

    Ok(location_map)
}

/// Map the [Block].
fn block_mapper(map: &mut LocationMap, block: &Block) {
    map.insert(block.location, BLOCK.to_string());

    for statement in &block.statements {
        statement_mapper(map, statement);
    }
}

/// Map the [Expression].
fn expression_mapper(map: &mut LocationMap, expression: &Expression) {
    if let Expression::FunctionCall(call) = expression {
        let id = match call.name {
            Name::UserDefined(_) => FUNCTION_CALL.to_string(),
            _ => format!("{:?}", call.name),
        };
        map.insert(expression.location(), id);

        for expression in &call.arguments {
            expression_mapper(map, expression);
        }
    }
}

/// Map the [Statement].
fn statement_mapper(map: &mut LocationMap, statement: &Statement) {
    match statement {
        Statement::Object(object) => object_mapper(map, object),

        Statement::Code(code) => block_mapper(map, &code.block),

        Statement::Block(block) => block_mapper(map, block),

        Statement::ForLoop(for_loop) => {
            map.insert(for_loop.location, FOR.to_string());

            expression_mapper(map, &for_loop.condition);
            block_mapper(map, &for_loop.body);
            block_mapper(map, &for_loop.initializer);
            block_mapper(map, &for_loop.finalizer);
        }

        Statement::IfConditional(if_conditional) => {
            map.insert(if_conditional.location, IF.to_string());

            expression_mapper(map, &if_conditional.condition);
            block_mapper(map, &if_conditional.block);
        }

        Statement::Expression(expression) => expression_mapper(map, expression),

        Statement::Continue(location) => {
            map.insert(*location, CONTINUE.to_string());
        }

        Statement::Leave(location) => {
            map.insert(*location, LEAVE.to_string());
        }

        Statement::Break(location) => {
            map.insert(*location, BREAK.to_string());
        }

        Statement::Switch(switch) => {
            map.insert(switch.expression.location(), SWITCH.to_string());

            expression_mapper(map, &switch.expression);
            for case in &switch.cases {
                block_mapper(map, &case.block);
            }
            if let Some(block) = switch.default.as_ref() {
                block_mapper(map, block);
            }
        }

        Statement::Assignment(assignment) => {
            map.insert(assignment.location, ASSIGNMENT.to_string());

            expression_mapper(map, &assignment.initializer);
        }

        Statement::VariableDeclaration(declaration) => {
            map.insert(declaration.location, DECLARATION.to_string());

            if let Some(expression) = declaration.expression.as_ref() {
                expression_mapper(map, expression);
            }
        }

        Statement::FunctionDefinition(definition) => {
            map.insert(definition.location, FUNCTION_DEFINITION.to_string());

            block_mapper(map, &definition.body);
        }
    }
}

/// Map the [Object].
fn object_mapper(map: &mut LocationMap, object: &Object) {
    map.insert(object.location, object.identifier.clone());

    block_mapper(map, &object.code.block);

    if let Some(object) = object.inner_object.as_ref() {
        object_mapper(map, object);
    }
}
