use crate::error::{Result, SeoError};
use redis::{AsyncCommands, Client, Script, aio::ConnectionManager};

/// Redis-backed crawl frontier.
///
/// Keys are scoped per job to allow concurrent jobs without collision:
/// - `seo:{job_id}:seen`     — SET of normalized URL strings (dedup)
/// - `seo:{job_id}:queue`    — LIST used as FIFO queue (RPUSH / LPOP)
/// - `seo:{job_id}:inflight` — integer counter of enqueued-but-not-completed URLs
///
/// Entry encoding in the queue: `"{depth}|{url}"`.
pub struct Frontier {
    conn: ConnectionManager,
    job_id: String,
    enqueue_script: Script,
}

impl Frontier {
    // ── Key helpers ─────────────────────────────────────────────────────────

    fn key_seen(&self) -> String {
        format!("seo:{}:seen", self.job_id)
    }

    fn key_queue(&self) -> String {
        format!("seo:{}:queue", self.job_id)
    }

    fn key_inflight(&self) -> String {
        format!("seo:{}:inflight", self.job_id)
    }

    // ── Encode / decode helpers ──────────────────────────────────────────────

    fn encode(depth: u32, url: &str) -> String {
        format!("{depth}|{url}")
    }

    fn decode(entry: &str) -> Option<(u32, String)> {
        // split_once stops at the first '|', so URLs that contain '|' are preserved.
        let (depth_str, url) = entry.split_once('|')?;
        let depth: u32 = depth_str.parse().ok()?;
        Some((depth, url.to_owned()))
    }

    // ── Lua script ──────────────────────────────────────────────────────────

    /// Build the atomic enqueue script.
    ///
    /// KEYS[1] = seen SET
    /// KEYS[2] = queue LIST
    /// KEYS[3] = inflight counter
    /// ARGV[1] = normalized URL (dedup key)
    /// ARGV[2] = encoded entry  ("{depth}|{url}")
    ///
    /// Returns 1 if the URL was new (enqueued), 0 if it was a duplicate.
    fn build_enqueue_script() -> Script {
        Script::new(
            r#"
if redis.call('SADD', KEYS[1], ARGV[1]) == 1 then
    redis.call('RPUSH', KEYS[2], ARGV[2])
    redis.call('INCR', KEYS[3])
    return 1
else
    return 0
end
"#,
        )
    }

    // ── Constructor ─────────────────────────────────────────────────────────

    /// Connect to Redis and create a frontier scoped to `job_id`.
    pub async fn new(redis_url: &str, job_id: String) -> Result<Self> {
        let client = Client::open(redis_url).map_err(SeoError::Redis)?;
        let conn = ConnectionManager::new(client)
            .await
            .map_err(SeoError::Redis)?;
        Ok(Self {
            conn,
            job_id,
            enqueue_script: Self::build_enqueue_script(),
        })
    }

    // ── Public API ───────────────────────────────────────────────────────────

    /// Atomically add `url` at `depth` to the frontier.
    ///
    /// Returns `true` if the URL was new and has been enqueued,
    /// `false` if it was already seen (duplicate — not enqueued again).
    pub async fn enqueue(&mut self, url: &str, depth: u32) -> Result<bool> {
        let encoded = Self::encode(depth, url);
        let result: i64 = self
            .enqueue_script
            .key(self.key_seen())
            .key(self.key_queue())
            .key(self.key_inflight())
            .arg(url)
            .arg(&encoded)
            .invoke_async(&mut self.conn)
            .await
            .map_err(SeoError::Redis)?;
        Ok(result == 1)
    }

    /// Pop the oldest entry from the queue (FIFO).
    ///
    /// Returns `Ok(None)` when the queue is empty.
    /// The inflight counter is NOT decremented here — call `mark_done` after
    /// finishing work on the returned URL.
    pub async fn dequeue(&mut self) -> Result<Option<(u32, String)>> {
        let raw: Option<String> = self
            .conn
            .lpop(self.key_queue(), None)
            .await
            .map_err(SeoError::Redis)?;

        match raw {
            None => Ok(None),
            Some(entry) => {
                let decoded = Self::decode(&entry).ok_or_else(|| {
                    SeoError::Parse(format!("frontier: malformed queue entry: {entry:?}"))
                })?;
                Ok(Some(decoded))
            }
        }
    }

    /// Signal that one unit of inflight work has been completed.
    ///
    /// Must be called exactly once per successful `dequeue` result.
    pub async fn mark_done(&mut self) -> Result<()> {
        self.conn
            .decr::<_, _, i64>(self.key_inflight(), 1_i64)
            .await
            .map_err(SeoError::Redis)?;
        Ok(())
    }

    /// Returns `true` only when both the queue is empty **and** inflight == 0.
    ///
    /// Using `<= 0` for inflight guards against underflow in edge cases.
    pub async fn is_complete(&mut self) -> Result<bool> {
        let llen: i64 = self
            .conn
            .llen(self.key_queue())
            .await
            .map_err(SeoError::Redis)?;

        let inflight: Option<i64> = self
            .conn
            .get(self.key_inflight())
            .await
            .map_err(SeoError::Redis)?;

        Ok(llen == 0 && inflight.unwrap_or(0) <= 0)
    }

    /// Delete all per-job Redis keys.
    pub async fn clear(&mut self) -> Result<()> {
        let keys = [self.key_seen(), self.key_queue(), self.key_inflight()];
        self.conn
            .del::<_, ()>(keys.as_slice())
            .await
            .map_err(SeoError::Redis)?;
        Ok(())
    }
}
