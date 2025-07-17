//! Utility for building a map from source locations to AST statements.
//!
//! TODO: Refactor when the AST visitor is implemented.

use std::collections::HashMap;

use revive_yul::parser::statement::{
    block::Block,
    expression::{function_call::name::Name, Expression},
    object::Object,
    Statement,
};

/// Map the [Block].
pub fn block_mapper(map: &mut HashMap<u32, Vec<String>>, block: &Block) {
    map.entry(block.location.line)
        .or_default()
        .push("block".to_string());

    for statement in &block.statements {
        statement_mapper(map, statement);
    }
}

/// Map the [Expression].
pub fn expression_mapper(map: &mut HashMap<u32, Vec<String>>, expression: &Expression) {
    if let Expression::FunctionCall(call) = expression {
        let id = match call.name {
            Name::UserDefined(_) => "function_call".to_string(),
            _ => format!("{:?}", call.name),
        };
        map.entry(expression.location().line).or_default().push(id);

        for expression in &call.arguments {
            expression_mapper(map, expression);
        }
    }
}

/// Map the [Statement].
pub fn statement_mapper(map: &mut HashMap<u32, Vec<String>>, statement: &Statement) {
    match statement {
        Statement::Object(object) => object_mapper(map, object),

        Statement::Code(code) => block_mapper(map, &code.block),

        Statement::Block(block) => block_mapper(map, block),

        Statement::ForLoop(for_loop) => {
            map.entry(for_loop.location.line)
                .or_default()
                .push("for".to_string());

            expression_mapper(map, &for_loop.condition);
            block_mapper(map, &for_loop.body);
            block_mapper(map, &for_loop.initializer);
            block_mapper(map, &for_loop.finalizer);
        }

        Statement::IfConditional(if_conditional) => {
            map.entry(if_conditional.location.line)
                .or_default()
                .push("if".to_string());

            expression_mapper(map, &if_conditional.condition);
            block_mapper(map, &if_conditional.block);
        }

        Statement::Expression(expression) => expression_mapper(map, expression),

        Statement::Continue(location) => map
            .entry(location.line)
            .or_default()
            .push("continue".to_string()),

        Statement::Leave(location) => map
            .entry(location.line)
            .or_default()
            .push("leave".to_string()),

        Statement::Break(location) => map
            .entry(location.line)
            .or_default()
            .push("break".to_string()),

        Statement::Switch(switch) => {
            map.entry(switch.expression.location().line)
                .or_default()
                .push("switch".to_string());

            expression_mapper(map, &switch.expression);
            for case in &switch.cases {
                block_mapper(map, &case.block);
            }
            if let Some(block) = switch.default.as_ref() {
                block_mapper(map, block);
            }
        }

        Statement::Assignment(assignment) => {
            map.entry(assignment.location.line)
                .or_default()
                .push("assignment".to_string());

            expression_mapper(map, &assignment.initializer);
        }

        Statement::VariableDeclaration(declaration) => {
            map.entry(declaration.location.line)
                .or_default()
                .push("let".to_string());

            if let Some(expression) = declaration.expression.as_ref() {
                expression_mapper(map, expression);
            }
        }

        Statement::FunctionDefinition(definition) => {
            map.entry(definition.location.line)
                .or_default()
                .push("function_definition".to_string());

            block_mapper(map, &definition.body);
        }
    }
}

/// Map the [Object].
pub fn object_mapper(map: &mut HashMap<u32, Vec<String>>, object: &Object) {
    map.entry(object.location.line)
        .or_default()
        .push(object.identifier.clone());

    block_mapper(map, &object.code.block);

    if let Some(object) = object.inner_object.as_ref() {
        object_mapper(map, object);
    }
}
