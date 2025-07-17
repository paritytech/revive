//! The revive explorer leverages debug info to get insights into emitted code.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use revive_yul::lexer::token::location::Location;

use crate::location_mapper::{map_locations, LocationMap};

/// The debug info analyzer.
#[derive(Debug, Default)]
pub struct DwarfdumpAnalyzer {
    /// The YUL source file path.
    source: PathBuf,

    /// The YUL location to statements map.
    location_map: LocationMap,

    /// The `llvm-dwarfdump --debug-lines` output.
    debug_lines: String,

    /// The observed statements.
    statements_count: HashMap<String, usize>,
    /// The observed statement to instructions size.
    statements_size: HashMap<String, u64>,
}

impl DwarfdumpAnalyzer {
    /// The debug info analyzer constructor.
    ///
    /// `source` is the path to the YUL source file.
    /// `debug_lines` is the `llvm-dwarfdump --debug-lines` output.
    pub fn new(source: &Path, debug_lines: String) -> Self {
        Self {
            source: source.to_path_buf(),
            debug_lines,
            ..Default::default()
        }
    }

    /// Run the analysis.
    pub fn analyze(&mut self) -> anyhow::Result<()> {
        self.map_locations()?;
        self.analyze_statements()?;
        Ok(())
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

    fn map_locations(&mut self) -> anyhow::Result<()> {
        self.location_map = map_locations(&self.source)?;

        self.statements_count = HashMap::with_capacity(self.location_map.len());
        self.statements_size = HashMap::with_capacity(self.location_map.len());

        for statement in self.location_map.values() {
            // Populate the maps so that we can always unwrap later.
            if self.statements_size.get(statement).is_none() {
                self.statements_size.insert(statement.clone(), 0);
            }

            *self.statements_count.entry(statement.clone()).or_insert(0) += 1;
        }

        Ok(())
    }

    /// Analyze how much bytes of insturctions each statement contributes.
    fn analyze_statements(&mut self) -> anyhow::Result<()> {
        let mut previous_offset = 0;
        let mut previous_location = Location::new(0, 0);

        for line in self
            .debug_lines
            .lines()
            .skip_while(|line| !line.starts_with("Address"))
            .skip(2)
        {
            let mut parts = line.split_whitespace();
            let (Some(offset), Some(line), Some(column)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };

            let current_offset = u64::from_str_radix(offset.trim_start_matches("0x"), 16)?;
            let mut current_location = Location::new(line.parse()?, column.parse()?);

            // TODO: A bug? Needs further investigation.
            if current_location.line == 0 && current_location.column != 0 {
                current_location.line = previous_location.line;
            }
            if current_location.line == 0
                && current_location.column == 0
                && previous_location.column != 0
            {
                current_location = previous_location;
            }

            if let Some(statement) = self.location_map.get(&previous_location) {
                let contribution = current_offset - previous_offset;
                *self.statements_size.get_mut(statement).unwrap() += contribution;
            }

            previous_offset = current_offset;
            previous_location = current_location;
        }

        Ok(())
    }
}
