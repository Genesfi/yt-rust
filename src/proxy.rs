use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

const FIRST_BLOCK: u64 = 64 * 1024;
const BLOCK_SIZE: u64 = 512 * 1024;
const WORKERS: usize = 4;
const PREFETCH_AHEAD: usize = 24;
const KEEP_BEHIND: usize = 8;
const MAX_ACTIVE_RETAIN_SPAN: usize = 64;
const MAX_SESSIONS: usize = 32;
const VISIONOS_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";
const WEB_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36";

#[derive(Clone)]
pub struct StreamProxy {
    port: u16,
    http: reqwest::Client,
    state: Arc<Mutex<ProxyState>>,
    handle: tokio::runtime::Handle,
}

struct ProxyState {
    next_id: u64,
    sessions: HashMap<u64, Arc<StreamSession>>,
}

struct StreamSession {
    url: String,
    user_agent: String,
    size: u64,
    num_blocks: usize,
    blocks: Mutex<HashMap<usize, Vec<u8>>>,
    in_flight: Mutex<HashMap<usize, bool>>,
    cursors: Mutex<HashMap<u64, usize>>,
    latest_conn: AtomicU64,
    next_conn_id: AtomicU64,
    last_cursor: AtomicUsize,
    head_limit: AtomicUsize,
    active_readers: AtomicUsize,
    cancelled: AtomicBool,
    failed: AtomicBool,
    forbidden: AtomicBool,
    notify: Notify,
}

