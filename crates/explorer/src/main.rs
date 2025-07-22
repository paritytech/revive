use std::path::PathBuf;

use clap::Parser;

use revive_explorer::{dwarfdump, dwarfdump_analyzer::DwarfdumpAnalyzer, yul_phaser};

/// The `revive-explorer` is a helper utility for exploring the compilers YUL lowering unit.
///
/// It analyzes a given shared objects from the debug dump and outputs:
/// - The count of each YUL statement translated.
/// - A per YUL statement break-down of bytecode size contributed per.
/// - Estimated `yul-phaser` cost parameters.
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

    /// Path of the shared object to analyze.
    file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

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
