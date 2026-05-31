export const runtime = "nodejs";

import { spawn } from "child_process";
import fs from "fs";
import os from "os";
import path from "path";
import { randomUUID } from "crypto";

export async function POST(request: Request): Promise<Response> {
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

  try {
    const result = await new Promise<{ exitCode: number; stderr: string }>(
      (resolve) => {
        const stderrChunks: Buffer[] = [];

        const child = spawn(binPath, [url, "-o", tempPath], {
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

    return Response.json({ report });
  } finally {
    try {
      fs.unlinkSync(tempPath);
    } catch {
      // best-effort cleanup — ignore errors
    }
  }
}
