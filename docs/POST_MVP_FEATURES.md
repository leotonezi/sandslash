# Post-MVP Features

Features beyond the CLI plan (`IMPLEMENTATION.md`) needed to make seo-rs a robust free platform.

---

## Platform Infrastructure

### Live Demo Deployment (Railway / Fly.io)
- Dockerfile: multi-stage build — Rust binary + Next.js frontend in one image
- Frontend API route shells out to binary (current approach, zero code changes)
- Single Railway/Fly.io service, `SEO_RS_BIN=/usr/local/bin/sandslash`
- Redis add-on for multi-page crawl support (`--redis-url` from env)
- Gives a public demo URL for portfolio / recruiters
- **Approach:** Option 1 (shell-out) now; Option 2 (axum HTTP API) when SSE real-time progress is needed

### Audit History + Trends
- Store reports in Postgres (one row per audit run per URL)
- Show score over time per site in the UI
- Regression detection: alert when score drops > N points

### Scheduled Re-audits
- Cron jobs that re-crawl sites on a schedule (daily/weekly)
- Email/webhook alerts on regressions
- **Risk:** turns seo-rs into a SaaS product — defer until CLI phases 2–4 are solid; building auth + cron + email freezes the Rust core for months

### Multi-site Management
- UI lists managed sites, not one-shot URL entry
- Per-site audit history, last crawl status, average score
- **Risk:** same scope creep as scheduled re-audits; only makes sense after Postgres persistence exists

### ✓ Real-time Crawl Progress
- SSE or WebSockets from crawl engine → browser
- Currently the UI blocks until the binary exits (temp file approach)
- Show pages crawled, queue depth, current score as crawl runs

---

## High-value Free Differentiators

### Core Web Vitals
- Integrate Lighthouse via `@lhci/cli` or PageSpeed Insights API (free quota)
- CWV (LCP, INP, CLS) are a confirmed ranking factor
- Add `Category::Performance` to the scoring model
- **Risk:** PageSpeed API requires a key + has rate limits; `@lhci/cli` requires a Node.js runtime — both are external deps that break in CI/prod; CWV measures real-user perf, not HTML quality, so it's a different tool category; if `Category::Performance` is added, CWV absence silently zeros the score — isolate behind a `--cwv` flag if built

### Structured Data / Schema.org Validation
- Parse `<script type="application/ld+json">` blocks
- Validate against schema.org vocabulary (Article, Product, BreadcrumbList, FAQ, etc.)
- Flag malformed JSON-LD, missing required fields, invalid types
- Near-zero competition from free tools

### hreflang Checks
- Parse `<link rel="alternate" hreflang="...">` tags
- Validate: reciprocal links exist, x-default present, language codes valid
- Common problem on multilingual sites, rarely audited for free

### Page Speed Hints (static, no network)
- Count render-blocking `<script>` / `<link rel="stylesheet">` in `<head>`
- Flag large inline scripts (> N KB)
- Flag missing `loading="lazy"` on below-fold images
- Flag missing `width`/`height` on images (causes CLS)
- All derivable from HTML alone — no Lighthouse needed

### Social Preview Renderer
- Render OG/Twitter card preview in the UI
- Show exactly what LinkedIn, X, Facebook will display
- Pulls from existing `og:*` and `twitter:*` data already audited
- **Risk:** low differentiation — every browser extension does this; deprioritize

---

## Missing — Worth Adding

### ✓ Canonical URL Audit
- Check `<link rel="canonical">` is present, self-referential, and consistent across redirect chains
- Common misconfiguration, pure HTML parsing, zero external deps

### ✓ Benchmark Suite (`criterion`)
- Measure fetch throughput vs. concurrency level
- Measure audit pipeline throughput (pages/sec)
- Required before making any performance claims; significant gap for senior showcase

### ✓ `--diff` Mode
- Compare two audit JSON reports and emit score delta per page/category
- Natural extension once Postgres or file-based history exists
- High demo value: shows regressions at a glance

---

## Not Worth Building (requires data moat)

- **Backlinks** — needs a web-scale crawl index (Ahrefs/Moz built theirs over years)
- **Keyword rankings** — needs SERP scraping at scale
- **Competitor analysis** — depends on both of the above

---

## Priority Order

```
1. Finish IMPLEMENTATION.md phases 2–4 (crawler is the unlock)
2. ✓ Benchmark suite (criterion) — needed before performance claims
3. Live demo deployment (Railway/Fly.io) — public URL for portfolio
4. ✓ Canonical URL audit — pure HTML, zero deps, high impact
5. ✓ Postgres persistence + audit history UI
6. ✓ SSE real-time progress in UI
7. ✓ --diff mode (natural once history exists)
8. Structured data / Schema.org validation
9. hreflang checks
10. Page speed hints (static)
11. Scheduled re-audits (defer — SaaS scope, freezes Rust core)
12. Multi-site management (depends on #5)
13. Core Web Vitals — isolate behind --cwv flag, Node.js dep
14. Social preview renderer (low priority — low differentiation)
```
