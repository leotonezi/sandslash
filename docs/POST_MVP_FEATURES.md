# Post-MVP Features

Features beyond the CLI plan (`IMPLEMENTATION.md`) needed to make seo-rs a robust free platform.

---

## Platform Infrastructure

### Audit History + Trends
- Store reports in Postgres (one row per audit run per URL)
- Show score over time per site in the UI
- Regression detection: alert when score drops > N points

### Scheduled Re-audits
- Cron jobs that re-crawl sites on a schedule (daily/weekly)
- Email/webhook alerts on regressions

### Multi-site Management
- UI lists managed sites, not one-shot URL entry
- Per-site audit history, last crawl status, average score

### Real-time Crawl Progress
- SSE or WebSockets from crawl engine → browser
- Currently the UI blocks until the binary exits (temp file approach)
- Show pages crawled, queue depth, current score as crawl runs

---

## High-value Free Differentiators

### Core Web Vitals
- Integrate Lighthouse via `@lhci/cli` or PageSpeed Insights API (free quota)
- CWV (LCP, INP, CLS) are a confirmed ranking factor
- Add `Category::Performance` to the scoring model

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

---

## Not Worth Building (requires data moat)

- **Backlinks** — needs a web-scale crawl index (Ahrefs/Moz built theirs over years)
- **Keyword rankings** — needs SERP scraping at scale
- **Competitor analysis** — depends on both of the above

---

## Priority Order

```
1. Finish IMPLEMENTATION.md phases 2–4 (crawler is the unlock)
2. Postgres persistence + audit history UI
3. SSE real-time progress in UI
4. Scheduled re-audits
5. Core Web Vitals (Lighthouse integration)
6. Structured data validation
7. hreflang checks
8. Page speed hints (static)
9. Social preview renderer
```
