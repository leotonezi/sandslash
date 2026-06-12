# Deploying sandslash to Railway

Single Docker image: `sandslash` Rust binary + Next.js 14 frontend.
The frontend API route shells out to the binary via `SEO_RS_BIN` — no code changes needed.

---

## Prerequisites

- [Railway account](https://railway.app)
- Railway CLI: `npm install -g @railway/cli`
- Docker (for local verification only)

---

## Environment variables

| Variable | Default in image | Notes |
|---|---|---|
| `SEO_RS_BIN` | `/usr/local/bin/sandslash` | Baked into Dockerfile — no action needed |
| `PORT` | `3000` | Set by Railway automatically |
| `NODE_ENV` | `production` | Baked into Dockerfile |

No variables need to be set in the Railway dashboard for basic operation.

---

## Deploy

```bash
# One-time setup
railway login
railway link          # select or create project + service

# Deploy
railway up            # builds Dockerfile, pushes, deploys
```

Railway streams build logs. When the deploy finishes, the public URL appears in the dashboard under **Settings → Domains**.

---

## Verify

```bash
PUBLIC_URL=https://your-app.up.railway.app

# Health check
curl -fsS "$PUBLIC_URL/api/health"
# → {"ok":true}

# Audit smoke test
curl -X POST "$PUBLIC_URL/api/audit" \
  -H 'content-type: application/json' \
  -d '{"url":"https://example.com"}'
# → {"report":{"pages":[...],...}}
```

---

## Local Docker verification

```bash
# Build
docker build -t sandslash-demo .

# Size check (should be < 250 MB)
docker images sandslash-demo

# Boot
docker run --rm -p 3000:3000 sandslash-demo

# In another terminal:
curl localhost:3000/api/health
curl -X POST localhost:3000/api/audit \
  -H 'content-type: application/json' \
  -d '{"url":"https://example.com"}'
```

---

## Rollback

Railway keeps previous deploys. To roll back:

1. Dashboard → **Deployments** tab
2. Click the previous successful deploy
3. **Redeploy**

Or via CLI:
```bash
railway rollback
```

---

## Notes

- `--depth 0` is hardcoded in the API route — multi-page crawl (Redis) is not wired yet.
- Cold start on Railway free tier (auto-sleep) is ~2–5s on first request after idle.
- Temp files written to `/tmp` during audits are cleaned up in a `finally` block in `route.ts`.
