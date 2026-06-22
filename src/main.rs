mod cli;

use anyhow::Context;
use clap::Parser;
use sandslash::pipeline;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // Load .env if present; silently ignore if absent.
    dotenvy::dotenv().ok();

    let cli = cli::Cli::parse();

    match cli.command {
        cli::Command::Audit(args) => {
            // When --verbose is set: show full tracing output, suppress progress bar.
            // Otherwise: restrict tracing to ERROR to avoid interleaving with the bar.
            let log_filter = if args.verbose {
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

            let config = args.into_config().context("invalid configuration")?;
            tracing::info!(root = %config.root, depth = config.depth, "starting audit");

            pipeline::run(config).await?;
        }

        cli::Command::Serve(args) => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "sandslash=info".into()),
                )
                .with_writer(std::io::stderr)
                .init();

            tracing::info!(bind = %args.bind, "starting HTTP server");
            sandslash::server::serve(args.bind).await?;
        }

        cli::Command::Diff(args) => {
            sandslash::diff::run(args.before, args.after, args.output, args.no_color)
                .context("diff failed")?;
        }
    }

    Ok(())
}
