use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, Sender, unbounded};
use serde::{Deserialize, Serialize};

use crate::auth::Session;
use crate::innertube::{self, FeedPage, WebConfig};
use crate::model::{
    ChannelInfo, CommentItem, FeedChip, LiveChatMessage, PlaylistOption, PlaylistSummary,
    PreferredQuality, SidebarChannel, SponsorSegment, VideoFormat, VideoItem, WatchData,
};
use crate::proxy::StreamProxy;
use crate::{login, mpv, sponsorblock, ytdlp};

pub enum Request {
    LoadHome {
        params: Option<String>,
        continuation: Option<String>,
    },
    LoadSubscriptions {
        continuation: Option<String>,
    },
    LoadHistory {
        continuation: Option<String>,
    },
    LoadPlaylist {
        browse_id: String,
        continuation: Option<String>,
    },
    LoadTrending,
    LoadChannel {
        channel_id: String,
        params: Option<String>,
        continuation: Option<String>,
    },
    SearchChannel {
        channel_id: String,
        query: String,
        continuation: Option<String>,
    },
    Search {
        query: String,
        continuation: Option<String>,
    },
    SearchSuggest {
        query: String,
    },
    Watch {
        video_id: String,
        quality: PreferredQuality,
        sync_history: bool,
    },
    LoadComments {
        video_id: String,
        continuation: String,
        append: bool,
    },
    LoadCommentReplies {
        video_id: String,
        comment_id: String,
        continuation: String,
    },
    PostComment {
        video_id: String,
        text: String,
        params: String,
        reload_continuation: Option<String>,
    },
    PostCommentReply {
        video_id: String,
        comment_id: String,
        text: String,
        params: String,
        replies_continuation: Option<String>,
    },
    LoadLiveChat {
        video_id: String,
        continuation: String,
    },
    SendLiveChat {
        video_id: String,
        params: String,
        message: String,
    },
    RemoveFromHistory {
        video_id: String,
        feedback_token: String,
    },
    SetLike {
        video_id: String,
        like: bool,
    },
    SetDislike {
        video_id: String,
        dislike: bool,
    },
    SetWatchLater {
        video_id: String,
        add: bool,
    },
    EditPlaylist {
        playlist_id: String,
        playlist_title: String,
        video_id: String,
        add: bool,
    },
    LoadPlaylistOptions {
        video_id: String,
    },
    CreatePlaylist {
        title: String,
        video_ids: Vec<String>,
    },
    SetSubscribe {
        channel_id: String,
        subscribe: bool,
    },
    DownloadVideo {
        video_id: String,
        title: String,
        audio_only: bool,
        max_height: u32,
        output_dir: Option<PathBuf>,
    },
    SignIn,
    SignOut,
    DownloadLibmpv,
    ResolveChannelAvatars {
        channel_ids: Vec<String>,
    },
}

pub enum Response {
    HomeLoaded {
        items: Vec<VideoItem>,
        chips: Vec<FeedChip>,
        continuation: Option<String>,
        append: bool,
        is_fallback_trending: bool,
    },
    SubscriptionsLoaded {
        items: Vec<VideoItem>,
        continuation: Option<String>,
        append: bool,
    },
    HistoryLoaded {
        items: Vec<VideoItem>,
        continuation: Option<String>,
        append: bool,
        feedback_tokens: HashMap<String, String>,
    },
    GuideLoaded {
        channels: Vec<SidebarChannel>,
        playlists: Vec<PlaylistSummary>,
    },
    PlaylistLoaded {
        browse_id: String,
        title: String,
        items: Vec<VideoItem>,
        continuation: Option<String>,
        append: bool,
    },
    TrendingLoaded {
        items: Vec<VideoItem>,
    },
    ChannelLoaded {
        channel_id: String,
        info: ChannelInfo,
        items: Vec<VideoItem>,
        continuation: Option<String>,
        append: bool,
    },
    ChannelSearchLoaded {
        channel_id: String,
        query: String,
        items: Vec<VideoItem>,
        continuation: Option<String>,
        append: bool,
    },
    SearchLoaded {
        query: String,
        items: Vec<VideoItem>,
        continuation: Option<String>,
        append: bool,
    },
    SearchSuggestions {
        query: String,
        suggestions: Vec<String>,
    },
    WatchReady {
        data: Box<WatchData>,
        selected_video_url: String,
        selected_audio_url: Option<String>,
        selected_format_idx: Option<usize>,
        used_ytdlp: bool,
    },
    CommentsLoaded {
        video_id: String,
        comments: Vec<CommentItem>,
        next_token: Option<String>,
        sort_options: Vec<(String, String)>,
        create_params: Option<String>,
        append: bool,
    },
    CommentRepliesLoaded {
        video_id: String,
        comment_id: String,
        replies: Vec<CommentItem>,
        next_token: Option<String>,
    },
    CommentPosted {
        video_id: String,
    },
    CommentReplyPosted {
        video_id: String,
        comment_id: String,
        text: String,
    },
    LiveChatLoaded {
        video_id: String,
        messages: Vec<LiveChatMessage>,
        next_token: Option<String>,
        send_params: Option<String>,
    },
    LiveChatSent {
        video_id: String,
    },
    HistoryRemoved,
    LikeUpdated {
        video_id: String,
        liked: bool,
    },
    DislikeUpdated {
        video_id: String,
        disliked: bool,
    },
    WatchLaterUpdated,
    PlaylistEdited {
        playlist_title: String,
        added: bool,
    },
    PlaylistOptionsLoaded {
        video_id: String,
        options: Vec<PlaylistOption>,
    },
    PlaylistThumbsResolved(HashMap<String, String>),
    ChannelAvatarsResolved(HashMap<String, String>),
    PlaylistCreated {
        title: String,
    },
    SubscribeUpdated {
        channel_id: String,
        subscribed: bool,
    },
    DownloadProgress {
        video_id: String,
        audio_only: bool,
        progress: crate::ytdlp::DownloadProgress,
    },
    VideoDownloaded {
        video_id: String,
        audio_only: bool,
        title: String,
        result: Result<PathBuf, String>,
    },
    SponsorSegments {
        video_id: String,
        segments: Vec<SponsorSegment>,
    },
    SignedIn {
        name: Option<String>,
        avatar: Option<String>,
    },
    SignedOut,
    LibmpvDownloaded(Result<PathBuf, String>),
    Error(String),
}

