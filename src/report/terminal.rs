use std::io::Write;

use comfy_table::Table;
use owo_colors::{AnsiColors, OwoColorize};

use crate::error::Result;
use crate::model::{AuditReport, Category, Severity};

pub struct TerminalOpts {
    pub quiet: bool,
    pub no_color: bool,
    pub is_tty: bool,
}

fn color_enabled(opts: &TerminalOpts) -> bool {
    !opts.no_color && std::env::var_os("NO_COLOR").is_none() && opts.is_tty
}

fn score_color(score: u8) -> AnsiColors {
    if score >= 90 {
        AnsiColors::Green
    } else if score >= 70 {
        AnsiColors::Yellow
    } else {
        AnsiColors::Red
    }
}

pub fn write_terminal<W: Write>(
    report: &AuditReport,
    opts: &TerminalOpts,
    writer: &mut W,
) -> Result<()> {
    if opts.quiet {
        writeln!(writer, "{}", report.site_score)?;
        return Ok(());
    }

    render_header(report, opts, writer)?;
    render_category_bars(report, opts, writer)?;
    render_page_table(report, opts, writer)?;
    Ok(())
}

fn render_header<W: Write>(
    report: &AuditReport,
    opts: &TerminalOpts,
    writer: &mut W,
) -> Result<()> {
    let use_color = color_enabled(opts);
    let root = report.root.as_str();
    let score = report.site_score;

    if use_color {
        let color = score_color(score);
        write!(
            writer,
            "SEO Report: {}\nSite Score: {}/100\n\n",
            root,
            score.color(color)
        )?;
    } else {
        write!(writer, "SEO Report: {root}\nSite Score: {score}/100\n\n",)?;
    }

    Ok(())
}

fn render_category_bars<W: Write>(
    report: &AuditReport,
    opts: &TerminalOpts,
    writer: &mut W,
) -> Result<()> {
    let use_color = color_enabled(opts);

    for cat in Category::all() {
        let scores: Vec<u8> = report
            .pages
            .iter()
            .filter_map(|p| p.category_scores.get(&cat).copied())
            .collect();

        let mean: u8 = if scores.is_empty() {
            0
        } else {
            let sum: u32 = scores.iter().map(|&s| s as u32).sum();
            (sum / scores.len() as u32) as u8
        };

        let filled = (mean as f64 * 10.0 / 100.0).round() as usize;
        let empty = 10 - filled;
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
        let name = format!("{cat:?}");

        if use_color {
            let color = score_color(mean);
            writeln!(writer, "{name:13} {bar} {:>3}", mean.color(color))?;
        } else {
            writeln!(writer, "{name:13} {bar} {mean:>3}")?;
        }
    }

    writeln!(writer)?;
    Ok(())
}

fn render_page_table<W: Write>(
    report: &AuditReport,
    _opts: &TerminalOpts,
    writer: &mut W,
) -> Result<()> {
    let mut table = Table::new();
    table.set_header(["URL", "Score", "Critical", "Warning", "Info"]);

    let mut pages: Vec<_> = report.pages.iter().collect();
    pages.sort_by_key(|p| p.score);

    for page in pages {
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

        let mut critical = 0u32;
        let mut warning = 0u32;
        let mut info = 0u32;
        for f in &page.findings {
            match f.severity {
                Severity::Critical => critical += 1,
                Severity::Warning => warning += 1,
                Severity::Info => info += 1,
            }
        }

        table.add_row([
            truncated,
            page.score.to_string(),
            critical.to_string(),
            warning.to_string(),
            info.to_string(),
        ]);
    }

    writeln!(writer, "{table}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AuditReport, Category, Finding, PageReport, Severity};
    use std::collections::HashMap;

    fn make_report_single(score: u8) -> AuditReport {
        let category_scores: HashMap<Category, u8> = Category::all().map(|c| (c, score)).collect();

        AuditReport {
            root: "https://example.com/"
                .parse()
                .expect("invariant: valid url"),
            pages: vec![PageReport {
                url: "https://example.com/"
                    .parse()
                    .expect("invariant: valid url"),
                findings: vec![],
                category_scores,
                score,
            }],
            site_score: score,
            crawled_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn no_color_renders_text_without_ansi() {
        let report = make_report_single(85);
        let opts = TerminalOpts {
            quiet: false,
            no_color: true,
            is_tty: false,
        };

        let mut buf = Vec::new();
        write_terminal(&report, &opts, &mut buf).expect("write_terminal should not fail");
        let output = String::from_utf8(buf).expect("output should be valid utf-8");

        assert!(output.contains("https://example.com/"));
        assert!(output.contains("Metadata"));
        assert!(output.contains("85"));
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn quiet_emits_exactly_score_plus_newline() {
        let report = make_report_single(85);
        let opts = TerminalOpts {
            quiet: true,
            no_color: true,
            is_tty: false,
        };

        let mut buf = Vec::new();
        write_terminal(&report, &opts, &mut buf).expect("write_terminal should not fail");
        let output = String::from_utf8(buf).expect("output should be valid utf-8");

        assert_eq!(output, "85\n");
    }

    #[test]
    fn page_table_sorted_by_score_ascending() {
        let make_page = |url: &str, score: u8, findings: Vec<Finding>| PageReport {
            url: url.parse().expect("invariant: valid url"),
            findings,
            category_scores: Category::all().map(|c| (c, score)).collect(),
            score,
        };

        let report = AuditReport {
            root: "https://example.com/"
                .parse()
                .expect("invariant: valid url"),
            pages: vec![
                make_page("https://example.com/a", 80, vec![]),
                make_page(
                    "https://example.com/b",
                    40,
                    vec![Finding {
                        check_id: "title.missing",
                        category: Category::Metadata,
                        severity: Severity::Critical,
                        message: "Missing title".to_owned(),
                        penalty: 30,
                    }],
                ),
                make_page("https://example.com/c", 90, vec![]),
            ],
            site_score: 70,
            crawled_at: "2026-01-01T00:00:00Z".to_owned(),
        };

        let opts = TerminalOpts {
            quiet: false,
            no_color: true,
            is_tty: false,
        };

        let mut buf = Vec::new();
        write_terminal(&report, &opts, &mut buf).expect("write_terminal should not fail");
        let output = String::from_utf8(buf).expect("output should be valid utf-8");

        let pos_40 = output
            .find("example.com/b")
            .expect("url with score 40 should appear");
        let pos_80 = output
            .find("example.com/a")
            .expect("url with score 80 should appear");
        let pos_90 = output
            .find("example.com/c")
            .expect("url with score 90 should appear");

        assert!(
            pos_40 < pos_80,
            "score=40 should appear before score=80 in output"
        );
        assert!(
            pos_80 < pos_90,
            "score=80 should appear before score=90 in output"
        );
    }
}
