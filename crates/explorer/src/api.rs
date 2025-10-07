//! REST API endpoints for the Compiler Explorer WebUI.
//!
//! This module provides HTTP endpoints that serve YUL source code,
//! RISC-V assembly instructions, and location mappings to the browser-based UI.

use crate::assembly_analyzer::AssemblyInstruction;
use crate::source_mapper::{LocationMapping, SourceMapper};
use serde::{Deserialize, Serialize};

/// Response containing YUL source code.
#[derive(Debug, Serialize)]
pub struct YulSourceResponse {
    /// The YUL source code content
    pub content: String,
    /// Name of the source file
    pub filename: String,
    /// Total number of lines
    pub line_count: usize,
}

impl YulSourceResponse {
    /// Creates a new YUL source response from file content.
    pub fn new(content: String, filename: String) -> Self {
        let line_count = content.lines().count();
        Self {
            content,
            filename,
            line_count,
        }
    }
}

/// Response containing RISC-V assembly instructions.
#[derive(Debug, Serialize)]
pub struct AssemblyResponse {
    /// List of assembly instructions
    pub instructions: Vec<AssemblyInstructionDto>,
    /// Total number of instructions
    pub instruction_count: usize,
}

impl AssemblyResponse {
    /// Creates a new assembly response from instruction data.
    pub fn new(instructions: Vec<AssemblyInstruction>) -> Self {
        let instruction_count = instructions.len();
        let instructions = instructions
            .into_iter()
            .map(AssemblyInstructionDto::from)
            .collect();

        Self {
            instructions,
            instruction_count,
        }
    }
}

/// Data transfer object for assembly instructions.
#[derive(Debug, Serialize)]
pub struct AssemblyInstructionDto {
    /// Formatted memory address (e.g., "0x1000")
    pub address: String,
    /// Raw instruction bytes
    pub bytes: String,
    /// Full instruction text (mnemonic + operands)
    pub instruction: String,
    /// Associated source line number (if available)
    pub line_number: Option<usize>,
}

impl From<AssemblyInstruction> for AssemblyInstructionDto {
    fn from(instr: AssemblyInstruction) -> Self {
        let instruction = if instr.operands.is_empty() {
            instr.mnemonic
        } else {
            format!("{}\t{}", instr.mnemonic, instr.operands)
        };

        Self {
            address: format!("0x{:x}", instr.address),
            bytes: instr.bytes,
            instruction,
            line_number: instr.line_number,
        }
    }
}

/// Response containing source-to-assembly mappings.
#[derive(Debug, Serialize)]
pub struct MappingResponse {
    /// List of location mappings
    pub mappings: Vec<LocationMapping>,
    /// Total number of mappings
    pub mapping_count: usize,
}

impl MappingResponse {
    /// Creates a new mapping response from source mapper.
    pub fn new(source_mapper: &SourceMapper) -> Self {
        let mappings = source_mapper.to_location_mappings();
        let mapping_count = mappings.len();

        Self {
            mappings,
            mapping_count,
        }
    }
}

/// Complete analysis data for the WebUI.
#[derive(Debug, Serialize)]
pub struct AnalysisResponse {
    /// YUL source information
    pub yul_source: YulSourceResponse,
    /// Assembly instructions
    pub assembly: AssemblyResponse,
    /// Location mappings
    pub mapping: MappingResponse,
    /// Analysis metadata
    pub metadata: AnalysisMetadata,
}

/// Metadata about the analysis.
#[derive(Debug, Serialize)]
pub struct AnalysisMetadata {
    /// Path to the analyzed shared object
    pub shared_object_path: String,
    /// Path to the YUL source file
    pub yul_source_path: String,
    /// Timestamp when analysis was performed
    pub analyzed_at: String,
    /// Tool versions used
    pub tools: ToolVersions,
}

/// Information about tool versions used in analysis.
#[derive(Debug, Serialize)]
pub struct ToolVersions {
    /// dwarfdump version/path
    pub dwarfdump: String,
    /// objdump version/path
    pub objdump: String,
    /// revive-explorer version
    pub revive_explorer: String,
}

impl ToolVersions {
    /// Creates tool version info with default/detected values.
    pub fn detect() -> Self {
        Self {
            dwarfdump: "llvm-dwarfdump".to_string(),
            objdump: "objdump".to_string(),
            revive_explorer: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Error response for API endpoints.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error type/category
    pub error: String,
    /// Human-readable error message
    pub message: String,
    /// Optional detailed error information
    pub details: Option<String>,
}

impl ErrorResponse {
    /// Creates a new error response.
    pub fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
            details: None,
        }
    }

    /// Creates an error response with details.
    pub fn with_details(
        error: impl Into<String>,
        message: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
            details: Some(details.into()),
        }
    }
}

/// Request parameters for line-based queries.
#[derive(Debug, Deserialize)]
pub struct LineQueryParams {
    /// Line number to query (1-based)
    pub line: u32,
}

/// Request parameters for address-based queries.
#[derive(Debug, Deserialize)]
pub struct AddressQueryParams {
    /// Memory address to query (hex format, e.g., "0x1000")
    pub address: String,
}

impl AddressQueryParams {
    /// Parses the address string to a u64.
    pub fn parse_address(&self) -> Result<u64, String> {
        let addr_str = self.address.trim_start_matches("0x");
        u64::from_str_radix(addr_str, 16)
            .map_err(|_| format!("Invalid address format: {}", self.address))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly_analyzer::AssemblyInstruction;

    #[test]
    fn test_yul_source_response() {
        let content = "function test() {\n    let x := 1\n}".to_string();
        let response = YulSourceResponse::new(content.clone(), "test.yul".to_string());

        assert_eq!(response.content, content);
        assert_eq!(response.filename, "test.yul");
        assert_eq!(response.line_count, 3);
    }

    #[test]
    fn test_assembly_instruction_dto() {
        let instruction = AssemblyInstruction {
            address: 0x1000,
            bytes: "13 81 00 02".to_string(),
            mnemonic: "addi".to_string(),
            operands: "sp,sp,2".to_string(),
            line_number: Some(5),
        };

        let dto = AssemblyInstructionDto::from(instruction);
        assert_eq!(dto.address, "0x1000");
        assert_eq!(dto.instruction, "addi\tsp,sp,2");
        assert_eq!(dto.line_number, Some(5));
    }

    #[test]
    fn test_address_query_params() {
        let params = AddressQueryParams {
            address: "0x1000".to_string(),
        };

        assert_eq!(params.parse_address().unwrap(), 0x1000);

        let invalid_params = AddressQueryParams {
            address: "invalid".to_string(),
        };

        assert!(invalid_params.parse_address().is_err());
    }
}
