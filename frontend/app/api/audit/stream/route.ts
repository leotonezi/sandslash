export const runtime = "nodejs";

import { saveAuditRun } from "@/lib/history";
import type { AuditReport } from "@/app/components/ReportView.types";

export async function GET(request: Request): Promise<Response> {
  const { searchParams } = new URL(request.url);
  const jobId = searchParams.get("job");

  if (!jobId) {
    return Response.json({ error: "Missing job query param" }, { status: 400 });
  }

  const rustServerUrl = process.env.NEXT_PUBLIC_SEO_RS_URL;
  if (!rustServerUrl) {
    return Response.json({ error: "Streaming not configured" }, { status: 503 });
  }

  let upstream: Response;
  try {
    upstream = await fetch(`${rustServerUrl}/api/audits/${jobId}/events`, {
      headers: { Accept: "text/event-stream", "Cache-Control": "no-cache" },
    });
  } catch (err) {
    return Response.json(
      { error: `Failed to reach Rust server: ${String(err)}` },
      { status: 502 }
    );
  }

  if (!upstream.ok || !upstream.body) {
    return Response.json(
      { error: `Rust server returned ${upstream.status}` },
      { status: upstream.status }
    );
  }

  const upstreamBody = upstream.body;

  const stream = new ReadableStream({
    async start(controller) {
      const reader = upstreamBody.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;

          const chunk = decoder.decode(value, { stream: true });
          buffer += chunk;
          controller.enqueue(value);

          // Scan buffer for complete SSE events to intercept Done
          let boundary: number;
          while ((boundary = buffer.indexOf("\n\n")) !== -1) {
            const block = buffer.slice(0, boundary);
            buffer = buffer.slice(boundary + 2);

            let eventName = "";
            let eventData = "";
            for (const line of block.split("\n")) {
              if (line.startsWith("event: ")) eventName = line.slice(7).trim();
              if (line.startsWith("data: ")) eventData = line.slice(6).trim();
            }

            if (eventName === "Done" && eventData) {
              try {
                const parsed = JSON.parse(eventData) as { report?: AuditReport };
                if (parsed.report) {
                  await saveAuditRun(parsed.report).catch((err) => {
                    console.error("[stream] persistence failed:", err);
                  });
                }
              } catch {
                // ignore parse errors — we already forwarded the raw bytes
              }
            }
          }
        }
      } catch {
        // upstream closed or errored — end our stream cleanly
      } finally {
        reader.releaseLock();
        controller.close();
      }
    },
  });

  return new Response(stream, {
    headers: {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      Connection: "keep-alive",
    },
  });
}
