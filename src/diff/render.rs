use std::io::Write;

use comfy_table::Table;
use owo_colors::OwoColorize;

use crate::error::{Result, SeoError};
use crate::model::Category;

use super::model::{DiffReport, PageDiffKind};

// ── Color helper ─────────────────────────────────────────────────────────────

/// Returns `true` when ANSI color output is appropriate.
///
/// Mirrors the pattern from `src/report/terminal.rs`.
fn color_enabled(no_color: bool) -> bool {
    !no_color && std::env::var_os("NO_COLOR").is_none()
}

// ── Summary counts ────────────────────────────────────────────────────────────

struct Summary {
    regressions: usize,
    improvements: usize,
    unchanged: usize,
    added: usize,
    removed: usize,
}

impl Summary {
    fn from_report(report: &DiffReport) -> Self {
        let mut s = Summary {
            regressions: 0,
            improvements: 0,
            unchanged: 0,
            added: 0,
            removed: 0,
        };
        for page in &report.pages {
            match page.kind {
                PageDiffKind::Added => s.added += 1,
                PageDiffKind::Removed => s.removed += 1,
                PageDiffKind::Unchanged => s.unchanged += 1,
                PageDiffKind::Changed => {
                    if page.delta < 0 {
                        s.regressions += 1;
                    } else {
                        s.improvements += 1;
                    }
                }
            }
        }
        s
    }
}

// ── Text renderer ─────────────────────────────────────────────────────────────

/// Write a human-readable diff report to `writer`.
///
/// Color is suppressed when `no_color` is `true` or the `NO_COLOR` env var is set.
pub fn write_text<W: Write>(diff: &DiffReport, no_color: bool, writer: &mut W) -> Result<()> {
    let use_color = color_enabled(no_color);
    let s = Summary::from_report(diff);

    // Summary line.
    writeln!(
        writer,
        "{} regressions, {} improvements, {} unchanged, {} added, {} removed",
        s.regressions, s.improvements, s.unchanged, s.added, s.removed
    )?;

    // Site score delta.
    let delta = diff.site_score_delta;
    if use_color {
        let delta_str = format_delta(delta);
        if delta > 0 {
            writeln!(
                writer,
                "Site score: {} → {} ({})",
                diff.before_site_score,
                diff.after_site_score,
                delta_str.green()
            )?;
        } else if delta < 0 {
            writeln!(
                writer,
                "Site score: {} → {} ({})",
                diff.before_site_score,
                diff.after_site_score,
                delta_str.red()
            )?;
        } else {
            writeln!(
                writer,
                "Site score: {} → {} ({})",
                diff.before_site_score, diff.after_site_score, delta_str
            )?;
        }
    } else {
        writeln!(
            writer,
            "Site score: {} → {} ({})",
            diff.before_site_score,
            diff.after_site_score,
            format_delta(delta)
        )?;
    }

    writeln!(writer)?;

    // Category table.
    render_category_table(diff, use_color, writer)?;

    writeln!(writer)?;

    // Per-page table (already sorted by delta asc in compute.rs).
    render_page_table(diff, use_color, writer)?;

    Ok(())
}

fn format_delta(delta: i16) -> String {
    if delta > 0 {
        format!("+{delta}")
    } else {
        format!("{delta}")
    }
}

fn render_category_table<W: Write>(
    diff: &DiffReport,
    use_color: bool,
    writer: &mut W,
) -> Result<()> {
    let mut table = Table::new();
    table.set_header(["Category", "Before", "After", "Delta"]);

    for cat in Category::all() {
        let name = format!("{cat:?}");
        let delta = diff.category_deltas.get(&cat).copied().unwrap_or(0);
        let delta_str = format_delta(delta);

        if use_color {
            let colored = if delta > 0 {
                delta_str.green().to_string()
            } else if delta < 0 {
                delta_str.red().to_string()
            } else {
                delta_str
            };
            table.add_row([name, "—".to_owned(), "—".to_owned(), colored]);
        } else {
            table.add_row([name, "—".to_owned(), "—".to_owned(), delta_str]);
        }
    }

    writeln!(writer, "{table}")?;
    Ok(())
}

