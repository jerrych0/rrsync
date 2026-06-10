// src/main.rs
mod config;
mod orchestrator;

use clap::Parser;
use anyhow::Result;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to a configuration file
    #[arg(short, long, value_name = "FILE")]
    config: PathBuf,

    /// Perform a dry run without making any actual changes
    #[arg(long)]
    dry_run: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let cfg = config::Config::load(&args.config)?;

    for job in cfg.jobs {
        println!("
--- Running backup job: {} ---", job.name);
        println!("Source: {:?}", job.source);
        println!("Destination: {:?}", job.destination);
        // Exclude patterns from job can be passed to orchestrator later if needed

        let real_executor = orchestrator::RealCommandExecutor;
        let orchestrator = orchestrator::Orchestrator::new(
            &real_executor,
            job.source,
            job.destination,
            job.exclude,
            &job.retention_policy,
            args.dry_run, // Pass the dry_run flag
        );
        orchestrator.run_backup()?;
        println!("--- Backup job '{}' completed successfully. ---", job.name);
    }

    Ok(())
}
