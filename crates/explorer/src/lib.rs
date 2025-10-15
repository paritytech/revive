//! The revive explorer leverages debug info to get insights into emitted code.

pub mod dwarfdump;
pub mod dwarfdump_analyzer;
pub mod location_mapper;
pub mod yul_phaser;

// WebUI modules for issue #366
pub mod api;
pub mod assembly_analyzer;
pub mod source_mapper;
pub mod web_server;
