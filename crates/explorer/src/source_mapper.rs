//! Maps YUL source locations to assembly instructions using debug information.
//!
//! This module creates bidirectional mappings between YUL source code locations
//! and RISC-V assembly instructions, enabling the interactive navigation
//! requested in issue #366.

use std::collections::HashMap;
use revive_yul::lexer::token::location::Location;
use crate::assembly_analyzer::AssemblyInstruction;
use anyhow::{anyhow, Result};

/// Bidirectional mapping between YUL source and assembly instructions.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceMapper {
    /// Maps YUL source locations to assembly instruction addresses
    pub yul_to_assembly: HashMap<Location, Vec<u64>>,
    /// Maps assembly instruction addresses to YUL source locations
    pub assembly_to_yul: HashMap<u64, Location>,
    /// Maps line numbers to YUL locations for easier lookup
    pub line_to_location: HashMap<u32, Vec<Location>>,
}

/// Represents a mapping entry for the WebUI API.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LocationMapping {
    pub yul_line: u32,
    pub yul_column: u32,
    pub assembly_addresses: Vec<String>,
}

impl SourceMapper {
    /// Creates a new empty source mapper.
    pub fn new() -> Self {
        Self {
            yul_to_assembly: HashMap::new(),
            assembly_to_yul: HashMap::new(),
            line_to_location: HashMap::new(),
        }
    }

    /// Builds the mapping between YUL source and assembly instructions.
    ///
    /// This method correlates debug information from dwarfdump with assembly
    /// instructions from objdump to create the interactive navigation mapping.
    pub fn build_mapping(
        &mut self,
        debug_lines: &str,
        instructions: &[AssemblyInstruction],
    ) -> Result<()> {
        // Parse debug line information to get address-to-source mappings
        let debug_entries = self.parse_debug_lines(debug_lines)?;
        
        // Create address lookup for quick assembly instruction finding
        let address_to_instruction: HashMap<u64, usize> = instructions
            .iter()
            .enumerate()
            .map(|(idx, instr)| (instr.address, idx))
            .collect();

        // Build the bidirectional mapping
        for entry in debug_entries {
            let yul_location = Location::new(entry.line, entry.column);
            
            // Find assembly instructions that correspond to this source location
            let mut assembly_addresses = Vec::new();
            
            // Look for instructions in the address range
            for instruction in instructions {
                if instruction.address >= entry.address && 
                   instruction.address < entry.address + entry.size.unwrap_or(4) {
                    assembly_addresses.push(instruction.address);
                    
                    // Add to assembly-to-YUL mapping
                    self.assembly_to_yul.insert(instruction.address, yul_location);
                }
            }
            
            if !assembly_addresses.is_empty() {
                // Add to YUL-to-assembly mapping
                self.yul_to_assembly.insert(yul_location, assembly_addresses);
                
                // Add to line-to-location mapping for easier UI lookups
                self.line_to_location
                    .entry(entry.line)
                    .or_insert_with(Vec::new)
                    .push(yul_location);
            }
        }

        Ok(())
    }

