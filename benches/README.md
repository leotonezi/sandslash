# Benchmarks

Criterion-based benchmarks for seo-rs. No live network, no Redis — all I/O is served by
in-memory fixtures or a wiremock `MockServer` bound to localhost.

## Suites

### `parse_dom`

Measures the time to parse a static HTML fixture (`tests/fixtures/basic.html`) into a `Dom`
using `scraper`. Establishes the baseline cost of the parsing stage in the pipeline.

### `page_audit_throughput`

Measures the full synchronous audit pipeline on a single page:

1. `Dom::parse` — HTML parsing
2. All `PageAuditor` implementations run against the parsed DOM
3. `score_page` — finding aggregation and score rollup

Reports throughput in pages/sec via `Throughput::Elements(1)`.

### `fetch_throughput`

Measures HTTP fetch throughput at concurrency levels `{1, 4, 16, 32}`.

Each iteration spawns `N` concurrent `Fetcher::fetch` calls to a wiremock server
(always-200, minimal HTML body). Uses `FuturesUnordered` to drive all futures in parallel
within a single Tokio runtime.

Reports throughput in fetches/sec via `Throughput::Elements(1)`.

## Running

```bash
# Full benchmark run (all suites)
cargo bench

# Quick warmup-only run — useful for CI smoke tests
cargo bench --bench parse_dom            -- --quick
cargo bench --bench page_audit_throughput -- --quick
cargo bench --bench fetch_throughput      -- --quick

# Single named benchmark
cargo bench --bench parse_dom -- dom_parse_basic

# Save a named baseline for comparison
cargo bench -- --save-baseline main

# Compare against a saved baseline
cargo bench -- --baseline main
```

## HTML reports

After a full `cargo bench` run, Criterion generates HTML reports at:

```
target/criterion/report/index.html
```

Open this file in a browser to explore per-benchmark flame plots and statistical summaries.
