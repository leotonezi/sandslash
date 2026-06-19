"use client";

import type { RegressionResult } from "./ReportView.types";

interface Props {
  result: RegressionResult;
}

export default function RegressionBadge({ result }: Props) {
  if (!result.regressed) return null;

  const drop = Math.abs(result.deltaPoints ?? 0);

  return (
    <div
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "0.375rem",
        background: "var(--error-bg)",
        border: "1px solid var(--error-border)",
        color: "var(--error-text)",
        borderRadius: "0.375rem",
        padding: "0.375rem 0.75rem",
        fontSize: "0.875rem",
        fontWeight: 600,
        marginBottom: "1rem",
      }}
    >
      &#9660; Score dropped {drop} pt{drop !== 1 ? "s" : ""} vs. previous run
    </div>
  );
}
