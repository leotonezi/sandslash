mod audit;
mod cli;
mod config;
mod error;
mod fetcher;
mod model;
mod parser;
mod pipeline;
mod report;
mod score;

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
    tracing::info!(root = %config.root, depth = config.depth, "starting audit");

    pipeline::run(config).await?;

    Ok(())
}
