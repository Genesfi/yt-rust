use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, Sender, unbounded};
use serde::{Deserialize, Serialize};

use crate::auth::Session;
use crate::innertube::{self, FeedPage, WebConfig};
use crate::model::{
    AudioTrack, ChannelInfo, ChannelNotificationParams, CommentItem, FeedChip, LiveChatEmote,
    LiveChatMessage, NotificationLevel, NotificationMenuItem, PlaylistOption, PlaylistSummary,
    PoTokenMode, PreferredQuality, SidebarChannel, SponsorSegment, VideoFormat, VideoItem,
    WatchData,
};
use crate::potoken::PoTokenClient;
use crate::proxy::StreamProxy;
use crate::{login, mpv, sponsorblock, ytdlp};

pub enum Request {
    LoadHome {
        params: Option<String>,
        continuation: Option<String>,
        append: bool,
    },
    LoadCommunityMeta {
        video_id: String,
        dearrow: bool,
        ryd: bool,
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
    LoadFullPlaylistForQueue {
        browse_id: String,
        continuation: String,
        shuffle: bool,
    },
    PlaylistRemoveItem {
        playlist_id: String,
        video_id: String,
        set_video_id: Option<String>,
    },
    PlaylistMoveItem {
        playlist_id: String,
        set_video_id: String,
        successor_set_video_id: Option<String>,
    },
    LoadTrending,
    LoadShorts,
    LoadShortsQueue {
        seed_video_id: String,
    },
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
    RecoverStream {
        video_id: String,
        quality: PreferredQuality,
        preferred_height: Option<u32>,
        preferred_audio_id: Option<String>,
        resume_secs: f64,
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
        replay_offset_ms: Option<u64>,
    },
    SendLiveChat {
        video_id: String,
        params: String,
        message: String,
        emotes: Vec<LiveChatEmote>,
    },
    PerformCommentAction {
        video_id: String,
        comment_id: String,
        action_params: String,
        liked: bool,
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
    LoadNotifications,
    MarkNotificationsSeen {
        params: String,
    },
    SetChannelNotifications {
        channel_id: String,
        level: NotificationLevel,
        params: String,
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
    SetPoTokenMode(PoTokenMode),
    LoadAmbientArt {
        key: String,
        url: String,
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
        is_editable: bool,
        items: Vec<VideoItem>,
        continuation: Option<String>,
        append: bool,
    },
    FullPlaylistQueueLoaded {
        items: Vec<VideoItem>,
        shuffle: bool,
    },
    PlaylistItemRemoved,
    PlaylistReordered,
    TrendingLoaded {
        items: Vec<VideoItem>,
    },
    ShortsLoaded {
        items: Vec<VideoItem>,
    },
    ShortsQueueLoaded {
        seed_video_id: String,
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
    StreamRecovered {
        video_id: String,
        video_formats: Vec<VideoFormat>,
        audio_url: Option<String>,
        audio_content_length: u64,
        audio_tracks: Vec<AudioTrack>,
        hls_url: Option<String>,
        selected_video_url: String,
        selected_audio_url: Option<String>,
        selected_format_idx: Option<usize>,
        selected_audio_idx: usize,
        used_ytdlp: bool,
        resume_secs: f64,
    },
    StreamRecoverFailed {
        video_id: String,
        message: String,
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
        emotes: Vec<LiveChatEmote>,
        is_replay: bool,
    },
    LiveChatSent {
        video_id: String,
    },
    CommentActionUpdated {
        video_id: String,
        comment_id: String,
        liked: bool,
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
    NotificationsCountLoaded(u32),
    NotificationsLoaded {
        items: Vec<NotificationMenuItem>,
        unseen_count: u32,
        seen_params: Option<String>,
    },
    ChannelNotificationLevelUpdated {
        channel_id: String,
        level: NotificationLevel,
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
    DeArrowLoaded {
        video_id: String,
        title: Option<String>,
        thumbnail_url: Option<String>,
    },
    DislikesLoaded {
        video_id: String,
        dislikes_text: String,
    },
    SignedIn {
        name: Option<String>,
        avatar: Option<String>,
    },
    SignedOut,
    SessionRevoked,
    LibmpvDownloaded(Result<PathBuf, String>),
    AmbientArtReady {
        key: String,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
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
    latest_guide_channels: Arc<Mutex<Vec<SidebarChannel>>>,
    data_dir: PathBuf,
    resp_tx: Sender<Response>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    let Ok((channels, mut playlists)) = innertube::fetch_guide_fast(&http, &cfg, &session).await
    else {
        return;
    };
    *latest_guide_channels.lock().unwrap() = channels.clone();

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

    pub fn is_proxy_forbidden(&self, url: &str) -> bool {
        self.proxy
            .as_ref()
            .is_some_and(|p| p.is_session_forbidden(url))
    }

    pub fn is_proxy_failed(&self, url: &str) -> bool {
        self.proxy
            .as_ref()
            .is_some_and(|p| p.is_session_failed(url))
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
                    let latest_guide_channels: Arc<Mutex<Vec<SidebarChannel>>> =
                        Arc::new(Mutex::new(Vec::new()));
                    let latest_home_items: Arc<Mutex<Vec<VideoItem>>> =
                        Arc::new(Mutex::new(Vec::new()));
                    ytdlp::remove_leftover_cookies(&dir_clone);
                    let _ = std::fs::remove_file(dir_clone.join("notif_debug.json"));
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
                            latest_guide_channels.clone(),
                            dir_clone.clone(),
                            resp_tx.clone(),
                            wake.clone(),
                        ));
                        let http_sub = http.clone();
                        let cfg_sub = cfg.clone();
                        let s_sub = s.clone();
                        let tx_sub = resp_tx.clone();
                        let wake_sub = wake.clone();
                        let guide_sub = latest_guide_channels.clone();
                        let home_sub = latest_home_items.clone();
                        let tc_sub = thumb_cache.clone();
                        tokio::spawn(async move {
                            let (sub_res, unseen_res) = tokio::join!(
                                innertube::subscriptions_feed(&http_sub, None, &cfg_sub, &s_sub),
                                innertube::fetch_unseen_notification_count(
                                    &http_sub, &cfg_sub, &s_sub
                                ),
                            );
                            let native_unseen = unseen_res.unwrap_or(0);
                            if native_unseen > 0 {
                                let _ = tx_sub.send(Response::NotificationsCountLoaded(native_unseen));
                                wake_sub();
                            }
                            if let Ok(page) = sub_res {
                                if native_unseen == 0 {
                                    let guide_snap = guide_sub.lock().unwrap().clone();
                                    let home_snap = home_sub.lock().unwrap().clone();
                                    let av_snap = tc_sub.lock().unwrap().channel_avatars.clone();
                                    let seed = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map_or(1, |d| d.as_secs());
                                    let (_, fb_unseen) = innertube::build_subscription_notifications(
                                        &page.items,
                                        &guide_snap,
                                        &home_snap,
                                        &av_snap,
                                        seed,
                                    );
                                    if fb_unseen > 0 {
                                        let _ = tx_sub
                                            .send(Response::NotificationsCountLoaded(fb_unseen));
                                    }
                                }
                                let _ = tx_sub.send(Response::SubscriptionsLoaded {
                                    items: page.items,
                                    continuation: page.continuation,
                                    append: false,
                                });
                                wake_sub();
                            }
                        });
                    }

                    let shared_session = Arc::new(Mutex::new(session));
                    let potoken = Arc::new(PoTokenClient::new(&dir_clone));
                    let po_token_mode = Arc::new(AtomicU8::new(PoTokenMode::Auto.as_u8()));
                    {
                        let rot_http = http.clone();
                        let rot_sess = shared_session.clone();
                        let rot_dir = dir_clone.clone();
                        let rot_tx = resp_tx.clone();
                        let rot_wake = wake.clone();
                        tokio::spawn(async move {
                            rotate_cookies_loop(rot_http, rot_sess, rot_dir, rot_tx, rot_wake).await;
                        });
                    }

                    while let Ok(req) = req_rx.recv() {
                        let session = shared_session.lock().unwrap().clone();
                        let http = http.clone();
                        let resp_tx = resp_tx.clone();
                        let wake = wake.clone();
                        let data_dir = dir_clone.clone();
                        let thumb_cache = thumb_cache.clone();
                        let potoken = potoken.clone();
                        let po_token_mode = po_token_mode.clone();

                        match req {
                            Request::LoadHome {
                                params,
                                continuation,
                                append,
                            } => {
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
                                        if !append {
                                            *latest_home_items.lock().unwrap() = items.clone();
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
                            Request::LoadCommunityMeta {
                                video_id,
                                dearrow,
                                ryd,
                            } => {
                                if dearrow {
                                    let da_http = http.clone();
                                    let da_id = video_id.clone();
                                    let da_tx = resp_tx.clone();
                                    let da_wake = wake.clone();
                                    tokio::spawn(async move {
                                        if let Some(b) =
                                            sponsorblock::fetch_dearrow(&da_http, &da_id).await
                                        {
                                            let _ = da_tx.send(Response::DeArrowLoaded {
                                                video_id: da_id,
                                                title: b.title,
                                                thumbnail_url: b.thumbnail_url,
                                            });
                                            da_wake();
                                        }
                                    });
                                }
                                if ryd {
                                    let ryd_http = http.clone();
                                    let ryd_id = video_id.clone();
                                    let ryd_tx = resp_tx.clone();
                                    let ryd_wake = wake.clone();
                                    tokio::spawn(async move {
                                        if let Some(dislikes_text) =
                                            sponsorblock::fetch_dislikes_count(&ryd_http, &ryd_id)
                                                .await
                                        {
                                            let _ = ryd_tx.send(Response::DislikesLoaded {
                                                video_id: ryd_id,
                                                dislikes_text,
                                            });
                                            ryd_wake();
                                        }
                                    });
                                }
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
                                            is_editable,
                                            FeedPage {
                                                items,
                                                continuation,
                                                ..
                                            },
                                        )) => {
                                            let _ = resp_tx.send(Response::PlaylistLoaded {
                                                browse_id,
                                                title,
                                                is_editable,
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
                            Request::LoadFullPlaylistForQueue {
                                browse_id,
                                continuation,
                                shuffle,
                            } => {
                                let cfg_clone = cfg.clone();
                                let sess_clone = session.clone();
                                tokio::spawn(async move {
                                    let mut all = Vec::new();
                                    let mut cont = Some(continuation);
                                    for _ in 0..8 {
                                        let Some(tok) = cont.take() else { break };
                                        match innertube::playlist_feed(
                                            &http,
                                            &browse_id,
                                            Some(&tok),
                                            &cfg_clone,
                                            sess_clone.as_ref(),
                                        )
                                        .await
                                        {
                                            Ok((_, _, page)) => {
                                                all.extend(page.items);
                                                cont = page.continuation;
                                            }
                                            Err(_) => break,
                                        }
                                    }
                                    if !all.is_empty() {
                                        let _ = resp_tx.send(Response::FullPlaylistQueueLoaded {
                                            items: all,
                                            shuffle,
                                        });
                                        wake();
                                    }
                                });
                            }
                            Request::PlaylistRemoveItem {
                                playlist_id,
                                video_id,
                                set_video_id,
                            } => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to edit playlists".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::remove_playlist_item(
                                        &http,
                                        &playlist_id,
                                        &video_id,
                                        set_video_id.as_deref(),
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::PlaylistItemRemoved);
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Remove from playlist failed: {e:#}"
                                            )));
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::PlaylistMoveItem {
                                playlist_id,
                                set_video_id,
                                successor_set_video_id,
                            } => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to reorder playlists".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::reorder_playlist_item(
                                        &http,
                                        &playlist_id,
                                        &set_video_id,
                                        successor_set_video_id.as_deref(),
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::PlaylistReordered);
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Reorder playlist failed: {e:#}"
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
                            Request::LoadShorts => {
                                match innertube::shorts_feed(&http, &cfg, session.as_ref()).await {
                                    Ok(items) => {
                                        let _ = resp_tx.send(Response::ShortsLoaded { items });
                                    }
                                    Err(e) => {
                                        let _ = resp_tx.send(Response::Error(format!(
                                            "Shorts load failed: {e:#}"
                                        )));
                                    }
                                }
                                wake();
                            }
                            Request::LoadShortsQueue { seed_video_id } => {
                                let cfg_clone = cfg.clone();
                                let sess_clone = session.clone();
                                tokio::spawn(async move {
                                    if let Ok(items) = innertube::fetch_shorts_sequence(
                                        &http,
                                        &seed_video_id,
                                        &cfg_clone,
                                        sess_clone.as_ref(),
                                    )
                                    .await
                                    {
                                        let _ = resp_tx.send(Response::ShortsQueueLoaded {
                                            seed_video_id,
                                            items,
                                        });
                                        wake();
                                    }
                                });
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
                                replay_offset_ms,
                            } => {
                                let cfg_clone = cfg.clone();
                                let sess_clone = session.clone();
                                tokio::spawn(async move {
                                    if let Ok(page) = innertube::fetch_live_chat(
                                        &http,
                                        &continuation,
                                        replay_offset_ms,
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
                                            emotes: page.emotes,
                                            is_replay: page.is_replay,
                                        });
                                        wake();
                                    }
                                });
                            }
                            Request::SendLiveChat {
                                video_id,
                                params,
                                message,
                                emotes,
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
                                        &emotes,
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
                            Request::PerformCommentAction {
                                video_id,
                                comment_id,
                                action_params,
                                liked,
                            } => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to like comments".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::perform_comment_action(
                                        &http,
                                        &action_params,
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::CommentActionUpdated {
                                                video_id,
                                                comment_id,
                                                liked,
                                            });
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Comment action failed: {e:#}"
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
                                let guide_snap = latest_guide_channels.clone();
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
                                                guide_snap,
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
                            Request::LoadNotifications => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to view notifications".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                let guide_snap = latest_guide_channels.clone();
                                let home_snap = latest_home_items.clone();
                                let tc_snap = thumb_cache.clone();
                                let dir_save = dir_clone.clone();
                                tokio::spawn(async move {
                                    let menu_res = innertube::fetch_notifications_menu(
                                        &http, &cfg_clone, &s,
                                    )
                                    .await;
                                    match menu_res {
                                        Ok((items, unseen_count, seen_params))
                                            if !items.is_empty() =>
                                        {
                                            let _ = resp_tx.send(Response::NotificationsLoaded {
                                                items,
                                                unseen_count,
                                                seen_params,
                                            });
                                        }
                                        native_empty_or_err => {
                                            let sub_res = innertube::subscriptions_feed(
                                                &http, None, &cfg_clone, &s,
                                            )
                                            .await;
                                            match sub_res {
                                                Ok(page) => {
                                                    let guide = guide_snap.lock().unwrap().clone();
                                                    let home = home_snap.lock().unwrap().clone();
                                                    let av_map = tc_snap
                                                        .lock()
                                                        .unwrap()
                                                        .channel_avatars
                                                        .clone();
                                                    let seed = std::time::SystemTime::now()
                                                        .duration_since(std::time::UNIX_EPOCH)
                                                        .map_or(1, |d| d.as_nanos() as u64);
                                                    let (mut items, unseen_count) =
                                                        innertube::build_subscription_notifications(
                                                            &page.items,
                                                            &guide,
                                                            &home,
                                                            &av_map,
                                                            seed,
                                                        );
                                                    let missing_cids: Vec<String> = items
                                                        .iter()
                                                        .filter(|n| n.channel_avatar_url.is_none())
                                                        .filter_map(|n| n.channel_id.clone())
                                                        .collect();
                                                    if !missing_cids.is_empty() {
                                                        let fetched =
                                                            innertube::fetch_channel_avatars(
                                                                &http,
                                                                missing_cids,
                                                                &cfg_clone,
                                                                Some(&s),
                                                            )
                                                            .await;
                                                        if !fetched.is_empty() {
                                                            for n in &mut items {
                                                                if n.channel_avatar_url.is_none() {
                                                                    if let Some(cid) =
                                                                        &n.channel_id
                                                                    {
                                                                        if let Some(av) =
                                                                            fetched.get(cid)
                                                                        {
                                                                            n.channel_avatar_url =
                                                                                Some(av.clone());
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            let mut tc = tc_snap.lock().unwrap();
                                                            for (cid, av) in fetched {
                                                                tc.channel_avatars.insert(cid, av);
                                                            }
                                                            tc.save(&dir_save);
                                                        }
                                                    }
                                                    let _ = resp_tx.send(
                                                        Response::NotificationsLoaded {
                                                            items,
                                                            unseen_count,
                                                            seen_params: None,
                                                        },
                                                    );
                                                }
                                                Err(sub_err) => {
                                                    if let Ok((items, unseen_count, seen_params)) =
                                                        native_empty_or_err
                                                    {
                                                        let _ = resp_tx.send(
                                                            Response::NotificationsLoaded {
                                                                items,
                                                                unseen_count,
                                                                seen_params,
                                                            },
                                                        );
                                                    } else {
                                                        let _ = resp_tx.send(Response::Error(
                                                            format!(
                                                                "Notifications load failed: {sub_err:#}"
                                                            ),
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    wake();
                                });
                            }
                            Request::MarkNotificationsSeen { params } => {
                                let Some(s) = session.clone() else {
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    let _ = innertube::mark_notifications_seen(
                                        &http,
                                        &params,
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await;
                                    let _ = resp_tx.send(Response::NotificationsCountLoaded(0));
                                    wake();
                                });
                            }
                            Request::SetChannelNotifications {
                                channel_id,
                                level,
                                params,
                            } => {
                                let Some(s) = session.clone() else {
                                    let _ = resp_tx.send(Response::Error(
                                        "Sign in to change notification settings".into(),
                                    ));
                                    wake();
                                    continue;
                                };
                                let cfg_clone = cfg.clone();
                                tokio::spawn(async move {
                                    match innertube::modify_channel_notification_preference(
                                        &http,
                                        &params,
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(
                                                Response::ChannelNotificationLevelUpdated {
                                                    channel_id,
                                                    level,
                                                },
                                            );
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::Error(format!(
                                                "Notification update failed: {e:#}"
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

                                    let po_cli = potoken.clone();
                                    let po_mode =
                                        PoTokenMode::from_u8(po_token_mode.load(Ordering::Relaxed));
                                    match resolve_watch(
                                        &http,
                                        &data_dir,
                                        &video_id,
                                        quality,
                                        &cfg_clone,
                                        sess_clone.as_ref(),
                                        proxy_clone.as_ref(),
                                        &po_cli,
                                        po_mode,
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
                            Request::RecoverStream {
                                video_id,
                                quality,
                                preferred_height,
                                preferred_audio_id,
                                resume_secs,
                            } => {
                                let cfg_clone = cfg.clone();
                                let sess_clone = session.clone();
                                let proxy_clone = proxy.clone();
                                tokio::spawn(async move {
                                    let effective_quality = match preferred_height {
                                        Some(h) if h <= 480 => PreferredQuality::P480,
                                        Some(h) if h <= 720 => PreferredQuality::P720,
                                        Some(h) if h <= 1080 => PreferredQuality::P1080,
                                        Some(h) if h <= 1440 => PreferredQuality::P1440,
                                        Some(_) => PreferredQuality::Best4K,
                                        None => quality,
                                    };
                                    let po_cli = potoken.clone();
                                    let po_mode =
                                        PoTokenMode::from_u8(po_token_mode.load(Ordering::Relaxed));
                                    match resolve_watch(
                                        &http,
                                        &data_dir,
                                        &video_id,
                                        effective_quality,
                                        &cfg_clone,
                                        sess_clone.as_ref(),
                                        proxy_clone.as_ref(),
                                        &po_cli,
                                        po_mode,
                                    )
                                    .await
                                    {
                                        Ok(Response::WatchReady {
                                            data,
                                            mut selected_video_url,
                                            mut selected_audio_url,
                                            mut selected_format_idx,
                                            used_ytdlp,
                                        }) => {
                                            let mut selected_audio_idx = 0;
                                            if let Some(aid) = preferred_audio_id.as_deref() {
                                                if let Some(aidx) =
                                                    data.audio_tracks.iter().position(|t| t.id == aid)
                                                {
                                                    selected_audio_idx = aidx;
                                                }
                                            }
                                            if let Some(h) = preferred_height {
                                                if let Some(idx) =
                                                    pick_format_index(&data.video_formats, h)
                                                {
                                                    let fmt = &data.video_formats[idx];
                                                    let wrap = |url: &str, len: u64| -> String {
                                                        let is_hls = url.contains(".m3u8")
                                                            || url.contains("/manifest/hls");
                                                        match proxy_clone.as_ref() {
                                                            Some(p) if !is_hls => {
                                                                p.register(url, len)
                                                            }
                                                            _ => url.to_owned(),
                                                        }
                                                    };
                                                    selected_format_idx = Some(idx);
                                                    selected_video_url =
                                                        wrap(&fmt.url, fmt.content_length);
                                                    selected_audio_url = if fmt.has_audio {
                                                        None
                                                    } else if let Some(tr) =
                                                        data.audio_tracks.get(selected_audio_idx)
                                                    {
                                                        Some(wrap(&tr.url, tr.content_length))
                                                    } else {
                                                        data.audio_url.as_deref().map(|u| {
                                                            wrap(u, data.audio_content_length)
                                                        })
                                                    };
                                                }
                                            }
                                            let _ = resp_tx.send(Response::StreamRecovered {
                                                video_id,
                                                video_formats: data.video_formats,
                                                audio_url: data.audio_url,
                                                audio_content_length: data.audio_content_length,
                                                audio_tracks: data.audio_tracks,
                                                hls_url: data.hls_url,
                                                selected_video_url,
                                                selected_audio_url,
                                                selected_format_idx,
                                                selected_audio_idx,
                                                used_ytdlp,
                                                resume_secs,
                                            });
                                        }
                                        Ok(_) => {}
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::StreamRecoverFailed {
                                                video_id,
                                                message: format!("Stream recovery failed: {e:#}"),
                                            });
                                        }
                                    }
                                    wake();
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
                                            *shared_session.lock().unwrap() = Some(s.clone());
                                            let session_ref = Some(s);
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
                                                session_ref.as_ref(),
                                            )
                                            .await
                                            {
                                                if visitor_data.is_some() {
                                                    cfg.visitor_data = visitor_data;
                                                }
                                                *latest_home_items.lock().unwrap() = items.clone();
                                                let _ = resp_tx.send(Response::HomeLoaded {
                                                    items,
                                                    chips,
                                                    continuation,
                                                    append: false,
                                                    is_fallback_trending,
                                                });
                                            }
                                            if let Some(ref s) = session_ref {
                                                tokio::spawn(load_guide_and_backfill_thumbs(
                                                    http.clone(),
                                                    cfg.clone(),
                                                    s.clone(),
                                                    thumb_cache.clone(),
                                                    latest_guide_channels.clone(),
                                                    data_dir.clone(),
                                                    resp_tx.clone(),
                                                    wake.clone(),
                                                ));
                                                let http_sub = http.clone();
                                                let cfg_sub = cfg.clone();
                                                let s_sub = s.clone();
                                                let tx_sub = resp_tx.clone();
                                                let wake_sub = wake.clone();
                                                tokio::spawn(async move {
                                                    if let Ok(page) = innertube::subscriptions_feed(
                                                        &http_sub, None, &cfg_sub, &s_sub,
                                                    )
                                                    .await
                                                    {
                                                        let _ = tx_sub.send(
                                                            Response::SubscriptionsLoaded {
                                                                items: page.items,
                                                                continuation: page.continuation,
                                                                append: false,
                                                            },
                                                        );
                                                        wake_sub();
                                                    }
                                                });
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
                                *shared_session.lock().unwrap() = None;
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
                            Request::SetPoTokenMode(mode) => {
                                po_token_mode.store(mode.as_u8(), Ordering::Relaxed);
                                if mode == PoTokenMode::Off {
                                    potoken.close_if_running();
                                }
                            }
                            Request::LoadAmbientArt { key, url } => {
                                tokio::spawn(async move {
                                    let Ok(resp) = http
                                        .get(&url)
                                        .timeout(std::time::Duration::from_secs(10))
                                        .send()
                                        .await
                                    else {
                                        return;
                                    };
                                    let Ok(bytes) = resp.bytes().await else {
                                        return;
                                    };
                                    let ok = tokio::task::spawn_blocking(move || {
                                        blur_ambient_art(&bytes)
                                    })
                                    .await
                                    .ok()
                                    .flatten();
                                    if let Some((width, height, rgba)) = ok {
                                        let _ = resp_tx.send(Response::AmbientArtReady {
                                            key,
                                            width,
                                            height,
                                            rgba,
                                        });
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

async fn rotate_cookies_loop(
    http: reqwest::Client,
    shared_session: Arc<Mutex<Option<Session>>>,
    data_dir: PathBuf,
    resp_tx: Sender<Response>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    tokio::time::sleep(std::time::Duration::from_secs(15)).await;
    loop {
        let cur = shared_session.lock().unwrap().clone();
        if let Some(sess) = cur {
            match sess.rotate(&http).await {
                Ok(lines) => {
                    let updated = {
                        let mut guard = shared_session.lock().unwrap();
                        if let Some(ref active) = *guard {
                            if let Some(next) =
                                active.updated(lines.iter().map(String::as_str))
                            {
                                *guard = Some(next.clone());
                                Some(next)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };
                    if let Some(s) = updated {
                        s.save(&data_dir);
                    }
                }
                Err(e) if e.is::<crate::auth::RevokedError>() => {
                    *shared_session.lock().unwrap() = None;
                    Session::clear(&data_dir);
                    let _ = resp_tx.send(Response::SessionRevoked);
                    wake();
                }
                Err(e) => {
                    log::warn!("cookie rotation skipped (transient): {e:#}");
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(480)).await;
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

async fn populate_hls_variants_if_needed(http: &reqwest::Client, data: &mut WatchData) {
    let Some(hls) = data.hls_url.as_deref() else {
        return;
    };
    if !data.is_live && !data.video_formats.is_empty() {
        return;
    }
    let (fmts, audio) = innertube::fetch_hls_variants(http, hls).await;
    if !fmts.is_empty() {
        data.video_formats = fmts;
    }
    if (data.is_live || data.audio_tracks.is_empty() || data.video_formats.is_empty())
        && !audio.is_empty()
    {
        data.audio_url = audio.first().map(|t| t.url.clone());
        data.audio_content_length = 0;
        data.audio_tracks = audio;
    }
}

fn apply_ytdlp_streams(
    data: &mut WatchData,
    yt: ytdlp::YtdlpStreams,
    quality: PreferredQuality,
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
    if let Some(live) = yt.is_live {
        data.is_live = live;
    }
    if data.is_live {
        if let Some(hls) = data.hls_url.clone() {
            let hint_idx = if quality != PreferredQuality::Best4K {
                pick_format_index(&data.video_formats, quality.max_height())
            } else {
                None
            };
            return (hls, None, hint_idx);
        }
    }
    let fmt_idx = pick_format_index(&data.video_formats, quality.max_height());
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

async fn mint_po_tokens_if_needed(
    http: &reqwest::Client,
    cfg: &WebConfig,
    session: Option<&Session>,
    potoken: &PoTokenClient,
    video_id: &str,
) -> Option<crate::potoken::PoTokens> {
    let vd = match cfg.visitor_data.as_deref().filter(|s| !s.is_empty()) {
        Some(v) => v.to_owned(),
        None => innertube::fetch_web_config(http, session)
            .await
            .visitor_data
            .unwrap_or_default(),
    };
    if vd.is_empty() {
        log::warn!("potoken: visitor_data unavailable, skipping mint");
        return None;
    }
    match potoken.mint(&vd, video_id).await {
        Ok(toks) => {
            log::info!("potoken: minted BotGuard PO tokens for {video_id}");
            Some(toks)
        }
        Err(e) => {
            log::warn!("potoken: mint failed for {video_id}: {e:#}");
            None
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn resolve_ytdlp_with_potoken(
    http: &reqwest::Client,
    data_dir: &Path,
    video_id: &str,
    max_height: u32,
    cfg: &WebConfig,
    session: Option<&Session>,
    potoken: &PoTokenClient,
    po_mode: PoTokenMode,
    blocked_hint: bool,
) -> anyhow::Result<ytdlp::YtdlpStreams> {
    let cookie = session.map(|s| s.cookie.as_str());
    let eager_pot = match po_mode {
        PoTokenMode::On => mint_po_tokens_if_needed(http, cfg, session, potoken, video_id).await,
        PoTokenMode::Auto if blocked_hint => {
            mint_po_tokens_if_needed(http, cfg, session, potoken, video_id).await
        }
        _ => None,
    };

    let res = ytdlp::resolve_video(
        http,
        data_dir,
        cookie,
        video_id,
        max_height,
        eager_pot.as_ref(),
    )
    .await;

    match res {
        Ok(s) => Ok(s),
        Err(err) => {
            let msg = format!("{err:#}");
            if po_mode == PoTokenMode::Auto
                && eager_pot.is_none()
                && ytdlp::needs_potoken_retry(&msg)
            {
                if let Some(pot) =
                    mint_po_tokens_if_needed(http, cfg, session, potoken, video_id).await
                {
                    return ytdlp::resolve_video(
                        http,
                        data_dir,
                        cookie,
                        video_id,
                        max_height,
                        Some(&pot),
                    )
                    .await;
                }
            }
            Err(err)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn resolve_watch(
    http: &reqwest::Client,
    data_dir: &PathBuf,
    video_id: &str,
    quality: PreferredQuality,
    cfg: &WebConfig,
    session: Option<&Session>,
    proxy: Option<&StreamProxy>,
    potoken: &PoTokenClient,
    po_mode: PoTokenMode,
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
            let had_direct_formats = !data.video_formats.is_empty();
            populate_hls_variants_if_needed(http, &mut data).await;
            // Livestreams and HLS-only VODs (e.g. post-live broadcasts with noclen=1 adaptive URLs)
            // play with synced audio via master HLS manifest and native libmpv `vid` track switching.
            if data.is_live || !had_direct_formats {
                if let Some(hls) = data.hls_url.clone() {
                    if po_mode == PoTokenMode::Auto {
                        potoken.close_if_running();
                    }
                    let hint_idx = if quality != PreferredQuality::Best4K {
                        pick_format_index(&data.video_formats, quality.max_height())
                    } else {
                        None
                    };
                    return Ok(Response::WatchReady {
                        selected_video_url: hls,
                        selected_audio_url: None,
                        selected_format_idx: hint_idx,
                        data: Box::new(data),
                        used_ytdlp: false,
                    });
                }
            }
            if let Some(idx) = pick_format_index(&data.video_formats, quality.max_height()) {
                if po_mode == PoTokenMode::Auto {
                    potoken.close_if_running();
                }
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
                if po_mode == PoTokenMode::Auto {
                    potoken.close_if_running();
                }
                return Ok(Response::WatchReady {
                    selected_video_url: hls,
                    selected_audio_url: None,
                    selected_format_idx: None,
                    data: Box::new(data),
                    used_ytdlp: false,
                });
            }
            let yt = resolve_ytdlp_with_potoken(
                http,
                data_dir,
                video_id,
                quality.max_height(),
                cfg,
                session,
                potoken,
                po_mode,
                false,
            )
            .await?;
            let (selected_video_url, selected_audio_url, fmt_idx) =
                apply_ytdlp_streams(&mut data, yt, quality, proxy);
            Ok(Response::WatchReady {
                selected_video_url,
                selected_audio_url,
                selected_format_idx: fmt_idx,
                data: Box::new(data),
                used_ytdlp: true,
            })
        }
        Err(inner_err) => {
            let inner_msg = format!("{inner_err:#}");
            let blocked_hint = ytdlp::is_auth_required_error(&inner_msg)
                || inner_msg.contains("UNPLAYABLE");
            log::warn!("InnerTube watch failed ({inner_msg}), falling back to yt-dlp");
            let (yt_res, next_opt) = tokio::join!(
                resolve_ytdlp_with_potoken(
                    http,
                    data_dir,
                    video_id,
                    quality.max_height(),
                    cfg,
                    session,
                    potoken,
                    po_mode,
                    blocked_hint,
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
                dislikes_text: None,
                is_liked: false,
                is_disliked: false,
                is_subscribed: false,
                notifications: ChannelNotificationParams::default(),
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
                is_members_only: false,
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
                apply_ytdlp_streams(&mut data, yt, quality, proxy);
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

fn blur_ambient_art(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    const N: u32 = 64;
    let img = image::load_from_memory(bytes).ok()?.to_rgb8();
    let small = image::imageops::resize(&img, N, N, image::imageops::FilterType::Triangle);

    let mut sum = [0.0f32; 3];
    let mut best = (0.0f32, [120.0f32, 90.0, 210.0]);
    for p in small.pixels() {
        let rgb = [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0];
        sum[0] += rgb[0];
        sum[1] += rgb[1];
        sum[2] += rgb[2];
        let max_c = rgb[0].max(rgb[1]).max(rgb[2]);
        let min_c = rgb[0].min(rgb[1]).min(rgb[2]);
        let sat = if max_c > 0.001 {
            (max_c - min_c) / max_c
        } else {
            0.0
        };
        let score = sat * max_c * (1.0 - (max_c - 0.65).abs() * 0.4);
        if score > best.0 {
            best = (score, [p[0] as f32, p[1] as f32, p[2] as f32]);
        }
    }
    let count = (N * N) as f32;
    let tint = if best.0 > 0.08 {
        best.1
    } else {
        [
            (sum[0] / count * 255.0).max(56.0),
            (sum[1] / count * 255.0).max(56.0),
            (sum[2] / count * 255.0).max(72.0),
        ]
    };

    let mut px: Vec<[f32; 3]> = small
        .pixels()
        .map(|p| {
            let r = p[0] as f32;
            let g = p[1] as f32;
            let b = p[2] as f32;
            let luma = (0.299 * r + 0.587 * g + 0.114 * b) / 255.0;
            let dark = (1.0 - (luma / 0.45).clamp(0.0, 1.0)).powi(2);
            [
                r + (tint[0] - r) * dark * 0.42,
                g + (tint[1] - g) * dark * 0.42,
                b + (tint[2] - b) * dark * 0.42,
            ]
        })
        .collect();

    let n = N as usize;
    let box_pass = |src: &[[f32; 3]], horizontal: bool| -> Vec<[f32; 3]> {
        let r = 5isize;
        let d = (2 * r + 1) as f32;
        let mut out = vec![[0.0f32; 3]; src.len()];
        for y in 0..n {
            for x in 0..n {
                let mut acc = [0.0f32; 3];
                for o in -r..=r {
                    let (sx, sy) = if horizontal {
                        ((x as isize + o).clamp(0, n as isize - 1) as usize, y)
                    } else {
                        (x, (y as isize + o).clamp(0, n as isize - 1) as usize)
                    };
                    let p = src[sy * n + sx];
                    acc[0] += p[0];
                    acc[1] += p[1];
                    acc[2] += p[2];
                }
                out[y * n + x] = [acc[0] / d, acc[1] / d, acc[2] / d];
            }
        }
        out
    };
    for _ in 0..3 {
        let h = box_pass(&px, true);
        px = box_pass(&h, false);
    }

    let rgba: Vec<u8> = px
        .into_iter()
        .flat_map(|p| {
            [
                p[0].clamp(0.0, 255.0) as u8,
                p[1].clamp(0.0, 255.0) as u8,
                p[2].clamp(0.0, 255.0) as u8,
                255,
            ]
        })
        .collect();
    Some((N, N, rgba))
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