pub struct PlaybackCursor {
    pos_ms: AtomicU64,
    paused: AtomicBool,
    generation: AtomicU64,
}

impl PlaybackCursor {
    fn new() -> Self {
        Self {
            pos_ms: AtomicU64::new(0),
            paused: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        }
    }

    pub fn update(&self, pos_secs: f64, paused: bool) {
        self.pos_ms
            .store((pos_secs.max(0.0) * 1000.0) as u64, Ordering::Relaxed);
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    fn pos_secs(&self) -> f64 {
        self.pos_ms.load(Ordering::Relaxed) as f64 / 1000.0
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct ThumbDiskCache {
    playlist_thumbs: HashMap<String, String>,
    channel_avatars: HashMap<String, String>,
}

impl ThumbDiskCache {
    fn load(dir: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(dir.join("thumb_caches.json")) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    fn save(&self, dir: &Path) {
        if self.playlist_thumbs.is_empty() && self.channel_avatars.is_empty() {
            return;
        }
        if let Ok(json) = serde_json::to_string(self) {
            let _ = std::fs::write(dir.join("thumb_caches.json"), json);
        }
    }
}

async fn load_guide_and_backfill_thumbs(
    http: reqwest::Client,
    cfg: WebConfig,
    session: Session,
    thumb_cache: Arc<Mutex<ThumbDiskCache>>,
    data_dir: PathBuf,
    resp_tx: Sender<Response>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    let Ok((channels, mut playlists)) = innertube::fetch_guide_fast(&http, &cfg, &session).await
    else {
        return;
    };

    let mut missing = Vec::new();
    {
        let mut tc = thumb_cache.lock().unwrap();
        let mut cache_dirty = false;
        for ch in &channels {
            if let Some(av) = &ch.avatar_url {
                if tc
                    .channel_avatars
                    .insert(ch.browse_id.clone(), av.clone())
                    .as_deref()
                    != Some(av.as_str())
                {
                    cache_dirty = true;
                }
            }
        }
        for pl in &mut playlists {
            let key = pl.browse_id.strip_prefix("VL").unwrap_or(&pl.browse_id);
            if let Some(thumb) = &pl.thumbnail_url {
                if tc
                    .playlist_thumbs
                    .insert(key.to_owned(), thumb.clone())
                    .as_deref()
                    != Some(thumb.as_str())
                {
                    cache_dirty = true;
                }
            } else if let Some(cached) = tc.playlist_thumbs.get(key) {
                pl.thumbnail_url = Some(cached.clone());
            } else if !pl.browse_id.is_empty() {
                missing.push(pl.browse_id.clone());
            }
        }
        if cache_dirty {
            tc.save(&data_dir);
        }
    }

    let _ = resp_tx.send(Response::GuideLoaded {
        channels,
        playlists,
    });
    wake();

    if !missing.is_empty() {
        let resolved =
            innertube::fetch_missing_playlist_thumbs(&http, missing, &cfg, &session).await;
        if !resolved.is_empty() {
            {
                let mut tc = thumb_cache.lock().unwrap();
                for (pid, url) in &resolved {
                    tc.playlist_thumbs.insert(pid.clone(), url.clone());
                }
                tc.save(&data_dir);
            }
            let _ = resp_tx.send(Response::PlaylistThumbsResolved(resolved));
            wake();
        }
    }
}

#[derive(Clone)]
pub struct Backend {
    pub tx: Sender<Request>,
    pub rx: Receiver<Response>,
    pub data_dir: PathBuf,
    proxy: Option<StreamProxy>,
    pub cursor: Arc<PlaybackCursor>,
    http: reqwest::Client,
    rt: tokio::runtime::Handle,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Backend {
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn http(&self) -> reqwest::Client {
        self.http.clone()
    }

    pub fn spawn_task<F>(&self, fut: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.rt.spawn(fut);
    }

    pub fn wake(&self) {
        (self.wake)();
    }

    pub fn proxy_url(&self, raw_url: &str, content_length: u64) -> String {
        match &self.proxy {
            Some(p) => p.register(raw_url, content_length),
            None => raw_url.to_owned(),
        }
    }

    pub fn spawn(data_dir: PathBuf, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (req_tx, req_rx) = unbounded::<Request>();
        let (resp_tx, resp_rx) = unbounded::<Response>();
        let (proxy_tx, proxy_rx) = std::sync::mpsc::channel::<Option<StreamProxy>>();
        let cursor = Arc::new(PlaybackCursor::new());
        let cursor_bg = cursor.clone();
        let dir_clone = data_dir.clone();

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let rt_handle = rt.handle().clone();
        let http = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .build()
            .expect("reqwest client");
        let http_for_struct = http.clone();
        let wake_for_struct = wake.clone();

        std::thread::Builder::new()
            .name("yt-rust-backend".into())
            .spawn(move || {
                rt.block_on(async move {
                    let proxy = StreamProxy::start(http.clone()).await.ok();
                    let _ = proxy_tx.send(proxy.clone());

                    let thumb_cache = Arc::new(Mutex::new(ThumbDiskCache::load(&dir_clone)));
                    ytdlp::remove_leftover_cookies(&dir_clone);
                    let mut session = Session::load(&dir_clone);
                    let mut cfg = innertube::fetch_web_config(&http, session.as_ref()).await;

                    if let Some(ref mut s) = session {
                        if let Some((name, avatar)) =
                            innertube::fetch_account_info(&http, &cfg, s).await
                        {
                            s.account_name = Some(name.clone());
                            s.account_avatar = avatar.clone();
                            s.save(&dir_clone);
                            let _ = resp_tx.send(Response::SignedIn {
                                name: Some(name),
                                avatar,
                            });
                            wake();
                        } else {
                            let _ = resp_tx.send(Response::SignedIn {
                                name: s.account_name.clone(),
                                avatar: s.account_avatar.clone(),
                            });
                            wake();
                        }
                        tokio::spawn(load_guide_and_backfill_thumbs(
                            http.clone(),
                            cfg.clone(),
                            s.clone(),
                            thumb_cache.clone(),
                            dir_clone.clone(),
                            resp_tx.clone(),
                            wake.clone(),
                        ));
                    }

                    while let Ok(req) = req_rx.recv() {
                        let http = http.clone();
                        let resp_tx = resp_tx.clone();
                        let wake = wake.clone();
                        let data_dir = dir_clone.clone();
                        let thumb_cache = thumb_cache.clone();

                        match req {
                            Request::LoadHome { params, continuation } => {
                                let append = continuation.is_some();
                                match innertube::home_feed(
                                    &http,
                                    params.as_deref(),
                                    continuation.as_deref(),
                                    &cfg,
                                    session.as_ref(),
                                )
                                .await
                                {
                                    Ok(FeedPage {
                                        items,
                                        chips,
                                        continuation,
                                        visitor_data,
                                        is_fallback_trending,
                                    }) => {
                                        if visitor_data.is_some() {
                                            cfg.visitor_data = visitor_data;
                                        }
                                        let _ = resp_tx.send(Response::HomeLoaded {
                                            items,
                                            chips,
                                            continuation,
                                            append,
                                            is_fallback_trending,
                                        });
                                    }
                                    Err(e) => {
                                        let _ = resp_tx.send(Response::Error(format!(
                                            "Home feed failed: {e:#}"
                                        )));
                                    }
                                }
                                wake();
                            }
                            Request::LoadSubscriptions { continuation } => {
                                let Some(ref s) = session else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to view Subscriptions".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let append = continuation.is_some();
                                match innertube::subscriptions_feed(
                                    &http,
                                    continuation.as_deref(),
                                    &cfg,
                                    s,
                                )
                                .await
                                {
                                    Ok(FeedPage {
                                        items,
                                        continuation,
                                        visitor_data,
                                        ..
                                    }) => {
                                        if visitor_data.is_some() {
                                            cfg.visitor_data = visitor_data;
                                        }
                                        let _ = resp_tx.send(Response::SubscriptionsLoaded {
                                            items,
                                            continuation,
                                            append,
                                        });
                                    }
                                    Err(e) => {
                                        let _ = resp_tx.send(Response::Error(format!(
                                            "Subscriptions failed: {e:#}"
                                        )));
                                    }
                                }
                                wake();
                            }
                            Request::LoadHistory { continuation } => {
                                let Some(ref s) = session else {
                                    continue;
                                };
                                let append = continuation.is_some();
                                match innertube::history_feed(
                                    &http,
                                    continuation.as_deref(),
                                    &cfg,
                                    s,
                                )
                                .await
                                {
                                    Ok((
                                        FeedPage {
                                            items,
                                            continuation,
                                            visitor_data,
                                            ..
                                        },
                                        feedback_tokens,
                                    )) => {
                                        if visitor_data.is_some() {
                                            cfg.visitor_data = visitor_data;
                                        }
                                        let _ = resp_tx.send(Response::HistoryLoaded {
                                            items,
                                            continuation,
                                            append,
                                            feedback_tokens,
                                        });
                                    }
                                    Err(e) => {
                                        let _ = resp_tx.send(Response::Error(format!(
                                            "History failed: {e:#}"
                                        )));
                                    }
                                }
                                wake();
                            }
                            Request::LoadPlaylist {
                                browse_id,
                                continuation,
                            } => {
                                let append = continuation.is_some();
                                let cfg_clone = cfg.clone();
                                let sess_clone = session.clone();
                                tokio::spawn(async move {
                                    match innertube::playlist_feed(
                                        &http,
                                        &browse_id,
                                        continuation.as_deref(),
                                        &cfg_clone,
                                        sess_clone.as_ref(),
                                    )
                                    .await
                                    {
                                        Ok((
                                            title,
                                            FeedPage {
                                                items,
                                                continuation,
                                                ..
                                            },
                                        )) => {
                                            let _ = resp_tx.send(Response::PlaylistLoaded {
                                                browse_id,
                                                title,
                                                items,
                                                continuation,
                                                append,
                                            });
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Playlist failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::LoadTrending => {
                                match innertube::trending_feed(&http, &cfg, session.as_ref()).await {
                                    Ok(FeedPage {
                                        items,
                                        visitor_data,
                                        ..
                                    }) => {
                                        if visitor_data.is_some() {
                                            cfg.visitor_data = visitor_data;
                                        }
                                        let _ = resp_tx.send(Response::TrendingLoaded { items });
                                    }
                                    Err(e) => {
                                        let _ = resp_tx.send(Response::Error(format!(
                                            "Trending feed failed: {e:#}"
                                        )));
                                    }
                                }
                                wake();
                            }
                            Request::LoadChannel {
                                channel_id,
                                params,
                                continuation,
                            } => {
                                let append = continuation.is_some();
                                match innertube::channel_videos(
                                    &http,
                                    &channel_id,
                                    params.as_deref(),
                                    continuation.as_deref(),
                                    &cfg,
                                    session.as_ref(),
                                )
                                .await
                                {
                                    Ok((
                                        info,
                                        FeedPage {
                                            items,
                                            continuation,
                                            visitor_data,
                                            ..
                                        },
                                    )) => {
                                        if visitor_data.is_some() {
                                            cfg.visitor_data = visitor_data;
                                        }
                                        let _ = resp_tx.send(Response::ChannelLoaded {
                                            channel_id,
                                            info,
                                            items,
                                            continuation,
                                            append,
                                        });
                                    }
                                    Err(e) => {
                                        let _ = resp_tx.send(Response::Error(format!(
                                            "Channel load failed: {e:#}"
                                        )));
                                    }
                                }
                                wake();
                            }
                            Request::SearchChannel {
                                channel_id,
                                query,
                                continuation,
                            } => {
                                let append = continuation.is_some();
                                let cfg_clone = cfg.clone();
                                let sess_clone = session.clone();
                                tokio::spawn(async move {
                                    match innertube::search_channel(
                                        &http,
                                        &channel_id,
                                        &query,
                                        continuation.as_deref(),
                                        &cfg_clone,
                                        sess_clone.as_ref(),
                                    )
                                    .await
                                    {
                                        Ok(FeedPage {
                                            items,
                                            continuation,
                                            ..
                                        }) => {
                                            let _ = resp_tx.send(Response::ChannelSearchLoaded {
                                                channel_id,
                                                query,
                                                items,
                                                continuation,
                                                append,
                                            });
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Channel search failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::LoadComments {
                                video_id,
                                continuation,
                                append,
                            } => {
                                let cfg_clone = cfg.clone();
                                let sess_clone = session.clone();
                                tokio::spawn(async move {
                                    match innertube::fetch_comments(
                                        &http,
                                        &continuation,
                                        &cfg_clone,
                                        sess_clone.as_ref(),
                                    )
                                    .await
                                    {
                                        Ok(page) => {
                                            let _ = resp_tx.send(Response::CommentsLoaded {
                                                video_id,
                                                comments: page.comments,
                                                next_token: page.next_token,
                                                sort_options: page.sort_options,
                                                create_params: page.create_params,
                                                append,
                                            });
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Comments failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::LoadCommentReplies {
                                video_id,
                                comment_id,
                                continuation,
                            } => {
                                let cfg_clone = cfg.clone();
                                let sess_clone = session.clone();
                                tokio::spawn(async move {
                                    match innertube::fetch_comment_replies(
                                        &http,
                                        &continuation,
                                        &cfg_clone,
                                        sess_clone.as_ref(),
                                    )
                                    .await
                                    {
                                        Ok((replies, next_token)) => {
                                            let _ = resp_tx.send(Response::CommentRepliesLoaded {
                                                video_id,
                                                comment_id,
                                                replies,
                                                next_token,
                                            });
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Replies failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::PostComment {
                                video_id,
                                text,
                                params,
                                reload_continuation,
                            } => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to post comments".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::create_comment(
                                        &http,
                                        &text,
                                        &params,
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::CommentPosted {
                                                video_id: video_id.clone(),
                                            });
                                            if let Some(tok) = reload_continuation {
                                                if let Ok(page) = innertube::fetch_comments(
                                                    &http,
                                                    &tok,
                                                    &cfg_clone,
                                                    Some(&s),
                                                )
                                                .await
                                                {
                                                    let _ = resp_tx.send(Response::CommentsLoaded {
                                                        video_id,
                                                        comments: page.comments,
                                                        next_token: page.next_token,
                                                        sort_options: page.sort_options,
                                                        create_params: page.create_params,
                                                        append: false,
                                                    });
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Comment failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::PostCommentReply {
                                video_id,
                                comment_id,
                                text,
                                params,
                                replies_continuation,
                            } => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to reply to comments".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::create_comment_reply(
                                        &http,
                                        &text,
                                        &params,
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::CommentReplyPosted {
                                                video_id: video_id.clone(),
                                                comment_id: comment_id.clone(),
                                                text,
                                            });
                                            if let Some(tok) = replies_continuation {
                                                if let Ok((replies, next_token)) =
                                                    innertube::fetch_comment_replies(
                                                        &http,
                                                        &tok,
                                                        &cfg_clone,
                                                        Some(&s),
                                                    )
                                                    .await
                                                {
                                                    let _ = resp_tx.send(
                                                        Response::CommentRepliesLoaded {
                                                            video_id,
                                                            comment_id,
                                                            replies,
                                                            next_token,
                                                        },
                                                    );
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Reply failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::LoadLiveChat {
                                video_id,
                                continuation,
                            } => {
                                let cfg_clone = cfg.clone();
                                let sess_clone = session.clone();
                                tokio::spawn(async move {
                                    if let Ok(page) = innertube::fetch_live_chat(
                                        &http,
                                        &continuation,
                                        &cfg_clone,
                                        sess_clone.as_ref(),
                                    )
                                    .await
                                    {
                                        let _ = resp_tx.send(Response::LiveChatLoaded {
                                            video_id,
                                            messages: page.messages,
                                            next_token: page.next_token,
                                            send_params: page.send_params,
                                        });
                                        wake();
                                    }
                                });
                            }
                            Request::SendLiveChat {
                                video_id,
                                params,
                                message,
                            } => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to send live chat messages".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::send_live_chat_message(
                                        &http,
                                        &params,
                                        &message,
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::LiveChatSent {
                                                video_id,
                                            });
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Live chat send failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::RemoveFromHistory {
                                video_id: _,
                                feedback_token,
                            } => {
                                let Some(s) = session.clone() else {
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::remove_from_history(
                                        &http,
                                        &feedback_token,
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::HistoryRemoved);
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Remove from history failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::SetLike { video_id, like } => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to like videos".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::set_like(&http, &video_id, like, &cfg_clone, &s)
                                        .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::LikeUpdated {
                                                video_id,
                                                liked: like,
                                            });
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Like failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::SetDislike { video_id, dislike } => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to dislike videos".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::set_dislike(
                                        &http, &video_id, dislike, &cfg_clone, &s,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::DislikeUpdated {
                                                video_id,
                                                disliked: dislike,
                                            });
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Dislike failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::SetWatchLater { video_id, add } => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to edit Watch Later".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::set_watch_later(
                                        &http,
                                        &video_id,
                                        add,
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::WatchLaterUpdated);
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Watch Later failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::EditPlaylist {
                                playlist_id,
                                playlist_title,
                                video_id,
                                add,
                            } => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to save to playlists".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::edit_playlist(
                                        &http,
                                        &playlist_id,
                                        &video_id,
                                        add,
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::PlaylistEdited {
                                                playlist_title,
                                                added: add,
                                            });
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Playlist update failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::LoadPlaylistOptions { video_id } => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::PlaylistOptionsLoaded {
                                        video_id,
                                        options: Vec::new(),
                                    });
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::get_add_to_playlist(
                                        &http,
                                        &video_id,
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await
                                    {
                                        Ok(mut options) => {
                                            let mut missing = Vec::new();
                                            {
                                                let tc = thumb_cache.lock().unwrap();
                                                for opt in &mut options {
                                                    if opt.thumbnail_url.is_none() {
                                                        if let Some(cached) =
                                                            tc.playlist_thumbs.get(&opt.id)
                                                        {
                                                            opt.thumbnail_url = Some(cached.clone());
                                                        } else if opt.id != "WL" {
                                                            missing.push(opt.id.clone());
                                                        }
                                                    }
                                                }
                                            }
                                            let _ = resp_tx.send(Response::PlaylistOptionsLoaded {
                                                video_id,
                                                options,
                                            });
                                            wake();
                                            if !missing.is_empty() {
                                                let resolved =
                                                    innertube::fetch_missing_playlist_thumbs(
                                                        &http,
                                                        missing,
                                                        &cfg_clone,
                                                        &s,
                                                    )
                                                    .await;
                                                if !resolved.is_empty() {
                                                    {
                                                        let mut tc = thumb_cache.lock().unwrap();
                                                        for (pid, url) in &resolved {
                                                            tc.playlist_thumbs
                                                                .insert(pid.clone(), url.clone());
                                                        }
                                                        tc.save(&data_dir);
                                                    }
                                                    let _ = resp_tx.send(
                                                        Response::PlaylistThumbsResolved(resolved),
                                                    );
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Load playlists failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::CreatePlaylist { title, video_ids } => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to create playlists".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::create_playlist(
                                        &http,
                                        &title,
                                        &video_ids,
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::PlaylistCreated {
                                                title,
                                            });
                                            load_guide_and_backfill_thumbs(
                                                http,
                                                cfg_clone,
                                                s,
                                                thumb_cache,
                                                data_dir,
                                                resp_tx.clone(),
                                                wake.clone(),
                                            )
                                            .await;
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Create playlist failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::SetSubscribe {
                                channel_id,
                                subscribe,
                            } => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to subscribe to channels".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::set_subscribe(
                                        &http,
                                        &channel_id,
                                        subscribe,
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::SubscribeUpdated {
                                                channel_id,
                                                subscribed: subscribe,
                                            });
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Subscribe failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::Search { query, continuation } => {
                                let append = continuation.is_some();
                                match innertube::search(
                                    &http,
                                    &query,
                                    continuation.as_deref(),
                                    &cfg,
                                    session.as_ref(),
                                )
                                .await
                                {
                                    Ok(FeedPage {
                                        items,
                                        continuation,
                                        visitor_data,
                                        ..
                                    }) => {
                                        if visitor_data.is_some() {
                                            cfg.visitor_data = visitor_data;
                                        }
                                        let _ = resp_tx.send(Response::SearchLoaded {
                                            query,
                                            items,
                                            continuation,
                                            append,
                                        });
                                    }
                                    Err(e) => {
                                        let _ = resp_tx.send(Response::Error(format!(
                                            "Search failed: {e:#}"
                                        )));
                                    }
                                }
                                wake();
                            }
                            Request::SearchSuggest { query } => {
                                tokio::spawn(async move {
                                    if let Ok(suggestions) =
                                        innertube::search_suggestions(&http, &query).await
                                    {
                                        let _ = resp_tx.send(Response::SearchSuggestions {
                                            query,
                                            suggestions,
                                        });
                                        wake();
                                    }
                                });
                            }
                            Request::DownloadVideo {
                                video_id,
                                title,
                                audio_only,
                                max_height,
                                output_dir,
                            } => {
                                let cookie = session.as_ref().map(|s| s.cookie.clone());
                                tokio::spawn(async move {
                                    let vid_prog = video_id.clone();
                                    let tx_prog = resp_tx.clone();
                                    let wake_prog = wake.clone();
                                    let result = ytdlp::download_video(
                                        &http,
                                        &data_dir,
                                        output_dir.as_deref(),
                                        cookie.as_deref(),
                                        &video_id,
                                        audio_only,
                                        max_height,
                                        move |progress| {
                                            let _ = tx_prog.send(Response::DownloadProgress {
                                                video_id: vid_prog.clone(),
                                                audio_only,
                                                progress,
                                            });
                                            wake_prog();
                                        },
                                    )
                                    .await
                                    .map_err(|e| format!("{e:#}"));
                                    let _ = resp_tx.send(Response::VideoDownloaded {
                                        video_id,
                                        audio_only,
                                        title,
                                        result,
                                    });
                                    wake();
                                });
                            }
                            Request::Watch {
                                video_id,
                                quality,
                                sync_history,
                            } => {
                                let cfg_clone = cfg.clone();
                                let sess_clone = session.clone();
                                let proxy_clone = proxy.clone();
                                let play_gen = cursor_bg.generation.fetch_add(1, Ordering::Relaxed) + 1;
                                cursor_bg.update(0.0, false);
                                let cur_clone = cursor_bg.clone();
                                tokio::spawn(async move {
                                    let sb_http = http.clone();
                                    let sb_id = video_id.clone();
                                    let sb_tx = resp_tx.clone();
                                    let sb_wake = wake.clone();
                                    tokio::spawn(async move {
                                        if let Ok(segments) =
                                            sponsorblock::fetch_segments(&sb_http, &sb_id).await
                                        {
                                            if !segments.is_empty() {
                                                let _ = sb_tx.send(Response::SponsorSegments {
                                                    video_id: sb_id,
                                                    segments,
                                                });
                                                sb_wake();
                                            }
                                        }
                                    });

                                    match resolve_watch(
                                        &http,
                                        &data_dir,
                                        &video_id,
                                        quality,
                                        &cfg_clone,
                                        sess_clone.as_ref(),
                                        proxy_clone.as_ref(),
                                    )
                                    .await
                                    {
                                        Ok(res) => {
                                            let _ = resp_tx.send(res);
                                            wake();
                                            if sync_history {
                                                if let Some(sess) = sess_clone {
                                                    tokio::spawn(async move {
                                                        report_history_loop(
                                                            http,
                                                            data_dir,
                                                            video_id,
                                                            cfg_clone,
                                                            sess,
                                                            cur_clone,
                                                            play_gen,
                                                        )
                                                        .await;
                                                    });
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Playback failed: {e:#}"
                                            )));
                                            wake();
                                        }
                                    }
                                });
                            }
                            Request::SignIn => {
                                let profile = data_dir.join("login-webview");
                                let (tx, rx) = tokio::sync::oneshot::channel();
                                login::spawn(profile.clone(), move |outcome| {
                                    let _ = tx.send(outcome);
                                });
                                if let Ok(outcome) = rx.await {
                                    login::forget(&profile);
                                    match outcome {
                                        login::Outcome::Cookie(cookie) => {
                                            let mut s = Session::from_cookie(cookie);
                                            cfg = innertube::fetch_web_config(&http, Some(&s))
                                                .await;
                                            if let Some((name, avatar)) =
                                                innertube::fetch_account_info(&http, &cfg, &s).await
                                            {
                                                s.account_name = Some(name);
                                                s.account_avatar = avatar;
                                            }
                                            s.save(&data_dir);
                                            let name = s.account_name.clone();
                                            let avatar = s.account_avatar.clone();
                                            session = Some(s);
                                            let _ = resp_tx.send(Response::SignedIn { name, avatar });
                                            // Automatically refresh Home feed with authenticated session
                                            if let Ok(FeedPage {
                                                items,
                                                chips,
                                                continuation,
                                                visitor_data,
                                                is_fallback_trending,
                                            }) = innertube::home_feed(
                                                &http,
                                                None,
                                                None,
                                                &cfg,
                                                session.as_ref(),
                                            )
                                            .await
                                            {
                                                if visitor_data.is_some() {
                                                    cfg.visitor_data = visitor_data;
                                                }
                                                let _ = resp_tx.send(Response::HomeLoaded {
                                                    items,
                                                    chips,
                                                    continuation,
                                                    append: false,
                                                    is_fallback_trending,
                                                });
                                            }
                                            if let Some(ref s) = session {
                                                tokio::spawn(load_guide_and_backfill_thumbs(
                                                    http.clone(),
                                                    cfg.clone(),
                                                    s.clone(),
                                                    thumb_cache.clone(),
                                                    data_dir.clone(),
                                                    resp_tx.clone(),
                                                    wake.clone(),
                                                ));
                                            }
                                        }
                                        login::Outcome::Cancelled => {}
                                        login::Outcome::Failed(msg) => {
                                            let _ = resp_tx.send(Response::Error(msg));
                                        }
                                    }
                                    wake();
                                }
                            }
                            Request::SignOut => {
                                session = None;
                                Session::clear(&data_dir);
                                login::forget(&data_dir.join("login-webview"));
                                cfg = innertube::fetch_web_config(&http, None).await;
                                let _ = resp_tx.send(Response::SignedOut);
                                wake();
                            }
                            Request::DownloadLibmpv => {
                                tokio::spawn(async move {
                                    let res = mpv::download_libmpv_windows(&http, &data_dir)
                                        .await
                                        .map_err(|e| format!("{e:#}"));
                                    let _ = resp_tx.send(Response::LibmpvDownloaded(res));
                                    wake();
                                });
                            }
                            Request::ResolveChannelAvatars { channel_ids } => {
                                let cfg_clone = cfg.clone();
                                let sess_clone = session.clone();
                                tokio::spawn(async move {
                                    let mut resolved = HashMap::new();
                                    let mut missing = Vec::new();
                                    {
                                        let tc = thumb_cache.lock().unwrap();
                                        for cid in channel_ids {
                                            if let Some(av) = tc.channel_avatars.get(&cid) {
                                                resolved.insert(cid, av.clone());
                                            } else {
                                                missing.push(cid);
                                            }
                                        }
                                    }
                                    if !missing.is_empty() {
                                        let fetched = innertube::fetch_channel_avatars(
                                            &http,
                                            missing,
                                            &cfg_clone,
                                            sess_clone.as_ref(),
                                        )
                                        .await;
                                        if !fetched.is_empty() {
                                            {
                                                let mut tc = thumb_cache.lock().unwrap();
                                                for (cid, av) in &fetched {
                                                    tc.channel_avatars
                                                        .insert(cid.clone(), av.clone());
                                                }
                                                tc.save(&data_dir);
                                            }
                                            resolved.extend(fetched);
                                        }
                                    }
                                    if !resolved.is_empty() {
                                        let _ = resp_tx
                                            .send(Response::ChannelAvatarsResolved(resolved));
                                        wake();
                                    }
                                });
                            }
                        }
                    }
                });
            })
            .expect("spawn backend thread");

        let proxy = proxy_rx.recv().ok().flatten();
        Self {
            tx: req_tx,
            rx: resp_rx,
            data_dir,
            proxy,
            cursor,
            http: http_for_struct,
            rt: rt_handle,
            wake: wake_for_struct,
        }
    }
}

async fn report_history_loop(
    http: reqwest::Client,
    data_dir: PathBuf,
    video_id: String,
    cfg: WebConfig,
    session: Session,
    cursor: Arc<PlaybackCursor>,
    generation: u64,
) {
    let sts = innertube::fetch_signature_timestamp(&http, &data_dir).await;
    let Ok(w) = innertube::report_play(&http, &video_id, &cfg, &session, sts).await else {
        log::warn!("failed to report play for {video_id}");
        return;
    };

    let started = std::time::Instant::now();
    let mut flush = w.flush_at.iter().copied();
    let mut due = flush.next().unwrap_or(w.flush_every);
    let mut from = cursor.pos_secs();
    let mut at = from;

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let gone = cursor.generation.load(Ordering::Relaxed) != generation;
        if !gone && !cursor.paused.load(Ordering::Relaxed) {
            let now = cursor.pos_secs();
            if now < at || now > at + 5.0 {
                from = now;
            }
            at = now;
        }
        let elapsed = started.elapsed().as_secs();
        if !gone && elapsed < due {
            continue;
        }
        if at > from || gone {
            if let Err(e) = innertube::report_watchtime(
                &http,
                &w,
                &session,
                from,
                at,
                started.elapsed().as_secs_f64(),
                gone,
            )
            .await
            {
                log::warn!("watchtime report failed for {video_id}: {e:#}");
            }
            from = at;
        }
        if gone {
            return;
        }
        due = flush.next().unwrap_or(due + w.flush_every);
    }
}

fn apply_ytdlp_streams(
    data: &mut WatchData,
    yt: ytdlp::YtdlpStreams,
    max_height: u32,
    proxy: Option<&StreamProxy>,
) -> (String, Option<String>, Option<usize>) {
    let ua = yt.user_agent.as_deref().unwrap_or("");
    let wrap_yt = |url: &str, len: u64| -> String {
        let is_hls = url.contains(".m3u8") || url.contains("/manifest/hls");
        match proxy {
            Some(p) if !is_hls => p.register_with_ua(url, len, ua),
            _ => url.to_owned(),
        }
    };
    if !yt.video_formats.is_empty() {
        data.video_formats = yt.video_formats;
    }
    if !yt.audio_tracks.is_empty() {
        data.audio_tracks = yt.audio_tracks;
    }
    if yt.audio_url.is_some() {
        data.audio_url = yt.audio_url.clone();
        data.audio_content_length = yt.audio_content_length;
    }
    if yt.hls_url.is_some() {
        data.hls_url = yt.hls_url.clone();
    }
    let fmt_idx = pick_format_index(&data.video_formats, max_height);
    let (v_url, a_url) = if let Some(idx) = fmt_idx {
        let fmt = &data.video_formats[idx];
        let v = wrap_yt(&fmt.url, fmt.content_length);
        let a = if fmt.has_audio {
            None
        } else {
            data.audio_url
                .as_deref()
                .map(|u| wrap_yt(u, data.audio_content_length))
        };
        (v, a)
    } else if yt.video_url.contains(".m3u8") || yt.video_url.contains("/manifest/hls") {
        let a = yt
            .audio_url
            .as_deref()
            .map(|u| wrap_yt(u, yt.audio_content_length));
        (yt.video_url, a)
    } else {
        (
            wrap_yt(&yt.video_url, yt.video_content_length),
            yt.audio_url
                .as_deref()
                .map(|u| wrap_yt(u, yt.audio_content_length)),
        )
    };
    (v_url, a_url, fmt_idx)
}

async fn resolve_watch(
    http: &reqwest::Client,
    data_dir: &PathBuf,
    video_id: &str,
    quality: PreferredQuality,
    cfg: &WebConfig,
    session: Option<&Session>,
    proxy: Option<&StreamProxy>,
) -> anyhow::Result<Response> {
    let wrap = |url: &str, len: u64| -> String {
        let is_hls = url.contains(".m3u8") || url.contains("/manifest/hls");
        match proxy {
            Some(p) if !is_hls => p.register(url, len),
            _ => url.to_owned(),
        }
    };
    match innertube::watch(http, video_id, cfg, session).await {
        Ok(mut data) => {
            // Livestreams play much more reliably via HLS manifest than raw adaptive URLs
            if data.is_live {
                if let Some(hls) = data.hls_url.clone() {
                    return Ok(Response::WatchReady {
                        selected_video_url: hls,
                        selected_audio_url: None,
                        selected_format_idx: None,
                        data: Box::new(data),
                        used_ytdlp: false,
                    });
                }
            }
            if let Some(idx) = pick_format_index(&data.video_formats, quality.max_height()) {
                let fmt = &data.video_formats[idx];
                let video_url = wrap(&fmt.url, fmt.content_length);
                let audio = if fmt.has_audio {
                    None
                } else {
                    data.audio_url
                        .as_deref()
                        .map(|u| wrap(u, data.audio_content_length))
                };
                return Ok(Response::WatchReady {
                    selected_video_url: video_url,
                    selected_audio_url: audio,
                    selected_format_idx: Some(idx),
                    data: Box::new(data),
                    used_ytdlp: false,
                });
            }
            if let Some(hls) = data.hls_url.clone() {
                return Ok(Response::WatchReady {
                    selected_video_url: hls,
                    selected_audio_url: None,
                    selected_format_idx: None,
                    data: Box::new(data),
                    used_ytdlp: false,
                });
            }
            let yt = ytdlp::resolve_video(
                http,
                data_dir,
                session.map(|s| s.cookie.as_str()),
                video_id,
                quality.max_height(),
            )
            .await?;
            let (selected_video_url, selected_audio_url, fmt_idx) =
                apply_ytdlp_streams(&mut data, yt, quality.max_height(), proxy);
            Ok(Response::WatchReady {
                selected_video_url,
                selected_audio_url,
                selected_format_idx: fmt_idx,
                data: Box::new(data),
                used_ytdlp: true,
            })
        }
        Err(inner_err) => {
            log::warn!("InnerTube watch failed ({inner_err:#}), falling back to yt-dlp");
            let (yt_res, next_opt) = tokio::join!(
                ytdlp::resolve_video(
                    http,
                    data_dir,
                    session.map(|s| s.cookie.as_str()),
                    video_id,
                    quality.max_height(),
                ),
                innertube::fetch_next_watch_fallback(http, video_id, cfg, session),
            );
            let yt = yt_res?;
            let mut data = next_opt.unwrap_or_else(|| WatchData {
                id: video_id.to_owned(),
                title: format!("Video {video_id}"),
                channel: "Unknown".to_owned(),
                channel_id: None,
                channel_avatar_url: None,
                subscribers_text: String::new(),
                views_text: String::new(),
                published_text: String::new(),
                likes_text: String::new(),
                is_liked: false,
                is_disliked: false,
                is_subscribed: false,
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
                comments_token: None,
                live_chat_token: None,
                related: Vec::new(),
                is_live: false,
                resume_secs: None,
            });
            if let Some(t) = yt.title.as_deref().filter(|s| !s.is_empty()) {
                if data.title.starts_with("Video ") || data.title.is_empty() {
                    data.title = t.to_owned();
                }
            }
            if let Some(c) = yt.channel.as_deref().filter(|s| !s.is_empty()) {
                if data.channel == "Unknown" || data.channel.is_empty() {
                    data.channel = c.to_owned();
                }
            }
            if data.channel_id.is_none() {
                data.channel_id = yt.channel_id.clone();
            }
            if data.description.is_empty() && !yt.description.is_empty() {
                data.description = yt.description.clone();
            }
            if data.duration_secs <= 0.0 {
                data.duration_secs = yt.duration_secs;
            }
            let (selected_video_url, selected_audio_url, fmt_idx) =
                apply_ytdlp_streams(&mut data, yt, quality.max_height(), proxy);
            Ok(Response::WatchReady {
                selected_video_url,
                selected_audio_url,
                selected_format_idx: fmt_idx,
                data: Box::new(data),
                used_ytdlp: true,
            })
        }
    }
}

pub fn pick_format_index(formats: &[VideoFormat], max_height: u32) -> Option<usize> {
    if formats.is_empty() {
        return None;
    }
    formats
        .iter()
        .position(|f| f.height <= max_height && !f.has_audio)
        .or_else(|| formats.iter().position(|f| f.height <= max_height))
        .or_else(|| Some(formats.len() - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_highest_adaptive_within_max_height() {
        let fmt = |height: u32, has_audio: bool| VideoFormat {
            itag: 0,
            url: format!("u{height}"),
            mime: "video/mp4; codecs=\"avc1\"".into(),
            width: height * 16 / 9,
            height,
            fps: 30,
            bitrate: 1_000_000,
            content_length: 0,
            has_audio,
        };
        let formats = vec![fmt(2160, false), fmt(1080, false), fmt(720, true)];
        assert_eq!(pick_format_index(&formats, 1080), Some(1));
        assert_eq!(pick_format_index(&formats, 720), Some(2));
        assert_eq!(pick_format_index(&[], 1080), None);
    }
}
