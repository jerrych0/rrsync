// src/main.rs
mod config;
mod orchestrator;

use clap::Parser;
use anyhow::Result;
use std::path::PathBuf;
use log::info;

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
    env_logger::init();
    let args = Args::parse();

    let cfg = config::Config::load(&args.config)?;

    for job in cfg.jobs {
        info!("--- Running backup job: {} ---", job.name);
        info!("Source: {:?}", job.source);
        info!("Destination: {:?}", job.destination);
        
        let real_executor = orchestrator::RealCommandExecutor;
        let orchestrator = orchestrator::Orchestrator::new(
            &real_executor,
            job.source,
            job.destination,
            job.exclude,
            &job.retention_policy,
            args.dry_run,
        );
        orchestrator.run_backup()?;
        info!("--- Backup job '{}' completed successfully. ---", job.name);
    }

    Ok(())
}
