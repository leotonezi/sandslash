import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { detectRegression, saveAuditRun } from "./history.js";
import type { AuditReport } from "../app/components/ReportView.types.js";

describe("detectRegression", () => {
  it("returns regressed=true when score dropped beyond threshold", () => {
    const result = detectRegression(
      [{ site_score: 80 }, { site_score: 90 }],
      5,
    );
    assert.deepEqual(result, { regressed: true, deltaPoints: -10 });
  });

  it("returns regressed=false when score improved", () => {
    const result = detectRegression(
      [{ site_score: 92 }, { site_score: 90 }],
      5,
    );
    assert.deepEqual(result, { regressed: false });
  });

  it("returns regressed=false with fewer than 2 runs", () => {
    assert.deepEqual(detectRegression([], 5), { regressed: false });
    assert.deepEqual(detectRegression([{ site_score: 80 }], 5), { regressed: false });
  });

  it("returns regressed=false when drop equals threshold exactly", () => {
    const result = detectRegression(
      [{ site_score: 85 }, { site_score: 90 }],
      5,
    );
    assert.deepEqual(result, { regressed: false });
  });
});

describe("saveAuditRun", () => {
  it("resolves without throwing when DATABASE_URL is unset", async () => {
    const prev = process.env.DATABASE_URL;
    delete process.env.DATABASE_URL;

    const fakeReport: AuditReport = {
      root: "https://example.com",
      pages: [],
      site_score: 80,
      crawled_at: new Date().toISOString(),
    };

    await assert.doesNotReject(() => saveAuditRun(fakeReport));

    if (prev !== undefined) process.env.DATABASE_URL = prev;
  });
});