fn render_page_table<W: Write>(diff: &DiffReport, use_color: bool, writer: &mut W) -> Result<()> {
    let mut table = Table::new();
    table.set_header(["URL", "Kind", "Before", "After", "Delta"]);

    for page in &diff.pages {
        let url_str = page.url.as_str();
        let truncated = if url_str.chars().count() > 60 {
            let byte_idx = url_str
                .char_indices()
                .nth(60)
                .map(|(i, _)| i)
                .unwrap_or(url_str.len());
            format!("{}…", &url_str[..byte_idx])
        } else {
            url_str.to_string()
        };

        let kind_str = match page.kind {
            PageDiffKind::Added => "Added",
            PageDiffKind::Removed => "Removed",
            PageDiffKind::Changed => "Changed",
            PageDiffKind::Unchanged => "Unchanged",
        };

        let before_str = page
            .before_score
            .map(|s| s.to_string())
            .unwrap_or_else(|| "—".to_owned());
        let after_str = page
            .after_score
            .map(|s| s.to_string())
            .unwrap_or_else(|| "—".to_owned());

        let delta = page.delta;
        let delta_str = format_delta(delta);

        if use_color {
            let colored_delta = if delta > 0 {
                delta_str.green().to_string()
            } else if delta < 0 {
                delta_str.red().to_string()
            } else {
                delta_str
            };
            table.add_row([
                truncated,
                kind_str.to_owned(),
                before_str,
                after_str,
                colored_delta,
            ]);
        } else {
            table.add_row([
                truncated,
                kind_str.to_owned(),
                before_str,
                after_str,
                delta_str,
            ]);
        }
    }

    writeln!(writer, "{table}")?;
    Ok(())
}

// ── JSON renderer ─────────────────────────────────────────────────────────────

/// Write the [`DiffReport`] as pretty-printed JSON to `writer`.
pub fn write_json<W: Write>(diff: &DiffReport, writer: W) -> Result<()> {
    serde_json::to_writer_pretty(writer, diff).map_err(|e| SeoError::Io(std::io::Error::other(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::compute::diff_reports;
    use crate::model::{AuditReport, Category, PageReport};
    use std::collections::HashMap;

    fn make_page_report(url: &str, score: u8) -> PageReport {
        let category_scores: HashMap<Category, u8> = Category::all().map(|c| (c, score)).collect();
        PageReport {
            url: url.parse().expect("invariant: valid url"),
            findings: vec![],
            category_scores,
            score,
        }
    }

    fn make_audit_report(root: &str, pages: Vec<PageReport>, site_score: u8) -> AuditReport {
        AuditReport {
            root: root.parse().expect("invariant: valid url"),
            pages,
            site_score,
            crawled_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn no_color_output_has_no_ansi_codes() {
        let before = make_audit_report(
            "https://example.com/",
            vec![make_page_report("https://example.com/", 60)],
            60,
        );
        let after = make_audit_report(
            "https://example.com/",
            vec![make_page_report("https://example.com/", 80)],
            80,
        );

        let diff = diff_reports(&before, &after);
        let mut buf = Vec::new();
        write_text(&diff, true, &mut buf).expect("write_text must not fail");
        let text = String::from_utf8(buf).expect("must be valid utf-8");

        assert!(
            !text.contains("\x1b["),
            "no_color=true must not emit ANSI codes"
        );
    }

    #[test]
    fn summary_line_contains_counts() {
        let before = make_audit_report(
            "https://example.com/",
            vec![
                make_page_report("https://example.com/", 70),
                make_page_report("https://example.com/removed", 50),
            ],
            60,
        );
        let after = make_audit_report(
            "https://example.com/",
            vec![
                make_page_report("https://example.com/", 70),
                make_page_report("https://example.com/new", 90),
            ],
            80,
        );

        let diff = diff_reports(&before, &after);
        let mut buf = Vec::new();
        write_text(&diff, true, &mut buf).expect("write_text must not fail");
        let text = String::from_utf8(buf).expect("must be valid utf-8");

        // 0 regressions, 0 improvements, 1 unchanged, 1 added, 1 removed
        assert!(
            text.contains("0 regressions"),
            "expected '0 regressions' in: {text}"
        );
        assert!(
            text.contains("0 improvements"),
            "expected '0 improvements' in: {text}"
        );
        assert!(
            text.contains("1 unchanged"),
            "expected '1 unchanged' in: {text}"
        );
        assert!(text.contains("1 added"), "expected '1 added' in: {text}");
        assert!(
            text.contains("1 removed"),
            "expected '1 removed' in: {text}"
        );
    }

    #[test]
    fn json_output_is_valid_json() {
        let before = make_audit_report(
            "https://example.com/",
            vec![make_page_report("https://example.com/", 60)],
            60,
        );
        let after = make_audit_report(
            "https://example.com/",
            vec![make_page_report("https://example.com/", 80)],
            80,
        );

        let diff = diff_reports(&before, &after);
        let mut buf = Vec::new();
        write_json(&diff, &mut buf).expect("write_json must not fail");
        let text = String::from_utf8(buf).expect("must be valid utf-8");

        let parsed: serde_json::Value =
            serde_json::from_str(&text).expect("output must be valid JSON");
        assert_eq!(parsed["before_site_score"], 60);
        assert_eq!(parsed["after_site_score"], 80);
        assert_eq!(parsed["site_score_delta"], 20);
    }
}
