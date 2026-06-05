# Core Rust Features — Documentation Plan

Deep-dive docs for each topic in the Cross-phase Rust learning checklist
(`docs/IMPLEMENTATION.md` §"Cross-phase Rust learning checklist").

Each topic gets its own doc under `docs/rust/`, written against this project's
actual code. Branch pattern: `chore/docs/<slug>`.

---

## Progress

| # | Topic | Doc | Branch | Status |
|---|-------|-----|--------|--------|
| 1 | Ownership & borrowing | `docs/rust/01-ownership-borrowing.md` | `chore/docs/ownership-borrowing` | [x] |
| 2 | `Send` + `Sync` | `docs/rust/02-send-sync.md` | `chore/docs/send-sync` | [x] |
| 3 | Trait objects | `docs/rust/03-trait-objects.md` | `chore/docs/trait-objects` | [x] |
| 4 | `async_trait` | `docs/rust/04-async-trait.md` | `chore/docs/async-trait` | [ ] |
| 5 | Error handling | `docs/rust/05-error-handling.md` | `chore/docs/error-handling` | [ ] |
| 6 | `tokio::spawn` & `'static + Send` | `docs/rust/06-tokio-spawn.md` | `chore/docs/tokio-spawn` | [ ] |
| 7 | Channels (`mpsc`) & drop-sender pattern | `docs/rust/07-channels-mpsc.md` | `chore/docs/channels-mpsc` | [ ] |
| 8 | `Arc` vs `Rc` | `docs/rust/08-arc-vs-rc.md` | `chore/docs/arc-vs-rc` | [ ] |
| 9 | Mutex/DashMap guards across `.await` | `docs/rust/09-guards-across-await.md` | `chore/docs/guards-across-await` | [ ] |
| 10 | `spawn_blocking` | `docs/rust/10-spawn-blocking.md` | `chore/docs/spawn-blocking` | [ ] |
| 11 | `Semaphore` for bounded concurrency | `docs/rust/11-semaphore.md` | `chore/docs/semaphore` | [ ] |
| 12 | Atomics & memory ordering | `docs/rust/12-atomics.md` | `chore/docs/atomics` | [ ] |
| 13 | `Cow<str>` | `docs/rust/13-cow-str.md` | `chore/docs/cow-str` | [ ] |
| 14 | `LazyLock` / `once_cell` | `docs/rust/14-lazylockonce-cell.md` | `chore/docs/lazylockonce-cell` | [ ] |

---

## Doc structure (each file)

Every doc follows this template:

```
# <Topic>

## What it is
Core Rust concept in one paragraph.

## Why it exists
Problem it solves; what breaks without it.

## How it works under the hood
Internals: memory layout, vtables, runtime cost, compiler checks.

## This project — where it appears
Concrete file:line references from seo-rs source.

## Common mistakes
Pitfalls specific to async / this stack.

## Quick reference
Cheat-sheet: when to use what.
```

---

## Workflow per topic

1. Checkout `development`, pull.
2. `git checkout -b chore/docs/<slug>`
3. Create `docs/rust/<N>-<slug>.md` with full deep-dive.
4. Mark row `[x]` in this file.
5. Commit + PR targeting `development`.
