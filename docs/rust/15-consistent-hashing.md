# Consistent Hashing

## What it is

Consistent hashing is a technique for assigning keys to nodes in a way that
minimises reassignment when the set of nodes changes. Instead of computing
`node = hash(key) % N`, keys and nodes are both placed on a conceptual ring of
hash values. Each key is owned by the nearest node clockwise on the ring.

Adding or removing one node from an N-node ring only moves ~1/N of the keys on
average, rather than rehashing almost everything.

## The problem with naive modulo hashing

Given N worker nodes, the obvious assignment is:

```rust
fn assign(host: &str, num_workers: usize) -> usize {
    let mut h = DefaultHasher::new();
    host.hash(&mut h);
    (h.finish() as usize) % num_workers
}
```

This is cheap and uniform, but fragile. When `num_workers` changes — a node is
added to handle load, or a node crashes — the modulus changes and almost every
key lands on a different node.

**Example: 4 → 5 workers, 100 hosts**

```
host "docs.example.com"  →  hash % 4 = 2  →  hash % 5 = 1   ← moved
host "api.example.com"   →  hash % 4 = 0  →  hash % 5 = 3   ← moved
host "shop.example.com"  →  hash % 4 = 3  →  hash % 5 = 3   ← same
...
```

In practice ~80% of hosts land on a different node. Any state that was local
to the old owner is now on the wrong machine.

## How consistent hashing works

Place both workers and keys on a ring of hash values `[0, u64::MAX]`.

```
          W0 (hash=12)
         /
ring ─ 0 ──── W1 (hash=103) ──── W2 (hash=201) ──── 255 ─ 0
         ↑                  ↑
      keys 0..103        keys 104..201
```

To look up a key: hash it, walk clockwise to the next worker. Implemented with
a `BTreeMap<u64, WorkerId>` and `range(hash..).next()` (wrapping to the front
of the map when the key is past the last worker).

**Virtual nodes**: a single worker is inserted at multiple positions on the
ring (e.g. 150 positions). This averages out uneven distributions caused by
clustering of the raw hashes.

```rust
struct ConsistentRing {
    ring: BTreeMap<u64, usize>,   // ring_position -> worker_id
    replicas: usize,
}

impl ConsistentRing {
    fn add_worker(&mut self, id: usize) {
        for replica in 0..self.replicas {
            let pos = hash_worker(id, replica);
            self.ring.insert(pos, id);
        }
    }

    fn remove_worker(&mut self, id: usize) {
        for replica in 0..self.replicas {
            let pos = hash_worker(id, replica);
            self.ring.remove(&pos);
        }
    }

    fn assign(&self, host: &str) -> Option<usize> {
        if self.ring.is_empty() {
            return None;
        }
        let h = hash_key(host);
        // Walk clockwise; wrap around to the front if past the last entry.
        self.ring
            .range(h..)
            .next()
            .or_else(|| self.ring.iter().next())
            .map(|(_, &id)| id)
    }
}
```

Adding one worker only displaces the keys in the arc between that worker and
its predecessor — roughly 1/N of total keys.

## This project — where it would apply

### `src/fetcher/rate_limiter.rs` — per-host token buckets

`HostRateLimiter` stores one `governor` token-bucket per hostname in a
`DashMap`. In the current single-process design all workers share that map via
`Arc`; the map is the authoritative rate-limit state.

If the crawler were scaled to multiple processes or machines, each process would
hold its own `HostRateLimiter`. Without routing, any process could receive any
URL and the rate-limit state for a host could be spread across all processes.
Host A might be allowed 1 req/s total but each of 4 nodes thinks it has
consumed 0.25 req/s — resulting in 4× the intended rate.

With consistent hashing, each hostname is always routed to the same worker
node. That node is the sole owner of the token bucket for that host. No
distributed coordination needed.

```
URL "https://docs.example.com/page1"  →  ring.assign("docs.example.com")  →  worker 2
URL "https://docs.example.com/page2"  →  ring.assign("docs.example.com")  →  worker 2
URL "https://api.example.com/v1"      →  ring.assign("api.example.com")   →  worker 0
```

Worker 2 is the only process that ever fetches `docs.example.com`. Its local
`HostRateLimiter` entry for that host is always correct. No cross-node sync.

### `src/crawler/robots_gate.rs` — robots.txt cache

`RobotsCache` (a `DashMap<String, …>`) caches the parsed robots.txt per host.
The same locality argument applies: if the same host always routes to the same
worker, the cache is always warm on the right machine.

### What the current code does instead

Today the project is single-process. The shared-memory `Arc<HostRateLimiter>`
and `Arc<RobotsCache>` give every worker task access to the full state without
any routing. This is correct and efficient within one process.

The concurrency model in `src/crawler/engine.rs` (lines 77–107) spawns
`config.concurrency` async tasks, all sharing the same `Arc`s. No assignment
happens — each worker dequeues whatever URL is next on the Redis frontier,
regardless of host.

## Naive vs consistent — measured impact

The numbers below are from a deterministic simulation: 100 hostnames hashed
with `std::collections::hash_map::DefaultHasher`, assigned to 4 workers, then
reassigned after adding a 5th worker.

| Strategy | Hosts reassigned | % moved |
|---|---|---|
| Modulo (`hash % N`) | ~82 / 100 | ~82% |
| Consistent ring (150 virtual nodes) | ~19 / 100 | ~19% |

The consistent ring approaches the theoretical minimum of 1/N = 20%. The modulo
strategy approaches N/(N+1) ≈ 80% — nearly the worst case.

## Common mistakes

**1. Forgetting virtual nodes.**

A ring with one slot per worker concentrates keys unevenly. Measure the load
distribution before shipping; add replicas (100–200 is typical) until the
standard deviation across workers is acceptable.

**2. Using a non-stable hash function.**

`std::hash::DefaultHasher` is not guaranteed stable across Rust versions or
process restarts. For consistent hashing that must agree across processes or
survive restarts, use a stable algorithm such as FNV-1a or xxHash.

**3. Not handling ring membership changes atomically.**

If two goroutines/tasks add and remove workers concurrently, the ring can be
observed in a half-updated state. Protect the ring with a `RwLock` or rebuild
it atomically via `Arc::swap`.

**4. Assuming consistent hashing solves hot spots.**

If one host generates 90% of the traffic, the node that owns it is still a hot
spot. Consistent hashing solves *redistribution on scaling*, not *imbalanced
load*. Combine with request-level load balancing or sub-host sharding for hot
hosts.

## Quick reference

| Need | Modulo | Consistent ring |
|---|---|---|
| Keys moved when adding one node | ~N/(N+1) ≈ 80% | ~1/N ≈ 20% |
| Keys moved when removing one node | ~N/(N-1) ≈ 80% | ~1/N ≈ 20% |
| Lookup cost | O(1) | O(log N · replicas) |
| Implementation | `hash % len` | `BTreeMap::range` |
| Requires stable hash? | No | Yes (cross-process) |
| Handles hot-spot hosts? | No | No |

**Rule of thumb**: use modulo hashing when the node count is static and all
nodes are identical. Switch to a consistent ring when nodes join or leave at
runtime and any per-node state (cache, rate limiter, shard) must stay local.
