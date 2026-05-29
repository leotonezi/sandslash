# Releasing sandslash

## Prerequisites

```bash
cargo install cargo-release --locked
cargo install git-cliff --locked
```

## Branch policy

Tags are cut from **master only**. Flow:
1. Merge `development` → `master` via PR
2. From master, run the release command below

## Cutting a release

Dry-run first (no changes made):

```bash
cargo release patch --dry-run
```

Confirm the output shows the correct version bump, tag, and CHANGELOG diff, then execute:

```bash
# Patch: 0.1.0 → 0.1.1 (backwards-compatible fixes)
cargo release patch --execute

# Minor: 0.1.0 → 0.2.0 (new features or pre-1.0 breaking changes)
cargo release minor --execute

# Major: 0.x.y → 1.0.0 (stable public API)
cargo release major --execute
```

`cargo-release` will:
1. Bump version in `Cargo.toml` + `Cargo.lock`
2. Run the pre-release hook (`git cliff`) to regenerate `CHANGELOG.md`
3. Commit both files
4. Create and push the `vX.Y.Z` tag
5. GitHub Actions release workflow fires automatically

## SemVer rules (pre-1.0)

- **patch** (`0.1.x`): bug fixes, backwards-compatible
- **minor** (`0.x.0`): new features or breaking changes (pre-1.0 convention)
- **major** (`1.0.0`): stable public API — reserved for when the tool is production-ready

## Manual re-run (workflow_dispatch)

If the release workflow fails after the tag exists:

1. Go to Actions → Release → Run workflow
2. Enter the existing tag (e.g. `v0.1.1`) in the `tag` field
3. Click Run — the workflow re-runs validate + build + release without re-bumping the version

## Troubleshooting

**`CHANGELOG.md` drift**: if `pre-release-replacements` fails with "found 0 matches", the `[Unreleased]` header or compare link in `CHANGELOG.md` drifted from the expected pattern. Restore to the canonical form and retry:
```
## [Unreleased]
...
[unreleased]: https://github.com/leotonezi/sandslash/compare/vX.Y.Z...HEAD
```

**Hook failure**: `git cliff` must be on `PATH`. Run `which git-cliff` to verify.

**Tag already exists**: delete the local and remote tag before re-releasing:
```bash
git tag -d vX.Y.Z
git push origin :refs/tags/vX.Y.Z
```
Then re-run `cargo release <level> --execute`.
