"use client";

import { useState } from "react";
import type { AuditReport, AuditRunSummary, RegressionResult } from "./components/ReportView.types";
import ReportView from "./components/ReportView";
import HistoryChart from "./components/HistoryChart";
import RegressionBadge from "./components/RegressionBadge";

export default function Home() {
  const [url, setUrl] = useState("");
  const [report, setReport] = useState<AuditReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [history, setHistory] = useState<AuditRunSummary[]>([]);
  const [regression, setRegression] = useState<RegressionResult>({ regressed: false });

  async function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setReport(null);
    setError(null);
    setLoading(true);
    setHistory([]);
    setRegression({ regressed: false });

    try {
      const res = await fetch("/api/audit", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ url }),
      });

      const data = (await res.json()) as
        | { report: AuditReport }
        | { error: string; stderr?: string };

      if (!res.ok || "error" in data) {
        const msg =
          "error" in data
            ? data.error + ("stderr" in data && data.stderr ? `\n\n${data.stderr}` : "")
            : `HTTP ${res.status}`;
        setError(msg);
      } else {
        setReport(data.report);

        const host = new URL(data.report.root).host;
        try {
          const histRes = await fetch(`/api/history?host=${encodeURIComponent(host)}`);
          if (histRes.ok) {
            const histData = (await histRes.json()) as {
              runs: AuditRunSummary[];
              regression: RegressionResult;
            };
            setHistory(histData.runs);
            setRegression(histData.regression);
          }
        } catch {
          // history is non-critical — ignore failures
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unexpected error");
    } finally {
      setLoading(false);
    }
  }

  return (
    <main>
      <div className="hero">
        <h1>Blazing-fast SEO audits,<br />built with Rust.</h1>
        <p className="hero-sub">Checks titles, meta tags, headings, canonicals, and more.</p>
      </div>

      <section className="audit-card">
        <form className="audit-form" onSubmit={handleSubmit}>
          <div className="field">
            <label htmlFor="audit-url" className="field-label">Website URL</label>
            <p className="field-hint">Enter the full address of the page or site you want to audit.</p>
            <div className="input-row">
              <input
                id="audit-url"
                type="url"
                placeholder="https://example.com"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                required
                disabled={loading}
              />
              <button type="submit" disabled={loading || url.trim() === ""}>
                {loading ? "Running…" : "Run Audit"}
              </button>
            </div>
          </div>
        </form>
        {loading && <p className="loading-msg">Running audit, please wait…</p>}
      </section>

      {error && (
        <div className="error-box">
          <strong>Error:</strong>
          <pre style={{ marginTop: "0.5rem", whiteSpace: "pre-wrap", fontSize: "0.8rem" }}>
            {error}
          </pre>
        </div>
      )}

      {report && (
        <>
          <RegressionBadge result={regression} />
          <ReportView report={report} />
          <HistoryChart runs={history} />
        </>
      )}
    </main>
  );
}
