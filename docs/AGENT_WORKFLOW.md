# Agent Workflow

Spec-driven process for taking a step from `docs/IMPLEMENTATION.md` to merged PR.
The spec (acceptance criteria) gates every transition. Code only starts after human approval.

---

## Steps

### 1. Pick step
Choose the next incomplete step from `docs/IMPLEMENTATION.md`. Note phase, step number, goal, files, and verify criteria.

### 2. Spec sign-off (GATE — do not branch until approved)
Invoke `project-planner` in spec-extraction mode. Give it the raw step text.

Agent outputs a spec card:
```
Step: X.Y — <title>
Goal: <one sentence>
Files: <list of files to create/modify>
Acceptance criteria:
  [ ] criterion 1
  [ ] criterion 2
  ...
Out of scope: <what this step explicitly does NOT do>
Rust gotchas: <relevant !Send / async / ownership traps>
Open questions: <anything needing human decision>
```

**YOU review and approve this card before anything else happens.**
Resolve open questions. If scope is wrong, push back — change the spec, not the code.

### 3. Create feature branch
Only after step 2 is approved:
```bash
git checkout development
rtk git pull origin development
git checkout -b feat/phase-<P>-step-<S>-<short-slug>
```

### 4. Decompose with project-planner
Invoke `project-planner` again with the approved spec card. It produces a task breakdown:
- Numbered subtasks with affected files and done-criteria
- Key Rust decisions (ownership, trait choices, async boundaries)

### 5. Implement
Route to the correct worker based on what's changing:
- Core Rust (fetcher, parser, crawler, engine, scoring, pipeline) → `rust-worker`
- SEO audit checks (`src/audit/`) → `auditor-worker`
- Both → run sequentially if dependent, parallel if independent

**Pass both to the worker**: approved spec card (primary) + task breakdown from step 4 (supporting context).
The worker optimizes to satisfy the acceptance criteria, not the plan.

### 6. Evaluate with feature-evaluator
Invoke `feature-evaluator`. It evaluates each acceptance criterion from step 2 as binary:

```
[ ] criterion 1 — PASS / FAIL: <reason if fail>
[ ] criterion 2 — PASS / FAIL
...
Overall: PASS (all criteria met) / FAIL (N criteria failed)
```

If any criterion fails → fix and re-evaluate. Do not proceed to step 7 with any ✗.

### 7. Validate build with build-validator
Invoke `build-validator`. It runs:
- `cargo fmt -- --check`
- `cargo clippy -- -D warnings`
- `cargo check --all-targets`
- `cargo test`
- `cargo build --release`
- Root cleanliness check
- Static scans: `.unwrap()` in lib code, `unsafe`, `println!` outside main/report

If build-validator fails → route to correct worker to fix, re-run build-validator.

### 8. Open PR with pr-creator
Invoke `pr-creator`. It will:
- Verify branch is not `master` or `development`
- Target `development`
- Include the spec card acceptance criteria in the PR body
- Derive title + body from `git log` and `git diff`
- Push branch and open PR via `gh pr create`

---

## Agent Map

| Agent | When to use |
|---|---|
| `project-planner` | Step 2 — spec extraction + sign-off |
| `project-planner` | Step 4 — task decomposition (after approval) |
| `rust-worker` | Step 5 — fetcher, parser, crawler, engine, scoring, pipeline |
| `auditor-worker` | Step 5 — any `src/audit/` work |
| `feature-evaluator` | Step 6 — always, binary checklist against spec |
| `build-validator` | Step 7 — always, before PR |
| `pr-creator` | Step 8 — always |

---

## Branch Naming

```
feat/phase-<P>-step-<S>-<short-slug>   # step from docs/IMPLEMENTATION.md
fix/<short-slug>                        # bug fix
chore/<short-slug>                      # tooling, refactor, docs
```

---

## Example

```
docs/IMPLEMENTATION.md — Step 3.4: Per-host rate limiter

1. Pick step 3.4
2. project-planner extracts spec card:
     Goal: per-host token-bucket rate limiter using governor + dashmap
     Files: src/fetcher/rate_limiter.rs
     Criteria:
       [ ] HostRateLimiter::acquire(host) awaits a permit per host
       [ ] DashMap entry guard dropped before .await (no deadlock)
       [ ] Integration test: 10 requests at qps=2 take ≥ 4s wall time
     Gotchas: MUST clone Arc out of DashMap before awaiting
   YOU approve → branch created
3. git checkout -b feat/phase-3-step-3.4-rate-limiter
4. project-planner decomposes into subtasks
5. rust-worker receives spec card + subtasks → implements
6. feature-evaluator checks each criterion binary ✓/✗
7. build-validator → fmt + clippy -D warnings + tests + release build
8. pr-creator → PR to development with spec criteria in body
```
