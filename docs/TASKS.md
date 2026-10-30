# Tasks

GitHub issues are the source of truth. This file is a snapshot built from `docs/POST_MVP_FEATURES.md`, the README, and recent git history. `gh` was unavailable when it was written, so no open issues or PRs were checked. Every task below is marked "needs issue" until it is matched against the issue list.

## Now

- hreflang checks: reciprocal links, x-default, valid language codes (`docs/POST_MVP_FEATURES.md`, priority #9) — needs issue
- Update the README Roadmap, which stops at Phase 4 and omits all shipped post-MVP work (README.md:200) (inferred) — needs issue

## Next

- Static page-speed hints: render-blocking tags, large inline scripts, missing lazy-load and image width/height (`docs/POST_MVP_FEATURES.md`, priority #10) — needs issue

## Later

- Multi-site management UI, now that Postgres history exists (`docs/POST_MVP_FEATURES.md`, priority #12) — needs issue
- Core Web Vitals behind a `--cwv` flag; needs a Node.js or PageSpeed dependency (`docs/POST_MVP_FEATURES.md`, priority #13) — needs issue
- Scheduled re-audits with alerts; deferred because it turns the tool into a SaaS (`docs/POST_MVP_FEATURES.md`, priority #11) — needs issue

## Ideas

- Social preview renderer, marked low priority and low differentiation (`docs/POST_MVP_FEATURES.md`, priority #14)
- Score-regression alerts on audit history (`docs/POST_MVP_FEATURES.md`, "Audit History + Trends") — the history half shipped, the alerting half did not (inferred)