    /// Gets assembly addresses for a given YUL line number.
    pub fn get_assembly_for_line(&self, line: u32) -> Vec<u64> {
        self.line_to_location
            .get(&line)
            .map(|locations| {
                locations
                    .iter()
                    .flat_map(|loc| self.yul_to_assembly.get(loc))
                    .flatten()
                    .copied()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Gets the YUL location for a given assembly address.
    pub fn get_yul_for_address(&self, address: u64) -> Option<Location> {
        self.assembly_to_yul.get(&address).copied()
    }

    /// Converts the mapping to a format suitable for the WebUI API.
    pub fn to_location_mappings(&self) -> Vec<LocationMapping> {
        let mut mappings = Vec::new();
        
        for (location, addresses) in &self.yul_to_assembly {
            mappings.push(LocationMapping {
                yul_line: location.line,
                yul_column: location.column,
                assembly_addresses: addresses
                    .iter()
                    .map(|addr| format!("0x{:x}", addr))
                    .collect(),
            });
        }
        
        // Sort by line number for consistent output
        mappings.sort_by_key(|m| (m.yul_line, m.yul_column));
        mappings
    }

    /// Parses debug line information from dwarfdump output.
    fn parse_debug_lines(&self, debug_lines: &str) -> Result<Vec<DebugLineEntry>> {
        let mut entries = Vec::new();
        let mut parsing_entries = false;
        
        for line in debug_lines.lines() {
            // Skip until we reach the actual debug line entries
            if line.starts_with("Address") && line.contains("Line") && line.contains("Column") {
                parsing_entries = true;
                continue;
            }
            
            if !parsing_entries {
                continue;
            }
            
            // Skip separator lines
            if line.starts_with("-") || line.trim().is_empty() {
                continue;
            }
            
            if let Some(entry) = self.parse_debug_line_entry(line)? {
                entries.push(entry);
            }
        }
        
        Ok(entries)
    }

    /// Parses a single debug line entry.
    ///
    /// Example line format from dwarfdump:
    /// "0x0000000000001000     1      5      0             0  is_stmt"
    fn parse_debug_line_entry(&self, line: &str) -> Result<Option<DebugLineEntry>> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        
        if parts.len() < 3 {
            return Ok(None);
        }
        
        // Parse address
        let address_str = parts[0].trim_start_matches("0x");
        let address = u64::from_str_radix(address_str, 16)
            .map_err(|_| anyhow!("Failed to parse address: {}", parts[0]))?;
        
        // Parse line number
        let line_num = parts[1].parse::<u32>()
            .map_err(|_| anyhow!("Failed to parse line number: {}", parts[1]))?;
        
        // Parse column number
        let column_num = parts[2].parse::<u32>()
            .map_err(|_| anyhow!("Failed to parse column number: {}", parts[2]))?;
        
        Ok(Some(DebugLineEntry {
            address,
            line: line_num,
            column: column_num,
            size: None, // We don't have size info from dwarfdump, estimate as 4 bytes
        }))
    }
}

impl Default for SourceMapper {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a debug line entry from dwarfdump output.
#[derive(Debug, Clone)]
struct DebugLineEntry {
    address: u64,
    line: u32,
    column: u32,
    size: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_debug_line_entry() {
        let mapper = SourceMapper::new();
        
        let line = "0x0000000000001000     1      5      0             0  is_stmt";
        let result = mapper.parse_debug_line_entry(line).unwrap();
        
        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.address, 0x1000);
        assert_eq!(entry.line, 1);
        assert_eq!(entry.column, 5);
    }

    #[test]
    fn test_get_assembly_for_line() {
        let mut mapper = SourceMapper::new();
        
        // Manually insert test data
        let location = Location::new(10, 5);
        mapper.yul_to_assembly.insert(location, vec![0x1000, 0x1004]);
        mapper.line_to_location.entry(10).or_insert_with(Vec::new).push(location);
        
        let addresses = mapper.get_assembly_for_line(10);
        assert_eq!(addresses, vec![0x1000, 0x1004]);
        
        let empty_addresses = mapper.get_assembly_for_line(99);
        assert!(empty_addresses.is_empty());
    }

    #[test]
    fn test_to_location_mappings() {
        let mut mapper = SourceMapper::new();
        
        // Add test data
        let location1 = Location::new(1, 0);
        let location2 = Location::new(2, 5);
        mapper.yul_to_assembly.insert(location1, vec![0x1000]);
        mapper.yul_to_assembly.insert(location2, vec![0x1004, 0x1008]);
        
        let mappings = mapper.to_location_mappings();
        assert_eq!(mappings.len(), 2);
        
        // Should be sorted by line number
        assert_eq!(mappings[0].yul_line, 1);
        assert_eq!(mappings[1].yul_line, 2);
        assert_eq!(mappings[1].assembly_addresses, vec!["0x1004", "0x1008"]);
    }
}