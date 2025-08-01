//! Command-line interface definitions using clap derive API.

use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;

/// Adaptive Expert Platform CLI
#[derive(Parser)]
#[command(name = "acropolis-cli")]
#[command(about = "A secure, polyglot AI orchestration platform")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the HTTP server
    Serve {
        /// Address to bind to
        #[arg(long, default_value = "127.0.0.1:8080")]
        addr: SocketAddr,
    },
    /// Run a batch job from configuration file
    Run {
        /// Path to the batch configuration file
        #[arg(short, long)]
        config: PathBuf,
    },
}