impl StreamSession {
    fn new(url: String, user_agent: String, size: u64, head_limit: usize) -> Self {
        let num_blocks = block_count(size);
        Self {
            url,
            user_agent,
            size,
            num_blocks,
            blocks: Mutex::new(HashMap::new()),
            in_flight: Mutex::new(HashMap::new()),
            cursors: Mutex::new(HashMap::new()),
            latest_conn: AtomicU64::new(0),
            next_conn_id: AtomicU64::new(0),
            last_cursor: AtomicUsize::new(0),
            head_limit: AtomicUsize::new(head_limit),
            active_readers: AtomicUsize::new(0),
            cancelled: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            forbidden: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    fn register_cursor(&self, idx: usize) -> u64 {
        let conn_id = self.next_conn_id.fetch_add(1, Ordering::Relaxed) + 1;
        let clamped = idx.min(self.num_blocks.saturating_sub(1));
        self.cursors.lock().unwrap().insert(conn_id, clamped);
        self.latest_conn.store(conn_id, Ordering::Release);
        self.last_cursor.store(clamped, Ordering::Release);
        conn_id
    }

    fn update_cursor(&self, conn_id: u64, idx: usize) {
        let clamped = idx.min(self.num_blocks.saturating_sub(1));
        self.cursors.lock().unwrap().insert(conn_id, clamped);
        if self.latest_conn.load(Ordering::Acquire) == conn_id {
            self.last_cursor.store(clamped, Ordering::Release);
        }
    }

    fn remove_cursor(&self, conn_id: u64) {
        let mut cursors = self.cursors.lock().unwrap();
        cursors.remove(&conn_id);
        if self.latest_conn.load(Ordering::Acquire) == conn_id {
            let next_latest = cursors.keys().copied().max().unwrap_or(0);
            self.latest_conn.store(next_latest, Ordering::Release);
            if let Some(&c) = cursors.get(&next_latest) {
                self.last_cursor.store(c, Ordering::Release);
            }
        }
    }

    fn primary_cursor_locked(&self, cursors: &HashMap<u64, usize>) -> usize {
        let latest = self.latest_conn.load(Ordering::Acquire);
        let raw = cursors
            .get(&latest)
            .copied()
            .or_else(|| cursors.iter().max_by_key(|&(&id, _)| id).map(|(_, &c)| c))
            .unwrap_or_else(|| self.last_cursor.load(Ordering::Acquire));
        raw.min(self.num_blocks.saturating_sub(1))
    }

    fn claim_next(&self) -> Option<(usize, u64, u64)> {
        if self.cancelled.load(Ordering::Acquire) || self.failed.load(Ordering::Acquire) {
            return None;
        }
        let cap = self.head_limit.load(Ordering::Acquire);
        let (primary, other_needs) = {
            let cursors = self.cursors.lock().unwrap();
            let primary = self.primary_cursor_locked(&cursors);
            let mut others: Vec<usize> = cursors.values().copied().collect();
            others.sort_unstable();
            others.dedup();
            (primary, others)
        };

        let blocks = self.blocks.lock().unwrap();
        let mut flight = self.in_flight.lock().unwrap();

        // First ensure the primary (newest) reader's immediate block is claimed, then any
        // concurrent reader's immediate block (so simultaneous reads never starve), then prefetch ahead.
        let primary_end = (primary + PREFETCH_AHEAD).min(cap).min(self.num_blocks);
        if primary < primary_end
            && !blocks.contains_key(&primary)
            && !flight.get(&primary).copied().unwrap_or(false)
        {
            flight.insert(primary, true);
            let (a, b) = block_range(primary, self.size);
            return Some((primary, a, b));
        }

        for base in other_needs {
            let end = (base + 2).min(cap).min(self.num_blocks);
            for idx in base..end {
                if !blocks.contains_key(&idx) && !flight.get(&idx).copied().unwrap_or(false) {
                    flight.insert(idx, true);
                    let (a, b) = block_range(idx, self.size);
                    return Some((idx, a, b));
                }
            }
        }

        for idx in primary..primary_end {
            if !blocks.contains_key(&idx) && !flight.get(&idx).copied().unwrap_or(false) {
                flight.insert(idx, true);
                let (a, b) = block_range(idx, self.size);
                return Some((idx, a, b));
            }
        }
        None
    }

    fn finish_block(&self, idx: usize, data: Vec<u8>) {
        {
            let min_keep = {
                let cursors = self.cursors.lock().unwrap();
                let primary = self.primary_cursor_locked(&cursors);
                let floor = primary.saturating_sub(MAX_ACTIVE_RETAIN_SPAN);
                let active_min = cursors
                    .values()
                    .copied()
                    .min()
                    .map(|m| m.saturating_sub(2).max(floor))
                    .unwrap_or_else(|| primary.saturating_sub(KEEP_BEHIND));
                primary.saturating_sub(KEEP_BEHIND).min(active_min)
            };
            let mut flight = self.in_flight.lock().unwrap();
            flight.remove(&idx);
            let mut blocks = self.blocks.lock().unwrap();
            blocks.insert(idx, data);
            // Evict old blocks far behind the active reader cursor(s) to bound RAM
            blocks.retain(|&k, _| k >= min_keep);
        }
        self.notify.notify_waiters();
    }

    fn release_flight(&self, idx: usize) {
        let mut flight = self.in_flight.lock().unwrap();
        flight.remove(&idx);
    }
}

pub fn block_range(idx: usize, total_size: u64) -> (u64, u64) {
    let start = if idx == 0 {
        0
    } else {
        FIRST_BLOCK + (idx as u64 - 1) * BLOCK_SIZE
    };
    let next_start = if idx == 0 {
        FIRST_BLOCK
    } else {
        FIRST_BLOCK + idx as u64 * BLOCK_SIZE
    };
    let end = next_start.min(total_size).saturating_sub(1);
    (start, end)
}

pub fn block_of(pos: u64) -> usize {
    if pos < FIRST_BLOCK {
        0
    } else {
        1 + ((pos - FIRST_BLOCK) / BLOCK_SIZE) as usize
    }
}

pub fn block_count(total_size: u64) -> usize {
    if total_size == 0 {
        0
    } else {
        block_of(total_size - 1) + 1
    }
}

impl StreamProxy {
    pub async fn start(http: reqwest::Client) -> anyhow::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let state = Arc::new(Mutex::new(ProxyState {
            next_id: 1,
            sessions: HashMap::new(),
        }));
        let proxy = Self {
            port,
            http: http.clone(),
            state: state.clone(),
            handle: tokio::runtime::Handle::current(),
        };

        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    continue;
                };
                let state = state.clone();
                tokio::spawn(async move {
                    let _ = handle_conn(socket, state).await;
                });
            }
        });

        Ok(proxy)
    }

    /// Registers a googlevideo progressive/adaptive URL for parallel `&range=` prefetching.
    /// Returns the original URL unchanged if `size == 0` or if it's an HLS playlist.
    pub fn register(&self, url: &str, size: u64) -> String {
        self.register_with_ua(url, size, VISIONOS_UA)
    }

    pub fn register_with_ua(&self, url: &str, size: u64, user_agent: &str) -> String {
        self.register_inner(url, size, user_agent, usize::MAX)
    }

    /// Registers a stream for background head-only prefetch (first 2 blocks = ~576 KB)
    /// until a player actually opens the local proxy URL.
    pub fn register_prefetch_with_ua(&self, url: &str, size: u64, user_agent: &str) -> String {
        self.register_inner(url, size, user_agent, 2)
    }

    fn register_inner(
        &self,
        url: &str,
        size: u64,
        user_agent: &str,
        head_limit: usize,
    ) -> String {
        if size == 0 || url.contains(".m3u8") || url.contains("/manifest/hls") || url.contains("range=") {
            return url.to_owned();
        }
        let ua = if user_agent.trim().is_empty() {
            WEB_UA.to_owned()
        } else {
            user_agent.to_owned()
        };
        let session = Arc::new(StreamSession::new(url.to_owned(), ua, size, head_limit));
        let id = {
            let mut st = self.state.lock().unwrap();
            let id = st.next_id;
            st.next_id += 1;
            // Retain active watch tabs + active short + prefetched upcoming shorts.
            // Never evict an actively read session while idle/prefetch sessions exist.
            while st.sessions.len() >= MAX_SESSIONS {
                let evict_id = st
                    .sessions
                    .iter()
                    .filter(|(_, s)| {
                        s.active_readers.load(Ordering::Acquire) == 0
                            && s.head_limit.load(Ordering::Acquire) <= 2
                    })
                    .map(|(&k, _)| k)
                    .min()
                    .or_else(|| {
                        st.sessions
                            .iter()
                            .filter(|(_, s)| s.active_readers.load(Ordering::Acquire) == 0)
                            .map(|(&k, _)| k)
                            .min()
                    })
                    .or_else(|| st.sessions.keys().min().copied());
                if let Some(victim) = evict_id {
                    if let Some(old) = st.sessions.remove(&victim) {
                        old.cancel();
                    }
                } else {
                    break;
                }
            }
            st.sessions.insert(id, session.clone());
            id
        };

        let workers = if head_limit <= 2 { 2 } else { WORKERS };
        for _ in 0..workers {
            let http = self.http.clone();
            let sess = session.clone();
            self.handle.spawn(async move {
                worker_loop(http, sess).await;
            });
        }

        format!("http://127.0.0.1:{}/s/{id}", self.port)
    }

    fn session_id_from_url(&self, url: &str) -> Option<u64> {
        let prefix = format!("http://127.0.0.1:{}/s/", self.port);
        url.strip_prefix(&prefix)?
            .split('?')
            .next()?
            .parse::<u64>()
            .ok()
    }

    pub fn is_session_forbidden(&self, url: &str) -> bool {
        let Some(id) = self.session_id_from_url(url) else {
            return false;
        };
        self.state
            .lock()
            .unwrap()
            .sessions
            .get(&id)
            .is_some_and(|s| s.forbidden.load(Ordering::Acquire))
    }

    pub fn is_session_failed(&self, url: &str) -> bool {
        let Some(id) = self.session_id_from_url(url) else {
            return false;
        };
        self.state
            .lock()
            .unwrap()
            .sessions
            .get(&id)
            .is_some_and(|s| s.failed.load(Ordering::Acquire))
    }

    pub fn session_user_agent(&self, url: &str) -> Option<String> {
        let id = self.session_id_from_url(url)?;
        self.state
            .lock()
            .unwrap()
            .sessions
            .get(&id)
            .map(|s| s.user_agent.clone())
    }
}

