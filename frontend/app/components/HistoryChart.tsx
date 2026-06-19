"use client";

import type { AuditRunSummary } from "./ReportView.types";

interface Props {
  runs: AuditRunSummary[];
}

const WIDTH = 600;
const HEIGHT = 160;
const PAD = { top: 16, right: 16, bottom: 32, left: 36 };
const INNER_W = WIDTH - PAD.left - PAD.right;
const INNER_H = HEIGHT - PAD.top - PAD.bottom;

function scoreColor(score: number): string {
  if (score >= 90) return "#16a34a";
  if (score >= 70) return "#d97706";
  return "#dc2626";
}

export default function HistoryChart({ runs }: Props) {
  if (runs.length < 2) {
    return (
      <p className="history-empty" style={{ color: "var(--text-muted)", fontSize: "0.875rem", marginBottom: "1rem" }}>
        First audit — no history yet.
      </p>
    );
  }

  const ordered = [...runs].reverse();
  const xStep = INNER_W / (ordered.length - 1);

  const points = ordered.map((r, i) => ({
    x: PAD.left + i * xStep,
    y: PAD.top + INNER_H - (r.site_score / 100) * INNER_H,
    score: r.site_score,
    date: new Date(r.crawled_at).toLocaleDateString(),
  }));

  const polyline = points.map((p) => `${p.x},${p.y}`).join(" ");

  return (
    <div style={{ marginBottom: "1.5rem" }}>
      <h3 style={{ marginBottom: "0.5rem" }}>Score History</h3>
      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        style={{ width: "100%", maxWidth: WIDTH, display: "block", overflow: "visible" }}
        aria-label="Score history chart"
      >
        {[0, 50, 100].map((tick) => {
          const y = PAD.top + INNER_H - (tick / 100) * INNER_H;
          return (
            <g key={tick}>
              <line
                x1={PAD.left}
                y1={y}
                x2={PAD.left + INNER_W}
                y2={y}
                stroke="var(--border)"
                strokeWidth={1}
              />
              <text
                x={PAD.left - 6}
                y={y + 4}
                textAnchor="end"
                fontSize={10}
                fill="var(--text-muted)"
              >
                {tick}
              </text>
            </g>
          );
        })}

        <polyline
          points={polyline}
          fill="none"
          stroke="var(--accent)"
          strokeWidth={2}
          strokeLinejoin="round"
        />

        {points.map((p, i) => (
          <g key={i}>
            <circle cx={p.x} cy={p.y} r={4} fill={scoreColor(p.score)} />
            <title>{`${p.date}: ${p.score}`}</title>
            {i === 0 || i === points.length - 1 ? (
              <text
                x={p.x}
                y={HEIGHT - 4}
                textAnchor="middle"
                fontSize={10}
                fill="var(--text-muted)"
              >
                {p.date}
              </text>
            ) : null}
          </g>
        ))}
      </svg>
    </div>
  );
}
