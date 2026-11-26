use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender, unbounded};

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
    LoadGuide,
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
    LoadLiveChat {
        video_id: String,
        continuation: String,
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
        video_id: Option<String>,
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
    },
    SignIn,
    SignOut,
    DownloadLibmpv,
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
        append: bool,
    },
    LiveChatLoaded {
        video_id: String,
        messages: Vec<LiveChatMessage>,
        next_token: Option<String>,
    },
    LikeUpdated {
        video_id: String,
        liked: bool,
    },
    DislikeUpdated {
        video_id: String,
        disliked: bool,
    },
    WatchLaterUpdated {
        video_id: String,
        added: bool,
    },
    PlaylistEdited {
        playlist_title: String,
        added: bool,
    },
    PlaylistOptionsLoaded {
        video_id: String,
        options: Vec<PlaylistOption>,
    },
    PlaylistCreated {
        title: String,
    },
    SubscribeUpdated {
        channel_id: String,
        subscribed: bool,
    },
    VideoDownloaded {
        video_id: String,
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

pub struct Backend {
    pub tx: Sender<Request>,
    pub rx: Receiver<Response>,
    pub data_dir: PathBuf,
    proxy: Option<StreamProxy>,
    pub cursor: Arc<PlaybackCursor>,
}

impl Backend {
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

        std::thread::Builder::new()
            .name("yt-rust-backend".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                rt.block_on(async move {
                    let http = reqwest::Client::builder()
                        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                        .build()
                        .expect("reqwest client");

                    let proxy = StreamProxy::start(http.clone()).await.ok();
                    let _ = proxy_tx.send(proxy.clone());

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
                        if let Ok((channels, playlists)) =
                            innertube::fetch_guide(&http, &cfg, s).await
                        {
                            let _ = resp_tx.send(Response::GuideLoaded { channels, playlists });
                            wake();
                        }
                    }

                    while let Ok(req) = req_rx.recv() {
                        let http = http.clone();
                        let resp_tx = resp_tx.clone();
                        let wake = wake.clone();
                        let data_dir = dir_clone.clone();

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
                                    Ok(FeedPage {
                                        items,
                                        continuation,
                                        visitor_data,
                                        ..
                                    }) => {
                                        if visitor_data.is_some() {
                                            cfg.visitor_data = visitor_data;
                                        }
                                        let _ = resp_tx.send(Response::HistoryLoaded {
                                            items,
                                            continuation,
                                            append,
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
                            Request::LoadGuide => {
                                if let Some(s) = session.clone() {
                                    let cfg_clone = cfg.clone();
                                    tokio::spawn(async move {
                                        if let Ok((channels, playlists)) =
                                            innertube::fetch_guide(&http, &cfg_clone, &s).await
                                        {
                                            let _ = resp_tx.send(Response::GuideLoaded {
                                                channels,
                                                playlists,
                                            });
                                            wake();
                                        }
                                    });
                                }
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
                                        Ok((comments, next_token)) => {
                                            let _ = resp_tx.send(Response::CommentsLoaded {
                                                video_id,
                                                comments,
                                                next_token,
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
                            Request::LoadLiveChat {
                                video_id,
                                continuation,
                            } => {
                                let cfg_clone = cfg.clone();
                                let sess_clone = session.clone();
                                tokio::spawn(async move {
                                    if let Ok((messages, next_token)) =
                                        innertube::fetch_live_chat(
                                            &http,
                                            &continuation,
                                            &cfg_clone,
                                            sess_clone.as_ref(),
                                        )
                                        .await
                                    {
                                        let _ = resp_tx.send(Response::LiveChatLoaded {
                                            video_id,
                                            messages,
                                            next_token,
                                        });
                                        wake();
                                    }
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
                                            let _ = resp_tx.send(Response::WatchLaterUpdated {
                                                video_id,
                                                added: add,
                                            });
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
                                        Ok(options) => {
                                            let _ = resp_tx.send(Response::PlaylistOptionsLoaded {
                                                video_id,
                                                options,
                                            });
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
                            Request::CreatePlaylist { title, video_id } => {
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
                                        video_id.as_deref(),
                                        &cfg_clone,
                                        &s,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::PlaylistCreated {
                                                title,
                                            });
                                            if let Ok((channels, playlists)) =
                                                innertube::fetch_guide(&http, &cfg_clone, &s).await
                                            {
                                                let _ = resp_tx.send(Response::GuideLoaded {
                                                    channels,
                                                    playlists,
                                                });
                                            }
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
                            } => {
                                tokio::spawn(async move {
                                    let result = ytdlp::download_video(
                                        &http,
                                        &data_dir,
                                        &video_id,
                                        audio_only,
                                        max_height,
                                    )
                                    .await
                                    .map_err(|e| format!("{e:#}"));
                                    let _ = resp_tx.send(Response::VideoDownloaded {
                                        video_id,
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
                                                if let Ok((channels, playlists)) =
                                                    innertube::fetch_guide(&http, &cfg, s).await
                                                {
                                                    let _ = resp_tx.send(Response::GuideLoaded {
                                                        channels,
                                                        playlists,
                                                    });
                                                }
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
        match proxy {
            Some(p) => p.register(url, len),
            None => url.to_owned(),
        }
    };
    match innertube::watch(http, video_id, cfg, session).await {
        Ok(data) => {
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
            let yt = ytdlp::resolve_video(http, data_dir, video_id, quality.max_height()).await?;
            Ok(Response::WatchReady {
                selected_video_url: yt.video_url,
                selected_audio_url: yt.audio_url,
                selected_format_idx: None,
                data: Box::new(data),
                used_ytdlp: true,
            })
        }
        Err(inner_err) => {
            log::warn!("InnerTube watch failed ({inner_err:#}), falling back to yt-dlp");
            let yt = ytdlp::resolve_video(http, data_dir, video_id, quality.max_height()).await?;
            let data = WatchData {
                id: video_id.to_owned(),
                title: yt.title.unwrap_or_else(|| format!("Video {video_id}")),
                channel: yt.channel.unwrap_or_else(|| "Unknown".to_owned()),
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
                duration_secs: yt.duration_secs,
                video_formats: Vec::new(),
                audio_url: yt.audio_url.clone(),
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
            };
            Ok(Response::WatchReady {
                selected_video_url: yt.video_url,
                selected_audio_url: yt.audio_url,
                selected_format_idx: None,
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