async fn worker_loop(http: reqwest::Client, sess: Arc<StreamSession>) {
    loop {
        if sess.cancelled.load(Ordering::Acquire) || sess.failed.load(Ordering::Acquire) {
            return;
        }
        let notified = sess.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();

        let Some((idx, a, b)) = sess.claim_next() else {
            tokio::select! {
                () = notified => continue,
                () = tokio::time::sleep(Duration::from_millis(400)) => continue,
            }
        };

        let expected_len = (b - a + 1) as usize;
        let is_last_block = idx + 1 >= sess.num_blocks;
        let mut ok_data = None;
        let mut try_alt_ua = false;
        let mut saw_403 = false;
        for attempt in 0..6 {
            if sess.cancelled.load(Ordering::Acquire) {
                sess.release_flight(idx);
                return;
            }
            let sep = if sess.url.contains('?') { '&' } else { '?' };
            let ranged_url = format!("{}{sep}range={a}-{b}", sess.url);
            let ua = if try_alt_ua {
                if sess.user_agent == VISIONOS_UA {
                    WEB_UA
                } else {
                    VISIONOS_UA
                }
            } else {
                sess.user_agent.as_str()
            };
            let res = http
                .get(&ranged_url)
                .header("User-Agent", ua)
                .timeout(Duration::from_secs(12))
                .send()
                .await;

            match res {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(bytes) = resp.bytes().await {
                        let complete = if is_last_block {
                            !bytes.is_empty()
                        } else {
                            bytes.len() == expected_len
                        };
                        if complete {
                            saw_403 = false;
                            ok_data = Some(bytes.to_vec());
                            break;
                        }
                    }
                }
                Ok(resp) if resp.status().as_u16() == 403 => {
                    log::warn!("proxy chunk {idx} HTTP 403");
                    saw_403 = true;
                    if !try_alt_ua {
                        try_alt_ua = true;
                        continue;
                    }
                    break;
                }
                _ => {}
            }
            let backoff_ms = (150 * (attempt + 1)).min(1200);
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        }

        if let Some(data) = ok_data {
            sess.finish_block(idx, data);
        } else {
            if saw_403 {
                sess.forbidden.store(true, Ordering::Release);
            }
            sess.release_flight(idx);
            sess.failed.store(true, Ordering::Release);
            sess.notify.notify_waiters();
            return;
        }
    }
}

