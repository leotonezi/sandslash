# Changelog

All notable changes to sandslash are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html) — pre-1.0 breaking changes bump the minor version.

## [0.7.2] - 2026-06-09

### Documentation

- Speed up demo GIF 1.5x ([70c22ff](https://github.com/leotonezi/sandslash/commit/70c22ffa59e68bd5c0cb15352eb6c761c165da20))

## [0.7.1] - 2026-06-09

### Chores

- Release sandslash version 0.7.1 ([b9dc01c](https://github.com/leotonezi/sandslash/commit/b9dc01ca9d593ecbb4179954be9ab6d795d774a7))

### Documentation

- Add demo GIF and reference in README ([55131d0](https://github.com/leotonezi/sandslash/commit/55131d0d4da2df9bce4bf498c28eccd872e7ffd4))

## [0.7.0] - 2026-06-08

### Chores

- Release sandslash version 0.7.0 ([b24f44a](https://github.com/leotonezi/sandslash/commit/b24f44a53b2d9a27ff76c9f4fb793a6dec1d0b07))

### Features

- Enhance post mvp features ([64b1437](https://github.com/leotonezi/sandslash/commit/64b14370a7ce7257caae9462db3c1d0eabd15575))
- **benches**: Add criterion benchmark suite for parse, audit, and fetch ([600afd5](https://github.com/leotonezi/sandslash/commit/600afd5fc778b605b569f4ce1a53e8718e5278d6))

## [0.6.0] - 2026-06-05

### Bug Fixes

- **engine**: Allow too_many_arguments on spawn_crawl_workers ([d8a8a25](https://github.com/leotonezi/sandslash/commit/d8a8a25b48b02fd04c539e1c45bfc00b0e1d1889))
- **ci**: Rebase onto development, remove duplicate head, fix test AuditContext types ([f3765b9](https://github.com/leotonezi/sandslash/commit/f3765b9fa7779da2c0c379f6d5d9715835b442c3))
- **clippy**: Collapse nested if into match guard in extract_loc_urls ([a6f2896](https://github.com/leotonezi/sandslash/commit/a6f2896349740f7b3b0b3f180d348fc8d1ce7a5e))
- **crawler**: Unique job_id and correct max_pages counter rollback ([328a9be](https://github.com/leotonezi/sandslash/commit/328a9be7b97641ab857dcbac9ea666a9499ed2f7))

### Chores

- Release sandslash version 0.6.0 ([a4dfd3e](https://github.com/leotonezi/sandslash/commit/a4dfd3ec08c3733898f3fe8c963bffa56a44863b))

### Documentation

- Add CLI runbook with common dev commands ([0799673](https://github.com/leotonezi/sandslash/commit/07996735972484219126355bcd57b0bd9e2a7462))
- **rust**: Deep-dive on trait objects — fat pointers, vtables, object safety ([2e30dec](https://github.com/leotonezi/sandslash/commit/2e30decedba578a084c88df60a31605935b2fff8))
- **rust**: Deep-dive on Send+Sync and !Send scraper::Html ([23d4290](https://github.com/leotonezi/sandslash/commit/23d42907ec5234a5387cd2d1ae6e1e480930a82d))
- **rust**: Deep-dive on ownership & borrowing ([9f11d51](https://github.com/leotonezi/sandslash/commit/9f11d512462e25e4ea9636fd48450e5c7980965f))
- **rust**: Deep-dive on Arc vs Rc — when each applies ([1978787](https://github.com/leotonezi/sandslash/commit/1978787aa3efcee9141327fa9c949d594d32beec))
- **rust**: Deep-dive on LazyLock/once_cell for expensive statics ([2ac7c64](https://github.com/leotonezi/sandslash/commit/2ac7c64e0878c718fadea03520df6025de81662e))
- **readme**: Add Phase 4 CLI flags and mark phases 2–4 complete; add Redis service to CI and run ignored tests (closes #80, #81) ([bfae51a](https://github.com/leotonezi/sandslash/commit/bfae51acc0772b83a774b4820d75f633868d12e4))
- **rust**: Add semaphore deep-dive (topic 11) ([e9f2574](https://github.com/leotonezi/sandslash/commit/e9f2574a897b7eb2b264b939cc3ab130eaf84a97))
- **rust**: Deep-dive on holding guards across .await ([422b99c](https://github.com/leotonezi/sandslash/commit/422b99cbb35aadccfaa916db944e3695d6514f9d))
- **rust**: Add semaphore deep-dive (topic 11) ([ab79383](https://github.com/leotonezi/sandslash/commit/ab79383af0f61847063dc54212afb27f037d4f9e))
- **rust**: Add Cow<str> deep-dive (topic 13) ([9af937b](https://github.com/leotonezi/sandslash/commit/9af937b6eb8f47814a225ec613676a7b768f76a9))
- **rust**: Deep-dive on tokio::spawn and 'static + Send bounds ([2fc4a21](https://github.com/leotonezi/sandslash/commit/2fc4a211df9ae44dd18be112805e989e6f1c265c))
- **rust**: Add Arc vs Rc deep-dive (topic 8) ([bab1a95](https://github.com/leotonezi/sandslash/commit/bab1a959a7db4864deb77d746236868b3022a61b))
- **rust**: Add async-trait deep-dive (topic 4) ([38a97a8](https://github.com/leotonezi/sandslash/commit/38a97a89c7759b10da52c519f1db8b8045e24e03))
- **rust**: Add deep-dive on error handling with thiserror and anyhow ([cbc2cc4](https://github.com/leotonezi/sandslash/commit/cbc2cc4eb931cb58023b87e1bdd5c4d064f6c894))
- **rust**: Add async-trait deep-dive (topic 4) ([024e31e](https://github.com/leotonezi/sandslash/commit/024e31e69920af9958e6c788c444e694ccfb7c49))
- **rust**: Deep-dive on mpsc channels and drop-sender pattern ([b040477](https://github.com/leotonezi/sandslash/commit/b040477287cbdc2991714b537a0079542642b11e))
- **rust**: Deep-dive on mpsc channels and drop-sender pattern (topic 7) ([d202469](https://github.com/leotonezi/sandslash/commit/d20246979b23e28c2917afdfaafe9589c0ae288f))
- Mark topic 5 [x] in plan ([9aeb171](https://github.com/leotonezi/sandslash/commit/9aeb171bcbeedcbfa35ed0d453da30062620af01))
- Mark topic 12 [x] in plan ([be5a67a](https://github.com/leotonezi/sandslash/commit/be5a67a9cfb628e7a447c7ce44566f3cfc931bd0))
- Mark topic 6 [x] in plan ([7a599d3](https://github.com/leotonezi/sandslash/commit/7a599d32f36b745a5d54be99cf8c69d608c999b4))
- **rust**: Add deep-dive on error handling with thiserror and anyhow ([feafdbc](https://github.com/leotonezi/sandslash/commit/feafdbcac79ee697f015433d9b765f3840397e56))
- **rust**: Deep-dive on spawn_blocking for !Send and CPU-bound work ([6df8659](https://github.com/leotonezi/sandslash/commit/6df8659fb9738c0e121de811c3fd3d8aaa4ed69a))
- Mark topic 10 [x] in plan ([81789ea](https://github.com/leotonezi/sandslash/commit/81789ead243f6020eb7ee2be1da695f8141738df))
- Mark topic 8 [x] in plan ([2c6817b](https://github.com/leotonezi/sandslash/commit/2c6817b4537fd386900fe4da1d4833e047827acd))

### Features

- **fetcher**: Charset-aware body decoding with encoding_rs (closes #54) ([502a9ff](https://github.com/leotonezi/sandslash/commit/502a9ffcdc4c1e2198197760635d21c7d2504435))
- **report**: Progress bar with ProgressReporter — phase 4 step 4.3 (closes #56) ([c8afe4b](https://github.com/leotonezi/sandslash/commit/c8afe4bc90bd3de0795cddbe097b93c814657924))
- **safety-valves**: --global-timeout and --max-pages enforcement (closes #58) ([6ff7b81](https://github.com/leotonezi/sandslash/commit/6ff7b8131e141807e88a83db4b56c886891319a6))
- **sitemap**: Concurrent HEAD-probe validation pass — step 4.5 (closes #60) ([8be1a60](https://github.com/leotonezi/sandslash/commit/8be1a6087399fa82bebf554c5692af36fb0dbf68))
- **audit**: JS-rendered page detection — step 4.6 (closes #62) ([5ac5fcf](https://github.com/leotonezi/sandslash/commit/5ac5fcf1ec6d6b284f6b57565e812e7f9a5d62ac))
- **tests**: Wiremock integration test suite — step 4.7 (closes #68) ([205079f](https://github.com/leotonezi/sandslash/commit/205079f18cfd01a60b47c0953506606a41aa225d))

## [0.5.0] - 2026-06-03

### Bug Fixes

- **fetcher**: Collapse nested if to satisfy clippy::collapsible-if ([638a9f6](https://github.com/leotonezi/sandslash/commit/638a9f60c3d7a3cd9303e4463d93a6b4025c9788))
- **tests**: Remove duplicate make_fetcher, update run_crawl call sites ([eb5600f](https://github.com/leotonezi/sandslash/commit/eb5600fbf771fa7dded97d3b2502fecee9923e71))
- **ci**: Collapse nested if for clippy, reformat run_crawl calls for fmt ([5f5af69](https://github.com/leotonezi/sandslash/commit/5f5af69f7b55b660bf5b63a8a31103fd67bb179c))

### Chores

- Add AGPL-3.0 license, deploy guide, and .DS_Store gitignore ([2b6749e](https://github.com/leotonezi/sandslash/commit/2b6749ea38eb40cf54e07e817924c3b18f1aa757))
- Release sandslash version 0.4.0 ([60a9b67](https://github.com/leotonezi/sandslash/commit/60a9b674cda154152c65dc0d381312d43955d5e5))
- Release sandslash version 0.5.0 ([61c9e5f](https://github.com/leotonezi/sandslash/commit/61c9e5f8d9652f34acc6bcd037b914f7e232601f))

### Features

- Robots-gated crawl — RobotsCache, set_min_interval, parse_rules, engine integration (closes #49) ([9698ea0](https://github.com/leotonezi/sandslash/commit/9698ea09312295d7633c2f86e362aa902fa33c46))
- **frontend**: Rebrand to Sandslash with dark theme and UI polish ([c088447](https://github.com/leotonezi/sandslash/commit/c088447eba5c37b2c8a97a3ee4652e539c5d63eb))
- Integrate HostRateLimiter into Fetcher with 429/503 retry backoff (closes #43) ([4e57678](https://github.com/leotonezi/sandslash/commit/4e57678eda9439d5a9d11f36782bffb806f5add7))
- Implement worker-pool crawl engine (closes #45) ([2d194e5](https://github.com/leotonezi/sandslash/commit/2d194e52aa9bae209bac5fc960866c36bc356c1d))
- Wire crawler pipeline — branch pipeline on depth, add frontier.clear() and integration test (closes #47) ([3271897](https://github.com/leotonezi/sandslash/commit/3271897cf5562d80f11418a372d54ee63e4868a2))
- Broken-link auditor at scale (closes #52) ([304f488](https://github.com/leotonezi/sandslash/commit/304f48879b1d2e0d0da4ff8b2f9d0196fe81a091))

### Merge

- Resolve conflicts with development — keep 3.8 robots gating ([95f16fe](https://github.com/leotonezi/sandslash/commit/95f16fe9644c54a45a7a0f691206d5322c1d90c9))

## [0.4.0] - 2026-06-01

### Chores

- Release sandslash version 0.4.0 ([49a91d3](https://github.com/leotonezi/sandslash/commit/49a91d32097323076904008270b9c315a68c6545))

### Features

- Implement per-host rate limiter with governor + dashmap (closes #40) ([48366e4](https://github.com/leotonezi/sandslash/commit/48366e4c3bcf0b45c1e838ad7fe8cf88c8309e35))

## [0.3.0] - 2026-05-30

### Bug Fixes

- Collapse sitemap match arm to satisfy clippy::collapsible-match ([39ed19a](https://github.com/leotonezi/sandslash/commit/39ed19a7ccfc271d7709998c5fa6d9eddff893b9))
- Collapse nested if blocks in sitemap.rs to satisfy clippy::collapsible-if ([3e9730e](https://github.com/leotonezi/sandslash/commit/3e9730e4bf8bcdb7218d577b63fbd27a170379b6))
- Update frontend binary path from seo-rs to sandslash ([f3d7ab5](https://github.com/leotonezi/sandslash/commit/f3d7ab58b5a41e9a043c0eb1188a0fb9f7bb5b53))
- Collapse nested if in discover_links to satisfy clippy::collapsible-if ([29ea3c9](https://github.com/leotonezi/sandslash/commit/29ea3c9234dbdb86ffc277e3b0950a2bb6afbae2))
- Remove redundant pre-release-replacements conflicting with git-cliff hook ([cc85656](https://github.com/leotonezi/sandslash/commit/cc8565634d00aaa6e6f7fa4d467912ca637af905))

### Chores

- Release sandslash version 0.2.0 ([1ac3a1b](https://github.com/leotonezi/sandslash/commit/1ac3a1be551942247337c1c672bf1d760758d688))
- Trigger merge check refresh ([97f64ee](https://github.com/leotonezi/sandslash/commit/97f64eeb8d434ae4f616408b59188e325d73af3d))
- Release sandslash version 0.3.0 ([bb87d34](https://github.com/leotonezi/sandslash/commit/bb87d34838f909522f0a1b143ff56dfcb7e3d622))

### Features

- Extend Dom::resource_urls() for all mixed-content tags, add integration tests (closes #19) ([edcd66b](https://github.com/leotonezi/sandslash/commit/edcd66bf9c7577a5ebac4d35c01d63544397ed26))
- Implement redirects auditor and redirect-loop pipeline handling (closes #20) ([c89376d](https://github.com/leotonezi/sandslash/commit/c89376dee77ebc311e4555cde8a98195891d68bb))
- Implement RobotsAuditor as first SiteAuditor (closes #23) ([536ba18](https://github.com/leotonezi/sandslash/commit/536ba18032993755e4840284a3611bac67540af3))
- Implement SitemapAuditor as second SiteAuditor (closes #25) ([4e0ec76](https://github.com/leotonezi/sandslash/commit/4e0ec76847e2cde45b898efdfde6793965ab2832))
- Wire all auditors into pipeline and expose lib crate (closes #27) ([fbf3fa6](https://github.com/leotonezi/sandslash/commit/fbf3fa6ecda535ec4220028de74a49053db652cb))
- Implement terminal reporter and wire emit_report routing (closes #29) ([bf53ef8](https://github.com/leotonezi/sandslash/commit/bf53ef8050b1b3d554e2984e937c6fc97c7f63af))
- Implement URL normalization in parser/links.rs (closes #33) ([d7c4403](https://github.com/leotonezi/sandslash/commit/d7c44036391c27da19e8e5c8ed7f750e413df2d0))
- Implement link discovery and URL normalization in parser/links.rs (closes #35) ([5725869](https://github.com/leotonezi/sandslash/commit/5725869654ddd2c5d54151d5e6ca2b158a1a451a))
- Implement Redis-backed crawl frontier (closes #38) ([17ed68c](https://github.com/leotonezi/sandslash/commit/17ed68cfbb773004f2d6b86951cbc98071e55c1b))

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

[0.7.2]: https://github.com/leotonezi/sandslash/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/leotonezi/sandslash/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/leotonezi/sandslash/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/leotonezi/sandslash/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/leotonezi/sandslash/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/leotonezi/sandslash/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/leotonezi/sandslash/compare/v0.2.0...v0.3.0

