# Postgres Setup

Audit history is persisted to Postgres by the Next.js frontend. The schema is created automatically on first use.

## Local Development

Run Postgres via Docker:

```bash
docker run -d \
  --name sandslash-postgres \
  -e POSTGRES_USER=sandslash \
  -e POSTGRES_PASSWORD=sandslash \
  -e POSTGRES_DB=sandslash \
  -p 5432:5432 \
  postgres:16
```

Then set the environment variable in `frontend/.env.local`:

```
DATABASE_URL=postgresql://sandslash:sandslash@localhost:5432/sandslash
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | *(unset)* | Postgres connection string. If unset, persistence is silently skipped. |
| `REGRESSION_THRESHOLD` | `5` | Score drop (in points) that triggers the regression badge. |
| `PGSSL` | *(unset)* | Set to `true` to enable SSL with `rejectUnauthorized: false`. |

## Railway Add-on

1. In your Railway project, add a **Postgres** plugin.
2. In the service's **Variables** tab, add:
   ```
   DATABASE_URL=${{Postgres.DATABASE_URL}}
   ```
3. Deploy — the schema is created automatically on first audit request.

## Schema

The schema is defined in `frontend/lib/schema.sql` and applied idempotently on startup:

```sql
CREATE TABLE IF NOT EXISTS audit_runs (
  id         BIGSERIAL      PRIMARY KEY,
  host       TEXT           NOT NULL,
  root_url   TEXT           NOT NULL,
  site_score INTEGER        NOT NULL,
  report     JSONB          NOT NULL,
  crawled_at TIMESTAMPTZ    NOT NULL DEFAULT NOW()
);
```

History is scoped per `host` (e.g. `example.com` and `www.example.com` are tracked separately).
