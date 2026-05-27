mod cli;
mod config;
mod error;
mod model;

use anyhow::Context;
use clap::Parser;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "seo_rs=info".into()),
        )
        .init();

    let cli = cli::Cli::parse();
    let config = cli.into_config().context("invalid configuration")?;
    tracing::info!(?config, "starting audit");

    Ok(())
}
