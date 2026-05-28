# seo-rs

Rust CLI SEO auditing tool. Pipeline: `fetch -> parse -> audit -> score -> report`.

## Implementation reference
- Spec: `docs/PLAN.md`
- Step-by-step plan: `docs/IMPLEMENTATION.md` (phases 0–4, ~35 sessions)
- Agent workflow: `docs/AGENT_WORKFLOW.md`

## Root folder rules
Only these belong at root: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `src/`, `tests/`, `.gitignore`, `CLAUDE.md`, `.claude/`, `README.md`, `frontend/`
Everything else (specs, plans, workflow docs, ADRs) goes in `docs/`. Before adding any file to root, ask: does Cargo, Rust, or Claude Code require it here? If not, it goes in `docs/`.

## Stack
- `tokio` 1.x — async runtime
- `reqwest` 0.12 (rustls-tls) — HTTP client
- `scraper` 0.20 — HTML parsing (`!Send` — never hold across `.await`)
- `url` 2.x — URL handling
- `redis` 0.27 (tokio-comp) — crawl frontier
- `governor` + `dashmap` — per-host rate limiting
- `clap` 4.x derive — CLI
- `thiserror` in library code; `anyhow` at binary boundary only
- `encoding_rs` — non-UTF-8 response decoding
- `owo-colors` + `comfy-table` + `indicatif` — terminal output
- `wiremock` (dev) — mock HTTP in tests

## Hard rules
- `unsafe_code = "forbid"` — no exceptions
- No `.unwrap()` in library code — use `?` or `.expect("invariant: ...")`
- No blocking I/O inside async tasks — use `spawn_blocking`
- `scraper::Html` is `!Send` — never hold across `.await`
- DashMap entry guards must be dropped before any `.await`
- Respect `robots.txt` and `Crawl-delay` — never bypass
- Honest `User-Agent` — always the configured value, never spoofed

## Error handling pattern
```rust
// Library modules (src/**/*.rs except main.rs)
use crate::error::{Result, SeoError};

// main.rs / binary boundary
use anyhow::{Context, Result};
```

## Naming conventions
- Auditors: zero-sized structs — `pub struct TitleAuditor;`
- `check_id` strings: lowercase with dots — `"title.missing"`, `"headings.no-h1"`
- Modules: snake_case files, PascalCase types

## Test conventions
- Fixture HTML files: `tests/fixtures/*.html`
- HTTP mocks: `wiremock::MockServer` — no live network in tests
- Redis-dependent tests: `#[ignore]` with doc comment
- Run tests: `rtk cargo test`

## Progress tracking
Mark steps done in `docs/IMPLEMENTATION.md` immediately when complete:
- Step done: `### ✓ X.Y <title>`
- Phase done: `## ✓ Phase N — <title> (DONE)`
Do this before committing. Never batch-mark after the fact.

## Agent workflow
1. **project-planner** — plan the step, resolve design questions
2. **rust-worker** or **auditor-worker** — implement
3. **feature-evaluator** — verify correctness and Rust quality
4. **build-validator** — pre-PR check (fmt, clippy -D warnings, tests, release build)
5. **pr-creator** — open PR