async fn handle_conn(
    mut socket: TcpStream,
    state: Arc<Mutex<ProxyState>>,
) -> anyhow::Result<()> {
    let _ = socket.set_nodelay(true);
    let (read_half, mut write_half) = socket.split();
    let mut reader = BufReader::new(read_half);

    let mut req_line = String::new();
    if reader.read_line(&mut req_line).await? == 0 {
        return Ok(());
    }
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    let mut range_hdr: Option<String> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            if k.trim().eq_ignore_ascii_case("range") {
                range_hdr = Some(v.trim().to_owned());
            }
        }
    }

    let id: u64 = path
        .strip_prefix("/s/")
        .and_then(|s| s.split('?').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let sess = { state.lock().unwrap().sessions.get(&id).cloned() };
    let Some(sess) = sess else {
        write_half
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await?;
        return Ok(());
    };

    let total = sess.size;
    let (start, end, is_partial) = match range_hdr.as_deref().and_then(|h| parse_range(h, total)) {
        Some((s, e)) => (s, e, true),
        None => (0, total.saturating_sub(1), false),
    };

    if start >= total || end < start {
        let resp = format!(
            "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{total}\r\nConnection: close\r\n\r\n"
        );
        write_half.write_all(resp.as_bytes()).await?;
        return Ok(());
    }

    struct ReaderGuard(Arc<StreamSession>);
    impl Drop for ReaderGuard {
        fn drop(&mut self) {
            self.0.active_readers.fetch_sub(1, Ordering::Relaxed);
        }
    }
    sess.active_readers.fetch_add(1, Ordering::Relaxed);
    let _guard = ReaderGuard(sess.clone());

    if method.eq_ignore_ascii_case("HEAD") {
        let len = end - start + 1;
        let headers = if is_partial {
            format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Type: application/octet-stream\r\nAccept-Ranges: bytes\r\nContent-Range: bytes {start}-{end}/{total}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n"
            )
        } else {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nAccept-Ranges: bytes\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n"
            )
        };
        write_half.write_all(headers.as_bytes()).await?;
        return Ok(());
    }

    if sess.head_limit.swap(usize::MAX, Ordering::AcqRel) != usize::MAX {
        sess.notify.notify_waiters();
    }
    struct CursorGuard {
        sess: Arc<StreamSession>,
        conn_id: u64,
    }
    impl Drop for CursorGuard {
        fn drop(&mut self) {
            self.sess.remove_cursor(self.conn_id);
        }
    }
    let conn_id = sess.register_cursor(block_of(start));
    let _cursor_guard = CursorGuard {
        sess: sess.clone(),
        conn_id,
    };

    let mut headers_sent = false;
    let mut pos = start;
    let mut eof_buf = [0u8; 16];
    while pos <= end {
        if sess.cancelled.load(Ordering::Acquire) {
            break;
        }
        let idx = block_of(pos);
        sess.update_cursor(conn_id, idx);
        sess.notify.notify_waiters();

        let (b_start, _) = block_range(idx, total);
        let chunk = loop {
            if sess.cancelled.load(Ordering::Acquire) || sess.failed.load(Ordering::Acquire) {
                if !headers_sent {
                    let resp = if sess.forbidden.load(Ordering::Acquire) {
                        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    } else {
                        "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    };
                    let _ = write_half.write_all(resp.as_bytes()).await;
                }
                return Ok(());
            }
            let notified = sess.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            if let Some(data) = sess.blocks.lock().unwrap().get(&idx).cloned() {
                break data;
            }
            // Wake immediately if the player closed the TCP socket (e.g. during a seek),
            // otherwise wait for the next fetched block notification.
            tokio::select! {
                () = notified => {}
                res = reader.read(&mut eof_buf) => {
                    match res {
                        Ok(0) | Err(_) => return Ok(()),
                        Ok(_) => {}
                    }
                }
            }
        };

        if !headers_sent {
            let len = end - start + 1;
            let headers = if is_partial {
                format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Type: application/octet-stream\r\nAccept-Ranges: bytes\r\nContent-Range: bytes {start}-{end}/{total}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n"
                )
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nAccept-Ranges: bytes\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n"
                )
            };
            if write_half.write_all(headers.as_bytes()).await.is_err() {
                break;
            }
            headers_sent = true;
        }

        let offset = (pos - b_start) as usize;
        if offset >= chunk.len() {
            break;
        }
        let avail = chunk.len() - offset;
        let to_send = avail.min((end - pos + 1) as usize);
        if write_half.write_all(&chunk[offset..offset + to_send]).await.is_err() {
            break;
        }
        pos += to_send as u64;
    }

    Ok(())
}

