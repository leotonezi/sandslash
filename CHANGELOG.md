# Changelog

All notable changes to sandslash are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html) — pre-1.0 breaking changes bump the minor version.

## [0.3.0] - 2026-05-30

### Bug Fixes

- Collapse sitemap match arm to satisfy clippy::collapsible-match ([39ed19a](https://github.com/leotonezi/sandslash/commit/39ed19a7ccfc271d7709998c5fa6d9eddff893b9))
- Collapse nested if blocks in sitemap.rs to satisfy clippy::collapsible-if ([3e9730e](https://github.com/leotonezi/sandslash/commit/3e9730e4bf8bcdb7218d577b63fbd27a170379b6))
- Remove redundant pre-release-replacements conflicting with git-cliff hook ([cc85656](https://github.com/leotonezi/sandslash/commit/cc8565634d00aaa6e6f7fa4d467912ca637af905))

### Chores

- Release sandslash version 0.2.0 ([1ac3a1b](https://github.com/leotonezi/sandslash/commit/1ac3a1be551942247337c1c672bf1d760758d688))

### Features

- Extend Dom::resource_urls() for all mixed-content tags, add integration tests (closes #19) ([edcd66b](https://github.com/leotonezi/sandslash/commit/edcd66bf9c7577a5ebac4d35c01d63544397ed26))
- Implement redirects auditor and redirect-loop pipeline handling (closes #20) ([c89376d](https://github.com/leotonezi/sandslash/commit/c89376dee77ebc311e4555cde8a98195891d68bb))
- Implement RobotsAuditor as first SiteAuditor (closes #23) ([536ba18](https://github.com/leotonezi/sandslash/commit/536ba18032993755e4840284a3611bac67540af3))
- Implement SitemapAuditor as second SiteAuditor (closes #25) ([4e0ec76](https://github.com/leotonezi/sandslash/commit/4e0ec76847e2cde45b898efdfde6793965ab2832))
- Wire all auditors into pipeline and expose lib crate (closes #27) ([fbf3fa6](https://github.com/leotonezi/sandslash/commit/fbf3fa6ecda535ec4220028de74a49053db652cb))
- Implement terminal reporter and wire emit_report routing (closes #29) ([bf53ef8](https://github.com/leotonezi/sandslash/commit/bf53ef8050b1b3d554e2984e937c6fc97c7f63af))

### Style

- Fix rustfmt formatting in dom tests and audit_https imports ([d2a51d2](https://github.com/leotonezi/sandslash/commit/d2a51d2daab5dab5939c0aee6b2bc466a399656a))

## [0.2.0] - 2026-05-29

### Bug Fixes

- Clippy cloned_ref_to_slice_refs in pipeline.rs; add pre-push validation step to workflow ([594ed08](https://github.com/leotonezi/sandslash/commit/594ed0887b9819bc3e69d15e1b692b506a4cec31))
- Collapse nested if/if-let in fetcher to satisfy clippy (chore branch sync) ([b1f37cc](https://github.com/leotonezi/sandslash/commit/b1f37ccb9614b545907adf8d397b7b25e8972872))

### Chores

- Move planning docs to docs/, add root cleanliness rules ([31caba1](https://github.com/leotonezi/sandslash/commit/31caba16d2b68fbbf0fb66aedcbe5f8f84f3439b))
- Wire spec-driven development into agent workflow ([b793c72](https://github.com/leotonezi/sandslash/commit/b793c72919608794d465868d78e69a72ca7bf64c))
- Mark Phase 0 complete, add progress tracking convention ([fb89871](https://github.com/leotonezi/sandslash/commit/fb898714673f8b405951f9004687defb40e2fa15))
- Remove Claude Code footer from PR template ([d0830ee](https://github.com/leotonezi/sandslash/commit/d0830ee1d1c20cfcaacebfd3c4decca7d019331e))
- Tie spec cards to GitHub issues in workflow ([58e5060](https://github.com/leotonezi/sandslash/commit/58e5060714e05b949e2eb2eb2e7fa8d0f30c6b4e))
- Rename project from seo-rs to sandslash ([5af6dd2](https://github.com/leotonezi/sandslash/commit/5af6dd2555ef0ded85949f542d7797f79cd82234))
- Update Cargo.lock after rename to sandslash ([380f3f7](https://github.com/leotonezi/sandslash/commit/380f3f793532131fd3d246726c6e6e080172a301))
- Add semantic versioning tooling (closes #15) ([f26db92](https://github.com/leotonezi/sandslash/commit/f26db9282acd3e706313c1987f71dea27bb906e4))
- Add auto-release workflow triggered on development→master merge ([e4016c2](https://github.com/leotonezi/sandslash/commit/e4016c2246305e401cfd6b9a8f2933a47e156764))
- Add CI workflow (fmt, clippy, test, release build) ([6a166ce](https://github.com/leotonezi/sandslash/commit/6a166ce5b639638b1677e20a79790805a4ef8c5d))
- Add .github/, frontend/, docs/ to root allowlist in CLAUDE.md; sync agent workflow steps ([d3b7aa0](https://github.com/leotonezi/sandslash/commit/d3b7aa0774bf0da4f6767ee57caa771b7ae6b12e))
- Split CI into parallel jobs, use Swatinem/rust-cache@v2 ([9695d3d](https://github.com/leotonezi/sandslash/commit/9695d3d283a7e6af56f826b1d66dfcac4b0e1aab))
- Release sandslash version 0.2.0 ([2c56692](https://github.com/leotonezi/sandslash/commit/2c5669237342449828b0106d34b3878ecc7ac5e2))

### Documentation

- Add project README with usage, checks, scoring, and output format ([14f398e](https://github.com/leotonezi/sandslash/commit/14f398ea0959fa106839f0ffd2e8b7e8e88ed853))

### Features

- Phase 0 scaffolding — Cargo project, error types, model, config, CLI, logging ([2287aac](https://github.com/leotonezi/sandslash/commit/2287aac922393f6d9b2478ba601539f81819e17c))
- Phase 1 — MVP single-page fetch → parse → audit → JSON ([d60b9dd](https://github.com/leotonezi/sandslash/commit/d60b9ddfd68914e9107bd01aa322f2852b3f3a6b))
- Implement OpengraphAuditor and fix clippy/lint issues across Phase 1 modules ([af10928](https://github.com/leotonezi/sandslash/commit/af1092869f8fbdd1179fc9c4c718b7a7a5981762))
- Implement ImagesAuditor and fix pre-existing clippy/fmt issues ([811faa9](https://github.com/leotonezi/sandslash/commit/811faa948c90c103551c03c05df156d11528f631))
- Add Next.js 14 audit UI (phase 2, step 2.3-ui) ([fd457ad](https://github.com/leotonezi/sandslash/commit/fd457ad8be0a9c8b5322a04e104f2317207c8c24))
- Implement manual redirect handling with loop detection (phase 2, step 2.3) ([3205025](https://github.com/leotonezi/sandslash/commit/320502521b7a18a1209db7d1bc3d265d06a9c477))

[0.3.0]: https://github.com/leotonezi/sandslash/compare/v0.2.0...v0.3.0

