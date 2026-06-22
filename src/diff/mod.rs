pub mod compute;
pub mod model;
pub mod render;

use std::io;
use std::path::PathBuf;

use clap::ValueEnum;

use crate::error::{Result, SeoError};
use crate::model::AuditReport;

/// Output format for the diff subcommand.
///
/// Defined in the library so it can be imported by `cli.rs` without duplication.
/// `ValueEnum` is derived here directly, since `clap` is already a library dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text (default).
    Text,
    /// Machine-readable JSON.
    Json,
}

/// Entry point for `sandslash diff <before.json> <after.json>`.
///
/// Reads both files, deserializes them, optionally warns about cross-site diffs,
/// computes the [`model::DiffReport`], and dispatches to the appropriate renderer.
pub fn run(
    before: PathBuf,
    after: PathBuf,
    output: Option<OutputFormat>,
    no_color: bool,
) -> Result<()> {
    // Read both files — map IO errors via SeoError::Io (via From<io::Error>).
    let before_json = std::fs::read_to_string(&before)?;
    let after_json = std::fs::read_to_string(&after)?;

    // Deserialize — map JSON errors to SeoError::Io.
    let before: AuditReport =
        serde_json::from_str(&before_json).map_err(|e| SeoError::Io(io::Error::other(e)))?;
    let after: AuditReport =
        serde_json::from_str(&after_json).map_err(|e| SeoError::Io(io::Error::other(e)))?;

    // Warn on cross-site diff (different root URLs).
    if before.root != after.root {
        eprintln!(
            "warning: before and after reports have different root URLs ({} vs {}); proceeding anyway",
            before.root, after.root
        );
    }

    let diff = compute::diff_reports(&before, &after);

    // Dispatch to renderer.
    match output {
        Some(OutputFormat::Json) => {
            let stdout = io::stdout();
            render::write_json(&diff, stdout.lock())?;
        }
        None | Some(OutputFormat::Text) => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            render::write_text(&diff, no_color, &mut lock)?;
        }
    }

    Ok(())
}
