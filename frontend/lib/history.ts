import { getPool } from "./db";
import { ensureSchema } from "./migrate";
import type { AuditReport, AuditRunSummary, RegressionResult } from "@/app/components/ReportView.types";

export async function saveAuditRun(report: AuditReport): Promise<void> {
  const pool = getPool();
  if (!pool) return;
  await ensureSchema(pool);
  const host = new URL(report.root).host;
  await pool.query(
    `INSERT INTO audit_runs (host, root_url, site_score, report, crawled_at)
     VALUES ($1, $2, $3, $4, $5)`,
    [host, report.root, report.site_score, JSON.stringify(report), report.crawled_at],
  );
}

export async function getHistory(host: string, limit: number): Promise<AuditRunSummary[]> {
  const pool = getPool();
  if (!pool) return [];
  await ensureSchema(pool);
  const { rows } = await pool.query<AuditRunSummary>(
    `SELECT id, host, root_url, site_score, crawled_at
     FROM audit_runs
     WHERE host = $1
     ORDER BY crawled_at DESC
     LIMIT $2`,
    [host, limit],
  );
  return rows;
}

export function detectRegression(
  runs: { site_score: number }[],
  threshold: number,
): RegressionResult {
  if (runs.length < 2) return { regressed: false };
  const delta = runs[0].site_score - runs[1].site_score;
  if (delta < -threshold) return { regressed: true, deltaPoints: delta };
  return { regressed: false };
}
