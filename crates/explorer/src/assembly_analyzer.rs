//! Assembly analysis using objdump for RISC-V disassembly.
//!
//! This module integrates objdump to extract RISC-V assembly instructions
//! from compiled shared objects, as requested in issue #366.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use anyhow::{anyhow, Result};

pub static OBJDUMP_EXECUTABLE: &str = "objdump";
pub static OBJDUMP_DISASSEMBLE_ARGUMENTS: [&str; 2] = ["-d", "--no-show-raw-insn"];

/// Represents a single assembly instruction with metadata.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AssemblyInstruction {
    /// Memory address of the instruction
    pub address: u64,
    /// Raw instruction bytes
    pub bytes: String,
    /// Assembly mnemonic (e.g., "addi", "lw")
    pub mnemonic: String,
    /// Instruction operands
    pub operands: String,
    /// Associated source line number (if available from debug info)
    pub line_number: Option<usize>,
}

/// Assembly analyzer that uses objdump to disassemble RISC-V code.
pub struct AssemblyAnalyzer {
    objdump_path: Option<PathBuf>,
}

impl AssemblyAnalyzer {
    /// Creates a new assembly analyzer.
    ///
    /// `objdump_path` can be provided to override the default objdump executable.
    pub fn new(objdump_path: Option<PathBuf>) -> Self {
        Self { objdump_path }
    }

    /// Disassembles a shared object file and returns structured assembly instructions.
    ///
    /// This method calls objdump to disassemble the RISC-V code and parses the output
    /// into structured data that can be consumed by the WebUI.
    pub fn disassemble(&self, shared_object: &Path) -> Result<Vec<AssemblyInstruction>> {
        let output = self.run_objdump(shared_object)?;
        self.parse_objdump_output(&output)
    }

    /// Runs objdump with disassembly arguments.
    fn run_objdump(&self, shared_object: &Path) -> Result<String> {
        let executable = self
            .objdump_path
            .as_ref()
            .map(|p| p.as_path())
            .unwrap_or_else(|| Path::new(OBJDUMP_EXECUTABLE));

        let output = Command::new(executable)
            .args(&OBJDUMP_DISASSEMBLE_ARGUMENTS)
            .arg(shared_object)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
            .wait_with_output()?;

        if !output.status.success() {
            return Err(anyhow!(
                "objdump failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Parses objdump output into structured assembly instructions.
    fn parse_objdump_output(&self, output: &str) -> Result<Vec<AssemblyInstruction>> {
        let mut instructions = Vec::new();

        for line in output.lines() {
            if let Some(instruction) = self.parse_instruction_line(line)? {
                instructions.push(instruction);
            }
        }

        Ok(instructions)
    }

    /// Parses a single line of objdump output.
    ///
    /// Example objdump line:
    /// "  400000:	02 00 81 13 	addi	sp,sp,2"
    fn parse_instruction_line(&self, line: &str) -> Result<Option<AssemblyInstruction>> {
        let line = line.trim();
        
        // Skip empty lines and section headers
        if line.is_empty() || !line.contains(':') {
            return Ok(None);
        }

        // Skip lines that don't look like instruction disassembly
        if line.starts_with("Disassembly") || line.starts_with("file format") {
            return Ok(None);
        }

        // Parse address
        let colon_pos = line.find(':').ok_or_else(|| anyhow!("No colon found in line"))?;
        let address_str = &line[..colon_pos].trim();
        let address = u64::from_str_radix(address_str, 16)
            .map_err(|_| anyhow!("Failed to parse address: {}", address_str))?;

        // Parse the rest of the line after the colon
        let rest = &line[colon_pos + 1..].trim();
        
        // Split into bytes and instruction parts
        let parts: Vec<&str> = rest.splitn(2, '\t').collect();
        if parts.len() < 2 {
            return Ok(None);
        }

        let bytes = parts[0].trim().to_string();
        let instruction_part = parts[1].trim();

        // Split instruction into mnemonic and operands
        let instruction_parts: Vec<&str> = instruction_part.splitn(2, '\t').collect();
        let mnemonic = instruction_parts[0].trim().to_string();
        let operands = if instruction_parts.len() > 1 {
            instruction_parts[1].trim().to_string()
        } else {
            String::new()
        };

        Ok(Some(AssemblyInstruction {
            address,
            bytes,
            mnemonic,
            operands,
            line_number: None, // Will be populated by source mapper
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_instruction_line() {
        let analyzer = AssemblyAnalyzer::new(None);
        
        let line = "  400000:\t02 00 81 13 \taddi\tsp,sp,2";
        let result = analyzer.parse_instruction_line(line).unwrap();
        
        assert!(result.is_some());
        let instruction = result.unwrap();
        assert_eq!(instruction.address, 0x400000);
        assert_eq!(instruction.bytes, "02 00 81 13");
        assert_eq!(instruction.mnemonic, "addi");
        assert_eq!(instruction.operands, "sp,sp,2");
    }

    #[test]
    fn test_skip_invalid_lines() {
        let analyzer = AssemblyAnalyzer::new(None);
        
        let invalid_lines = [
            "Disassembly of section .text:",
            "",
            "file format elf64-littleriscv",
        ];
        
        for line in &invalid_lines {
            let result = analyzer.parse_instruction_line(line).unwrap();
            assert!(result.is_none(), "Should skip line: {}", line);
        }
    }
}