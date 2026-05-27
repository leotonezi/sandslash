# seo-rs

Rust CLI for SEO auditing. Fetches a page, parses the DOM, runs a suite of checks, and emits a scored JSON report.

```
$ seo-rs https://example.com -o report.json
```

---

## Install

**From source** (requires Rust stable):

```bash
git clone https://github.com/leotonezi/sandslash
cd sandslash
cargo install --path .
```

---

## Usage

```
seo-rs <URL> [OPTIONS]
```

### Options

| Flag | Default | Description |
|---|---|---|
| `<URL>` | — | Target URL to audit |
| `-d, --depth <N>` | `1` | Crawl depth (0 = single page) |
| `-c, --concurrency <N>` | `8` | Concurrent workers |
| `--rate <N>` | `2` | Requests per second per host |
| `--redis-url <URL>` | — | Redis URL for crawl frontier (env: `REDIS_URL`) |
| `--user-agent <UA>` | seo-rs/version | Custom User-Agent |
| `--timeout <secs>` | `30` | Per-request timeout |
| `--max-pages <N>` | — | Cap pages crawled |
| `--ignore-robots` | false | Skip robots.txt |
| `-q, --quiet` | false | Print score only |
| `--no-color` | false | Disable colored output |
| `-o, --output <FILE>` | stdout | Write JSON report to file |

### Examples

```bash
# Audit one page, print JSON to stdout
seo-rs https://example.com

# Audit one page, write report to file
seo-rs https://example.com -o report.json

# Crawl 3 levels deep, 4 workers, cap at 50 pages
seo-rs https://example.com -d 3 -c 4 --max-pages 50 -o report.json

# Verbose tracing output
RUST_LOG=seo_rs=debug seo-rs https://example.com
```

---

## Checks

### Metadata (`weight: 20%`)

| Check ID | Severity | Condition |
|---|---|---|
| `title.missing` | Critical | No `<title>` tag |
| `title.empty` | Critical | Title is blank |
| `title.short` | Warning | Title < 30 characters |
| `title.long` | Warning | Title > 60 characters |
| `description.missing` | Critical | No `<meta name="description">` |
| `description.short` | Warning | Description < 50 characters |
| `description.long` | Warning | Description > 160 characters |
| `canonical.missing` | Warning | No `<link rel="canonical">` |
| `canonical.off-host` | Warning | Canonical points to a different host |
| `canonical.mismatch` | Info | Canonical doesn't match page URL |

### Structure (`weight: 15%`)

| Check ID | Severity | Condition |
|---|---|---|
| `headings.no-h1` | Critical | Page has no `<h1>` |
| `headings.multiple-h1` | Warning | Page has more than one `<h1>` |
| `headings.skipped-level` | Warning | Heading levels skip (e.g. h1 → h3) |
| `headings.empty` | Warning | A heading tag has no text content |

### Security (`weight: 15%`)

| Check ID | Severity | Condition |
|---|---|---|
| `https.insecure` | Critical | Page URL uses `http://` |
| `https.mixed-content` | Warning | `https://` page loads `http://` resources |

### Media (`weight: 10%`)

| Check ID | Severity | Condition |
|---|---|---|
| `images.missing-alt` | Warning | `<img>` has no `alt` attribute |
| `images.empty-alt` | Info | `<img alt="">` on a content image |

### Social Tags (`weight: 5%`)

| Check ID | Severity | Condition |
|---|---|---|
| `og.title.missing` | Info | No `<meta property="og:title">` |
| `og.description.missing` | Info | No `<meta property="og:description">` |
| `og.image.missing` | Info | No `<meta property="og:image">` |
| `og.url.missing` | Info | No `<meta property="og:url">` |
| `twitter.card.missing` | Info | No `<meta name="twitter:card">` |

---

## Scoring

Each page gets a score from 0–100. Each category starts at 100 and findings deduct penalty points (clamped to 0). The page score is the weighted average across categories:

```
page_score = Σ (category_score × category_weight / 100)
site_score = mean(page_scores)
```

| Category | Weight |
|---|---|
| Metadata | 20% |
| Crawlability | 20% |
| Structure | 15% |
| Links | 15% |
| Security | 15% |
| Media | 10% |
| Social Tags | 5% |

---

## Output

JSON report structure:

```json
{
  "root": "https://example.com/",
  "site_score": 87,
  "crawled_at": "2026-05-27T14:00:00Z",
  "pages": [
    {
      "url": "https://example.com/",
      "score": 87,
      "category_scores": {
        "Metadata": 70,
        "Structure": 100,
        "Security": 100,
        "Media": 90,
        "SocialTags": 50,
        "Links": 100,
        "Crawlability": 100
      },
      "findings": [
        {
          "check_id": "title.short",
          "category": "Metadata",
          "severity": "Warning",
          "penalty": 15,
          "message": "Title is 18 chars (min 30)"
        }
      ]
    }
  ]
}
```

---

## Logging

Structured logs via `tracing`. Control verbosity with `RUST_LOG`:

```bash
RUST_LOG=seo_rs=info seo-rs https://example.com   # default
RUST_LOG=seo_rs=debug seo-rs https://example.com  # verbose
```

---

## Roadmap

- [x] Phase 0 — Scaffolding (config, CLI, logging)
- [x] Phase 1 — Single-page fetch, parse, audit, JSON output
- [ ] Phase 2 — Full auditor suite: redirects, robots.txt, sitemap, colored terminal report
- [ ] Phase 3 — Multi-page crawler with Redis frontier, per-host rate limiting
- [ ] Phase 4 — Broken-link checker, encoding robustness, progress bar, CI

---

## Development

```bash
cargo build          # debug build
cargo test           # run all tests
cargo clippy         # lint
cargo build --release
```

Tests use `wiremock` for HTTP mocking — no live network required.
