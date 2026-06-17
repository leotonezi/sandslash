export const runtime = "nodejs";

import { getHistory, detectRegression } from "@/lib/history";

const DEFAULT_LIMIT = 20;
const REGRESSION_THRESHOLD = Number(process.env.REGRESSION_THRESHOLD) || 5;

export async function GET(request: Request): Promise<Response> {
  const { searchParams } = new URL(request.url);
  const host = searchParams.get("host");

  if (!host || host.trim() === "") {
    return Response.json(
      { error: "Missing required query param: host" },
      { status: 400 },
    );
  }

  const rawLimit = Number(searchParams.get("limit") ?? DEFAULT_LIMIT);
  const limit = Math.min(Math.max(isNaN(rawLimit) ? DEFAULT_LIMIT : rawLimit, 1), 100);

  try {
    const runs = await getHistory(host.trim(), limit);
    const regression = detectRegression(runs, REGRESSION_THRESHOLD);
    return Response.json({ runs, regression });
  } catch (err) {
    console.error("[history] failed to load history:", err);
    return Response.json({ error: "Failed to load history" }, { status: 500 });
  }
}
