# sandslash — CLI Runbook

Common commands for development and manual testing.

---

## Build

```bash
# Debug build (fast compile, slow binary)
cargo build

# Release build (optimized)
cargo build --release
```

---

## Run (debug binary)

```bash
# Single-page audit, JSON to stdout
cargo run -- https://example.com

# Single-page audit, write JSON to file
cargo run -- https://example.com -o /tmp/report.json

# Single-page, quiet (score only)
cargo run -- https://example.com --quiet

# Single-page, no color
cargo run -- https://example.com --no-color

# Verbose tracing (logs to stderr, bar hidden)
cargo run -- https://example.com --verbose
```

---

## Run (multi-page crawl)

Requires Redis running locally.

```bash
# Start Redis (Docker)
docker run --rm -p 6379:6379 redis:7-alpine

# Crawl 2 levels deep, 8 workers
cargo run -- https://example.com -d 2 --redis-url redis://localhost:6379

# Crawl with page cap
cargo run -- https://example.com -d 3 --max-pages 20 --redis-url redis://localhost:6379

# Crawl, write report to file
cargo run -- https://example.com -d 2 --redis-url redis://localhost:6379 -o /tmp/report.json

# Crawl, custom rate and concurrency
cargo run -- https://example.com -d 2 -c 4 --rate 1 --redis-url redis://localhost:6379

# Crawl, ignore robots.txt
cargo run -- https://example.com -d 2 --ignore-robots --redis-url redis://localhost:6379
```

---

## Run (release binary)

```bash
# Build release first
cargo build --release

# Single-page
./target/release/sandslash https://example.com

# Multi-page
./target/release/sandslash https://example.com -d 2 --redis-url redis://localhost:6379
```

---

## RUST_LOG control

```bash
# Default (info)
RUST_LOG=sandslash=info cargo run -- https://example.com

# Debug (verbose)
RUST_LOG=sandslash=debug cargo run -- https://example.com

# Silence all logs
RUST_LOG=off cargo run -- https://example.com
```

---

## Tests

```bash
# All tests (Redis tests skipped if REDIS_URL absent)
cargo test

# Run ignored (Redis) tests — requires local Redis
REDIS_URL=redis://localhost:6379 cargo test -- --include-ignored

# Specific test file
cargo test --test crawler_engine

# Specific test by name
cargo test title_missing

# Lib unit tests only
cargo test --lib

# Show stdout from tests
cargo test -- --nocapture
```

---

## Lint & Format

```bash
cargo fmt                          # format in place
cargo fmt -- --check               # check only (CI mode)
cargo clippy                       # lint
cargo clippy --all-targets -- -D warnings   # CI mode (fail on warnings)
```

---

## Release (maintainers only)

See `docs/RELEASING.md`. Never edit `Cargo.toml` version manually.

```bash
cargo release patch --execute   # bug fix
cargo release minor --execute   # new feature
```
