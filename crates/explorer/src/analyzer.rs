//! The revive explorer leverages debug info to get insights into emitted code.

use std::{collections::HashMap, path::PathBuf};

use revive_yul::{lexer::Lexer, parser::statement::object::Object};

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
    /// The YUL location to statements map.
    location_map: HashMap<u32, Vec<String>>,
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
            self.update_line(line);
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

        self.populate_location_map(&object);
        self.ast = Some(object);

        Ok(())
    }

    fn populate_location_map(&mut self, object: &Object) {
        crate::location_mapper::object_mapper(&mut self.location_map, &object);
        for statements in self.location_map.values() {
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
        let Some(statements) = self.location_map.get(&self.line) else {
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
