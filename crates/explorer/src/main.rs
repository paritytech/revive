use std::path::PathBuf;

use clap::Parser;

use revive_explorer::{dwarfdump, dwarfdump_analyzer::DwarfdumpAnalyzer, yul_phaser};

/// The `revive-explorer` is a helper utility for exploring the compilers YUL lowering unit.
///
/// It analyzes a given shared objects from the debug dump and outputs:
/// - The count of each YUL statement translated.
/// - A per YUL statement break-down of bytecode size contributed per.
/// - Estimated `yul-phaser` cost parameters.
/// - WebUI mode: Interactive two-pane interface showing YUL ↔ RISC-V mapping (issue #366)
///
/// Note: This tool might not be fully accurate, especially when the code was optimized.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path of the dwarfdump executable.
    #[arg(short, long)]
    dwarfdump: Option<PathBuf>,

    /// The YUL phaser cost scale maximum value.
    #[arg(short, long, default_value_t = 10)]
    cost_scale: u64,

    /// Run the provided yul-phaser executable using the estimated costs.
    #[arg(short, long)]
    yul_phaser: Option<PathBuf>,

    /// Start web server mode for interactive YUL ↔ RISC-V analysis (issue #366).
    #[arg(long)]
    web: bool,

    /// Port for web server (default: 8080).
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// Path of the objdump executable for assembly disassembly.
    #[arg(long)]
    objdump: Option<PathBuf>,

    /// Path of the shared object to analyze.
    /// It must have been compiled with debug info (-g).
    file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Handle web mode for issue #366
    if args.web {
        return run_web_server(args);
    }

    let source_file = dwarfdump::source_file(&args.file, &args.dwarfdump)?;
    let debug_lines = dwarfdump::debug_lines(&args.file, &args.dwarfdump)?;
    let mut analyzer = DwarfdumpAnalyzer::new(source_file.as_path(), debug_lines);

    analyzer.analyze()?;

    if let Some(path) = args.yul_phaser.as_ref() {
        yul_phaser::run(
            path,
            source_file.as_path(),
            analyzer.phaser_costs(args.cost_scale).as_slice(),
            num_cpus::get() / 2, // TODO: should be configurable.
        )?;
        return Ok(());
    }

    analyzer.display_statement_count();
    analyzer.display_statement_size();
    analyzer.display_phaser_costs(args.cost_scale);

    Ok(())
}

/// Runs the web server mode for the Compiler Explorer UI (issue #366).
#[tokio::main]
async fn run_web_server(args: Args) -> anyhow::Result<()> {
    use revive_explorer::web_server::WebServer;

    let source_file = dwarfdump::source_file(&args.file, &args.dwarfdump)?;

    let server = WebServer::new(args.file, source_file, args.dwarfdump, args.objdump);

    println!("🚀 Starting Revive Compiler Explorer WebUI...");
    server.serve(args.port).await
}
