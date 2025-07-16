use std::path::PathBuf;

use clap::Parser;

use revive_explorer::{analyzer::Analyzer, objdump};

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
    let mut analyzer = Analyzer::new();

    for line in objdump::objdump(&args.file, args.objdump)?.lines() {
        analyzer.next_line(line);
    }

    analyzer.display();

    Ok(())
}
