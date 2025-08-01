//! Main entry point for the Adaptive Expert Platform CLI.

use adaptive_expert_platform::{
    batch, cli, server, settings::Settings, telemetry,
};
use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = cli::Cli::parse();

    // Load settings
    let settings = Settings::load()?;

    // Initialize telemetry
    telemetry::init(settings.otlp_endpoint.as_deref())?;

    // Execute the requested command
    match args.command {
        cli::Commands::Serve { addr: _ } => {
            server::serve().await
        }
        cli::Commands::Run { config } => {
            batch::run(config, settings).await
        }
    }
}
