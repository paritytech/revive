//! The revive explorer leverages debug info to get insights into emitted code.

use std::{collections::HashMap, path::PathBuf};

use revive_yul::{
    lexer::Lexer,
    parser::statement::{
        block::Block,
        expression::{function_call::name::Name, Expression},
        object::Object,
        Statement,
    },
};

static COMMENT_MARKER: &str = "; ";

/// The debug info analyzer.
#[derive(Default, Debug)]
pub struct Analyzer {
    /// The observed statement to instructions size.
    statements_size: HashMap<String, usize>,
    /// The observed statements.
    statements_count: HashMap<String, usize>,

    /// The YUL ast.
    ast: Option<Object>,

    /// The YUL line being currently processed.
    line: u32,
    /// The YUL line to statements map.
    line_map: HashMap<u32, Vec<String>>,
}

impl Analyzer {
    /// The debug info analyzer constructor.
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    /// Process the next line and update state accordingly.
    pub fn next_line(&mut self, line: &str) -> anyhow::Result<()> {
        if self.ast.is_none() {
            self.try_get_ast(line)?;
        }

        if line.starts_with(COMMENT_MARKER) {
            self.update_line(&line);
        } else {
            self.record_instruction_size();
        }

        Ok(())
    }

    fn try_get_ast(&mut self, line: &str) -> anyhow::Result<()> {
        if !line.starts_with(COMMENT_MARKER) {
            return Ok(());
        }

        let Some(path) = line
            .replace(COMMENT_MARKER, "")
            .split(":")
            .next()
            .filter(|maybe_source| maybe_source.ends_with(".yul"))
            .map(PathBuf::from)
        else {
            return Ok(());
        };

        let mut lexer = Lexer::new(std::fs::read_to_string(&path)?);
        let object = Object::parse(&mut lexer, None).map_err(|error| {
            anyhow::anyhow!("Contract `{}` parsing error: {:?}", path.display(), error)
        })?;

        self.populate_line_map(&object);
        self.ast = Some(object);

        Ok(())
    }

    fn populate_line_map(&mut self, object: &Object) {
        object_mapper(&mut self.line_map, &object);
        for statements in self.line_map.values() {
            for statement in statements {
                if self.statements_size.get(statement).is_none() {
                    self.statements_size.insert(statement.clone(), 0);
                }
                *self.statements_count.entry(statement.clone()).or_insert(0) += 1;
            }
        }
    }

    fn update_line(&mut self, line: &str) {
        let Some(Ok(line)) = line.split(".yul:").nth(1).map(|part| part.parse::<u32>()) else {
            return;
        };
        self.line = line;
    }

    /// Record an instruction for the current set of statements.
    fn record_instruction_size(&mut self) {
        let Some(statements) = self.line_map.get(&self.line) else {
            return;
        };

        for statement in statements {
            *self
                .statements_size
                .get_mut(statement)
                .expect("every statement should be present") += 1;
        }
    }

    /// The debug info analyzer visualizer.
    pub fn display(&self) {
        println!("statements count:");
        for (statement, count) in self.statements_count.iter() {
            println!("\t{statement} {count}");
        }

        println!("statements size:");
        for (statement, size) in self.statements_size.iter() {
            println!("\t{statement} {size}");
        }
    }
}

fn block_mapper(map: &mut HashMap<u32, Vec<String>>, block: &Block) {
    map.entry(block.location.line)
        .or_default()
        .push("block".to_string());

    for statement in &block.statements {
        statement_mapper(map, &statement);
    }
}

fn expression_mapper(map: &mut HashMap<u32, Vec<String>>, expression: &Expression) {
    match expression {
        Expression::FunctionCall(call) => {
            let id = match call.name {
                Name::UserDefined(_) => "function_call".to_string(),
                _ => format!("{:?}", call.name),
            };
            map.entry(expression.location().line).or_default().push(id);

            for expression in &call.arguments {
                expression_mapper(map, expression);
            }
        }
        _ => return,
    };
}

fn statement_mapper(map: &mut HashMap<u32, Vec<String>>, statement: &Statement) {
    match statement {
        Statement::Object(object) => object_mapper(map, &object),

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

fn object_mapper(map: &mut HashMap<u32, Vec<String>>, object: &Object) {
    map.entry(object.location.line)
        .or_default()
        .push(object.identifier.clone());

    block_mapper(map, &object.code.block);

    if let Some(object) = object.inner_object.as_ref() {
        object_mapper(map, object);
    }
}
