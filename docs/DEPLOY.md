# Deployment Guide (Option A — Split Stack)

Frontend on Vercel, Rust API on Railway. Frontend calls the Rust server via HTTP.

```
Vercel (Next.js) → POST https://<your-app>.railway.app/audit → Rust HTTP server
```

---

## 1. Add HTTP API to seo-rs

The Rust binary currently runs as a CLI. To deploy as a server, add an `axum` HTTP layer.

Add to `Cargo.toml`:
```toml
axum = "0.7"
tower = "0.4"
```

Create `src/server.rs`:
```rust
use axum::{extract::Json, response::Json as ResJson, routing::post, Router};
use serde::Deserialize;
use std::net::SocketAddr;

#[derive(Deserialize)]
struct AuditRequest {
    url: String,
    depth: Option<u32>,
}

async fn audit_handler(Json(req): Json<AuditRequest>) -> ResJson<serde_json::Value> {
    // build CrawlConfig from req, call pipeline::run, return AuditReport as JSON
    todo!()
}

pub async fn serve(addr: SocketAddr) {
    let app = Router::new().route("/audit", post(audit_handler));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Wire into `main.rs` via a `--serve` flag (or separate binary target).

---

## 2. Dockerize the Rust server

Create `Dockerfile` at repo root:
```dockerfile
FROM rust:1.78-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/sandslash /usr/local/bin/sandslash
EXPOSE 8080
CMD ["sandslash", "--serve", "0.0.0.0:8080"]
```

---

## 3. Deploy Rust API to Railway

1. Sign up at railway.app (free tier: 500h/month).
2. New project → Deploy from GitHub repo → select this repo.
3. Railway auto-detects Dockerfile.
4. Set environment variables in Railway dashboard:
   - `REDIS_URL` — if using crawler with Redis frontier (optional for single-page audits)
   - `RUST_LOG=sandslash=info`
5. After deploy, copy the public URL: `https://<your-app>.railway.app`

---

## 4. Update Next.js frontend

In `frontend/`, replace the subprocess call in `app/api/audit/route.ts` with an HTTP call:

```ts
const API_URL = process.env.SEO_RS_API_URL ?? "http://localhost:8080";

export async function POST(req: Request) {
  const body = await req.json();
  const res = await fetch(`${API_URL}/audit`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const report = await res.json();
  return Response.json(report);
}
```

---

## 5. Deploy frontend to Vercel

1. Sign up at vercel.com.
2. New project → Import Git repo → select this repo.
3. Set root directory to `frontend/`.
4. Add environment variable:
   - `SEO_RS_API_URL` = `https://<your-app>.railway.app`
5. Deploy. Vercel auto-deploys on every push to `master`.

---

## 6. Local development

Run both locally:

```bash
# Terminal 1 — Rust API
cargo run -- --serve 0.0.0.0:8080

# Terminal 2 — Next.js
cd frontend
SEO_RS_API_URL=http://localhost:8080 npm run dev
```

---

## Cost

| Service | Free tier |
|---|---|
| Railway | 500 CPU-hours/month, sleeps after inactivity |
| Vercel | Unlimited hobby deployments |
| Redis (frontier) | Railway Redis add-on, 25MB free |

For a demo/portfolio project the free tiers are sufficient. Railway spins down idle services — first request after idle may be slow (~5s cold start).
