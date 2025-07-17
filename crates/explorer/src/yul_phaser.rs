//! The revive explorer YUL phaser utility.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

const ARGUMENTS: [&str; 6] = [
    "--hide-round",
    "--rounds",
    "1000",
    "--random-population",
    "100",
    "--show-only-top-chromosome",
];

fn spawn_process(
    executable: PathBuf,
    source: PathBuf,
    costs: Vec<(String, u64)>,
    seed: usize,
) -> anyhow::Result<()> {
    let cost_parameters = costs
        .iter()
        .flat_map(|(parameter, cost)| vec![parameter.clone(), cost.to_string()]);

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    Command::new(executable)
        .args(cost_parameters)
        .args(ARGUMENTS)
        .arg("--seed")
        .arg((seed + secs as usize).to_string())
        .arg(source)
        .stdin(Stdio::null())
        .spawn()?
        .wait()?;

    Ok(())
}

/// Run multiple YUL phaser executables in parallel.
pub fn run(executable: &Path, source: &Path, costs: &[(String, u64)]) -> anyhow::Result<()> {
    let mut handles = Vec::new();

    for n in 0..num_cpus::get() / 2 {
        let executable = executable.to_path_buf();
        let source = source.to_path_buf();
        let costs = costs.to_vec();

        handles.push(thread::spawn(move || {
            spawn_process(executable, source, costs, n)
        }));
    }

    for handle in handles {
        let _ = handle.join();
    }

    Ok(())
}
