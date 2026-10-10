use clap::Parser;
use okv_eval::repeats::{run_repeats, NoisePolicy, RepeatOptions};
use std::path::PathBuf;
use std::time::Duration;

/// Isolate eligible okv-eval workloads in repeated processes; never choose a winner.
#[derive(Parser)]
struct Args {
    #[arg(default_value = "evals/suites/smoke.toml")]
    suite: PathBuf,
    #[arg(long, default_value = "dev")]
    profile: String,
    #[arg(long, default_value = "model-smoke")]
    workload: String,
    #[arg(long, default_value = "model")]
    backend: String,
    #[arg(long)]
    output_dir: PathBuf,
    /// Defaults to ProfileConfig.repeats; the frozen profile is never rewritten.
    #[arg(long)]
    repeats: Option<u32>,
    #[arg(long, default_value_t = 0)]
    warmups: u32,
    #[arg(long, default_value_t = 300)]
    timeout_seconds: u64,
    #[arg(long, default_value_t = 0.1)]
    relative_tolerance: f64,
    #[arg(long, default_value_t = 0.0)]
    absolute_tolerance: f64,
    #[arg(long)]
    allow_dirty: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let executable = std::env::current_exe()?
        .with_file_name(format!("okv-eval{}", std::env::consts::EXE_SUFFIX));
    let receipt = run_repeats(&RepeatOptions {
        executable,
        suite: args.suite,
        profile: args.profile,
        workload: args.workload,
        backend: args.backend,
        output_dir: args.output_dir,
        repeats: args.repeats,
        warmups: args.warmups,
        timeout: Duration::from_secs(args.timeout_seconds),
        policy: NoisePolicy {
            relative_tolerance: args.relative_tolerance,
            absolute_tolerance: args.absolute_tolerance,
        },
        allow_dirty: args.allow_dirty,
    })?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    if receipt.verdict == "discard" {
        std::process::exit(1);
    }
    Ok(())
}
