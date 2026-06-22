//! Integration tests for `sandslash diff`.
//!
//! These tests exercise the CLI binary directly via `std::process::Command`,
//! loading fixture JSON files from `tests/fixtures/diff/`.

use std::process::Command;

const BEFORE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/diff/before.json"
);
const AFTER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/diff/after.json"
);

// ── Helper ────────────────────────────────────────────────────────────────────

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sandslash"))
}

// ── Test: valid input exits 0 with summary text ───────────────────────────────

#[test]
fn diff_valid_input_exits_zero_with_summary() {
    let output = bin()
        .args(["diff", "--no-color", BEFORE, AFTER])
        .output()
        .expect("failed to run sandslash diff");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "diff should exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("regressions"),
        "stdout should contain summary; got: {stdout}"
    );
    assert!(
        stdout.contains("improvements"),
        "stdout should contain 'improvements'; got: {stdout}"
    );
}

// ── Test: missing file → non-zero exit, clear stderr error ───────────────────

#[test]
fn diff_missing_file_exits_nonzero_with_stderr_error() {
    let output = bin()
        .args(["diff", "/nonexistent/before.json", AFTER])
        .output()
        .expect("failed to run sandslash diff");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "diff with missing file should exit non-zero"
    );
    assert!(
        !stderr.is_empty(),
        "stderr should contain an error message; got empty"
    );
}

// ── Test: --output json produces valid JSON ───────────────────────────────────

#[test]
fn diff_json_output_is_valid_json() {
    let output = bin()
        .args(["diff", BEFORE, AFTER, "--output", "json"])
        .output()
        .expect("failed to run sandslash diff");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "diff --output json should exit 0; stderr: {stderr}"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("--output json must produce valid JSON");

    assert!(
        parsed["before_site_score"].is_number(),
        "before_site_score must be a number"
    );
    assert!(
        parsed["after_site_score"].is_number(),
        "after_site_score must be a number"
    );
    assert!(
        parsed["site_score_delta"].is_number(),
        "site_score_delta must be a number"
    );
    assert!(parsed["pages"].is_array(), "pages must be an array");

    // Verify fixture values.
    assert_eq!(parsed["before_site_score"], 65);
    assert_eq!(parsed["after_site_score"], 80);
    assert_eq!(parsed["site_score_delta"], 15);
}

// ── Test: --no-color output has no ANSI codes ─────────────────────────────────

#[test]
fn diff_no_color_has_no_ansi_codes() {
    let output = bin()
        .args(["diff", "--no-color", BEFORE, AFTER])
        .output()
        .expect("failed to run sandslash diff");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "diff --no-color should exit 0");
    assert!(
        !stdout.contains("\x1b["),
        "--no-color output must not contain ANSI escape codes; got: {stdout}"
    );
}

// ── Test: DiffReport fields via library API ───────────────────────────────────

#[test]
fn diff_report_fields_match_fixtures() {
    use sandslash::diff::compute::diff_reports;
    use sandslash::diff::model::PageDiffKind;
    use sandslash::model::AuditReport;

    let before: AuditReport = serde_json::from_str(include_str!("fixtures/diff/before.json"))
        .expect("before.json must deserialize");
    let after: AuditReport = serde_json::from_str(include_str!("fixtures/diff/after.json"))
        .expect("after.json must deserialize");

    let report = diff_reports(&before, &after);

    assert_eq!(report.before_site_score, 65);
    assert_eq!(report.after_site_score, 80);
    assert_eq!(report.site_score_delta, 15);

    // 4 pages total: unchanged, changed, removed, added.
    assert_eq!(report.pages.len(), 4, "expected 4 pages in diff");

    let find = |url: &str| {
        report
            .pages
            .iter()
            .find(|p| p.url.as_str() == url)
            .expect("page must exist in diff")
    };

    let unchanged = find("https://example.com/");
    assert_eq!(unchanged.kind, PageDiffKind::Unchanged);
    assert_eq!(unchanged.delta, 0);
    assert_eq!(unchanged.before_score, Some(70));
    assert_eq!(unchanged.after_score, Some(70));

    let changed = find("https://example.com/about");
    assert_eq!(changed.kind, PageDiffKind::Changed);
    assert_eq!(changed.delta, 25);
    assert_eq!(changed.before_score, Some(60));
    assert_eq!(changed.after_score, Some(85));

    let removed = find("https://example.com/removed");
    assert_eq!(removed.kind, PageDiffKind::Removed);
    assert_eq!(removed.before_score, Some(50));
    assert_eq!(removed.after_score, None);

    let added = find("https://example.com/new");
    assert_eq!(added.kind, PageDiffKind::Added);
    assert_eq!(added.before_score, None);
    assert_eq!(added.after_score, Some(90));
}

// ── Test: invalid JSON → non-zero exit ───────────────────────────────────────

#[test]
fn diff_invalid_json_exits_nonzero() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut bad_file = NamedTempFile::new().expect("must create temp file");
    bad_file
        .write_all(b"{ not valid json }")
        .expect("must write");
    bad_file.flush().expect("must flush");

    let output = bin()
        .args(["diff", bad_file.path().to_str().unwrap(), AFTER])
        .output()
        .expect("failed to run sandslash diff");

    assert!(
        !output.status.success(),
        "diff with invalid JSON should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.is_empty(),
        "stderr should contain error for invalid JSON"
    );
}

// ── Test: NO_COLOR env var suppresses ANSI codes ──────────────────────────────

#[test]
fn diff_no_color_env_var_suppresses_ansi_codes() {
    let output = bin()
        .args(["diff", BEFORE, AFTER])
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to run sandslash diff");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "diff with NO_COLOR=1 should exit 0; stderr: {stderr}"
    );
    assert!(
        !stdout.contains("\x1b["),
        "NO_COLOR=1 must suppress all ANSI escape codes; got: {stdout}"
    );
}

// ── Test: cross-site diff emits warning to stderr ─────────────────────────────

#[test]
fn diff_cross_site_warns_on_stderr() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Minimal valid AuditReport JSON with root = https://example.com/
    let before_json = r#"{
        "root": "https://example.com/",
        "site_score": 70,
        "crawled_at": "2026-01-01T00:00:00Z",
        "pages": []
    }"#;

    // Different root URL — https://other-site.com/ — triggers the cross-site warning.
    let after_json = r#"{
        "root": "https://other-site.com/",
        "site_score": 80,
        "crawled_at": "2026-06-01T00:00:00Z",
        "pages": []
    }"#;

    let mut before_file = NamedTempFile::new().expect("must create temp before file");
    before_file
        .write_all(before_json.as_bytes())
        .expect("must write before JSON");
    before_file.flush().expect("must flush before file");

    let mut after_file = NamedTempFile::new().expect("must create temp after file");
    after_file
        .write_all(after_json.as_bytes())
        .expect("must write after JSON");
    after_file.flush().expect("must flush after file");

    let output = bin()
        .args([
            "diff",
            "--no-color",
            before_file.path().to_str().unwrap(),
            after_file.path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run sandslash diff");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "cross-site diff should still exit 0; stderr: {stderr}"
    );
    assert!(
        stderr.contains("warning:"),
        "stderr must contain cross-site warning; got: {stderr}"
    );
    assert!(
        stderr.contains("different root URLs"),
        "stderr warning must mention 'different root URLs'; got: {stderr}"
    );
}
