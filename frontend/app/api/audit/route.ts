export const runtime = "nodejs";

import { spawn } from "child_process";
import fs from "fs";
import os from "os";
import path from "path";
import { randomUUID } from "crypto";
import { saveAuditRun } from "@/lib/history";
import type { AuditReport } from "@/app/components/ReportView.types";

const PROTECTION_ENABLED = process.env.LIVE_DEMO_RATE_LIMIT_PROTECTION === "true";
const MAX_CONCURRENT = 3;
const RATE_LIMIT_WINDOW_MS = 60_000;
const RATE_LIMIT_MAX = 5;

let activeAudits = 0;

interface RateEntry {
  count: number;
  windowStart: number;
}
const ipRateMap = new Map<string, RateEntry>();

function getClientIp(request: Request): string {
  return (
    request.headers.get("x-forwarded-for")?.split(",")[0]?.trim() ??
    "unknown"
  );
}

function isRateLimited(ip: string): boolean {
  const now = Date.now();
  const entry = ipRateMap.get(ip);
  if (!entry || now - entry.windowStart >= RATE_LIMIT_WINDOW_MS) {
    ipRateMap.set(ip, { count: 1, windowStart: now });
    return false;
  }
  if (entry.count >= RATE_LIMIT_MAX) return true;
  entry.count++;
  return false;
}

export async function POST(request: Request): Promise<Response> {
  const ip = getClientIp(request);

  if (PROTECTION_ENABLED && isRateLimited(ip)) {
    return Response.json(
      { error: "Too many requests. Try again in a minute." },
      { status: 429 }
    );
  }

  if (PROTECTION_ENABLED && activeAudits >= MAX_CONCURRENT) {
    return Response.json(
      { error: "Server busy. Try again in a moment." },
      { status: 429 }
    );
  }

  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return Response.json({ error: "Invalid JSON body" }, { status: 400 });
  }

  if (
    typeof body !== "object" ||
    body === null ||
    !("url" in body) ||
    typeof (body as Record<string, unknown>).url !== "string"
  ) {
    return Response.json({ error: "Missing required field: url" }, { status: 400 });
  }

  const url = (body as { url: string }).url;

  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return Response.json(
      { error: `Invalid URL: "${url}"` },
      { status: 400 }
    );
  }

  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    return Response.json(
      { error: `URL must use http or https protocol, got: "${parsed.protocol}"` },
      { status: 400 }
    );
  }

  const tempPath = path.join(os.tmpdir(), `${randomUUID()}.json`);
  const binPath =
    process.env.SEO_RS_BIN ??
    path.resolve(process.cwd(), "../target/release/sandslash");

  activeAudits++;
  try {
    const result = await new Promise<{ exitCode: number; stderr: string }>(
      (resolve) => {
        const stderrChunks: Buffer[] = [];

        const child = spawn(binPath, [url, "--depth", "0", "-o", tempPath], {
          stdio: ["ignore", "ignore", "pipe"],
        });

        child.stderr.on("data", (chunk: Buffer) => {
          stderrChunks.push(chunk);
        });

        child.on("close", (code) => {
          resolve({
            exitCode: code ?? 1,
            stderr: Buffer.concat(stderrChunks).toString("utf-8"),
          });
        });

        child.on("error", (err) => {
          resolve({
            exitCode: 1,
            stderr: `Failed to start binary: ${err.message}`,
          });
        });
      }
    );

    if (result.exitCode !== 0) {
      return Response.json(
        {
          error: `seo-rs exited with code ${result.exitCode}`,
          stderr: result.stderr,
        },
        { status: 500 }
      );
    }

    let raw: string;
    try {
      raw = fs.readFileSync(tempPath, "utf-8");
    } catch (err) {
      return Response.json(
        {
          error: "Failed to read output file",
          stderr: result.stderr,
          detail: String(err),
        },
        { status: 500 }
      );
    }

    let report: unknown;
    try {
      report = JSON.parse(raw);
    } catch {
      return Response.json(
        {
          error: "seo-rs produced invalid JSON",
          stderr: result.stderr,
        },
        { status: 500 }
      );
    }

    try {
      await saveAuditRun(report as AuditReport);
    } catch (err) {
      console.error("[audit] persistence failed:", err);
    }

    return Response.json({ report });
  } finally {
    activeAudits--;
    try {
      fs.unlinkSync(tempPath);
    } catch {
      // best-effort cleanup — ignore errors
    }
  }
}
