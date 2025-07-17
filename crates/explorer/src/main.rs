use std::path::PathBuf;

use clap::Parser;

use revive_explorer::{dwarfdump, dwarfdump_analyzer::DwarfdumpAnalyzer};

/// The revive explorer analyzes debug builds.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path of the objdump executable
    #[arg(short, long)]
    objdump: Option<PathBuf>,

    /// Path of the shared object to analyze
    file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut analyzer = DwarfdumpAnalyzer::new(
        dwarfdump::source_file(&args.file, &args.objdump)?.as_path(),
        dwarfdump::debug_lines(&args.file, &args.objdump)?,
    );

    analyzer.analyze()?;
    analyzer.display();

    Ok(())
}