fn parse_range(hdr: &str, total: u64) -> Option<(u64, u64)> {
    let spec = hdr.strip_prefix("bytes=")?;
    let (a, b) = spec.split_once('-')?;
    if a.is_empty() {
        let suffix: u64 = b.parse().ok()?;
        let start = total.saturating_sub(suffix);
        return Some((start, total.saturating_sub(1)));
    }
    let start: u64 = a.parse().ok()?;
    let end = if b.is_empty() {
        total.saturating_sub(1)
    } else {
        b.parse::<u64>().ok()?.min(total.saturating_sub(1))
    };
    Some((start, end))
}

static _UNUSED_KEEPALIVE: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_block_ranges_and_offsets() {
        assert_eq!(block_range(0, 2_000_000), (0, FIRST_BLOCK - 1));
        assert_eq!(
            block_range(1, 2_000_000),
            (FIRST_BLOCK, FIRST_BLOCK + BLOCK_SIZE - 1)
        );
        assert_eq!(block_of(0), 0);
        assert_eq!(block_of(FIRST_BLOCK - 1), 0);
        assert_eq!(block_of(FIRST_BLOCK), 1);
    }

    #[test]
    fn parses_http_byte_ranges() {
        assert_eq!(parse_range("bytes=0-", 1000), Some((0, 999)));
        assert_eq!(parse_range("bytes=100-199", 1000), Some((100, 199)));
        assert_eq!(parse_range("bytes=-200", 1000), Some((800, 999)));
    }

    #[test]
    fn register_works_outside_tokio_runtime_thread() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let proxy = rt.block_on(async {
            StreamProxy::start(reqwest::Client::new()).await.unwrap()
        });
        let p = proxy.clone();
        let url = std::thread::spawn(move || p.register("https://example.com/v.mp4", 1024))
            .join()
            .unwrap();
        assert!(url.starts_with("http://127.0.0.1:"));
        assert!(!proxy.is_session_forbidden(&url));
        assert!(!proxy.is_session_failed(&url));
        if let Some(id) = proxy.session_id_from_url(&url) {
            proxy
                .state
                .lock()
                .unwrap()
                .sessions
                .get(&id)
                .unwrap()
                .forbidden
                .store(true, Ordering::Release);
            assert!(proxy.is_session_forbidden(&url));
        }
    }

    #[test]
    fn latest_seek_cursor_takes_priority_over_old_reader() {
        let sess = StreamSession::new(
            "https://example.com/v.mp4".to_owned(),
            VISIONOS_UA.to_owned(),
            50 * 1024 * 1024,
            usize::MAX,
        );
        let old_conn = sess.register_cursor(1);
        let new_conn = sess.register_cursor(40);
        // Simulate old closing reader updating its cursor after new seek already registered.
        sess.update_cursor(old_conn, 2);
        let claimed = sess.claim_next().expect("primary cursor block should be claimed");
        assert_eq!(claimed.0, 40);
        // Secondary reader's immediate block can still be claimed while primary is in flight.
        let claimed_other = sess.claim_next().expect("other active cursor block should be claimed");
        assert_eq!(claimed_other.0, 2);
        sess.remove_cursor(old_conn);
        sess.remove_cursor(new_conn);
    }

    #[test]
    fn eviction_preserves_active_sessions_over_idle_prefetches() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let proxy = rt.block_on(async {
            StreamProxy::start(reqwest::Client::new()).await.unwrap()
        });
        let active_url = proxy.register("https://example.com/active.mp4", 1024 * 1024);
        let active_id = proxy.session_id_from_url(&active_url).unwrap();
        proxy
            .state
            .lock()
            .unwrap()
            .sessions
            .get(&active_id)
            .unwrap()
            .active_readers
            .store(1, Ordering::Release);

        for i in 0..(MAX_SESSIONS + 4) {
            let u = format!("https://example.com/short_{i}.mp4");
            let _ = proxy.register_prefetch_with_ua(&u, 1024 * 1024, VISIONOS_UA);
        }

        let st = proxy.state.lock().unwrap();
        assert!(st.sessions.contains_key(&active_id));
        assert!(st.sessions.len() <= MAX_SESSIONS);
    }
}
