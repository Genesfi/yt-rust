use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde_json::{Value, json};

use crate::auth::Session;
use crate::model::{
    AudioTrack, CaptionTrack, Chapter, ChannelInfo, ChannelTab, CommentItem, FeedChip,
    LiveChatEmote, LiveChatKind, LiveChatMessage, LiveChatRun, PlaylistOption, PlaylistSummary,
    SidebarChannel, StoryboardSpec, VideoFormat, VideoItem, WatchData,
};

const ORIGIN: &str = "https://www.youtube.com";
const WEB_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36";
const FALLBACK_WEB_VERSION: &str = "2.20261001.00.00";
const VISIONOS_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";
const CONSENT_COOKIE: &str = "SOCS=CAISAiAD";

#[derive(Clone, Debug)]
pub struct WebConfig {
    pub client_version: String,
    pub visitor_data: Option<String>,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            client_version: FALLBACK_WEB_VERSION.to_owned(),
            visitor_data: None,
        }
    }
}

pub struct FeedPage {
    pub items: Vec<VideoItem>,
    pub chips: Vec<FeedChip>,
    pub continuation: Option<String>,
    pub visitor_data: Option<String>,
    pub is_fallback_trending: bool,
}

pub fn extract_video_id(input: &str) -> Option<String> {
    let s = input.trim();
    if s.len() == 11
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Some(s.to_owned());
    }
    if let Ok(u) = url::Url::parse(s) {
        let host = u.host_str().unwrap_or_default().to_ascii_lowercase();
        if host.contains("youtu.be") {
            let seg = u.path_segments()?.next()?;
            if seg.len() == 11 {
                return Some(seg.to_owned());
            }
        }
        if host.contains("youtube.com") || host.contains("youtube-nocookie.com") {
            if let Some((_, v)) = u.query_pairs().find(|(k, _)| k == "v") {
                if !v.is_empty() {
                    return Some(v.into_owned());
                }
            }
            let mut segs = u.path_segments()?;
            if let Some(first) = segs.next() {
                if matches!(first, "shorts" | "embed" | "live") {
                    if let Some(id) = segs.next() {
                        if !id.is_empty() {
                            return Some(id.to_owned());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Scrape live INNERTUBE_CLIENT_VERSION and VISITOR_DATA from www.youtube.com.
pub async fn fetch_web_config(
    http: &reqwest::Client,
    session: Option<&Session>,
) -> WebConfig {
    let mut req = http
        .get(ORIGIN)
        .header("User-Agent", WEB_UA)
        .header("Accept-Language", "en-US,en;q=0.9")
        .timeout(Duration::from_secs(10));
    req = match session {
        Some(s) => s.sign(req, ORIGIN),
        None => req.header("Cookie", CONSENT_COOKIE),
    };
    let Ok(resp) = req.send().await else {
        return WebConfig::default();
    };
    let Ok(html) = resp.text().await else {
        return WebConfig::default();
    };

    let client_version = extract_json_str(&html, "\"INNERTUBE_CLIENT_VERSION\":\"")
        .unwrap_or_else(|| FALLBACK_WEB_VERSION.to_owned());
    let visitor_data = extract_json_str(&html, "\"VISITOR_DATA\":\"");

    WebConfig {
        client_version,
        visitor_data,
    }
}

fn extract_json_str(haystack: &str, needle: &str) -> Option<String> {
    let start = haystack.find(needle)? + needle.len();
    let rest = &haystack[start..];
    let end = rest.find('"')?;
    let val = &rest[..end];
    (!val.is_empty()).then(|| val.to_owned())
}

fn web_context(cfg: &WebConfig) -> Value {
    let mut client = json!({
        "clientName": "WEB",
        "clientVersion": cfg.client_version,
        "hl": "en",
        "gl": "US",
        "userAgent": WEB_UA,
    });
    if let Some(vd) = cfg.visitor_data.as_deref().filter(|v| !v.is_empty()) {
        client["visitorData"] = json!(vd);
    }
    json!({ "client": client })
}

async fn post_web(
    http: &reqwest::Client,
    endpoint: &str,
    mut body: Value,
    cfg: &WebConfig,
    session: Option<&Session>,
) -> anyhow::Result<Value> {
    body["context"] = web_context(cfg);
    let url = format!("{ORIGIN}/youtubei/v1/{endpoint}?prettyPrint=false");
    let mut req = http
        .post(url)
        .header("User-Agent", WEB_UA)
        .header("Origin", ORIGIN)
        .header("Referer", format!("{ORIGIN}/"))
        .header("X-YouTube-Client-Name", "1")
        .header("X-YouTube-Client-Version", &cfg.client_version)
        .timeout(Duration::from_secs(12))
        .json(&body);

    if let Some(s) = session {
        req = s.sign(req, ORIGIN);
    } else {
        req = req.header("Cookie", CONSENT_COOKIE);
    }
    if let Some(vd) = cfg.visitor_data.as_deref().filter(|v| !v.is_empty()) {
        req = req.header("X-Goog-Visitor-Id", vd);
    }
    let resp = req.send().await?.error_for_status()?.json().await?;
    Ok(resp)
}

pub async fn home_feed(
    http: &reqwest::Client,
    params: Option<&str>,
    continuation: Option<&str>,
    cfg: &WebConfig,
    session: Option<&Session>,
) -> anyhow::Result<FeedPage> {
    let body = match continuation {
        Some(token) => json!({ "continuation": token }),
        None => {
            let mut b = json!({ "browseId": "FEwhat_to_watch" });
            if let Some(p) = params {
                b["params"] = json!(p);
            }
            b
        }
    };
    let v = post_web(http, "browse", body, cfg, session).await?;
    let new_vd = v["responseContext"]["visitorData"]
        .as_str()
        .map(str::to_owned);
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    collect_videos(&v, &mut items, &mut seen);
    let chips = collect_chips(&v);
    let cont = find_continuation(&v);

    // YouTube returns empty FEwhat_to_watch when signed out; gracefully fall back to Trending
    if items.is_empty() && continuation.is_none() && params.is_none() {
        if let Ok(mut tr) = trending_feed(http, cfg, session).await {
            tr.is_fallback_trending = true;
            return Ok(tr);
        }
    }

    Ok(FeedPage {
        items,
        chips,
        continuation: cont,
        visitor_data: new_vd,
        is_fallback_trending: false,
    })
}

pub async fn subscriptions_feed(
    http: &reqwest::Client,
    continuation: Option<&str>,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<FeedPage> {
    let body = match continuation {
        Some(token) => json!({ "continuation": token }),
        None => json!({ "browseId": "FEsubscriptions" }),
    };
    let v = post_web(http, "browse", body, cfg, Some(session)).await?;
    let new_vd = v["responseContext"]["visitorData"]
        .as_str()
        .map(str::to_owned);
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    collect_videos(&v, &mut items, &mut seen);
    let cont = find_continuation(&v);
    Ok(FeedPage {
        items,
        chips: Vec::new(),
        continuation: cont,
        visitor_data: new_vd,
        is_fallback_trending: false,
    })
}

pub async fn history_feed(
    http: &reqwest::Client,
    continuation: Option<&str>,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<(FeedPage, HashMap<String, String>)> {
    let body = match continuation {
        Some(token) => json!({ "continuation": token }),
        None => json!({ "browseId": "FEhistory" }),
    };
    let v = post_web(http, "browse", body, cfg, Some(session)).await?;
    let new_vd = v["responseContext"]["visitorData"]
        .as_str()
        .map(str::to_owned);
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    collect_videos(&v, &mut items, &mut seen);
    let mut feedback_tokens = HashMap::new();
    collect_history_feedback_tokens(&v, &mut feedback_tokens);
    let cont = find_continuation(&v);
    Ok((
        FeedPage {
            items,
            chips: Vec::new(),
            continuation: cont,
            visitor_data: new_vd,
            is_fallback_trending: false,
        },
        feedback_tokens,
    ))
}

fn find_first_feedback_token(v: &Value) -> Option<String> {
    match v {
        Value::Object(map) => {
            if let Some(tok) = map
                .get("feedbackEndpoint")
                .and_then(|fe| fe.get("feedbackToken"))
                .and_then(|t| t.as_str())
            {
                return Some(tok.to_owned());
            }
            for val in map.values() {
                if let Some(found) = find_first_feedback_token(val) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(arr) => arr.iter().find_map(find_first_feedback_token),
        _ => None,
    }
}

fn collect_history_feedback_tokens(v: &Value, out: &mut HashMap<String, String>) {
    match v {
        Value::Object(map) => {
            if let Some(vr) = map.get("videoRenderer") {
                if let Some(id) = vr["videoId"].as_str() {
                    if let Some(tok) = find_first_feedback_token(&vr["menu"]) {
                        out.entry(id.to_owned()).or_insert(tok);
                    }
                }
            }
            if let Some(lm) = map.get("lockupViewModel") {
                if let Some(id) = lm["contentId"].as_str() {
                    if id.len() == 11 {
                        if let Some(tok) = find_first_feedback_token(
                            &lm["metadata"]["lockupMetadataViewModel"]["menuButton"],
                        )
                        .or_else(|| find_first_feedback_token(lm))
                        {
                            out.entry(id.to_owned()).or_insert(tok);
                        }
                    }
                }
            }
            for val in map.values() {
                collect_history_feedback_tokens(val, out);
            }
        }
        Value::Array(arr) => {
            for val in arr {
                collect_history_feedback_tokens(val, out);
            }
        }
        _ => {}
    }
}

pub async fn remove_from_history(
    http: &reqwest::Client,
    feedback_token: &str,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<()> {
    post_web(
        http,
        "feedback",
        json!({ "feedbackTokens": [feedback_token] }),
        cfg,
        Some(session),
    )
    .await?;
    Ok(())
}

pub async fn fetch_guide_fast(
    http: &reqwest::Client,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<(Vec<SidebarChannel>, Vec<PlaylistSummary>)> {
    let guide_fut = post_web(http, "guide", json!({ "fetchLiveState": true }), cfg, Some(session));
    let pl_fut = post_web(
        http,
        "browse",
        json!({ "browseId": "FEplaylist_aggregations" }),
        cfg,
        Some(session),
    );
    let lib_fut = post_web(
        http,
        "browse",
        json!({ "browseId": "FElibrary" }),
        cfg,
        Some(session),
    );
    let (guide_res, pl_res, lib_res) = tokio::join!(guide_fut, pl_fut, lib_fut);
    let guide_val = guide_res?;
    let (channels, mut playlists) = parse_guide_response(&guide_val);
    if let Ok(mut pl_val) = pl_res {
        merge_playlists_from_browse(&pl_val, &mut playlists);
        for _ in 0..4 {
            let Some(cont) = find_continuation(&pl_val) else {
                break;
            };
            let Ok(next_pl) = post_web(
                http,
                "browse",
                json!({ "continuation": cont }),
                cfg,
                Some(session),
            )
            .await
            else {
                break;
            };
            merge_playlists_from_browse(&next_pl, &mut playlists);
            pl_val = next_pl;
        }
    }
    if let Ok(lib_val) = lib_res {
        merge_playlists_from_browse(&lib_val, &mut playlists);
    }
    Ok((channels, playlists))
}

/// Fetches thumbnail URLs for playlists that lack one by inspecting each playlist's
/// header banner or first video concurrently. Keys in the returned map are stripped
/// of any `VL` prefix so both `GuideLoaded` (`VL...`) and `PlaylistOption` (`PL...`) match.
pub async fn fetch_missing_playlist_thumbs(
    http: &reqwest::Client,
    playlist_ids: Vec<String>,
    cfg: &WebConfig,
    session: &Session,
) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(10));
    let mut set = tokio::task::JoinSet::new();
    let mut seen = HashSet::new();

    for raw in playlist_ids.into_iter().take(200) {
        let stripped = raw.strip_prefix("VL").unwrap_or(&raw).to_owned();
        if stripped.is_empty() || stripped == "WL" || !seen.insert(stripped.clone()) {
            continue;
        }
        let bid = if raw.starts_with("VL") {
            raw
        } else {
            format!("VL{stripped}")
        };
        let http = http.clone();
        let cfg = cfg.clone();
        let session = session.clone();
        let sem = sem.clone();
        set.spawn(async move {
            let _permit = sem.acquire_owned().await.ok()?;
            let v = post_web(&http, "browse", json!({ "browseId": bid }), &cfg, Some(&session))
                .await
                .ok()?;
            let thumb = extract_first_playlist_video_thumb(&v)?;
            Some((stripped, thumb))
        });
    }

    while let Some(res) = set.join_next().await {
        if let Ok(Some((pid, thumb))) = res {
            out.insert(pid, thumb);
        }
    }
    out
}

/// Resolves channel avatar URLs concurrently from `browse(UC...)` for feed cards
/// whose renderer (e.g. `playlistVideoRenderer`) omits channel thumbnails.
pub async fn fetch_channel_avatars(
    http: &reqwest::Client,
    channel_ids: Vec<String>,
    cfg: &WebConfig,
    session: Option<&Session>,
) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(8));
    let mut set = tokio::task::JoinSet::new();
    let mut seen = HashSet::new();

    for cid in channel_ids.into_iter().take(36) {
        if !cid.starts_with("UC") || !seen.insert(cid.clone()) {
            continue;
        }
        let http = http.clone();
        let cfg = cfg.clone();
        let sess = session.cloned();
        let sem = sem.clone();
        set.spawn(async move {
            let _permit = sem.acquire_owned().await.ok()?;
            let v = post_web(
                &http,
                "browse",
                json!({ "browseId": cid }),
                &cfg,
                sess.as_ref(),
            )
            .await
            .ok()?;
            let avatar = extract_channel_avatar_from_browse(&v)?;
            Some((cid, avatar))
        });
    }

    while let Some(res) = set.join_next().await {
        if let Ok(Some((cid, avatar))) = res {
            out.insert(cid, avatar);
        }
    }
    out
}

fn extract_channel_avatar_from_browse(v: &Value) -> Option<String> {
    let c4 = &v["header"]["c4TabbedHeaderRenderer"];
    let ph = &v["header"]["pageHeaderRenderer"]["content"]["pageHeaderViewModel"];
    c4["avatar"]["thumbnails"]
        .as_array()
        .and_then(|a| a.last())
        .and_then(|t| t["url"].as_str())
        .map(normalize_url)
        .or_else(|| find_first_thumb_url(&ph["image"]))
        .or_else(|| find_first_thumb_url(&v["metadata"]["channelMetadataRenderer"]["avatar"]))
}

fn extract_first_playlist_video_thumb(v: &Value) -> Option<String> {
    if let Some(u) = find_first_thumb_url(
        &v["header"]["playlistHeaderRenderer"]["playlistHeaderBanner"]
            ["heroPlaylistThumbnailRenderer"]["thumbnail"],
    )
    .or_else(|| {
        find_first_thumb_url(
            &v["header"]["pageHeaderRenderer"]["content"]["pageHeaderViewModel"]["heroImage"],
        )
    }) {
        return Some(u);
    }

    fn walk(v: &Value) -> Option<String> {
        match v {
            Value::Object(map) => {
                for key in [
                    "playlistVideoRenderer",
                    "playlistPanelVideoRenderer",
                    "gridVideoRenderer",
                    "videoRenderer",
                ] {
                    if let Some(item) = map.get(key) {
                        if let Some(u) = find_first_thumb_url(&item["thumbnail"]) {
                            return Some(u);
                        }
                        if let Some(vid) = item["videoId"].as_str() {
                            return Some(format!("https://i.ytimg.com/vi/{vid}/mqdefault.jpg"));
                        }
                    }
                }
                if let Some(lm) = map.get("lockupViewModel") {
                    if let Some(u) = find_first_thumb_url(&lm["contentImage"]) {
                        return Some(u);
                    }
                    if let Some(vid) = lm["contentId"].as_str() {
                        if vid.len() == 11 {
                            return Some(format!("https://i.ytimg.com/vi/{vid}/mqdefault.jpg"));
                        }
                    }
                }
                for val in map.values() {
                    if let Some(u) = walk(val) {
                        return Some(u);
                    }
                }
                None
            }
            Value::Array(arr) => arr.iter().find_map(walk),
            _ => None,
        }
    }
    walk(v)
}

fn find_first_thumb_url(v: &Value) -> Option<String> {
    match v {
        Value::Object(map) => {
            for key in ["sources", "thumbnails"] {
                if let Some(arr) = map.get(key).and_then(Value::as_array) {
                    if let Some(u) = arr.last().and_then(|t| t["url"].as_str()) {
                        return Some(normalize_url(u));
                    }
                }
            }
            for val in map.values() {
                if let Some(u) = find_first_thumb_url(val) {
                    return Some(u);
                }
            }
            None
        }
        Value::Array(arr) => {
            for val in arr {
                if let Some(u) = find_first_thumb_url(val) {
                    return Some(u);
                }
            }
            None
        }
        _ => None,
    }
}

fn upsert_playlist(
    out: &mut Vec<PlaylistSummary>,
    bid: String,
    title: String,
    subtitle: String,
    thumbnail_url: Option<String>,
) {
    if let Some(existing) = out.iter_mut().find(|p| p.browse_id == bid) {
        if existing.thumbnail_url.is_none() {
            existing.thumbnail_url = thumbnail_url;
        }
        if existing.subtitle.is_empty() && !subtitle.is_empty() {
            existing.subtitle = subtitle;
        }
    } else {
        out.push(PlaylistSummary {
            title,
            browse_id: bid,
            subtitle,
            thumbnail_url,
        });
    }
}

fn merge_playlists_from_browse(v: &Value, out: &mut Vec<PlaylistSummary>) {
    fn walk(v: &Value, out: &mut Vec<PlaylistSummary>) {
        match v {
            Value::Object(map) => {
                if let Some(gp) = map.get("gridPlaylistRenderer") {
                    if let (Some(pid), Some(title)) =
                        (gp["playlistId"].as_str(), text(&gp["title"]))
                    {
                        if pid != "WL" && pid != "LL" {
                            let bid = format!("VL{pid}");
                            let subtitle = text(&gp["videoCountShortText"])
                                .or_else(|| text(&gp["videoCountText"]))
                                .unwrap_or_default();
                            let thumbnail_url = find_first_thumb_url(&gp["thumbnail"])
                                .or_else(|| find_first_thumb_url(gp));
                            upsert_playlist(out, bid, title, subtitle, thumbnail_url);
                        }
                    }
                } else if let Some(pl) = map.get("playlistRenderer") {
                    if let (Some(pid), Some(title)) =
                        (pl["playlistId"].as_str(), text(&pl["title"]))
                    {
                        if pid != "WL" && pid != "LL" {
                            let bid = format!("VL{pid}");
                            let subtitle = text(&pl["videoCountText"]).unwrap_or_default();
                            let thumbnail_url = find_first_thumb_url(&pl["thumbnails"])
                                .or_else(|| find_first_thumb_url(pl));
                            upsert_playlist(out, bid, title, subtitle, thumbnail_url);
                        }
                    }
                } else if let Some(lm) = map.get("lockupViewModel") {
                    let ctype = lm["contentType"].as_str().unwrap_or_default();
                    if ctype.contains("PLAYLIST") {
                        if let (Some(pid), Some(title)) = (
                            lm["contentId"].as_str(),
                            lm["metadata"]["lockupMetadataViewModel"]["title"]["content"].as_str(),
                        ) {
                            if pid != "WL" && pid != "LL" {
                                let bid = if pid.starts_with("VL") {
                                    pid.to_owned()
                                } else {
                                    format!("VL{pid}")
                                };
                                let subtitle = lm["metadata"]["lockupMetadataViewModel"]["metadata"]
                                    ["contentMetadataViewModel"]["metadataRows"]
                                    .as_array()
                                    .and_then(|rows| rows.last())
                                    .and_then(|r| r["metadataParts"].as_array())
                                    .and_then(|parts| parts.first())
                                    .and_then(|p| p["text"]["content"].as_str())
                                    .unwrap_or("")
                                    .to_owned();
                                let thumbnail_url = find_first_thumb_url(&lm["contentImage"]);
                                upsert_playlist(
                                    out,
                                    bid,
                                    title.to_owned(),
                                    subtitle,
                                    thumbnail_url,
                                );
                            }
                        }
                    }
                }
                for val in map.values() {
                    walk(val, out);
                }
            }
            Value::Array(arr) => {
                for val in arr {
                    walk(val, out);
                }
            }
            _ => {}
        }
    }
    walk(v, out);
}

fn parse_guide_response(v: &Value) -> (Vec<SidebarChannel>, Vec<PlaylistSummary>) {
    let mut channels = Vec::new();
    let mut playlists = Vec::new();
    let mut seen_ch = HashSet::new();
    let mut seen_pl = HashSet::new();

    fn walk_guide(
        node: &Value,
        in_subs: bool,
        channels: &mut Vec<SidebarChannel>,
        playlists: &mut Vec<PlaylistSummary>,
        seen_ch: &mut HashSet<String>,
        seen_pl: &mut HashSet<String>,
    ) {
        match node {
            Value::Object(map) => {
                if let Some(sub_sec) = map.get("guideSubscriptionsSectionRenderer") {
                    walk_guide(sub_sec, true, channels, playlists, seen_ch, seen_pl);
                    return;
                }
                if let Some(entry) = map.get("guideEntryRenderer") {
                    let browse_id = entry["navigationEndpoint"]["browseEndpoint"]["browseId"]
                        .as_str()
                        .unwrap_or_default();
                    let title = text(&entry["formattedTitle"])
                        .or_else(|| text(&entry["title"]))
                        .unwrap_or_default();
                    if !browse_id.is_empty() && !title.is_empty() {
                        if in_subs && browse_id.starts_with("UC") {
                            if seen_ch.insert(browse_id.to_owned()) {
                                let avatar_url = entry["thumbnail"]["thumbnails"]
                                    .as_array()
                                    .and_then(|a| a.last())
                                    .and_then(|t| t["url"].as_str())
                                    .map(normalize_url);
                                let is_live = entry["badges"]["liveBroadcasting"]
                                    .as_bool()
                                    .unwrap_or(false);
                                channels.push(SidebarChannel {
                                    name: title,
                                    browse_id: browse_id.to_owned(),
                                    avatar_url,
                                    is_live,
                                });
                            }
                        } else if let Some(pl_id) = browse_id.strip_prefix("VL") {
                            if pl_id != "WL" && pl_id != "LL" && seen_pl.insert(browse_id.to_owned()) {
                                let subtitle = text(&entry["subtitle"]).unwrap_or_default();
                                let thumbnail_url = entry["thumbnail"]["thumbnails"]
                                    .as_array()
                                    .and_then(|a| a.last())
                                    .and_then(|t| t["url"].as_str())
                                    .map(normalize_url);
                                playlists.push(PlaylistSummary {
                                    title,
                                    browse_id: browse_id.to_owned(),
                                    subtitle,
                                    thumbnail_url,
                                });
                            }
                        }
                    }
                }
                for val in map.values() {
                    walk_guide(val, in_subs, channels, playlists, seen_ch, seen_pl);
                }
            }
            Value::Array(arr) => {
                for val in arr {
                    walk_guide(val, in_subs, channels, playlists, seen_ch, seen_pl);
                }
            }
            _ => {}
        }
    }

    walk_guide(&v["items"], false, &mut channels, &mut playlists, &mut seen_ch, &mut seen_pl);
    (channels, playlists)
}

pub async fn playlist_feed(
    http: &reqwest::Client,
    browse_id: &str,
    continuation: Option<&str>,
    cfg: &WebConfig,
    session: Option<&Session>,
) -> anyhow::Result<(String, bool, FeedPage)> {
    let body = match continuation {
        Some(token) => json!({ "continuation": token }),
        None => json!({ "browseId": browse_id }),
    };
    let v = post_web(http, "browse", body, cfg, session).await?;
    let new_vd = v["responseContext"]["visitorData"]
        .as_str()
        .map(str::to_owned);
    let title = v["metadata"]["playlistMetadataRenderer"]["title"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| text(&v["header"]["playlistHeaderRenderer"]["title"]))
        .or_else(|| text(&v["header"]["pageHeaderRenderer"]["pageTitle"]))
        .unwrap_or_else(|| match browse_id {
            "VLWL" => "Watch Later".to_owned(),
            "VLLL" => "Liked Videos".to_owned(),
            _ => "Playlist".to_owned(),
        });
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    collect_videos(&v, &mut items, &mut seen);
    let cont = find_continuation(&v);
    let is_editable = browse_id != "VLLL"
        && session.is_some()
        && (v["header"]["playlistHeaderRenderer"]["isEditable"]
            .as_bool()
            .unwrap_or(false)
            || find_nested_bool(&v, "isEditable").unwrap_or(false)
            || browse_id == "VLWL"
            || items.iter().any(|it| it.playlist_set_video_id.is_some()));
    Ok((
        title,
        is_editable,
        FeedPage {
            items,
            chips: Vec::new(),
            continuation: cont,
            visitor_data: new_vd,
            is_fallback_trending: false,
        },
    ))
}

fn find_nested_bool(v: &Value, key: &str) -> Option<bool> {
    match v {
        Value::Object(map) => {
            if let Some(b) = map.get(key).and_then(|x| x.as_bool()) {
                return Some(b);
            }
            for val in map.values() {
                if let Some(b) = find_nested_bool(val, key) {
                    return Some(b);
                }
            }
            None
        }
        Value::Array(arr) => arr.iter().find_map(|val| find_nested_bool(val, key)),
        _ => None,
    }
}

pub async fn set_watch_later(
    http: &reqwest::Client,
    video_id: &str,
    add: bool,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<()> {
    edit_playlist(http, "WL", video_id, add, cfg, session).await
}

pub async fn edit_playlist(
    http: &reqwest::Client,
    playlist_id: &str,
    video_id: &str,
    add: bool,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<()> {
    let pid = playlist_id.strip_prefix("VL").unwrap_or(playlist_id);
    let action = if add {
        json!({
            "action": "ACTION_ADD_VIDEO",
            "addedVideoId": video_id,
        })
    } else {
        json!({
            "action": "ACTION_REMOVE_VIDEO_BY_VIDEO_ID",
            "removedVideoId": video_id,
        })
    };
    post_web(
        http,
        "browse/edit_playlist",
        json!({
            "playlistId": pid,
            "actions": [action],
        }),
        cfg,
        Some(session),
    )
    .await?;
    Ok(())
}

pub async fn remove_playlist_item(
    http: &reqwest::Client,
    playlist_id: &str,
    video_id: &str,
    set_video_id: Option<&str>,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<()> {
    let pid = playlist_id.strip_prefix("VL").unwrap_or(playlist_id);
    let action = match set_video_id.filter(|s| !s.is_empty()) {
        Some(sid) => json!({
            "action": "ACTION_REMOVE_VIDEO",
            "setVideoId": sid,
        }),
        None => json!({
            "action": "ACTION_REMOVE_VIDEO_BY_VIDEO_ID",
            "removedVideoId": video_id,
        }),
    };
    post_web(
        http,
        "browse/edit_playlist",
        json!({
            "playlistId": pid,
            "actions": [action],
        }),
        cfg,
        Some(session),
    )
    .await?;
    Ok(())
}

pub async fn reorder_playlist_item(
    http: &reqwest::Client,
    playlist_id: &str,
    set_video_id: &str,
    successor_set_video_id: Option<&str>,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<()> {
    let pid = playlist_id.strip_prefix("VL").unwrap_or(playlist_id);
    let mut action = json!({
        "action": "ACTION_MOVE_VIDEO_BEFORE",
        "setVideoId": set_video_id,
    });
    if let Some(succ) = successor_set_video_id.filter(|s| !s.is_empty()) {
        action["movedSetVideoIdSuccessor"] = json!(succ);
    }
    post_web(
        http,
        "browse/edit_playlist",
        json!({
            "playlistId": pid,
            "actions": [action],
        }),
        cfg,
        Some(session),
    )
    .await?;
    Ok(())
}

pub async fn get_add_to_playlist(
    http: &reqwest::Client,
    video_id: &str,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<Vec<PlaylistOption>> {
    let v = post_web(
        http,
        "playlist/get_add_to_playlist",
        json!({ "videoIds": [video_id] }),
        cfg,
        Some(session),
    )
    .await?;
    Ok(parse_add_to_playlist(&v))
}

fn parse_add_to_playlist(v: &Value) -> Vec<PlaylistOption> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    fn walk(v: &Value, out: &mut Vec<PlaylistOption>, seen: &mut HashSet<String>) {
        match v {
            Value::Object(map) => {
                if let Some(opt) = map.get("playlistAddToOptionRenderer") {
                    if let (Some(pid), Some(title)) =
                        (opt["playlistId"].as_str(), text(&opt["title"]))
                    {
                        if seen.insert(pid.to_owned()) {
                            let status = opt["containsSelectedVideos"].as_str().unwrap_or("");
                            let is_added = matches!(status, "ALL" | "SOME");
                            let thumbnail_url = find_first_thumb_url(&opt["thumbnail"])
                                .or_else(|| find_first_thumb_url(opt));
                            out.push(PlaylistOption {
                                id: pid.to_owned(),
                                title,
                                is_added,
                                thumbnail_url,
                            });
                        }
                    }
                }
                for val in map.values() {
                    walk(val, out, seen);
                }
            }
            Value::Array(arr) => {
                for val in arr {
                    walk(val, out, seen);
                }
            }
            _ => {}
        }
    }
    walk(v, &mut out, &mut seen);
    out
}

pub async fn create_playlist(
    http: &reqwest::Client,
    title: &str,
    video_ids: &[String],
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<()> {
    let mut body = json!({
        "title": title,
        "privacyStatus": "PRIVATE",
    });
    if !video_ids.is_empty() {
        body["videoIds"] = json!(video_ids);
    }
    post_web(http, "playlist/create", body, cfg, Some(session)).await?;
    Ok(())
}

pub async fn search_suggestions(
    http: &reqwest::Client,
    query: &str,
) -> anyhow::Result<Vec<String>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let resp: Value = http
        .get("https://suggestqueries-clients6.youtube.com/complete/search")
        .query(&[("client", "firefox"), ("ds", "yt"), ("q", q)])
        .header("User-Agent", WEB_UA)
        .timeout(Duration::from_secs(4))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let list = resp
        .get(1)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .take(8)
                .collect()
        })
        .unwrap_or_default();
    Ok(list)
}

#[derive(Clone, Debug)]
pub struct WatchtimeHandle {
    pub url: Option<String>,
    pub cpn: String,
    pub flush_at: Vec<u64>,
    pub flush_every: u64,
}

fn generate_cpn() -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut x = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0x9e37_79b9_7f4a_7c15, |d| d.as_nanos() as u64)
        | 1;
    (0..16)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            ALPHABET[(x & 63) as usize] as char
        })
        .collect()
}

/// Fetch YouTube web player's signatureTimestamp (`sts`), cached for 24 hours on disk.
pub async fn fetch_signature_timestamp(
    http: &reqwest::Client,
    data_dir: &std::path::Path,
) -> Option<u32> {
    let cache_path = data_dir.join("player_sts.txt");
    if let Ok(meta) = std::fs::metadata(&cache_path) {
        if let Ok(mod_time) = meta.modified() {
            if std::time::SystemTime::now()
                .duration_since(mod_time)
                .unwrap_or_default()
                < Duration::from_secs(24 * 3600)
            {
                if let Ok(txt) = std::fs::read_to_string(&cache_path) {
                    if let Ok(sts) = txt.trim().parse::<u32>() {
                        return Some(sts);
                    }
                }
            }
        }
    }

    let iframe = http
        .get("https://www.youtube.com/iframe_api")
        .header("User-Agent", WEB_UA)
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let marker = "/s/player/";
    let start = iframe.find(marker)? + marker.len();
    let pid = iframe[start..].split('/').next()?;
    if pid.is_empty() {
        return None;
    }
    let js_url = format!("https://www.youtube.com/s/player/{pid}/player_ias.vflset/en_US/base.js");
    let js = http
        .get(js_url)
        .header("User-Agent", WEB_UA)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let sts_key = "signatureTimestamp:";
    let idx = js.find(sts_key)? + sts_key.len();
    let digits: String = js[idx..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let sts: u32 = digits.parse().ok()?;
    let _ = std::fs::write(&cache_path, sts.to_string());
    Some(sts)
}

/// Tell YouTube the signed-in account started playing `video_id`, so it appears in Watch History.
pub async fn report_play(
    http: &reqwest::Client,
    video_id: &str,
    cfg: &WebConfig,
    session: &Session,
    sts: Option<u32>,
) -> anyhow::Result<WatchtimeHandle> {
    let mut body = json!({
        "videoId": video_id,
        "contentCheckOk": true,
        "racyCheckOk": true,
    });
    if let Some(sts) = sts {
        body["playbackContext"] =
            json!({ "contentPlaybackContext": { "signatureTimestamp": sts } });
    }
    let player = post_web(http, "player", body, cfg, Some(session)).await?;
    let tracking = &player["playbackTracking"];
    let base_url = tracking["videostatsPlaybackUrl"]["baseUrl"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no videostatsPlaybackUrl in signed-in WEB response"))?;
    let cpn = generate_cpn();
    let mut url = url::Url::parse(base_url)?;
    url.query_pairs_mut()
        .append_pair("ver", "2")
        .append_pair("c", "WEB")
        .append_pair("cpn", &cpn);

    let req = session
        .sign(
            http.get(url)
                .header("User-Agent", WEB_UA)
                .header("Origin", ORIGIN)
                .header("Referer", format!("{ORIGIN}/"))
                .timeout(Duration::from_secs(8)),
            ORIGIN,
        );
    req.send().await?.error_for_status()?;

    let flush_at = tracking["videostatsScheduledFlushWalltimeSeconds"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_else(|| vec![10, 20, 30]);
    let flush_every = tracking["videostatsDefaultFlushIntervalSeconds"]
        .as_u64()
        .filter(|&s| s > 0)
        .unwrap_or(40);

    Ok(WatchtimeHandle {
        url: tracking["videostatsWatchtimeUrl"]["baseUrl"]
            .as_str()
            .map(str::to_owned),
        cpn,
        flush_at,
        flush_every,
    })
}

/// Report playback progress (`from`..`to` seconds) to YouTube so resume position & history stay synced.
pub async fn report_watchtime(
    http: &reqwest::Client,
    w: &WatchtimeHandle,
    session: &Session,
    from: f64,
    to: f64,
    elapsed: f64,
    is_final: bool,
) -> anyhow::Result<()> {
    let Some(base_url) = &w.url else {
        return Ok(());
    };
    let st = format!("{from:.3}");
    let et = format!("{to:.3}");
    let rt = format!("{elapsed:.3}");
    let state = if is_final { "paused" } else { "playing" };

    let mut url = url::Url::parse(base_url)?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("ver", "2")
            .append_pair("c", "WEB")
            .append_pair("cpn", &w.cpn)
            .append_pair("cmt", &et)
            .append_pair("st", &st)
            .append_pair("et", &et)
            .append_pair("rt", &rt)
            .append_pair("state", state);
        if is_final {
            qp.append_pair("final", "1");
        }
    }

    let req = session.sign(
        http.get(url)
            .header("User-Agent", WEB_UA)
            .header("Origin", ORIGIN)
            .header("Referer", format!("{ORIGIN}/"))
            .timeout(Duration::from_secs(8)),
        ORIGIN,
    );
    req.send().await?.error_for_status()?;
    Ok(())
}

pub async fn trending_feed(
    http: &reqwest::Client,
    cfg: &WebConfig,
    session: Option<&Session>,
) -> anyhow::Result<FeedPage> {
    for browse_id in ["FEexplore", "FEtrending"] {
        if let Ok(v) = post_web(http, "browse", json!({ "browseId": browse_id }), cfg, session).await
        {
            let new_vd = v["responseContext"]["visitorData"]
                .as_str()
                .map(str::to_owned);
            let mut items = Vec::new();
            let mut seen = HashSet::new();
            collect_videos(&v, &mut items, &mut seen);
            if !items.is_empty() {
                let cont = find_continuation(&v);
                return Ok(FeedPage {
                    items,
                    chips: Vec::new(),
                    continuation: cont,
                    visitor_data: new_vd,
                    is_fallback_trending: false,
                });
            }
        }
    }
    search(http, "trending videos 2026", None, cfg, session).await
}

pub async fn search(
    http: &reqwest::Client,
    query: &str,
    continuation: Option<&str>,
    cfg: &WebConfig,
    session: Option<&Session>,
) -> anyhow::Result<FeedPage> {
        let body = match continuation {
            Some(token) => json!({ "continuation": token }),
            None => json!({ "query": query, "params": "EgIQAQ%3D%3D" }),
        };
        let v = post_web(http, "search", body, cfg, session).await?;
        let new_vd = v["responseContext"]["visitorData"]
            .as_str()
            .map(str::to_owned);
        let mut items = Vec::new();
        let mut seen = HashSet::new();
        collect_videos(&v, &mut items, &mut seen);
        let cont = find_continuation(&v);
        Ok(FeedPage {
            items,
            chips: Vec::new(),
            continuation: cont,
            visitor_data: new_vd,
            is_fallback_trending: false,
        })
    }

pub async fn channel_videos(
    http: &reqwest::Client,
    channel_id: &str,
    params: Option<&str>,
    continuation: Option<&str>,
    cfg: &WebConfig,
    session: Option<&Session>,
) -> anyhow::Result<(ChannelInfo, FeedPage)> {
    // EgZ2aWRlb3PyBgQKAjoA selects the Videos tab on a channel page when no tab params specified
    let body = match continuation {
        Some(token) => json!({ "continuation": token }),
        None => json!({
            "browseId": channel_id,
            "params": params.unwrap_or("EgZ2aWRlb3PyBgQKAjoA"),
        }),
    };
    let v = post_web(http, "browse", body, cfg, session).await?;
    let new_vd = v["responseContext"]["visitorData"]
        .as_str()
        .map(str::to_owned);
    let channel_name = v["metadata"]["channelMetadataRenderer"]["title"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| text(&v["header"]["c4TabbedHeaderRenderer"]["title"]))
        .or_else(|| {
            v["header"]["pageHeaderRenderer"]["pageTitle"]
                .as_str()
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Channel".to_owned());

    let c4 = &v["header"]["c4TabbedHeaderRenderer"];
    let ph = &v["header"]["pageHeaderRenderer"]["content"]["pageHeaderViewModel"];
    let avatar_url = c4["avatar"]["thumbnails"]
        .as_array()
        .and_then(|a| a.last())
        .and_then(|t| t["url"].as_str())
        .or_else(|| {
            ph["image"]["decoratedAvatarViewModel"]["avatar"]["avatarViewModel"]["image"]["sources"]
                .as_array()
                .and_then(|a| a.last())
                .and_then(|s| s["url"].as_str())
        })
        .map(normalize_url);
    let banner_url = c4["banner"]["thumbnails"]
        .as_array()
        .and_then(|a| a.last())
        .and_then(|t| t["url"].as_str())
        .or_else(|| {
            ph["banner"]["imageBannerViewModel"]["image"]["sources"]
                .as_array()
                .and_then(|a| a.last())
                .and_then(|s| s["url"].as_str())
        })
        .map(normalize_url);
    let mut handle = text(&c4["channelHandleText"]).unwrap_or_default();
    let mut sub_parts = Vec::new();
    if let Some(sub) = text(&c4["subscriberCountText"]) {
        sub_parts.push(sub);
    }
    if let Some(vc) = text(&c4["videosCountText"]) {
        sub_parts.push(vc);
    }
    if sub_parts.is_empty() || handle.is_empty() {
        if let Some(rows) = ph["metadata"]["contentMetadataViewModel"]["metadataRows"].as_array() {
            for r in rows {
                if let Some(parts) = r["metadataParts"].as_array() {
                    for p in parts {
                        if let Some(s) = p["text"]["content"].as_str() {
                            if handle.is_empty() && s.starts_with('@') {
                                handle = s.to_owned();
                            } else if (s.contains("subscriber") || s.contains("video"))
                                && !sub_parts.iter().any(|x| x == s)
                            {
                                sub_parts.push(s.to_owned());
                            }
                        }
                    }
                }
            }
        }
    }
    let subscribers_text = sub_parts.join(" · ");
    let is_subscribed = find_is_subscribed(&v["header"]);
    let tabs = parse_channel_tabs(&v);

    let mut items = Vec::new();
    let mut seen = HashSet::new();
    collect_videos_with_channel_items(&v, &mut items, &mut seen, &channel_name, channel_id);
    for it in &mut items {
        if it.channel_avatar_url.is_none() {
            it.channel_avatar_url = avatar_url.clone();
        }
    }
    let cont = find_continuation(&v);
    Ok((
        ChannelInfo {
            id: channel_id.to_owned(),
            name: channel_name,
            handle,
            avatar_url,
            banner_url,
            subscribers_text,
            is_subscribed,
            tabs,
        },
        FeedPage {
            items,
            chips: Vec::new(),
            continuation: cont,
            visitor_data: new_vd,
            is_fallback_trending: false,
        },
    ))
}

pub async fn search_channel(
    http: &reqwest::Client,
    channel_id: &str,
    query: &str,
    continuation: Option<&str>,
    cfg: &WebConfig,
    session: Option<&Session>,
) -> anyhow::Result<FeedPage> {
    let body = match continuation {
        Some(token) => json!({ "continuation": token }),
        None => json!({
            "browseId": channel_id,
            "params": "EgZzZWFyY2jyBgQKAloA",
            "query": query,
        }),
    };
    let v = post_web(http, "browse", body, cfg, session).await?;
    let new_vd = v["responseContext"]["visitorData"]
        .as_str()
        .map(str::to_owned);
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    collect_videos_with_channel_items(&v, &mut items, &mut seen, "", channel_id);
    let cont = find_continuation(&v);
    Ok(FeedPage {
        items,
        chips: Vec::new(),
        continuation: cont,
        visitor_data: new_vd,
        is_fallback_trending: false,
    })
}

fn find_is_subscribed(v: &Value) -> bool {
    match v {
        Value::Object(map) => {
            if let Some(b) = map.get("subscribed").and_then(|x| x.as_bool()) {
                return b;
            }
            if let Some(b) = map.get("isSubscribed").and_then(|x| x.as_bool()) {
                return b;
            }
            map.values().any(find_is_subscribed)
        }
        Value::Array(arr) => arr.iter().any(find_is_subscribed),
        _ => false,
    }
}

fn parse_channel_tabs(v: &Value) -> Vec<ChannelTab> {
    let mut tabs = Vec::new();
    let Some(arr) = v["contents"]["twoColumnBrowseResultsRenderer"]["tabs"].as_array() else {
        return tabs;
    };
    for t in arr {
        let tr = &t["tabRenderer"];
        if !tr.is_object() {
            continue;
        }
        let Some(title) = tr["title"].as_str() else { continue };
        let lower = title.to_ascii_lowercase();
        if !matches!(
            lower.as_str(),
            "home" | "videos" | "shorts" | "live" | "playlists" | "podcasts" | "releases"
        ) {
            continue;
        }
        let params = tr["endpoint"]["browseEndpoint"]["params"]
            .as_str()
            .map(str::to_owned);
        let selected = tr["selected"].as_bool().unwrap_or(false);
        tabs.push(ChannelTab {
            title: title.to_owned(),
            params,
            selected,
        });
    }
    tabs
}

fn collect_videos_with_channel_items(
    v: &Value,
    out: &mut Vec<VideoItem>,
    seen: &mut HashSet<String>,
    channel_name: &str,
    channel_id: &str,
) {
    fn walk(
        v: &Value,
        out: &mut Vec<VideoItem>,
        seen: &mut HashSet<String>,
        channel_name: &str,
        channel_id: &str,
    ) {
        match v {
            Value::Object(map) => {
                if let Some(vr) = map
                    .get("videoRenderer")
                    .or_else(|| map.get("compactVideoRenderer"))
                    .or_else(|| map.get("gridVideoRenderer"))
                {
                    if let Some(mut item) = parse_video_renderer(vr) {
                        if item.channel.is_empty() {
                            item.channel = channel_name.to_owned();
                        }
                        if item.channel_id.is_none() {
                            item.channel_id = Some(channel_id.to_owned());
                        }
                        if seen.insert(item.id.clone()) {
                            out.push(item);
                        }
                    }
                } else if let Some(lm) = map.get("lockupViewModel") {
                    let ctype = lm["contentType"].as_str().unwrap_or_default();
                    if ctype.contains("PLAYLIST") {
                        if let (Some(pid), Some(title)) = (
                            lm["contentId"].as_str(),
                            lm["metadata"]["lockupMetadataViewModel"]["title"]["content"].as_str(),
                        ) {
                            let bid = if pid.starts_with("VL") {
                                pid.to_owned()
                            } else {
                                format!("VL{pid}")
                            };
                            if seen.insert(bid.clone()) {
                                let thumb = lm["contentImage"]["collectionThumbnailViewModel"]
                                    ["primaryThumbnail"]["thumbnailViewModel"]["image"]["sources"]
                                    .as_array()
                                    .or_else(|| {
                                        lm["contentImage"]["thumbnailViewModel"]["image"]["sources"]
                                            .as_array()
                                    })
                                    .and_then(|a| a.last())
                                    .and_then(|s| s["url"].as_str())
                                    .map(normalize_url)
                                    .unwrap_or_default();
                                out.push(VideoItem {
                                    id: bid.clone(),
                                    title: title.to_owned(),
                                    channel: channel_name.to_owned(),
                                    channel_id: Some(channel_id.to_owned()),
                                    channel_avatar_url: None,
                                    views_text: "Playlist".to_owned(),
                                    published_text: String::new(),
                                    duration_text: "Playlist".to_owned(),
                                    thumbnail_url: thumb,
                                    is_live: false,
                                    watched_percent: None,
                                    resume_secs: None,
                                    playlist_browse_id: Some(bid),
                                    playlist_set_video_id: None,
                                });
                            }
                        }
                    } else if let Some(mut item) = parse_lockup_view_model(lm) {
                        if item.channel.is_empty() {
                            item.channel = channel_name.to_owned();
                        }
                        if item.channel_id.is_none() {
                            item.channel_id = Some(channel_id.to_owned());
                        }
                        if seen.insert(item.id.clone()) {
                            out.push(item);
                        }
                    }
                } else if let Some(sl) = map.get("shortsLockupViewModel") {
                    let vid = sl["onTap"]["innertubeCommand"]["reelWatchEndpoint"]["videoId"]
                        .as_str()
                        .or_else(|| {
                            sl["entityId"]
                                .as_str()
                                .and_then(|s| s.strip_prefix("shorts-shelf-item-"))
                        });
                    if let Some(vid) = vid.filter(|s| s.len() == 11) {
                        if seen.insert(vid.to_owned()) {
                            let title = sl["overlayMetadata"]["primaryText"]["content"]
                                .as_str()
                                .or_else(|| sl["accessibilityText"].as_str())
                                .unwrap_or("Short")
                                .to_owned();
                            let views = sl["overlayMetadata"]["secondaryText"]["content"]
                                .as_str()
                                .unwrap_or("")
                                .to_owned();
                            let thumb = sl["thumbnail"]["sources"]
                                .as_array()
                                .and_then(|a| a.last())
                                .and_then(|s| s["url"].as_str())
                                .map(normalize_url)
                                .unwrap_or_else(|| {
                                    format!("https://i.ytimg.com/vi/{vid}/mqdefault.jpg")
                                });
                            out.push(VideoItem {
                                id: vid.to_owned(),
                                title,
                                channel: channel_name.to_owned(),
                                channel_id: Some(channel_id.to_owned()),
                                channel_avatar_url: None,
                                views_text: views,
                                published_text: String::new(),
                                duration_text: "Short".to_owned(),
                                thumbnail_url: thumb,
                                is_live: false,
                                watched_percent: None,
                                resume_secs: None,
                                playlist_browse_id: None,
                                playlist_set_video_id: None,
                            });
                        }
                    }
                } else if let Some(gp) = map.get("gridPlaylistRenderer") {
                    if let (Some(pid), Some(title)) =
                        (gp["playlistId"].as_str(), text(&gp["title"]))
                    {
                        let bid = format!("VL{pid}");
                        if seen.insert(bid.clone()) {
                            let count = text(&gp["videoCountShortText"])
                                .or_else(|| text(&gp["videoCountText"]))
                                .unwrap_or_else(|| "Playlist".to_owned());
                            let thumb = gp["thumbnail"]["thumbnails"]
                                .as_array()
                                .and_then(|a| a.last())
                                .and_then(|t| t["url"].as_str())
                                .map(normalize_url)
                                .unwrap_or_default();
                            out.push(VideoItem {
                                id: bid.clone(),
                                title,
                                channel: channel_name.to_owned(),
                                channel_id: Some(channel_id.to_owned()),
                                channel_avatar_url: None,
                                views_text: count,
                                published_text: String::new(),
                                duration_text: "Playlist".to_owned(),
                                thumbnail_url: thumb,
                                is_live: false,
                                watched_percent: None,
                                resume_secs: None,
                                playlist_browse_id: Some(bid),
                                playlist_set_video_id: None,
                            });
                        }
                    }
                }
                for val in map.values() {
                    walk(val, out, seen, channel_name, channel_id);
                }
            }
            Value::Array(arr) => {
                for val in arr {
                    walk(val, out, seen, channel_name, channel_id);
                }
            }
            _ => {}
        }
    }
    walk(v, out, seen, channel_name, channel_id);
}

pub struct CommentPage {
    pub comments: Vec<CommentItem>,
    pub next_token: Option<String>,
    pub sort_options: Vec<(String, String)>,
    pub create_params: Option<String>,
}

fn find_nested_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    match v {
        Value::Object(map) => {
            if let Some(s) = map.get(key).and_then(|x| x.as_str()) {
                return Some(s);
            }
            for val in map.values() {
                if let Some(found) = find_nested_str(val, key) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(arr) => arr.iter().find_map(|val| find_nested_str(val, key)),
        _ => None,
    }
}

fn parse_comment_entry(
    entry: &Value,
    entity_map: &std::collections::HashMap<String, Value>,
    surface_map: &std::collections::HashMap<String, Value>,
) -> Option<CommentItem> {
    let thread = if entry["commentThreadRenderer"].is_object() {
        &entry["commentThreadRenderer"]
    } else {
        entry
    };
    let (reply_count_text, replies_token) = {
        let rr = &thread["replies"]["commentRepliesRenderer"];
        let label = text(&rr["viewReplies"]["buttonRenderer"]["text"]).unwrap_or_default();
        let tok = rr["contents"]
            .as_array()
            .and_then(|a| {
                a.iter().find_map(|x| {
                    x["continuationItemRenderer"]["continuationEndpoint"]
                        ["continuationCommand"]["token"]
                        .as_str()
                })
            })
            .map(str::to_owned);
        (label, tok)
    };

    let c_val = if thread["comment"]["commentRenderer"].is_object() {
        Some(&thread["comment"]["commentRenderer"])
    } else if thread["commentRenderer"].is_object() {
        Some(&thread["commentRenderer"])
    } else {
        None
    };

    if let Some(cr) = c_val {
        let id = cr["commentId"].as_str().unwrap_or_default().to_owned();
        let author = text(&cr["authorText"]).unwrap_or_else(|| "User".to_owned());
        let avatar_url = cr["authorThumbnail"]["thumbnails"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|t| t["url"].as_str())
            .map(normalize_url);
        let published = text(&cr["publishedTimeText"]).unwrap_or_default();
        let likes = text(&cr["voteCount"]).unwrap_or_default();
        let body_txt = text(&cr["contentText"]).unwrap_or_default();
        if body_txt.is_empty() {
            return None;
        }
        let reply_params = find_nested_str(cr, "createReplyParams")
            .or_else(|| find_nested_str(thread, "createReplyParams"))
            .map(str::to_owned);
        let action_btns = &cr["actionButtons"]["commentActionButtonsRenderer"];
        let is_liked = cr["isLiked"].as_bool().unwrap_or(false)
            || action_btns["likeButton"]["toggleButtonRenderer"]["isToggled"]
                .as_bool()
                .unwrap_or(false);
        let like_action_params = action_btns["likeButton"]["toggleButtonRenderer"]
            ["defaultServiceEndpoint"]["performCommentActionEndpoint"]["action"]
            .as_str()
            .map(str::to_owned);
        let unlike_action_params = action_btns["likeButton"]["toggleButtonRenderer"]
            ["toggledServiceEndpoint"]["performCommentActionEndpoint"]["action"]
            .as_str()
            .map(str::to_owned);
        let is_pinned = cr["pinnedCommentBadge"].is_object();
        let has_creator_heart = action_btns["creatorHeart"]["creatorHeartRenderer"]["isHearted"]
            .as_bool()
            .unwrap_or(false);
        return Some(CommentItem {
            id,
            author,
            avatar_url,
            published,
            likes,
            text: body_txt,
            reply_count_text,
            replies_token,
            reply_params,
            replies: Vec::new(),
            replies_loading: false,
            is_liked,
            like_action_params,
            unlike_action_params,
            is_pinned,
            has_creator_heart,
        });
    }

    let vm = if thread["commentViewModel"]["commentViewModel"].is_object() {
        &thread["commentViewModel"]["commentViewModel"]
    } else {
        &entry["commentViewModel"]
    };
    let key = vm["commentKey"].as_str()?;
    let payload = entity_map.get(key)?;
    let author = payload["author"]["displayName"]
        .as_str()
        .unwrap_or("User")
        .to_owned();
    let avatar_url = payload["author"]["avatarThumbnailUrl"]
        .as_str()
        .map(normalize_url);
    let published = payload["properties"]["publishedTime"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let toolbar = &payload["toolbar"];
    let likes = toolbar["likeCountLiked"]
        .as_str()
        .filter(|_| toolbar["likeState"].as_str() == Some("TOOLBAR_LIKE_STATE_LIKED"))
        .or_else(|| toolbar["likeCountNotliked"].as_str())
        .unwrap_or_default()
        .trim()
        .to_owned();
    let body_txt = payload["properties"]["content"]["content"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    if body_txt.is_empty() {
        return None;
    }
    let surface = vm["commentSurfaceKey"]
        .as_str()
        .and_then(|sk| surface_map.get(sk));
    let reply_params = surface
        .and_then(|s| find_nested_str(s, "createReplyParams"))
        .or_else(|| find_nested_str(payload, "createReplyParams"))
        .or_else(|| find_nested_str(thread, "createReplyParams"))
        .map(str::to_owned);
    let is_liked = toolbar["likeState"].as_str() == Some("TOOLBAR_LIKE_STATE_LIKED");
    let like_action_params = surface
        .and_then(|s| {
            find_nested_str(&s["likeCommand"], "action")
                .or_else(|| find_nested_str(&s["toolbar"]["likeCommand"], "action"))
        })
        .map(str::to_owned);
    let unlike_action_params = surface
        .and_then(|s| {
            find_nested_str(&s["unlikeCommand"], "action")
                .or_else(|| find_nested_str(&s["toolbar"]["unlikeCommand"], "action"))
        })
        .map(str::to_owned);
    let is_pinned = vm["pinnedText"].as_str().is_some_and(|s| !s.is_empty())
        || payload["pinnedText"].as_str().is_some_and(|s| !s.is_empty())
        || surface.is_some_and(|s| s["pinnedText"].as_str().is_some_and(|t| !t.is_empty()));
    let has_creator_heart = toolbar["heartState"].as_str() == Some("TOOLBAR_HEART_STATE_HEARTED")
        || toolbar["heartActiveTooltip"].as_str().is_some_and(|s| !s.is_empty());
    Some(CommentItem {
        id: payload["properties"]["commentId"]
            .as_str()
            .unwrap_or(key)
            .to_owned(),
        author,
        avatar_url,
        published,
        likes,
        text: body_txt,
        reply_count_text,
        replies_token,
        reply_params,
        replies: Vec::new(),
        replies_loading: false,
        is_liked,
        like_action_params,
        unlike_action_params,
        is_pinned,
        has_creator_heart,
    })
}

pub async fn fetch_comments(
    http: &reqwest::Client,
    continuation: &str,
    cfg: &WebConfig,
    session: Option<&Session>,
) -> anyhow::Result<CommentPage> {
    let v = post_web(
        http,
        "next",
        json!({ "continuation": continuation }),
        cfg,
        session,
    )
    .await?;

    let mut entity_map = std::collections::HashMap::<String, Value>::new();
    let mut surface_map = std::collections::HashMap::<String, Value>::new();
    if let Some(muts) = v["frameworkUpdates"]["entityBatchUpdate"]["mutations"].as_array() {
        for m in muts {
            if let Some(p) = m["payload"]["commentEntityPayload"].as_object() {
                if let Some(key) = p.get("key").and_then(|k| k.as_str()) {
                    entity_map.insert(key.to_owned(), Value::Object(p.clone()));
                }
            }
            if let Some(p) = m["payload"]["commentSurfaceEntityPayload"].as_object() {
                if let Some(key) = p.get("key").and_then(|k| k.as_str()) {
                    surface_map.insert(key.to_owned(), Value::Object(p.clone()));
                }
            }
        }
    }

    let mut out = Vec::new();
    let mut next_token = None;
    let mut sort_options = Vec::new();
    let mut create_params = None;

    if let Some(eps) = v["onResponseReceivedEndpoints"].as_array() {
        for ep in eps {
            let items = ep["reloadContinuationItemsCommand"]["continuationItems"]
                .as_array()
                .or_else(|| ep["appendContinuationItemsAction"]["continuationItems"].as_array());
            let Some(arr) = items else { continue };
            for entry in arr {
                if let Some(header) = entry.get("commentsHeaderRenderer") {
                    if let Some(menu_items) = header["sortMenu"]["sortFilterSubMenuRenderer"]
                        ["subMenuItems"]
                        .as_array()
                    {
                        for mi in menu_items {
                            let label = mi["title"].as_str().unwrap_or_default().to_owned();
                            let tok = mi["serviceEndpoint"]["continuationCommand"]["token"]
                                .as_str()
                                .or_else(|| {
                                    mi["continuationEndpoint"]["continuationCommand"]["token"]
                                        .as_str()
                                })
                                .map(str::to_owned);
                            if let (false, Some(tok)) = (label.is_empty(), tok) {
                                sort_options.push((label, tok));
                            }
                        }
                    }
                    if create_params.is_none() {
                        create_params = find_nested_str(header, "createCommentParams")
                            .map(str::to_owned);
                    }
                    continue;
                }
                if let Some(tok) = entry["continuationItemRenderer"]["continuationEndpoint"]
                    ["continuationCommand"]["token"]
                    .as_str()
                {
                    next_token = Some(tok.to_owned());
                    continue;
                }
                if let Some(item) = parse_comment_entry(entry, &entity_map, &surface_map) {
                    out.push(item);
                }
            }
        }
    }

    Ok(CommentPage {
        comments: out,
        next_token,
        sort_options,
        create_params,
    })
}

pub async fn fetch_comment_replies(
    http: &reqwest::Client,
    continuation: &str,
    cfg: &WebConfig,
    session: Option<&Session>,
) -> anyhow::Result<(Vec<CommentItem>, Option<String>)> {
    let page = fetch_comments(http, continuation, cfg, session).await?;
    Ok((page.comments, page.next_token))
}

pub async fn create_comment(
    http: &reqwest::Client,
    text_body: &str,
    create_params: &str,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<()> {
    post_web(
        http,
        "comment/create_comment",
        json!({
            "commentText": text_body,
            "createCommentParams": create_params,
        }),
        cfg,
        Some(session),
    )
    .await?;
    Ok(())
}

pub async fn create_comment_reply(
    http: &reqwest::Client,
    text_body: &str,
    reply_params: &str,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<()> {
    post_web(
        http,
        "comment/create_comment_reply",
        json!({
            "commentText": text_body,
            "createReplyParams": reply_params,
        }),
        cfg,
        Some(session),
    )
    .await?;
    Ok(())
}

pub async fn perform_comment_action(
    http: &reqwest::Client,
    action_params: &str,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<()> {
    post_web(
        http,
        "comment/perform_comment_action",
        json!({
            "actions": [action_params],
        }),
        cfg,
        Some(session),
    )
    .await?;
    Ok(())
}

pub async fn set_like(
    http: &reqwest::Client,
    video_id: &str,
    like: bool,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<()> {
    let endpoint = if like { "like/like" } else { "like/removelike" };
    post_web(
        http,
        endpoint,
        json!({ "target": { "videoId": video_id } }),
        cfg,
        Some(session),
    )
    .await?;
    Ok(())
}

pub async fn set_dislike(
    http: &reqwest::Client,
    video_id: &str,
    dislike: bool,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<()> {
    let endpoint = if dislike {
        "like/dislike"
    } else {
        "like/removelike"
    };
    post_web(
        http,
        endpoint,
        json!({ "target": { "videoId": video_id } }),
        cfg,
        Some(session),
    )
    .await?;
    Ok(())
}

pub async fn set_subscribe(
    http: &reqwest::Client,
    channel_id: &str,
    subscribe: bool,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<()> {
    let endpoint = if subscribe {
        "subscription/subscribe"
    } else {
        "subscription/unsubscribe"
    };
    post_web(
        http,
        endpoint,
        json!({ "channelIds": [channel_id] }),
        cfg,
        Some(session),
    )
    .await?;
    Ok(())
}

pub struct LiveChatPage {
    pub messages: Vec<LiveChatMessage>,
    pub next_token: Option<String>,
    pub send_params: Option<String>,
    pub emotes: Vec<LiveChatEmote>,
    pub is_replay: bool,
}

pub async fn fetch_live_chat(
    http: &reqwest::Client,
    continuation: &str,
    player_offset_ms: Option<u64>,
    cfg: &WebConfig,
    session: Option<&Session>,
) -> anyhow::Result<LiveChatPage> {
    let (endpoint, body) = match player_offset_ms {
        Some(ms) => (
            "live_chat/get_live_chat_replay",
            json!({
                "continuation": continuation,
                "currentPlayerState": { "playerOffsetMs": ms.to_string() },
            }),
        ),
        None => (
            "live_chat/get_live_chat",
            json!({ "continuation": continuation }),
        ),
    };
    let v = post_web(http, endpoint, body, cfg, session).await?;
    Ok(parse_live_chat_response(&v))
}

fn parse_live_chat_runs(
    msg_val: &Value,
    emotes_out: &mut Vec<LiveChatEmote>,
) -> (Vec<LiveChatRun>, String) {
    let mut runs = Vec::new();
    let mut body = String::new();
    if let Some(arr) = msg_val["runs"].as_array() {
        for run in arr {
            if let Some(t) = run["text"].as_str() {
                body.push_str(t);
                runs.push(LiveChatRun::Text(t.to_owned()));
            } else if let Some(em) = run.get("emoji") {
                let url = em["image"]["thumbnails"]
                    .as_array()
                    .and_then(|a| a.last())
                    .and_then(|t| t["url"].as_str())
                    .map(normalize_url);
                let alt = em["shortcuts"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|s| s.as_str())
                    .or_else(|| em["emojiId"].as_str())
                    .unwrap_or("")
                    .to_owned();
                if em["isCustomEmoji"].as_bool().unwrap_or(false) && !alt.is_empty() {
                    if let Some(eid) = em["emojiId"].as_str() {
                        if !emotes_out.iter().any(|e| e.emoji_id == eid) {
                            emotes_out.push(LiveChatEmote {
                                shortcut: alt.clone(),
                                emoji_id: eid.to_owned(),
                                url: url.clone(),
                            });
                        }
                    }
                }
                body.push_str(&alt);
                runs.push(LiveChatRun::Emoji { alt, url });
            }
        }
    }
    if body.is_empty() {
        body = text(msg_val).unwrap_or_default();
    }
    (runs, body)
}

fn parse_live_chat_badges(r: &Value) -> (bool, bool) {
    let badges = r["authorBadges"].as_array();
    let is_owner = badges.is_some_and(|b| {
        b.iter()
            .any(|x| x["liveChatAuthorBadgeRenderer"]["icon"]["iconType"] == "OWNER")
    });
    let is_mod = badges.is_some_and(|b| {
        b.iter()
            .any(|x| x["liveChatAuthorBadgeRenderer"]["icon"]["iconType"] == "MODERATOR")
    });
    (is_owner, is_mod)
}

fn collect_picker_emotes(v: &Value, out: &mut Vec<LiveChatEmote>) {
    match v {
        Value::Object(map) => {
            if let Some(em) = map.get("emoji").filter(|e| e.is_object()) {
                if em["isCustomEmoji"].as_bool().unwrap_or(false) {
                    if let Some(eid) = em["emojiId"].as_str() {
                        let shortcut = em["shortcuts"]
                            .as_array()
                            .and_then(|a| a.first())
                            .and_then(|s| s.as_str())
                            .unwrap_or(eid)
                            .to_owned();
                        let url = em["image"]["thumbnails"]
                            .as_array()
                            .and_then(|a| a.last())
                            .and_then(|t| t["url"].as_str())
                            .map(normalize_url);
                        if !out.iter().any(|e| e.emoji_id == eid) {
                            out.push(LiveChatEmote {
                                shortcut,
                                emoji_id: eid.to_owned(),
                                url,
                            });
                        }
                    }
                }
            }
            if let (Some(eid), Some(thumbs)) = (
                map.get("emojiId").and_then(|s| s.as_str()),
                map.get("image").and_then(|i| i["thumbnails"].as_array()),
            ) {
                if map
                    .get("isCustomEmoji")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false)
                {
                    let shortcut = map
                        .get("shortcuts")
                        .and_then(|s| s.as_array())
                        .and_then(|a| a.first())
                        .and_then(|s| s.as_str())
                        .unwrap_or(eid)
                        .to_owned();
                    let url = thumbs
                        .last()
                        .and_then(|t| t["url"].as_str())
                        .map(normalize_url);
                    if !out.iter().any(|e| e.emoji_id == eid) {
                        out.push(LiveChatEmote {
                            shortcut,
                            emoji_id: eid.to_owned(),
                            url,
                        });
                    }
                }
            }
            for val in map.values() {
                collect_picker_emotes(val, out);
            }
        }
        Value::Array(arr) => {
            for val in arr {
                collect_picker_emotes(val, out);
            }
        }
        _ => {}
    }
}

fn push_chat_item(
    item: &Value,
    offset_ms: Option<u64>,
    msgs: &mut Vec<LiveChatMessage>,
    emotes: &mut Vec<LiveChatEmote>,
) {
    if let Some(r) = item.get("liveChatTextMessageRenderer").filter(|x| x.is_object()) {
        let Some(id) = r["id"].as_str().map(str::to_owned) else {
            return;
        };
        let author = text(&r["authorName"]).unwrap_or_else(|| "User".to_owned());
        let avatar_url = r["authorPhoto"]["thumbnails"]
            .as_array()
            .and_then(|a| a.last())
            .and_then(|t| t["url"].as_str())
            .map(normalize_url);
        let (runs, body) = parse_live_chat_runs(&r["message"], emotes);
        if body.is_empty() && runs.is_empty() {
            return;
        }
        let (is_owner, is_mod) = parse_live_chat_badges(r);
        msgs.push(LiveChatMessage {
            id,
            author,
            avatar_url,
            text: body,
            runs,
            is_owner,
            is_mod,
            kind: LiveChatKind::Normal,
            offset_ms,
        });
    } else if let Some(r) = item.get("liveChatPaidMessageRenderer").filter(|x| x.is_object()) {
        let Some(id) = r["id"].as_str().map(str::to_owned) else {
            return;
        };
        let author = text(&r["authorName"]).unwrap_or_else(|| "User".to_owned());
        let avatar_url = r["authorPhoto"]["thumbnails"]
            .as_array()
            .and_then(|a| a.last())
            .and_then(|t| t["url"].as_str())
            .map(normalize_url);
        let amount = text(&r["purchaseAmountText"]).unwrap_or_else(|| "Super Chat".to_owned());
        let bg_color = r["bodyBackgroundColor"]
            .as_u64()
            .or_else(|| r["headerBackgroundColor"].as_u64())
            .unwrap_or(0xFF15_65C0) as u32;
        let (runs, body) = parse_live_chat_runs(&r["message"], emotes);
        let (is_owner, is_mod) = parse_live_chat_badges(r);
        msgs.push(LiveChatMessage {
            id,
            author,
            avatar_url,
            text: body,
            runs,
            is_owner,
            is_mod,
            kind: LiveChatKind::SuperChat { amount, bg_color },
            offset_ms,
        });
    } else if let Some(r) = item.get("liveChatMembershipItemRenderer").filter(|x| x.is_object()) {
        let Some(id) = r["id"].as_str().map(str::to_owned) else {
            return;
        };
        let author = text(&r["authorName"]).unwrap_or_else(|| "Member".to_owned());
        let avatar_url = r["authorPhoto"]["thumbnails"]
            .as_array()
            .and_then(|a| a.last())
            .and_then(|t| t["url"].as_str())
            .map(normalize_url);
        let header = text(&r["headerSubtext"])
            .or_else(|| text(&r["headerPrimaryText"]))
            .unwrap_or_else(|| "Channel Member".to_owned());
        let (runs, body) = parse_live_chat_runs(&r["message"], emotes);
        let (is_owner, is_mod) = parse_live_chat_badges(r);
        msgs.push(LiveChatMessage {
            id,
            author,
            avatar_url,
            text: body,
            runs,
            is_owner,
            is_mod,
            kind: LiveChatKind::Membership { header },
            offset_ms,
        });
    }
}

pub fn parse_live_chat_response(v: &Value) -> LiveChatPage {
    let lc = &v["continuationContents"]["liveChatContinuation"];
    let mut is_replay = false;
    let next_tok = lc["continuations"]
        .as_array()
        .and_then(|arr| {
            arr.iter().find_map(|c| {
                if let Some(tok) = c["liveChatReplayContinuationData"]["continuation"].as_str() {
                    is_replay = true;
                    return Some(tok);
                }
                c["invalidationContinuationData"]["continuation"]
                    .as_str()
                    .or_else(|| c["timedContinuationData"]["continuation"].as_str())
                    .or_else(|| c["reloadContinuationData"]["continuation"].as_str())
            })
        })
        .map(str::to_owned);
    let send_params = find_nested_str(&lc["actionPanel"], "sendLiveChatMessageEndpoint")
        .map(str::to_owned)
        .or_else(|| {
            lc["actionPanel"]["liveChatMessageInputRenderer"]["sendButton"]["buttonRenderer"]
                ["serviceEndpoint"]["sendLiveChatMessageEndpoint"]["params"]
                .as_str()
                .map(str::to_owned)
        });

    let mut emotes = Vec::new();
    collect_picker_emotes(&lc["actionPanel"], &mut emotes);

    let mut msgs = Vec::new();
    if let Some(actions) = lc["actions"].as_array() {
        for act in actions {
            if let Some(replay) = act.get("replayChatItemAction").filter(|x| x.is_object()) {
                is_replay = true;
                let offset_ms = replay["videoOffsetTimeMsec"]
                    .as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .or_else(|| replay["videoOffsetTimeMsec"].as_u64());
                if let Some(inner_acts) = replay["actions"].as_array() {
                    for inner in inner_acts {
                        push_chat_item(
                            &inner["addChatItemAction"]["item"],
                            offset_ms,
                            &mut msgs,
                            &mut emotes,
                        );
                    }
                }
            } else {
                push_chat_item(
                    &act["addChatItemAction"]["item"],
                    None,
                    &mut msgs,
                    &mut emotes,
                );
            }
        }
    }
    LiveChatPage {
        messages: msgs,
        next_token: next_tok,
        send_params,
        emotes,
        is_replay,
    }
}

pub fn build_live_chat_segments(message: &str, emotes: &[LiveChatEmote]) -> Vec<Value> {
    if emotes.is_empty() {
        return vec![json!({ "text": message })];
    }
    let mut segments = Vec::new();
    let mut rest = message;
    let mut buf = String::new();
    while !rest.is_empty() {
        if let Some(em) = emotes
            .iter()
            .find(|e| !e.shortcut.is_empty() && rest.starts_with(&e.shortcut))
        {
            if !buf.is_empty() {
                segments.push(json!({ "text": std::mem::take(&mut buf) }));
            }
            segments.push(json!({ "emojiId": em.emoji_id }));
            rest = &rest[em.shortcut.len()..];
        } else if let Some(ch) = rest.chars().next() {
            buf.push(ch);
            rest = &rest[ch.len_utf8()..];
        } else {
            break;
        }
    }
    if !buf.is_empty() {
        segments.push(json!({ "text": buf }));
    }
    if segments.is_empty() {
        segments.push(json!({ "text": message }));
    }
    segments
}

pub async fn send_live_chat_message(
    http: &reqwest::Client,
    params: &str,
    message: &str,
    emotes: &[LiveChatEmote],
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<()> {
    let client_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let segments = build_live_chat_segments(message, emotes);
    post_web(
        http,
        "live_chat/send_message",
        json!({
            "params": params,
            "richMessage": {
                "textSegments": segments,
            },
            "clientMessageId": format!("yt-rust-{client_id}"),
        }),
        cfg,
        Some(session),
    )
    .await?;
    Ok(())
}

/// Fetch signed-in user display name / avatar from account_menu.
pub async fn fetch_account_info(
    http: &reqwest::Client,
    cfg: &WebConfig,
    session: &Session,
) -> Option<(String, Option<String>)> {
    let v = post_web(http, "account/account_menu", json!({}), cfg, Some(session))
        .await
        .ok()?;
    let header = &v["actions"][0]["openPopupAction"]["popup"]["multiPageMenuRenderer"]["header"]
        ["activeAccountHeaderRenderer"];
    let name = text(&header["accountName"])
        .or_else(|| text(&header["channelHandle"]))?;
    let avatar = header["accountPhoto"]["thumbnails"]
        .as_array()
        .and_then(|a| a.last())
        .and_then(|t| t["url"].as_str())
        .map(|u| if u.starts_with("//") { format!("https:{u}") } else { u.to_owned() });
    Some((name, avatar))
}

pub async fn fetch_next_watch_fallback(
    http: &reqwest::Client,
    video_id: &str,
    cfg: &WebConfig,
    session: Option<&Session>,
) -> Option<WatchData> {
    let next = post_web(http, "next", json!({ "videoId": video_id }), cfg, session)
        .await
        .ok()?;
    let contents = next["contents"]["twoColumnWatchNextResults"]["results"]["results"]["contents"]
        .as_array()?;
    let title = contents
        .iter()
        .find_map(|c| text(&c["videoPrimaryInfoRenderer"]["title"]))
        .unwrap_or_else(|| format!("Video {video_id}"));
    let owner_run = contents.iter().find_map(|c| {
        let r = &c["videoSecondaryInfoRenderer"]["owner"]["videoOwnerRenderer"]["title"]["runs"][0];
        r["text"].as_str().map(|_| r)
    });
    let channel = owner_run
        .and_then(|r| r["text"].as_str())
        .unwrap_or("Unknown")
        .to_owned();
    let channel_id = owner_run
        .and_then(|r| r["navigationEndpoint"]["browseEndpoint"]["browseId"].as_str())
        .map(str::to_owned);
    let (
        views_text,
        published_text,
        likes_text,
        is_liked,
        is_disliked,
        is_subscribed,
        subscribers_text,
        channel_avatar_url,
        comments_token,
        live_chat_token,
        related,
    ) = parse_next_metadata(&next, video_id);
    let resume_secs = as_secs_f64(&next["currentVideoEndpoint"]["watchEndpoint"]["startTimeSeconds"])
        .filter(|&s| s > 2.0);
    Some(WatchData {
        id: video_id.to_owned(),
        title,
        channel,
        channel_id,
        channel_avatar_url,
        subscribers_text,
        views_text,
        published_text,
        likes_text,
        is_liked,
        is_disliked,
        is_subscribed,
        description: String::new(),
        duration_secs: 0.0,
        video_formats: Vec::new(),
        audio_url: None,
        audio_content_length: 0,
        audio_tracks: Vec::new(),
        hls_url: None,
        captions: Vec::new(),
        chapters: Vec::new(),
        storyboard: None,
        comments_token,
        live_chat_token,
        related,
        is_live: is_next_live(&next),
        resume_secs,
    })
}

fn is_next_live(next: &Value) -> bool {
    next["contents"]["twoColumnWatchNextResults"]["results"]["results"]["contents"]
        .as_array()
        .is_some_and(|a| {
            a.iter().any(|x| {
                x["videoPrimaryInfoRenderer"]["viewCount"]["videoViewCountRenderer"]["isLive"]
                    .as_bool()
                    .unwrap_or(false)
            })
        })
}

pub async fn watch(
    http: &reqwest::Client,
    video_id: &str,
    cfg: &WebConfig,
    session: Option<&Session>,
) -> anyhow::Result<WatchData> {
    let next_fut = post_web(http, "next", json!({ "videoId": video_id }), cfg, session);
    let player_fut = fetch_player(http, video_id, cfg, session);
    let (next_res, player_res) = tokio::join!(next_fut, player_fut);

    let player = player_res?;
    let next = next_res.ok();

    let vd = &player["videoDetails"];
    let title = vd["title"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            next.as_ref().and_then(|n| {
                text(&n["contents"]["twoColumnWatchNextResults"]["results"]["results"]["contents"][0]["videoPrimaryInfoRenderer"]["title"])
            })
        })
        .unwrap_or_else(|| format!("Video {video_id}"));

    let channel = vd["author"].as_str().unwrap_or("Unknown").to_owned();
    let channel_id = vd["channelId"].as_str().map(str::to_owned);
    let description = vd["shortDescription"].as_str().unwrap_or_default().to_owned();
    let duration_secs = vd["lengthSeconds"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let is_live = vd["isLiveContent"].as_bool().unwrap_or(false)
        && vd["isLive"].as_bool().unwrap_or(false);

    let (
        views_text,
        published_text,
        likes_text,
        is_liked,
        is_disliked,
        is_subscribed,
        subscribers_text,
        channel_avatar_url,
        comments_token,
        live_chat_token,
        related,
    ) = if let Some(ref n) = next {
        parse_next_metadata(n, video_id)
    } else {
        (
            vd["viewCount"]
                .as_str()
                .map(|c| format!("{c} views"))
                .unwrap_or_default(),
            String::new(),
            String::new(),
            false,
            false,
            false,
            String::new(),
            None,
            None,
            None,
            Vec::new(),
        )
    };

    let adaptive = player["streamingData"]["adaptiveFormats"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let muxed = player["streamingData"]["formats"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();

    let audio_tracks = parse_audio_tracks(adaptive);
    let audio_url = audio_tracks.first().map(|t| t.url.clone());
    let audio_content_length = audio_tracks.first().map_or(0, |t| t.content_length);

    let mut video_formats = Vec::new();
    for f in adaptive.iter().chain(muxed.iter()) {
        let Some(mime) = f["mimeType"].as_str() else {
            continue;
        };
        if !mime.starts_with("video/") {
            continue;
        }
        let Some(url) = f["url"].as_str() else {
            continue;
        };
        if is_noclen_live_tail(f, url) {
            continue;
        }
        let Some(itag) = f["itag"].as_u64() else {
            continue;
        };
        let height = f["height"].as_u64().unwrap_or(0) as u32;
        let width = f["width"].as_u64().unwrap_or(0) as u32;
        if height == 0 {
            continue;
        }
        let fps = f["fps"].as_u64().unwrap_or(30) as u32;
        let bitrate = f["bitrate"].as_u64().unwrap_or(0);
        let content_length = f["contentLength"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let has_audio = f["audioQuality"].is_string() || f["audioChannels"].is_number();
        video_formats.push(VideoFormat {
            itag,
            url: url.to_owned(),
            mime: mime.to_owned(),
            width,
            height,
            fps,
            bitrate,
            content_length,
            has_audio,
        });
    }

    video_formats.sort_by(|a, b| {
        b.height
            .cmp(&a.height)
            .then_with(|| b.fps.cmp(&a.fps))
            .then_with(|| codec_rank(&b.mime).cmp(&codec_rank(&a.mime)))
            .then_with(|| b.bitrate.cmp(&a.bitrate))
    });
    let mut deduped: Vec<VideoFormat> = Vec::new();
    for f in video_formats {
        let same = deduped.iter().any(|d| {
            d.height == f.height
                && d.fps == f.fps
                && codec_rank(&d.mime) == codec_rank(&f.mime)
                && d.has_audio == f.has_audio
        });
        if !same {
            deduped.push(f);
        }
    }

    let hls_url = player["streamingData"]["hlsManifestUrl"]
        .as_str()
        .map(str::to_owned);

    let captions = parse_captions(&player);
    let chapters = parse_chapters(&description, duration_secs);
    let storyboard = player["storyboards"]["playerStoryboardSpecRenderer"]["spec"]
        .as_str()
        .and_then(parse_storyboard);
    let resume_secs = next
        .as_ref()
        .and_then(|n| as_secs_f64(&n["currentVideoEndpoint"]["watchEndpoint"]["startTimeSeconds"]))
        .or_else(|| as_secs_f64(&player["playerConfig"]["playbackStartConfig"]["startSeconds"]))
        .filter(|&s| s > 2.0);

    Ok(WatchData {
        id: video_id.to_owned(),
        title,
        channel,
        channel_id,
        channel_avatar_url,
        subscribers_text,
        views_text,
        published_text,
        likes_text,
        is_liked,
        is_disliked,
        is_subscribed,
        description,
        duration_secs,
        video_formats: deduped,
        audio_url,
        audio_content_length,
        audio_tracks,
        hls_url,
        captions,
        chapters,
        storyboard,
        comments_token,
        live_chat_token,
        related,
        is_live,
        resume_secs,
    })
}

pub async fn fetch_hls_variants(
    http: &reqwest::Client,
    hls_url: &str,
) -> (Vec<VideoFormat>, Vec<AudioTrack>) {
    let Ok(resp) = http
        .get(hls_url)
        .header("User-Agent", VISIONOS_UA)
        .timeout(Duration::from_secs(6))
        .send()
        .await
    else {
        return (Vec::new(), Vec::new());
    };
    if !resp.status().is_success() {
        return (Vec::new(), Vec::new());
    }
    let Ok(text) = resp.text().await else {
        return (Vec::new(), Vec::new());
    };
    parse_hls_master_variants(&text, hls_url)
}

fn parse_m3u8_attrs(raw: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i] == b',' || bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        let k_start = i;
        while i < bytes.len() && bytes[i] != b'=' && bytes[i] != b',' {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            break;
        }
        let key = raw[k_start..i].trim().to_ascii_uppercase();
        i += 1;
        let val = if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            let v_start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            let s = &raw[v_start..i];
            if i < bytes.len() {
                i += 1;
            }
            s.to_owned()
        } else {
            let v_start = i;
            while i < bytes.len() && bytes[i] != b',' {
                i += 1;
            }
            raw[v_start..i].trim().to_owned()
        };
        if !key.is_empty() {
            out.push((key, val));
        }
    }
    out
}

fn hls_codec_mime(codecs: &str) -> String {
    let lc = codecs.to_ascii_lowercase();
    if lc.contains("av01") {
        "video/mp4; codecs=\"av01.0.08M.08\"".to_owned()
    } else if lc.contains("vp09") || lc.contains("vp9") {
        "video/webm; codecs=\"vp9\"".to_owned()
    } else if lc.contains("avc1") {
        "video/mp4; codecs=\"avc1.640028\"".to_owned()
    } else if lc.contains("hev1") || lc.contains("hvc1") {
        "video/mp4; codecs=\"hev1\"".to_owned()
    } else if codecs.is_empty() {
        "video/mp4; codecs=\"avc1.640028\"".to_owned()
    } else {
        format!("video/mp4; codecs=\"{codecs}\"")
    }
}

pub fn parse_hls_master_variants(
    manifest: &str,
    base_url: &str,
) -> (Vec<VideoFormat>, Vec<AudioTrack>) {
    let base = url::Url::parse(base_url).ok();
    let resolve = |raw: &str| -> String {
        let trimmed = raw.trim();
        if let Some(ref b) = base {
            if let Ok(u) = b.join(trimmed) {
                return u.into();
            }
        }
        trimmed.to_owned()
    };

    let mut audio_tracks: Vec<AudioTrack> = Vec::new();
    let mut raw_formats: Vec<VideoFormat> = Vec::new();
    let lines: Vec<&str> = manifest.lines().map(str::trim).collect();

    for (idx, line) in lines.iter().enumerate() {
        if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA:") {
            let attrs = parse_m3u8_attrs(rest);
            let get = |k: &str| attrs.iter().find(|(ak, _)| ak == k).map(|(_, v)| v.as_str());
            if get("TYPE") != Some("AUDIO") {
                continue;
            }
            let Some(uri) = get("URI").filter(|u| !u.is_empty()) else {
                continue;
            };
            let group_id = get("GROUP-ID").unwrap_or_default().to_owned();
            let name = get("NAME")
                .or_else(|| get("LANGUAGE"))
                .unwrap_or("Default")
                .to_owned();
            let is_default = get("DEFAULT").is_some_and(|v| v.eq_ignore_ascii_case("YES"));
            let resolved = resolve(uri);
            if !audio_tracks.iter().any(|t| t.url == resolved) {
                audio_tracks.push(AudioTrack {
                    id: group_id,
                    label: name,
                    is_default,
                    url: resolved,
                    content_length: 0,
                });
            }
        } else if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let Some(uri_line) = lines[idx + 1..]
                .iter()
                .copied()
                .find(|l| !l.is_empty() && !l.starts_with('#'))
            else {
                continue;
            };
            let attrs = parse_m3u8_attrs(rest);
            let get = |k: &str| attrs.iter().find(|(ak, _)| ak == k).map(|(_, v)| v.as_str());
            let Some((width, height)) = get("RESOLUTION").and_then(|r| {
                let (w, h) = r.split_once('x')?;
                Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
            }) else {
                continue;
            };
            if height == 0 {
                continue;
            }
            let fps = get("FRAME-RATE")
                .and_then(|s| s.parse::<f64>().ok())
                .map_or(30, |f| f.round() as u32)
                .max(1);
            let bitrate = get("BANDWIDTH")
                .or_else(|| get("AVERAGE-BANDWIDTH"))
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let mime = hls_codec_mime(get("CODECS").unwrap_or_default());
            let has_audio = get("AUDIO").is_none_or(str::is_empty);
            raw_formats.push(VideoFormat {
                itag: 0,
                url: resolve(uri_line),
                mime,
                width,
                height,
                fps,
                bitrate,
                content_length: 0,
                has_audio,
            });
        }
    }

    audio_tracks.sort_by(|a, b| b.is_default.cmp(&a.is_default));
    let mut dedup_audio: Vec<AudioTrack> = Vec::new();
    for tr in audio_tracks {
        if !dedup_audio.iter().any(|x| x.label == tr.label) {
            dedup_audio.push(tr);
        }
    }

    raw_formats.sort_by(|a, b| {
        b.height
            .cmp(&a.height)
            .then_with(|| b.fps.cmp(&a.fps))
            .then_with(|| codec_rank(&b.mime).cmp(&codec_rank(&a.mime)))
            .then_with(|| b.bitrate.cmp(&a.bitrate))
    });
    let mut deduped: Vec<VideoFormat> = Vec::new();
    for mut f in raw_formats {
        if dedup_audio.is_empty() {
            f.has_audio = true;
        }
        let same = deduped.iter().any(|d| {
            d.height == f.height
                && d.fps == f.fps
                && codec_rank(&d.mime) == codec_rank(&f.mime)
                && d.has_audio == f.has_audio
        });
        if !same {
            deduped.push(f);
        }
    }

    (deduped, dedup_audio)
}

pub fn parse_storyboard(spec: &str) -> Option<StoryboardSpec> {
    let mut parts = spec.split('|');
    let base_url = parts.next()?.to_owned();
    let levels: Vec<(usize, &str)> = parts.enumerate().collect();
    if levels.is_empty() {
        return None;
    }
    // Prefer a crisp medium/high level (width >= 80, usually level 2 or 1)
    let mut parsed = Vec::new();
    for (level, raw) in levels {
        let f: Vec<&str> = raw.split('#').collect();
        if f.len() < 8 {
            continue;
        }
        let Ok(width) = f[0].parse::<u32>() else { continue };
        let Ok(height) = f[1].parse::<u32>() else { continue };
        let Ok(count) = f[2].parse::<usize>() else { continue };
        let Ok(cols) = f[3].parse::<usize>() else { continue };
        let Ok(rows) = f[4].parse::<usize>() else { continue };
        let Ok(interval_ms) = f[5].parse::<u64>() else { continue };
        let name_template = f[6].to_owned();
        let sigh = f[7].to_owned();
        parsed.push(StoryboardSpec {
            base_url: base_url.clone(),
            level,
            width,
            height,
            count,
            cols,
            rows,
            interval_ms,
            name_template,
            sigh,
        });
    }
    parsed
        .iter()
        .filter(|s| s.width >= 100 && s.width <= 240)
        .max_by_key(|s| s.width)
        .cloned()
        .or_else(|| parsed.into_iter().max_by_key(|s| s.width))
}

fn parse_audio_tracks(adaptive: &[Value]) -> Vec<AudioTrack> {
    let itag_score = |itag: u64, bitrate: u64| -> u64 {
        let base = match itag {
            251 => 500,
            140 => 450,
            250 => 300,
            249 => 200,
            139 => 150,
            _ => 100,
        };
        base * 1_000_000 + bitrate
    };
    let mut best_by_id: Vec<(u64, AudioTrack)> = Vec::new();
    for f in adaptive {
        let Some(mime) = f["mimeType"].as_str() else { continue };
        if !mime.starts_with("audio/") {
            continue;
        }
        let Some(url) = f["url"].as_str() else { continue };
        if is_noclen_live_tail(f, url) {
            continue;
        }
        let itag = f["itag"].as_u64().unwrap_or(0);
        let bitrate = f["bitrate"].as_u64().unwrap_or(0);
        let content_length = f["contentLength"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let at = &f["audioTrack"];
        let id = at["id"].as_str().unwrap_or("default").to_owned();
        let label = at["displayName"]
            .as_str()
            .unwrap_or("Default")
            .to_owned();
        let is_default = at["audioIsDefault"].as_bool().unwrap_or(false)
            || label.to_ascii_lowercase().contains("original");
        let score = itag_score(itag, bitrate);
        if let Some((best_score, existing)) = best_by_id.iter_mut().find(|(_, t)| t.id == id) {
            if score > *best_score {
                *best_score = score;
                existing.url = url.to_owned();
                existing.content_length = content_length;
                existing.is_default |= is_default;
            }
        } else {
            best_by_id.push((
                score,
                AudioTrack {
                    id,
                    label,
                    is_default,
                    url: url.to_owned(),
                    content_length,
                },
            ));
        }
    }
    let mut tracks: Vec<AudioTrack> = best_by_id.into_iter().map(|(_, t)| t).collect();
    tracks.sort_by(|a, b| b.is_default.cmp(&a.is_default).then_with(|| a.label.cmp(&b.label)));
    tracks
}

fn parse_captions(player: &Value) -> Vec<CaptionTrack> {
    let mut tracks = Vec::new();
    let Some(arr) = player["captions"]["playerCaptionsTracklistRenderer"]["captionTracks"].as_array()
    else {
        return tracks;
    };
    for t in arr {
        let Some(base_url) = t["baseUrl"].as_str() else {
            continue;
        };
        let language_code = t["languageCode"].as_str().unwrap_or("").to_owned();
        let name = text(&t["name"]).unwrap_or_else(|| language_code.clone());
        let is_auto = t["kind"].as_str() == Some("asr");
        let vtt_url = if base_url.contains("fmt=") {
            base_url.to_owned()
        } else if base_url.contains('?') {
            format!("{base_url}&fmt=vtt")
        } else {
            format!("{base_url}?fmt=vtt")
        };
        tracks.push(CaptionTrack {
            name,
            language_code,
            vtt_url,
            is_auto,
        });
    }
    tracks
}

pub fn parse_chapters(description: &str, duration_secs: f64) -> Vec<Chapter> {
    let mut chapters = Vec::new();
    for line in description.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Look for timestamp token at start or wrapped in parens/brackets
        let mut found: Option<(f64, String)> = None;
        for token in trimmed.split_whitespace().take(3) {
            let clean = token.trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | '-' | ':'));
            if let Some(secs) = parse_timestamp(clean) {
                let rest = trimmed
                    .split_once(token)
                    .map(|(before, after)| {
                        let b = before.trim().trim_end_matches(|c: char| matches!(c, '-' | '•' | '|'));
                        let a = after.trim().trim_start_matches(|c: char| matches!(c, '-' | ':' | '•' | '|'));
                        if a.is_empty() { b } else { a }
                    })
                    .unwrap_or("")
                    .trim();
                if !rest.is_empty() && (duration_secs <= 0.0 || secs < duration_secs) {
                    found = Some((secs, rest.to_owned()));
                }
                break;
            }
        }
        if let Some((start_secs, title)) = found {
            chapters.push(Chapter { start_secs, title });
        }
    }
    if chapters.len() < 2 {
        return Vec::new();
    }
    chapters.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
    chapters.dedup_by(|a, b| (a.start_secs - b.start_secs).abs() < 1.0);
    chapters
}

fn parse_timestamp(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 && parts.len() != 3 {
        return None;
    }
    if parts.iter().any(|p| p.is_empty() || p.len() > 3 || !p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    let nums: Vec<u64> = parts.iter().filter_map(|p| p.parse().ok()).collect();
    if nums.len() != parts.len() {
        return None;
    }
    if nums.len() == 2 {
        let (m, sec) = (nums[0], nums[1]);
        (sec < 60).then_some((m * 60 + sec) as f64)
    } else {
        let (h, m, sec) = (nums[0], nums[1], nums[2]);
        (m < 60 && sec < 60).then_some((h * 3600 + m * 60 + sec) as f64)
    }
}

fn codec_rank(mime: &str) -> u8 {
    if mime.contains("avc1") {
        3
    } else if mime.contains("vp9") || mime.contains("vp09") {
        2
    } else if mime.contains("av01") {
        1
    } else {
        0
    }
}

fn is_login_or_age_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string();
    msg.contains("LOGIN_REQUIRED")
        || msg.contains("Sign in")
        || msg.contains("UNPLAYABLE")
        || msg.contains("age")
}

async fn fetch_player(
    http: &reqwest::Client,
    video_id: &str,
    cfg: &WebConfig,
    session: Option<&Session>,
) -> anyhow::Result<Value> {
    let visitor_data = cfg.visitor_data.as_deref();
    let mut last_err: Option<anyhow::Error> = None;
    let mut playable_fallback: Option<Value> = None;
    let mut non_playable: Option<Value> = None;
    let mut tried_auth = false;

    match player_visionos(http, video_id, visitor_data).await {
        Ok(v) if is_preferred_player_response(&v) => return Ok(v),
        Ok(v) if has_playable_url(&v) => playable_fallback = Some(v),
        Ok(v) => non_playable = Some(v),
        Err(e) => {
            if is_login_or_age_error(&e) {
                if let Some(sess) = session {
                    tried_auth = true;
                    if let Some(v) = try_authenticated_players(http, video_id, cfg, sess).await {
                        return Ok(v);
                    }
                }
            }
            last_err = Some(e);
        }
    }

    match player_android_vr(http, video_id, visitor_data).await {
        Ok(vr) if is_preferred_player_response(&vr) => return Ok(vr),
        Ok(vr) if has_playable_url(&vr) => {
            if playable_fallback.is_none() {
                playable_fallback = Some(vr);
            }
        }
        Ok(vr) => {
            if non_playable.is_none() {
                non_playable = Some(vr);
            }
        }
        Err(e) => {
            if last_err.is_none() {
                last_err = Some(e);
            }
        }
    }

    if !tried_auth {
        if let Some(sess) = session {
            if let Some(v) = try_authenticated_players(http, video_id, cfg, sess).await {
                if is_preferred_player_response(&v) || playable_fallback.is_none() {
                    return Ok(v);
                }
            }
        }
    }

    if let Some(v) = playable_fallback.or(non_playable) {
        return Ok(v);
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("All InnerTube player clients failed")))
}

async fn try_authenticated_players(
    http: &reqwest::Client,
    video_id: &str,
    cfg: &WebConfig,
    session: &Session,
) -> Option<Value> {
    if let Ok(v) = player_tv(http, video_id, cfg.visitor_data.as_deref(), session).await {
        if has_playable_url(&v) {
            return Some(v);
        }
    }
    if let Ok(v) = player_web_safari(http, video_id, cfg, session).await {
        if has_playable_url(&v) {
            return Some(v);
        }
    }
    None
}

async fn player_tv(
    http: &reqwest::Client,
    video_id: &str,
    visitor_data: Option<&str>,
    session: &Session,
) -> anyhow::Result<Value> {
    const TV_VERSION: &str = "7.20250923.13.00";
    const TV_UA: &str = "Mozilla/5.0 (ChromiumStylePlatform) Cobalt/Version";
    let mut client = json!({
        "clientName": "TVHTML5",
        "clientVersion": TV_VERSION,
        "userAgent": TV_UA,
        "hl": "en",
        "gl": "US",
    });
    if let Some(vd) = visitor_data.filter(|v| !v.is_empty()) {
        client["visitorData"] = json!(vd);
    }
    let body = json!({
        "context": { "client": client },
        "videoId": video_id,
        "contentCheckOk": true,
        "racyCheckOk": true,
    });
    let mut req = http
        .post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false")
        .header("User-Agent", TV_UA)
        .header("X-YouTube-Client-Name", "7")
        .header("X-YouTube-Client-Version", TV_VERSION)
        .header("Origin", ORIGIN)
        .header("Referer", format!("{ORIGIN}/tv"))
        .timeout(Duration::from_secs(10))
        .json(&body);
    if let Some(vd) = visitor_data.filter(|v| !v.is_empty()) {
        req = req.header("X-Goog-Visitor-Id", vd);
    }
    req = session.sign(req, ORIGIN);
    let resp: Value = req.send().await?.error_for_status()?.json().await?;
    check_playability(&resp)?;
    Ok(resp)
}

async fn player_web_safari(
    http: &reqwest::Client,
    video_id: &str,
    cfg: &WebConfig,
    session: &Session,
) -> anyhow::Result<Value> {
    const SAFARI_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/15.5 Safari/605.1.15";
    let mut client = json!({
        "clientName": "WEB",
        "clientVersion": cfg.client_version,
        "userAgent": SAFARI_UA,
        "hl": "en",
        "gl": "US",
    });
    if let Some(vd) = cfg.visitor_data.as_deref().filter(|v| !v.is_empty()) {
        client["visitorData"] = json!(vd);
    }
    let body = json!({
        "context": { "client": client },
        "videoId": video_id,
        "contentCheckOk": true,
        "racyCheckOk": true,
    });
    let mut req = http
        .post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false")
        .header("User-Agent", SAFARI_UA)
        .header("X-YouTube-Client-Name", "1")
        .header("X-YouTube-Client-Version", &cfg.client_version)
        .header("Origin", ORIGIN)
        .header("Referer", format!("{ORIGIN}/watch?v={video_id}"))
        .timeout(Duration::from_secs(10))
        .json(&body);
    if let Some(vd) = cfg.visitor_data.as_deref().filter(|v| !v.is_empty()) {
        req = req.header("X-Goog-Visitor-Id", vd);
    }
    req = session.sign(req, ORIGIN);
    let resp: Value = req.send().await?.error_for_status()?.json().await?;
    check_playability(&resp)?;
    Ok(resp)
}

fn is_noclen_live_tail(f: &Value, url: &str) -> bool {
    let has_clen = f["contentLength"]
        .as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| f["contentLength"].as_u64())
        .is_some_and(|n| n > 0);
    !has_clen
        && (url.contains("noclen=1")
            || (url.contains("yt_live_broadcast") && url.contains("&live=1")))
}

fn has_adaptive_video_url(v: &Value) -> bool {
    v["streamingData"]["adaptiveFormats"]
        .as_array()
        .is_some_and(|a| {
            a.iter().any(|f| {
                f["mimeType"]
                    .as_str()
                    .is_some_and(|m| m.starts_with("video/"))
                    && f["url"]
                        .as_str()
                        .is_some_and(|u| !is_noclen_live_tail(f, u))
            })
        })
}

fn has_playable_url(v: &Value) -> bool {
    let has_adaptive = v["streamingData"]["adaptiveFormats"]
        .as_array()
        .is_some_and(|a| {
            a.iter().any(|f| {
                f["url"]
                    .as_str()
                    .is_some_and(|u| !is_noclen_live_tail(f, u))
            })
        });
    let has_hls = v["streamingData"]["hlsManifestUrl"].is_string();
    has_adaptive || has_hls
}

fn is_preferred_player_response(v: &Value) -> bool {
    let is_live = v["videoDetails"]["isLive"].as_bool().unwrap_or(false);
    if is_live {
        has_playable_url(v)
    } else {
        has_adaptive_video_url(v)
    }
}

async fn player_visionos(
    http: &reqwest::Client,
    video_id: &str,
    visitor_data: Option<&str>,
) -> anyhow::Result<Value> {
    let mut client = json!({
        "clientName": "VISIONOS",
        "clientVersion": "1.02",
        "deviceMake": "Apple",
        "deviceModel": "RealityDevice17,1",
        "userAgent": VISIONOS_UA,
        "osName": "visionOS",
        "osVersion": "26.5.23O471",
        "hl": "en",
        "gl": "US",
    });
    if let Some(vd) = visitor_data.filter(|v| !v.is_empty()) {
        client["visitorData"] = json!(vd);
    }
    let body = json!({
        "context": { "client": client },
        "videoId": video_id,
        "contentCheckOk": true,
        "racyCheckOk": true,
    });
    let mut req = http
        .post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false")
        .header("User-Agent", VISIONOS_UA)
        .header("X-YouTube-Client-Name", "101")
        .header("X-YouTube-Client-Version", "1.02")
        .header("Origin", ORIGIN)
        .timeout(Duration::from_secs(10))
        .json(&body);
    if let Some(vd) = visitor_data.filter(|v| !v.is_empty()) {
        req = req.header("X-Goog-Visitor-Id", vd);
    }
    let resp: Value = req.send().await?.error_for_status()?.json().await?;
    check_playability(&resp)?;
    Ok(resp)
}

async fn player_android_vr(
    http: &reqwest::Client,
    video_id: &str,
    visitor_data: Option<&str>,
) -> anyhow::Result<Value> {
    const VERSION: &str = "1.73.21";
    let mut client = json!({
        "clientName": "ANDROID_VR",
        "clientVersion": VERSION,
        "deviceMake": "Pico",
        "deviceModel": "A8110",
        "osName": "Android",
        "osVersion": "10",
        "androidSdkVersion": 29,
        "hl": "en",
        "gl": "US",
    });
    if let Some(vd) = visitor_data.filter(|v| !v.is_empty()) {
        client["visitorData"] = json!(vd);
    }
    let body = json!({
        "context": { "client": client },
        "videoId": video_id,
        "contentCheckOk": true,
        "racyCheckOk": true,
    });
    let resp: Value = http
        .post("https://youtubei.googleapis.com/youtubei/v1/player?prettyPrint=false")
        .header(
            "User-Agent",
            format!("com.google.android.apps.youtube.vr.pico/{VERSION} (Linux; U; Android 10; A8110 Build/QKQ1) gzip"),
        )
        .header("X-YouTube-Client-Name", "28")
        .header("X-YouTube-Client-Version", VERSION)
        .timeout(Duration::from_secs(10))
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    check_playability(&resp)?;
    Ok(resp)
}

fn check_playability(resp: &Value) -> anyhow::Result<()> {
    let status = &resp["playabilityStatus"];
    if status["status"] != "OK" {
        let code = status["status"].as_str().unwrap_or("UNPLAYABLE");
        let reason = status["reason"].as_str().unwrap_or("Stream unavailable");
        anyhow::bail!("{code}: {reason}");
    }
    Ok(())
}

fn parse_next_metadata(
    next: &Value,
    current_id: &str,
) -> (
    String,
    String,
    String,
    bool,
    bool,
    bool,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Vec<VideoItem>,
) {
    let contents = &next["contents"]["twoColumnWatchNextResults"]["results"]["results"]["contents"];
    let mut views_text = String::new();
    let mut published_text = String::new();
    let mut likes_text = String::new();
    let mut is_liked = false;
    let mut is_disliked = false;
    let mut is_subscribed = false;
    let mut subscribers_text = String::new();
    let mut avatar_url = None;
    let mut comments_token = None;

    if let Some(arr) = contents.as_array() {
        for item in arr {
            let p = &item["videoPrimaryInfoRenderer"];
            if p.is_object() {
                if let Some(v) = text(&p["viewCount"]["videoViewCountRenderer"]["shortViewCountText"])
                    .or_else(|| text(&p["viewCount"]["videoViewCountRenderer"]["viewCount"]))
                {
                    views_text = v;
                }
                if let Some(d) = text(&p["dateText"]) {
                    published_text = d;
                }
                let mut like_status_entity_key: Option<String> = None;
                if let Some(btns) = p["videoActions"]["menuRenderer"]["topLevelButtons"].as_array() {
                    for b in btns {
                        let seg = &b["segmentedLikeDislikeButtonViewModel"];
                        if seg.is_object() {
                            let like_vm = &seg["likeButtonViewModel"]["likeButtonViewModel"];
                            if let Some(k) = like_vm["likeStatusEntityKey"]
                                .as_str()
                                .or_else(|| seg["likeStatusEntityKey"].as_str())
                            {
                                like_status_entity_key = Some(k.to_owned());
                            }
                            let like_toggle =
                                &like_vm["toggleButtonViewModel"]["toggleButtonViewModel"];
                            if let Some(t) =
                                like_toggle["defaultButtonViewModel"]["buttonViewModel"]["title"]
                                    .as_str()
                            {
                                likes_text = t.to_owned();
                            }
                            if let Some(toggled) = like_toggle["isToggled"].as_bool() {
                                is_liked = toggled;
                            }
                            let dislike_vm = &seg["dislikeButtonViewModel"]["dislikeButtonViewModel"];
                            if like_status_entity_key.is_none() {
                                if let Some(k) = dislike_vm["likeStatusEntityKey"].as_str() {
                                    like_status_entity_key = Some(k.to_owned());
                                }
                            }
                            let dislike_toggle =
                                &dislike_vm["toggleButtonViewModel"]["toggleButtonViewModel"];
                            if let Some(toggled) = dislike_toggle["isToggled"].as_bool() {
                                is_disliked = toggled;
                            }
                            // Also check inline likeStatusEntity if present directly on the view model
                            if let Some(status) = like_vm["likeStatusEntity"]["likeStatus"]
                                .as_str()
                                .or_else(|| seg["likeStatusEntity"]["likeStatus"].as_str())
                            {
                                match status {
                                    "LIKE" => {
                                        is_liked = true;
                                        is_disliked = false;
                                    }
                                    "DISLIKE" => {
                                        is_liked = false;
                                        is_disliked = true;
                                    }
                                    "INDIFFERENT" => {
                                        is_liked = false;
                                        is_disliked = false;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        let legacy_seg = &b["segmentedLikeDislikeButtonRenderer"];
                        if legacy_seg.is_object() {
                            let lt = &legacy_seg["likeButton"]["toggleButtonRenderer"];
                            if likes_text.is_empty() {
                                if let Some(t) = text(&lt["defaultText"]) {
                                    likes_text = t;
                                }
                            }
                            if let Some(toggled) = lt["isToggled"].as_bool() {
                                is_liked = toggled;
                            }
                            let dt = &legacy_seg["dislikeButton"]["toggleButtonRenderer"];
                            if let Some(toggled) = dt["isToggled"].as_bool() {
                                is_disliked = toggled;
                            }
                        }
                    }
                }
                // InnerTube /next stores authoritative user like state in frameworkUpdates mutations
                if let Some(mutations) =
                    next["frameworkUpdates"]["entityBatchUpdate"]["mutations"].as_array()
                {
                    let matched = like_status_entity_key
                        .as_deref()
                        .and_then(|key| {
                            mutations.iter().find(|m| {
                                m["entityKey"].as_str() == Some(key)
                                    && m["payload"]["likeStatusEntity"]["likeStatus"].is_string()
                            })
                        })
                        .or_else(|| {
                            mutations.iter().find(|m| {
                                m["payload"]["likeStatusEntity"]["likeStatus"].is_string()
                            })
                        });
                    if let Some(status) =
                        matched.and_then(|m| m["payload"]["likeStatusEntity"]["likeStatus"].as_str())
                    {
                        match status {
                            "LIKE" => {
                                is_liked = true;
                                is_disliked = false;
                            }
                            "DISLIKE" => {
                                is_liked = false;
                                is_disliked = true;
                            }
                            "INDIFFERENT" => {
                                is_liked = false;
                                is_disliked = false;
                            }
                            _ => {}
                        }
                    }
                }
            }
            let s = &item["videoSecondaryInfoRenderer"];
            if s.is_object() {
                let owner = &s["owner"]["videoOwnerRenderer"];
                if let Some(sub) = text(&owner["subscriberCountText"]) {
                    subscribers_text = sub;
                }
                if let Some(url) = owner["thumbnail"]["thumbnails"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|t| t["url"].as_str())
                {
                    avatar_url = Some(normalize_url(url));
                }
                if let Some(subbed) = s["subscribeButton"]["subscribeButtonRenderer"]["subscribed"].as_bool() {
                    is_subscribed = subbed;
                }
            }
            let sec = &item["itemSectionRenderer"];
            if sec.is_object()
                && (sec["sectionIdentifier"] == "comment-item-section"
                    || sec["targetId"] == "comments-section")
            {
                if let Some(tok) = sec["contents"]
                    .as_array()
                    .and_then(|c| c.first())
                    .and_then(|x| {
                        x["continuationItemRenderer"]["continuationEndpoint"]
                            ["continuationCommand"]["token"]
                            .as_str()
                    })
                {
                    comments_token = Some(tok.to_owned());
                }
            }
        }
    }

    let live_chat_token = next["contents"]["twoColumnWatchNextResults"]["conversationBar"]
        ["liveChatRenderer"]["continuations"]
        .as_array()
        .and_then(|arr| {
            arr.iter().find_map(|c| {
                c["reloadContinuationData"]["continuation"]
                    .as_str()
                    .or_else(|| c["invalidationContinuationData"]["continuation"].as_str())
                    .or_else(|| c["timedContinuationData"]["continuation"].as_str())
            })
        })
        .map(str::to_owned);

    let mut related = Vec::new();
    let mut seen = HashSet::new();
    seen.insert(current_id.to_owned());
    let sec = &next["contents"]["twoColumnWatchNextResults"]["secondaryResults"];
    collect_videos(sec, &mut related, &mut seen);

    (
        views_text,
        published_text,
        likes_text,
        is_liked,
        is_disliked,
        is_subscribed,
        subscribers_text,
        avatar_url,
        comments_token,
        live_chat_token,
        related,
    )
}

fn as_secs_f64(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
}

fn normalize_url(u: &str) -> String {
    if u.starts_with("//") {
        format!("https:{u}")
    } else {
        u.to_owned()
    }
}

fn text(v: &Value) -> Option<String> {
    if let Some(s) = v["simpleText"].as_str() {
        return Some(s.to_owned());
    }
    if let Some(s) = v["content"].as_str() {
        return Some(s.to_owned());
    }
    if let Some(runs) = v["runs"].as_array() {
        let joined: String = runs.iter().filter_map(|r| r["text"].as_str()).collect();
        if !joined.is_empty() {
            return Some(joined);
        }
    }
    None
}

fn pick_thumb(v: &Value, video_id: &str) -> String {
    if let Some(arr) = v.as_array() {
        if let Some(best) = arr
            .iter()
            .filter_map(|t| Some((t["width"].as_u64().unwrap_or(0), t["url"].as_str()?)))
            .filter(|(w, _)| *w >= 320)
            .min_by_key(|(w, _)| *w)
            .or_else(|| {
                arr.iter()
                    .filter_map(|t| Some((t["width"].as_u64().unwrap_or(0), t["url"].as_str()?)))
                    .max_by_key(|(w, _)| *w)
            })
        {
            return normalize_url(best.1);
        }
    }
    format!("https://i.ytimg.com/vi/{video_id}/mqdefault.jpg")
}

fn collect_chips(v: &Value) -> Vec<FeedChip> {
    let mut out = Vec::new();
    fn walk(v: &Value, out: &mut Vec<FeedChip>) {
        match v {
            Value::Object(map) => {
                if let Some(chips) = map
                    .get("feedFilterChipBarRenderer")
                    .or_else(|| map.get("chipCloudRenderer"))
                    .and_then(|c| c.get("contents"))
                    .or_else(|| map.get("chipCloudRenderer").and_then(|c| c.get("chips")))
                    .and_then(|c| c.as_array())
                {
                    for c in chips {
                        let cr = &c["chipCloudChipRenderer"];
                        if let Some(title) = text(&cr["text"]) {
                            let selected = cr["isSelected"].as_bool().unwrap_or(false);
                            let params = cr["navigationEndpoint"]["browseEndpoint"]["params"]
                                .as_str()
                                .map(str::to_owned);
                            out.push(FeedChip {
                                title,
                                params,
                                selected,
                            });
                        }
                    }
                    return;
                }
                for val in map.values() {
                    if !out.is_empty() {
                        return;
                    }
                    walk(val, out);
                }
            }
            Value::Array(arr) => {
                for val in arr {
                    if !out.is_empty() {
                        return;
                    }
                    walk(val, out);
                }
            }
            _ => {}
        }
    }
    walk(v, &mut out);
    out
}

fn collect_videos(v: &Value, out: &mut Vec<VideoItem>, seen: &mut HashSet<String>) {
    match v {
        Value::Object(map) => {
            if let Some(vr) = map.get("videoRenderer").or_else(|| map.get("compactVideoRenderer")) {
                if let Some(item) = parse_video_renderer(vr) {
                    if seen.insert(item.id.clone()) {
                        out.push(item);
                    }
                }
            } else if let Some(pvr) = map.get("playlistVideoRenderer") {
                if let Some(item) = parse_playlist_video_renderer(pvr) {
                    if seen.insert(item.id.clone()) {
                        out.push(item);
                    }
                }
            } else if let Some(lm) = map.get("lockupViewModel") {
                if let Some(item) = parse_lockup_view_model(lm) {
                    if seen.insert(item.id.clone()) {
                        out.push(item);
                    }
                }
            }
            for val in map.values() {
                collect_videos(val, out, seen);
            }
        }
        Value::Array(arr) => {
            for val in arr {
                collect_videos(val, out, seen);
            }
        }
        _ => {}
    }
}

fn parse_video_renderer(vr: &Value) -> Option<VideoItem> {
    let id = vr["videoId"].as_str()?.to_owned();
    let title = text(&vr["title"])?;
    let channel_run = vr["ownerText"]["runs"]
        .get(0)
        .or_else(|| vr["longBylineText"]["runs"].get(0))
        .or_else(|| vr["shortBylineText"]["runs"].get(0));
    let channel = channel_run
        .and_then(|r| r["text"].as_str())
        .map(str::to_owned)
        .or_else(|| text(&vr["ownerText"]))
        .or_else(|| text(&vr["shortBylineText"]))
        .unwrap_or_default();
    let channel_id = channel_run
        .and_then(|r| r["navigationEndpoint"]["browseEndpoint"]["browseId"].as_str())
        .map(str::to_owned);
    let channel_avatar_url = vr["channelThumbnailSupportedRenderers"]
        ["channelThumbnailWithLinkRenderer"]["thumbnail"]["thumbnails"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|t| t["url"].as_str())
        .map(normalize_url)
        .or_else(|| find_first_thumb_url(&vr["channelThumbnailSupportedRenderers"]))
        .or_else(|| find_first_thumb_url(&vr["channelThumbnail"]));
    let views_text = text(&vr["shortViewCountText"])
        .or_else(|| text(&vr["viewCountText"]))
        .unwrap_or_default();
    let published_text = text(&vr["publishedTimeText"]).unwrap_or_default();
    let duration_text = text(&vr["lengthText"]).unwrap_or_default();
    let is_live = duration_text.is_empty()
        && vr["badges"].as_array().is_some_and(|b| {
            b.iter()
                .any(|x| x["metadataBadgeRenderer"]["style"] == "BADGE_STYLE_TYPE_LIVE_NOW")
        });
    let thumbnail_url = pick_thumb(&vr["thumbnail"]["thumbnails"], &id);
    let watched_percent = vr["thumbnailOverlays"].as_array().and_then(|arr| {
        arr.iter().find_map(|o| {
            o["thumbnailOverlayResumePlaybackRenderer"]["percentDurationWatched"]
                .as_f64()
                .filter(|&p| p > 0.0)
                .map(|p| (p as f32 / 100.0).clamp(0.0, 1.0))
        })
    });
    let resume_secs = vr["navigationEndpoint"]["watchEndpoint"]["startTimeSeconds"]
        .as_f64()
        .filter(|&s| s > 2.0);

    Some(VideoItem {
        id,
        title,
        channel,
        channel_id,
        channel_avatar_url,
        views_text,
        published_text,
        duration_text: if is_live { "LIVE".to_owned() } else { duration_text },
        thumbnail_url,
        is_live,
        watched_percent,
        resume_secs,
        playlist_browse_id: None,
        playlist_set_video_id: None,
    })
}

fn parse_playlist_video_renderer(pvr: &Value) -> Option<VideoItem> {
    let id = pvr["videoId"].as_str()?.to_owned();
    let title = text(&pvr["title"])?;
    let channel_run = pvr["shortBylineText"]["runs"].get(0);
    let channel = channel_run
        .and_then(|r| r["text"].as_str())
        .map(str::to_owned)
        .or_else(|| text(&pvr["shortBylineText"]))
        .unwrap_or_default();
    let channel_id = channel_run
        .and_then(|r| r["navigationEndpoint"]["browseEndpoint"]["browseId"].as_str())
        .map(str::to_owned);
    let duration_text = text(&pvr["lengthText"]).unwrap_or_default();
    let thumbnail_url = pick_thumb(&pvr["thumbnail"]["thumbnails"], &id);
    let watched_percent = pvr["thumbnailOverlays"].as_array().and_then(|arr| {
        arr.iter().find_map(|o| {
            o["thumbnailOverlayResumePlaybackRenderer"]["percentDurationWatched"]
                .as_f64()
                .filter(|&p| p > 0.0)
                .map(|p| (p as f32 / 100.0).clamp(0.0, 1.0))
        })
    });
    let resume_secs = pvr["navigationEndpoint"]["watchEndpoint"]["startTimeSeconds"]
        .as_f64()
        .filter(|&s| s > 2.0);
    let playlist_set_video_id = pvr["setVideoId"].as_str().map(str::to_owned);
    Some(VideoItem {
        id,
        title,
        channel,
        channel_id,
        channel_avatar_url: None,
        views_text: String::new(),
        published_text: String::new(),
        duration_text,
        thumbnail_url,
        is_live: false,
        watched_percent,
        resume_secs,
        playlist_browse_id: None,
        playlist_set_video_id,
    })
}

fn parse_lockup_view_model(lm: &Value) -> Option<VideoItem> {
    let ctype = lm["contentType"].as_str().unwrap_or_default();
    if !ctype.is_empty() && !ctype.contains("VIDEO") {
        return None;
    }
    let id = lm["contentId"].as_str()?.to_owned();
    if id.len() != 11 {
        return None;
    }
    let meta = &lm["metadata"]["lockupMetadataViewModel"];
    let title = meta["title"]["content"].as_str()?.to_owned();

    let mut channel = String::new();
    let mut views_text = String::new();
    let mut published_text = String::new();

    if let Some(rows) = meta["metadata"]["contentMetadataViewModel"]["metadataRows"].as_array() {
        for (idx, row) in rows.iter().enumerate() {
            let parts: Vec<&str> = row["metadataParts"]
                .as_array()
                .map(|a| a.iter().filter_map(|p| p["text"]["content"].as_str()).collect())
                .unwrap_or_default();
            if idx == 0 && !parts.is_empty() {
                if parts[0].contains("view") || parts[0].contains("watching") {
                    views_text = parts[0].to_owned();
                    for &p in &parts[1..] {
                        if published_text.is_empty() {
                            published_text = p.to_owned();
                        }
                    }
                } else {
                    channel = parts[0].to_owned();
                }
            } else {
                for p in parts {
                    if p.contains("view") || p.contains("watching") {
                        views_text = p.to_owned();
                    } else if published_text.is_empty() {
                        published_text = p.to_owned();
                    }
                }
            }
        }
    }

    let channel_avatar_url = find_first_thumb_url(&meta["image"]);

    let thumb_vm = &lm["contentImage"]["thumbnailViewModel"];
    let thumbnail_url = pick_thumb(&thumb_vm["image"]["sources"], &id);
    let (duration_text, mut watched_percent) = extract_lockup_thumb_meta(thumb_vm);
    let is_live = duration_text.eq_ignore_ascii_case("LIVE");
    let channel_id = meta["image"]["decoratedAvatarViewModel"]["rendererContext"]
        ["commandContext"]["onTap"]["innertubeCommand"]["browseEndpoint"]["browseId"]
        .as_str()
        .filter(|s| s.starts_with("UC"))
        .map(str::to_owned)
        .or_else(|| find_first_uc_browse_id(&meta["image"]))
        .or_else(|| find_first_uc_browse_id(meta));
    let mut resume_secs = find_start_time_seconds(lm);
    if watched_percent.is_none() {
        if let (Some(rs), Some(dur)) = (resume_secs, parse_timestamp(&duration_text)) {
            if dur > 10.0 && rs > 2.0 {
                watched_percent = Some(((rs / dur) as f32).clamp(0.01, 1.0));
            }
        }
    } else if resume_secs.is_none() {
        if let (Some(pct), Some(dur)) = (watched_percent, parse_timestamp(&duration_text)) {
            if dur > 10.0 && pct > 0.02 && pct < 0.96 {
                resume_secs = Some(f64::from(pct) * dur);
            }
        }
    }

    Some(VideoItem {
        id,
        title,
        channel,
        channel_id,
        channel_avatar_url,
        views_text,
        published_text,
        duration_text,
        thumbnail_url,
        is_live,
        watched_percent,
        resume_secs,
        playlist_browse_id: None,
        playlist_set_video_id: None,
    })
}

fn find_first_uc_browse_id(v: &Value) -> Option<String> {
    match v {
        Value::Object(map) => {
            if let Some(bid) = map
                .get("browseEndpoint")
                .and_then(|b| b.get("browseId"))
                .and_then(|s| s.as_str())
                .filter(|s| s.starts_with("UC"))
            {
                return Some(bid.to_owned());
            }
            for val in map.values() {
                if let Some(bid) = find_first_uc_browse_id(val) {
                    return Some(bid);
                }
            }
            None
        }
        Value::Array(arr) => arr.iter().find_map(find_first_uc_browse_id),
        _ => None,
    }
}

fn extract_lockup_thumb_meta(v: &Value) -> (String, Option<f32>) {
    let mut duration = String::new();
    let mut watched_pct: Option<f32> = None;
    fn walk(v: &Value, duration: &mut String, watched_pct: &mut Option<f32>) {
        match v {
            Value::Object(map) => {
                if let Some(badge) = map.get("thumbnailBadgeViewModel") {
                    if let Some(t) = badge["text"].as_str() {
                        if t.eq_ignore_ascii_case("LIVE") || t.contains(':') {
                            *duration = t.to_owned();
                        } else if duration.is_empty() {
                            *duration = t.to_owned();
                        }
                    }
                }
                for key in [
                    "startPercent",
                    "percentDurationWatched",
                    "startPercentage",
                    "percentage",
                ] {
                    if watched_pct.is_none() {
                        if let Some(p) = map.get(key).and_then(as_secs_f64).filter(|&p| p > 0.0) {
                            let norm = if p > 1.0 { p / 100.0 } else { p };
                            *watched_pct = Some((norm as f32).clamp(0.0, 1.0));
                        }
                    }
                }
                for val in map.values() {
                    walk(val, duration, watched_pct);
                }
            }
            Value::Array(arr) => {
                for val in arr {
                    walk(val, duration, watched_pct);
                }
            }
            _ => {}
        }
    }
    walk(v, &mut duration, &mut watched_pct);
    (duration, watched_pct)
}

fn find_start_time_seconds(v: &Value) -> Option<f64> {
    match v {
        Value::Object(map) => {
            if let Some(we) = map.get("watchEndpoint") {
                if let Some(s) = as_secs_f64(&we["startTimeSeconds"]).filter(|&s| s > 2.0) {
                    return Some(s);
                }
            }
            for val in map.values() {
                if let Some(s) = find_start_time_seconds(val) {
                    return Some(s);
                }
            }
            None
        }
        Value::Array(arr) => arr.iter().find_map(find_start_time_seconds),
        _ => None,
    }
}

fn find_continuation(v: &Value) -> Option<String> {
    match v {
        Value::Object(map) => {
            if let Some(tok) = map
                .get("continuationCommand")
                .and_then(|c| c.get("token"))
                .and_then(|t| t.as_str())
            {
                return Some(tok.to_owned());
            }
            for val in map.values() {
                if let Some(tok) = find_continuation(val) {
                    return Some(tok);
                }
            }
            None
        }
        Value::Array(arr) => {
            for val in arr {
                if let Some(tok) = find_continuation(val) {
                    return Some(tok);
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_video_ids_from_various_youtube_links() {
        assert_eq!(extract_video_id("dQw4w9WgXcQ").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(
            extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42s").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(
            extract_video_id("https://youtu.be/dQw4w9WgXcQ?si=abc").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(
            extract_video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(extract_video_id("rust egui tutorial"), None);
    }

    #[test]
    fn parses_chapters_from_description() {
        let desc = "Check this out:\n0:00 Intro\n01:30 - Main Topic\n1:02:10 Outro";
        let ch = parse_chapters(desc, 4000.0);
        assert_eq!(ch.len(), 3);
        assert_eq!((ch[0].start_secs, ch[0].title.as_str()), (0.0, "Intro"));
        assert_eq!((ch[1].start_secs, ch[1].title.as_str()), (90.0, "Main Topic"));
        assert_eq!((ch[2].start_secs, ch[2].title.as_str()), (3730.0, "Outro"));
    }

    #[test]
    fn parses_storyboard_spec_and_computes_frame_uv() {
        let spec = "https://i.ytimg.com/sb/abc/storyboard3_L$L/$N.jpg?sqp=1|48#27#10#10#10#1000#default#s0|160#90#12#5#2#5000#M$M#s2";
        let sb = parse_storyboard(spec).unwrap();
        assert_eq!((sb.level, sb.width, sb.cols, sb.rows), (1, 160, 5, 2));
        // 0s -> frame 0 -> sheet 0, col 0, row 0 (10 frames on sheet 0 -> 2 rows)
        let (u0, uv0) = sb.frame_at(0.0, 60.0).unwrap();
        assert_eq!(u0, "https://i.ytimg.com/sb/abc/storyboard3_L1/M0.jpg?sqp=1&sigh=s2");
        assert!((uv0[0] - 0.0).abs() < 1e-4 && (uv0[2] - 0.2).abs() < 1e-4);
        assert!((uv0[1] - 0.0).abs() < 1e-4 && (uv0[3] - 0.5).abs() < 1e-4);
        // 50s -> frame 10 -> sheet 1 (only 2 frames -> 1 row), col 0, row 0
        let (u1, uv1) = sb.frame_at(50.0, 60.0).unwrap();
        assert_eq!(u1, "https://i.ytimg.com/sb/abc/storyboard3_L1/M1.jpg?sqp=1&sigh=s2");
        assert!((uv1[1] - 0.0).abs() < 1e-4 && (uv1[3] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn parses_resume_bar_and_multi_audio_tracks() {
        let vr = json!({
            "videoId": "abcdefghijk",
            "title": { "simpleText": "Test" },
            "thumbnailOverlays": [{
                "thumbnailOverlayResumePlaybackRenderer": { "percentDurationWatched": 45 }
            }],
            "navigationEndpoint": { "watchEndpoint": { "startTimeSeconds": 128 } }
        });
        let item = parse_video_renderer(&vr).unwrap();
        assert_eq!(item.watched_percent, Some(0.45));
        assert_eq!(item.resume_secs, Some(128.0));

        let adaptive = vec![
            json!({
                "itag": 140,
                "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"",
                "bitrate": 128000,
                "url": "https://example.com/id",
                "audioTrack": { "id": "id.3", "displayName": "Indonesian", "audioIsDefault": false }
            }),
            json!({
                "itag": 251,
                "mimeType": "audio/webm; codecs=\"opus\"",
                "bitrate": 160000,
                "url": "https://example.com/en",
                "audioTrack": { "id": "en.4", "displayName": "English original", "audioIsDefault": true }
            }),
        ];
        let tracks = parse_audio_tracks(&adaptive);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].label, "English original");
        assert!(tracks[0].is_default);
        assert_eq!(tracks[1].label, "Indonesian");

        let add_to = json!({
            "contents": [{
                "addToPlaylistRenderer": {
                    "playlists": [
                        { "playlistAddToOptionRenderer": { "playlistId": "WL", "title": { "simpleText": "Watch Later" }, "containsSelectedVideos": "NONE" } },
                        { "playlistAddToOptionRenderer": { "playlistId": "PL123", "title": { "simpleText": "Favorites" }, "containsSelectedVideos": "ALL" } }
                    ]
                }
            }]
        });
        let opts = parse_add_to_playlist(&add_to);
        assert_eq!(opts.len(), 2);
        assert_eq!((opts[0].id.as_str(), opts[0].is_added), ("WL", false));
        assert_eq!((opts[1].id.as_str(), opts[1].is_added), ("PL123", true));

        let lockup = json!({
            "contentType": "LOCKUP_CONTENT_TYPE_VIDEO",
            "contentId": "dQw4w9WgXcQ",
            "contentImage": {
                "thumbnailViewModel": {
                    "overlays": [{
                        "thumbnailBottomOverlayViewModel": {
                            "badges": [{
                                "thumbnailBadgeViewModel": { "text": "3:32" }
                            }],
                            "progressBar": {
                                "thumbnailOverlayProgressBarViewModel": { "startPercent": 52 }
                            }
                        }
                    }]
                }
            },
            "metadata": {
                "lockupMetadataViewModel": {
                    "title": { "content": "Song" }
                }
            }
        });
        let lm_item = parse_lockup_view_model(&lockup).unwrap();
        assert_eq!(lm_item.duration_text, "3:32");
        assert_eq!(lm_item.watched_percent, Some(0.52));
        assert!(lm_item.resume_secs.is_some());
    }

    #[test]
    fn parses_like_status_from_framework_updates() {
        let next = json!({
            "contents": {
                "twoColumnWatchNextResults": {
                    "results": {
                        "results": {
                            "contents": [{
                                "videoPrimaryInfoRenderer": {
                                    "videoActions": {
                                        "menuRenderer": {
                                            "topLevelButtons": [{
                                                "segmentedLikeDislikeButtonViewModel": {
                                                    "likeButtonViewModel": {
                                                        "likeButtonViewModel": {
                                                            "likeStatusEntityKey": "LIKE_KEY_1",
                                                            "toggleButtonViewModel": {
                                                                "toggleButtonViewModel": {
                                                                    "isToggled": false,
                                                                    "defaultButtonViewModel": {
                                                                        "buttonViewModel": { "title": "313" }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    },
                                                    "dislikeButtonViewModel": {
                                                        "dislikeButtonViewModel": {
                                                            "toggleButtonViewModel": {
                                                                "toggleButtonViewModel": { "isToggled": false }
                                                            }
                                                        }
                                                    }
                                                }
                                            }]
                                        }
                                    }
                                }
                            }]
                        }
                    }
                }
            },
            "frameworkUpdates": {
                "entityBatchUpdate": {
                    "mutations": [{
                        "entityKey": "LIKE_KEY_1",
                        "payload": {
                            "likeStatusEntity": {
                                "likeStatus": "LIKE"
                            }
                        }
                    }]
                }
            }
        });
        let (_, _, likes, liked, disliked, _, _, _, _, _, _) =
            parse_next_metadata(&next, "abc12345678");
        assert_eq!(likes, "313");
        assert!(liked);
        assert!(!disliked);
    }

    #[test]
    fn archived_vod_with_live_chat_token_is_not_marked_live() {
        let archived = json!({
            "contents": {
                "twoColumnWatchNextResults": {
                    "results": {
                        "results": {
                            "contents": [{
                                "videoPrimaryInfoRenderer": {
                                    "viewCount": {
                                        "videoViewCountRenderer": { "isLive": false }
                                    }
                                }
                            }]
                        }
                    },
                    "conversationBar": {
                        "liveChatRenderer": {
                            "continuations": [{
                                "reloadContinuationData": { "continuation": "REPLAY_TOKEN" }
                            }]
                        }
                    }
                }
            }
        });
        assert!(!is_next_live(&archived));

        let active = json!({
            "contents": {
                "twoColumnWatchNextResults": {
                    "results": {
                        "results": {
                            "contents": [{
                                "videoPrimaryInfoRenderer": {
                                    "viewCount": {
                                        "videoViewCountRenderer": { "isLive": true }
                                    }
                                }
                            }]
                        }
                    }
                }
            }
        });
        assert!(is_next_live(&active));
    }

    #[test]
    fn parses_hls_master_playlist_variants_and_audio() {
        let m3u8 = "\
#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"234\",NAME=\"English\",DEFAULT=YES,URI=\"audio/234.m3u8\"
#EXT-X-STREAM-INF:BANDWIDTH=1200000,RESOLUTION=854x480,FRAME-RATE=30.000,CODECS=\"avc1.4d401f\",AUDIO=\"234\"
480p.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=4500000,RESOLUTION=1920x1080,FRAME-RATE=60.000,CODECS=\"avc1.64002a\",AUDIO=\"234\"
1080p60.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=2500000,RESOLUTION=1280x720,FRAME-RATE=30.000,CODECS=\"avc1.4d401f\",AUDIO=\"234\"
720p.m3u8
";
        let (fmts, audio) = parse_hls_master_variants(
            m3u8,
            "https://manifest.googlevideo.com/api/manifest/hls_variant/master.m3u8",
        );
        assert_eq!(audio.len(), 1);
        assert_eq!(
            audio[0].url,
            "https://manifest.googlevideo.com/api/manifest/hls_variant/audio/234.m3u8"
        );
        assert_eq!(fmts.len(), 3);
        assert_eq!((fmts[0].height, fmts[0].fps, fmts[0].has_audio), (1080, 60, false));
        assert_eq!(
            fmts[0].url,
            "https://manifest.googlevideo.com/api/manifest/hls_variant/1080p60.m3u8"
        );
        assert_eq!((fmts[1].height, fmts[2].height), (720, 480));
    }

    #[test]
    fn classifies_login_required_and_playable_urls() {
        assert!(is_login_or_age_error(&anyhow::anyhow!(
            "LOGIN_REQUIRED: Sign in to confirm your age"
        )));
        assert!(!is_login_or_age_error(&anyhow::anyhow!(
            "VIDEO_NOT_FOUND: Video unavailable"
        )));
        let no_url = json!({
            "streamingData": {
                "adaptiveFormats": [{ "itag": 137, "signatureCipher": "s=abc" }]
            }
        });
        assert!(!has_playable_url(&no_url));
        let with_url = json!({
            "streamingData": {
                "hlsManifestUrl": "https://manifest.googlevideo.com/live.m3u8"
            }
        });
        assert!(has_playable_url(&with_url));
        assert!(!is_preferred_player_response(&with_url));
        let live_hls = json!({
            "videoDetails": { "isLive": true },
            "streamingData": {
                "hlsManifestUrl": "https://manifest.googlevideo.com/live.m3u8"
            }
        });
        assert!(is_preferred_player_response(&live_hls));

        let post_live_noclen = json!({
            "videoDetails": { "isLive": false, "lengthSeconds": "11456" },
            "streamingData": {
                "adaptiveFormats": [{
                    "itag": 299,
                    "mimeType": "video/mp4; codecs=\"avc1.64002a\"",
                    "url": "https://rr4.googlevideo.com/videoplayback?source=yt_live_broadcast&live=1&noclen=1"
                }],
                "hlsManifestUrl": "https://manifest.googlevideo.com/vod.m3u8"
            }
        });
        assert!(!has_adaptive_video_url(&post_live_noclen));
        assert!(has_playable_url(&post_live_noclen));
        assert!(!is_preferred_player_response(&post_live_noclen));
    }

    #[test]
    fn parses_playlist_video_set_video_id() {
        let pvr = json!({
            "videoId": "dQw4w9WgXcQ",
            "title": { "simpleText": "Track 1" },
            "setVideoId": "SET_VID_ABC",
            "lengthText": { "simpleText": "3:32" }
        });
        let item = parse_playlist_video_renderer(&pvr).unwrap();
        assert_eq!(item.playlist_set_video_id.as_deref(), Some("SET_VID_ABC"));
    }

    #[test]
    fn parses_live_chat_replay_superchat_membership_and_emotes() {
        let resp = json!({
            "continuationContents": {
                "liveChatContinuation": {
                    "continuations": [{
                        "liveChatReplayContinuationData": { "continuation": "NEXT_REPLAY_TOK" }
                    }],
                    "actions": [
                        {
                            "replayChatItemAction": {
                                "videoOffsetTimeMsec": "42500",
                                "actions": [{
                                    "addChatItemAction": {
                                        "item": {
                                            "liveChatPaidMessageRenderer": {
                                                "id": "sc1",
                                                "authorName": { "simpleText": "Supporter" },
                                                "purchaseAmountText": { "simpleText": "$5.00" },
                                                "bodyBackgroundColor": 4280191205u64,
                                                "message": {
                                                    "runs": [
                                                        { "text": "Great stream " },
                                                        {
                                                            "emoji": {
                                                                "emojiId": "UC/EM1",
                                                                "isCustomEmoji": true,
                                                                "shortcuts": [":hype:"],
                                                                "image": { "thumbnails": [{ "url": "https://yt.gg/em1.png" }] }
                                                            }
                                                        }
                                                    ]
                                                }
                                            }
                                        }
                                    }
                                }]
                            }
                        },
                        {
                            "addChatItemAction": {
                                "item": {
                                    "liveChatMembershipItemRenderer": {
                                        "id": "mem1",
                                        "authorName": { "simpleText": "NewMem" },
                                        "headerSubtext": { "simpleText": "Member for 2 months" }
                                    }
                                }
                            }
                        }
                    ]
                }
            }
        });
        let page = parse_live_chat_response(&resp);
        assert!(page.is_replay);
        assert_eq!(page.next_token.as_deref(), Some("NEXT_REPLAY_TOK"));
        assert_eq!(page.messages.len(), 2);
        assert_eq!(page.messages[0].offset_ms, Some(42500));
        assert!(matches!(
            page.messages[0].kind,
            LiveChatKind::SuperChat { ref amount, .. } if amount == "$5.00"
        ));
        assert!(matches!(
            page.messages[1].kind,
            LiveChatKind::Membership { ref header } if header == "Member for 2 months"
        ));
        assert_eq!(page.emotes.len(), 1);
        let segs = build_live_chat_segments("hi :hype: bye", &page.emotes);
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[1]["emojiId"], "UC/EM1");
    }

    #[test]
    fn parses_comment_pinned_heart_and_like_params() {
        let mut entity_map = std::collections::HashMap::new();
        entity_map.insert(
            "K1".to_owned(),
            json!({
                "properties": { "commentId": "C1", "publishedTime": "1d ago", "content": { "content": "Hello!" } },
                "author": { "displayName": "Alice" },
                "toolbar": {
                    "likeCountNotliked": "42",
                    "likeState": "TOOLBAR_LIKE_STATE_INDIFFERENT",
                    "heartState": "TOOLBAR_HEART_STATE_HEARTED"
                }
            }),
        );
        let mut surface_map = std::collections::HashMap::new();
        surface_map.insert(
            "S1".to_owned(),
            json!({
                "likeCommand": { "innertubeCommand": { "performCommentActionEndpoint": { "action": "ACT_LIKE" } } },
                "unlikeCommand": { "innertubeCommand": { "performCommentActionEndpoint": { "action": "ACT_UNLIKE" } } }
            }),
        );
        let entry = json!({
            "commentThreadRenderer": {
                "commentViewModel": {
                    "commentViewModel": {
                        "commentKey": "K1",
                        "commentSurfaceKey": "S1",
                        "pinnedText": "Pinned by Creator"
                    }
                }
            }
        });
        let item = parse_comment_entry(&entry, &entity_map, &surface_map).unwrap();
        assert!(item.is_pinned);
        assert!(item.has_creator_heart);
        assert!(!item.is_liked);
        assert_eq!(item.like_action_params.as_deref(), Some("ACT_LIKE"));
        assert_eq!(item.unlike_action_params.as_deref(), Some("ACT_UNLIKE"));
    }
}
