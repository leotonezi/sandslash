import type { AuditReport, Finding, PageReport, Severity } from "./ReportView.types";

function scoreClass(score: number): string {
  if (score >= 90) return "score-good";
  if (score >= 70) return "score-warn";
  return "score-bad";
}

function severityClass(severity: Severity): string {
  switch (severity) {
    case "Critical":
      return "severity-critical";
    case "Warning":
      return "severity-warning";
    case "Info":
      return "severity-info";
  }
}

function FindingItem({ finding }: { finding: Finding }) {
  return (
    <li>
      <span className={severityClass(finding.severity)}>{finding.severity}</span>
      <span className="check-id">[{finding.check_id}]</span>
      <span>{finding.message}</span>
    </li>
  );
}

function PageBlock({ page }: { page: PageReport }) {
  const categoryEntries = Object.entries(page.category_scores) as [string, number][];

  return (
    <div className="page-block">
      <p className="page-url">{page.url}</p>
      <p>
        Page score:{" "}
        <span className={scoreClass(page.score)}>{page.score}</span>
      </p>

      {categoryEntries.length > 0 && (
        <>
          <h3>Category Scores</h3>
          <table>
            <thead>
              <tr>
                <th>Category</th>
                <th>Score</th>
              </tr>
            </thead>
            <tbody>
              {categoryEntries.map(([cat, score]) => (
                <tr key={cat}>
                  <td>{cat}</td>
                  <td className={scoreClass(score)}>{score}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}

      {page.findings.length > 0 && (
        <>
          <h3>Findings ({page.findings.length})</h3>
          <ul className="findings-list">
            {page.findings.map((f, i) => (
              <FindingItem key={`${f.check_id}-${i}`} finding={f} />
            ))}
          </ul>
        </>
      )}

      {page.findings.length === 0 && (
        <p className="severity-info">No findings — page looks great!</p>
      )}
    </div>
  );
}

export default function ReportView({ report }: { report: AuditReport }) {
  return (
    <div>
      <div className="report-header">
        <h2>{report.root}</h2>
        <p className="meta">Crawled at: {report.crawled_at}</p>
        <div className="site-score">
          Site score:{" "}
          <span className={scoreClass(report.site_score)}>{report.site_score}</span>
        </div>
      </div>

      <h2>Pages ({report.pages.length})</h2>
      {report.pages.map((page, i) => (
        <PageBlock key={`${page.url}-${i}`} page={page} />
      ))}
    </div>
  );
}
