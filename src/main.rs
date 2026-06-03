mod cli;

use anyhow::Context;
use clap::Parser;
use sandslash::pipeline;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();

    // When --verbose is set: show full tracing output, suppress progress bar.
    // Otherwise: restrict tracing to ERROR to avoid interleaving with the bar.
    let log_filter = if cli.verbose {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "sandslash=info".into())
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "sandslash=error".into())
    };

    tracing_subscriber::fmt()
        .with_env_filter(log_filter)
        .with_writer(std::io::stderr)
        .init();

    let config = cli.into_config().context("invalid configuration")?;
    tracing::info!(root = %config.root, depth = config.depth, "starting audit");

    pipeline::run(config).await?;

    Ok(())
}
