# Agent Workflow

Standard process for taking a step from IMPLEMENTATION.md to merged PR.

---

## Steps

### 1. Pick step
Choose the next incomplete step from IMPLEMENTATION.md (e.g. "Step 1.4 — Metadata auditor"). Note the phase and step number.

### 2. Create feature branch
```bash
git checkout development
rtk git pull origin development
git checkout -b feat/phase-<P>-step-<S>-<short-slug>
```
Example: `feat/phase-1-step-1.4-metadata-auditor`

### 3. Plan with project-planner
Invoke `project-planner`. Give it:
- The IMPLEMENTATION.md step text (phase, step number, goal, files, verify criteria)
- Any relevant context from the current codebase state

Agent outputs: **Goal**, **Subtasks** (with done-criteria), **Key Rust decisions**, **Gotchas**, **Open questions**.
Resolve open questions before proceeding.

### 4. Implement
Route to the correct worker based on what's changing:
- Core Rust (fetcher, parser, crawler, engine, scoring, pipeline) → `rust-worker`
- SEO audit checks (`src/audit/`) → `auditor-worker`
- Both → run both agents (sequentially if dependent, parallel if independent)

Each worker receives the plan output from step 3 as context.

### 5. Evaluate with feature-evaluator
Invoke `feature-evaluator`. It checks:
- Implementation matches IMPLEMENTATION.md acceptance criteria for the step
- Tests pass (unit + integration)
- Idiomatic Rust (no `.unwrap()`, no blocking in async, `!Send` handled correctly)
- No regressions

If evaluation fails → fix issues, re-evaluate.

### 6. Validate build with build-validator
Invoke `build-validator`. It runs:
- `cargo fmt -- --check`
- `cargo clippy -- -D warnings`
- `cargo check --all-targets`
- `cargo test`
- `cargo build --release`
- Static scans: `.unwrap()` in lib code, `unsafe`, `println!` outside main/report

If build-validator fails → route to correct worker to fix, then re-run build-validator.

### 7. Open PR with pr-creator
Invoke `pr-creator`. It will:
- Verify branch is not `master` or `development`
- Target `development` (feature → development; development → master handled separately)
- Reference the IMPLEMENTATION.md step in the PR body
- Derive title + body from `git log` and `git diff`
- Push branch and open PR via `gh pr create`

---

## Agent Map

| Agent | When to use |
|---|---|
| `project-planner` | Step 3 — always |
| `rust-worker` | Step 4 — fetcher, parser, crawler, engine, scoring, pipeline |
| `auditor-worker` | Step 4 — any `src/audit/` work |
| `feature-evaluator` | Step 5 — always |
| `build-validator` | Step 6 — always (before PR) |
| `pr-creator` | Step 7 — always |

---

## Branch Naming

```
feat/phase-<P>-step-<S>-<short-slug>   # new implementation from IMPLEMENTATION.md
fix/<short-slug>                        # bug fix
chore/<short-slug>                      # tooling, refactor, docs
```

---

## Example

```
IMPLEMENTATION.md — Step 3.4: Per-host rate limiter

1. git checkout -b feat/phase-3-step-3.4-rate-limiter
2. project-planner   → plan subtasks, flag DashMap guard gotcha
3. rust-worker       → implement src/fetcher/rate_limiter.rs
4. feature-evaluator → verify no guard held across .await, test 10-req wall time
5. build-validator   → fmt + clippy -D warnings + tests + release build
6. pr-creator        → open PR to master
```
