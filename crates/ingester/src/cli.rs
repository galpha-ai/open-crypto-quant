use std::path::PathBuf;
use clap::Parser;

#[derive(Parser)]
#[command(
    author,
    version,
    about,
    long_about = Some("Subscribes to Redis events and writes them to Clickhouse")
)]
pub struct Cli {
    /// Path to YAML config file
    #[arg(short = 'c', value_name = "FILE")]
    pub config: Option<PathBuf>,
}