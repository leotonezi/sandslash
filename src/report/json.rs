use std::io::Write;

use crate::error::{Result, SeoError};
use crate::model::AuditReport;

pub fn write_json<W: Write>(report: &AuditReport, writer: W) -> Result<()> {
    serde_json::to_writer_pretty(writer, report)
        .map_err(|e| SeoError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AuditReport, PageReport};
    use std::collections::HashMap;

    fn minimal_report() -> AuditReport {
        AuditReport {
            root: "https://example.com/".parse().unwrap(),
            pages: vec![PageReport {
                url: "https://example.com/".parse().unwrap(),
                findings: vec![],
                category_scores: HashMap::new(),
                score: 100,
            }],
            site_score: 100,
            crawled_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn serializes_to_valid_json() {
        let report = minimal_report();
        let mut buf = Vec::new();
        write_json(&report, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["root"], "https://example.com/");
        assert_eq!(parsed["site_score"], 100);
        assert!(parsed["pages"].is_array());
        assert!(parsed["crawled_at"].is_string());
    }
}
