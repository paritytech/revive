use std::path::PathBuf;

use clap::Parser;

use revive_explorer::{dwarfdump, dwarfdump_analyzer::DwarfdumpAnalyzer, yul_phaser};

/// The revive explorer analyzes debug builds.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path of the dwarfdump executable.
    #[arg(short, long)]
    dwarfdump: Option<PathBuf>,

    /// The YUL phaser cost scale maximum value.
    #[arg(short, long, default_value_t = 100)]
    cost_scale: u64,

    /// Run the provided yul-phaser executable with the calculated costs.
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
        )?;
        return Ok(());
    }

    analyzer.display_statement_count();
    analyzer.display_statement_size();
    analyzer.display_phaser_costs(args.cost_scale);

    Ok(())
}
