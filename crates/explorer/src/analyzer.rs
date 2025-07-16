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
    /// The YUL already seen lines.
    lines_seen: HashMap<u32, Vec<String>>,
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
            self.update_line_statements(line);
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
        self.ast = Some(Object::parse(&mut lexer, None).map_err(|error| {
            anyhow::anyhow!("Contract `{}` parsing error: {:?}", path.display(), error)
        })?);

        for stmt in self.ast.as_ref().unwrap() {}

        Ok(())
    }

    fn update_line(&mut self, line: &str) {
        let Some(Ok(line)) = line.split(".yul:").nth(1).map(|part| part.parse::<u32>()) else {
            return;
        };
        self.line = line;
    }

    /// Process the next set of statements.
    fn update_line_statements(&mut self, line: &str) {
        if self.lines_seen.contains_key(&self.line) {
            return;
        }
    }

    /// Record an instruction for the current set of statements.
    fn record_instruction_size(&mut self) {}

    /// Record an instruction for the current set of statements.
    fn record_instruction_count(&mut self) {}

    /// The debug info analyzer visualizer.
    pub fn display(&self) {
        println!("runtime statements count:");
    }
}
