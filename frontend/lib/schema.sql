CREATE TABLE IF NOT EXISTS audit_runs (
  id         BIGSERIAL      PRIMARY KEY,
  host       TEXT           NOT NULL,
  root_url   TEXT           NOT NULL,
  site_score INTEGER        NOT NULL,
  report     JSONB          NOT NULL,
  crawled_at TIMESTAMPTZ    NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_audit_runs_host_time
  ON audit_runs (host, crawled_at DESC);
