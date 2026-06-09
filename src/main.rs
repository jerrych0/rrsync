// src/main.rs
mod config;
mod orchestrator; 

use clap::Parser;
use anyhow::Result;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Source directory to backup
    #[arg(short, long, value_name = "DIR")]
    source: Option<PathBuf>,

    /// Destination directory for backups
    #[arg(short, long, value_name = "DIR")]
    destination: Option<PathBuf>,

    /// Path to a configuration file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let (source, destination) = if let Some(config_path) = args.config {
        let cfg = config::Config::load(&config_path)?;
        (cfg.source, cfg.destination)
    } else {
        let src = args.source.ok_or_else(|| anyhow::anyhow!("Source directory not provided"))?;
        let dest = args.destination.ok_or_else(|| anyhow::anyhow!("Destination directory not provided"))?;
        (src, dest)
    };

    println!("Source: {:?}", source);
    println!("Destination: {:?}", destination);

    // Create and run the orchestrator
    let real_executor = orchestrator::RealCommandExecutor; // Owned by main
    let orchestrator = orchestrator::Orchestrator::new(
        &real_executor, // Pass a reference
        source,
        destination,
    );
    orchestrator.run_backup()?;

    Ok(())
}
