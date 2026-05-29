# Changelog

All notable changes to sandslash are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html) — pre-1.0 breaking changes bump the minor version.

## [Unreleased]

## [0.1.0] - 2026-05-29

### Features

- Phase 0: project scaffold — Cargo project, error types, model, config, CLI, tracing
- Phase 1: single-page MVP — HTTP fetcher, DOM parser, metadata/headings/HTTPS auditors, scoring, JSON reporter
- Phase 2 (partial): OpenGraph/Twitter auditor, images auditor, manual redirect handling, Next.js audit UI

### Chores

- Rename project from `seo-rs` to `sandslash`
- Split CI into parallel jobs with Swatinem/rust-cache@v2
- Spec-driven agent workflow with project-planner, rust-worker, auditor-worker

[unreleased]: https://github.com/leotonezi/sandslash/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/leotonezi/sandslash/releases/tag/v0.1.0
