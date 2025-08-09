//! The core dwarf dump analyzer library.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use revive_yul::lexer::token::location::Location;

use crate::location_mapper::{self, LocationMapper};

/// The dwarf dump analyzer.
///
/// Loads debug information from `llvm-dwarfdump` and calculates statistics
/// about the compiled YUL statements:
/// - Statements count
/// - Per-statement
#[derive(Debug, Default)]
pub struct DwarfdumpAnalyzer {
    /// The YUL source file path.
    source: PathBuf,

    /// The YUL location to statements map.
    location_map: HashMap<Location, String>,

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

    /// Populate the maps so that we can always unwrap later.
    fn map_locations(&mut self) -> anyhow::Result<()> {
        self.location_map = LocationMapper::map_locations(&self.source)?;

        self.statements_count = HashMap::with_capacity(self.location_map.len());
        self.statements_size = HashMap::with_capacity(self.location_map.len());

        for statement in self.location_map.values() {
            if !self.statements_size.contains_key(statement) {
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

            if let Some(statement) = self.location_map.get(&previous_location) {
                let contribution = current_offset - previous_offset;
                *self.statements_size.get_mut(statement).unwrap() += contribution;
            }

            previous_offset = current_offset;
            previous_location = current_location;
        }

        Ok(())
    }

    /// Print the per-statement count break-down.
    pub fn display_statement_count(&self) {
        println!("statements count:");
        for (statement, count) in self.statements_count.iter() {
            println!("\t{statement} {count}");
        }
    }

    /// Print the per-statement byte size contribution break-down.
    pub fn display_statement_size(&self) {
        println!("bytes per statement:");
        for (statement, size) in self.statements_size.iter() {
            println!("\t{statement} {size}");
        }
    }

    /// Print the estimated `yul-phaser` cost parameters.
    pub fn display_phaser_costs(&self, yul_phaser_scale: u64) {
        println!("yul-phaser parameters:");
        for (parameter, cost) in self.phaser_costs(yul_phaser_scale) {
            println!("\t{parameter} {cost}");
        }
    }

    /// Estimate the `yul-phaser` costs using the simplified weight function:
    /// `Total size / toal count = cost`
    pub fn phaser_costs(&self, yul_phaser_scale: u64) -> Vec<(String, u64)> {
        let mut costs: HashMap<String, (usize, u64)> = HashMap::with_capacity(16);
        for (statement, count) in self
            .statements_count
            .iter()
            .filter(|(_, count)| **count > 0)
        {
            let size = self.statements_size.get(statement).unwrap();
            let cost = match statement.as_str() {
                location_mapper::FOR => "--for-loop-cost",
                location_mapper::OTHER => continue,
                location_mapper::INTERNAL => continue,
                location_mapper::BLOCK => "--block-cost",
                location_mapper::FUNCTION_CALL => "--function-call-cost",
                location_mapper::IF => "--if-cost",
                location_mapper::SWITCH => "--switch-cost",
                location_mapper::DECLARATION => "--variable-declaration-cost",
                location_mapper::ASSIGNMENT => "--assignment-cost",
                location_mapper::FUNCTION_DEFINITION => "--function-definition-cost",
                location_mapper::IDENTIFIER => "--identifier-cost",
                location_mapper::LITERAL => "--literal-cost",
                _ => "--expression-statement-cost",
            };

            let entry = costs.entry(cost.to_string()).or_default();
            entry.0 += count;
            entry.1 += size;
        }

        let costs = costs
            .iter()
            .map(|(cost, (count, size))| {
                let ratio = *size / *count as u64;
                (cost.to_string(), ratio.min(100))
            })
            .collect::<Vec<_>>();

        let scaled_costs = scale_to(
            costs
                .iter()
                .map(|(_, ratio)| *ratio)
                .collect::<Vec<_>>()
                .as_slice(),
            yul_phaser_scale,
        );

        costs
            .iter()
            .zip(scaled_costs)
            .map(|((cost, _), scaled_ratio)| (cost.to_string(), scaled_ratio))
            .collect()
    }
}

/// Given a slice of u64 values, returns a Vec<u64> where each element
/// is linearly scaled into the closed interval [1, 10].
fn scale_to(data: &[u64], scale_max: u64) -> Vec<u64> {
    if data.is_empty() {
        return Vec::new();
    }

    let mut min = data[0];
    let mut max = data[0];
    for &x in &data[1..] {
        if x < min {
            min = x;
        }
        if x > max {
            max = x;
        }
    }
    if max < scale_max {
        return data.to_vec();
    }

    let range = max - min;
    data.iter()
        .map(|&x| {
            if range == 0 {
                1
            } else {
                1 + (x - min) * scale_max / range
            }
        })
        .collect()
}
