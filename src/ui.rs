use std::collections::HashMap;
use std::time::{Duration, Instant};

use eframe::glow;
use egui::{
    Align, Align2, Color32, CornerRadius, CursorIcon, Frame, Key, Layout, Margin, Rect, RichText,
    ScrollArea, Sense, Stroke, Vec2, ViewportCommand, pos2, vec2,
};

use crate::backend::{Backend, Request, Response};
use crate::innertube;
use crate::model::{
    ChannelInfo, CommentItem, FeedChip, LiveChatMessage, PlaylistOption, PlaylistSummary,
    PreferredQuality, SidebarChannel, SponsorCategory, SponsorSegment, VideoItem, WatchData,
};
use crate::mpv::{GlProcAddrFn, MpvPlayer, PlaybackState};
use crate::theme::{self, icon};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Subscriptions,
    Trending,
    Playlist,
    Channel,
    Search,
    Watch,
    History,
    Saved,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RightTab {
    UpNext,
    Queue,
    LiveChat,
}

pub struct YtApp {
    backend: Backend,
    mpv: Option<MpvPlayer>,
    mpv_error: Option<String>,
    get_proc_address: Option<GlProcAddrFn>,
    downloading_mpv: bool,

    page: Page,
    prev_page: Page,
    sidebar_expanded: bool,

    signed_in: bool,
    signing_in: bool,
    account_name: Option<String>,
    account_avatar: Option<String>,

    search_input: String,
    active_query: String,
    search_suggestions: Vec<String>,
    last_suggest_query: String,
    last_suggest_edit: Option<Instant>,
    search_popup_open: bool,

    guide_channels: Vec<SidebarChannel>,
    guide_playlists: Vec<PlaylistSummary>,
    subs_sidebar_expanded: bool,
    playlists_sidebar_expanded: bool,

    active_playlist_id: Option<String>,
    active_playlist_title: String,
    playlist_items: Vec<VideoItem>,
    playlist_continuation: Option<String>,
    playlist_loading: bool,

    home_items: Vec<VideoItem>,
    home_chips: Vec<FeedChip>,
    home_is_fallback: bool,
    home_continuation: Option<String>,
    home_loading: bool,
    selected_topic_chip: usize,
    chips_scroll_x: f32,
    chips_dragging: bool,
    chips_drag_dist: f32,

    subs_items: Vec<VideoItem>,
    subs_continuation: Option<String>,
    subs_loading: bool,

    trending_items: Vec<VideoItem>,
    trending_loading: bool,

    channel_id: Option<String>,
    channel_name: String,
    channel_info: Option<ChannelInfo>,
    channel_tab_params: Option<String>,
    channel_items: Vec<VideoItem>,
    channel_continuation: Option<String>,
    channel_loading: bool,

    search_items: Vec<VideoItem>,
    search_continuation: Option<String>,
    search_loading: bool,

    history: Vec<VideoItem>,
    remote_history: Vec<VideoItem>,
    remote_history_continuation: Option<String>,
    remote_history_loading: bool,
    saved: Vec<VideoItem>,
    queue: Vec<VideoItem>,
    resume_positions: HashMap<String, f64>,
    pending_resume: Option<(String, f64, Option<f32>)>,

    watch_loading: bool,
    watch: Option<WatchData>,
    selected_format_idx: Option<usize>,
    selected_audio_idx: usize,
    selected_caption_idx: Option<usize>,
    used_ytdlp: bool,
    sponsor_segments: Vec<SponsorSegment>,
    last_skipped_end: f64,

    comments: Vec<CommentItem>,
    comments_next_token: Option<String>,
    comments_loading: bool,
    show_chapters: bool,

    live_chat: Vec<LiveChatMessage>,
    live_chat_token: Option<String>,
    live_chat_next_poll: Option<Instant>,
    right_tab: RightTab,

    playback: PlaybackState,
    seeking_pos: Option<f64>,
    volume: f64,
    muted: bool,
    speed: f64,
    preferred_quality: PreferredQuality,
    sponsorblock_enabled: bool,
    sync_history: bool,
    autoplay_next: bool,
    theater_mode: bool,
    fullscreen: bool,
    show_description: bool,

    status_toast: Option<(String, Instant)>,
    last_error: Option<String>,
    playlist_modal_video: Option<VideoItem>,
    playlist_options: Vec<PlaylistOption>,
    playlist_options_loading: bool,
    playlist_filter: String,
    new_playlist_title: String,
}

const GUEST_TOPICS: &[(&str, Option<&str>)] = &[
    ("All", None),
    ("Music", Some("official music video 2026")),
    ("Gaming", Some("gaming gameplay highlights")),
    ("Tech", Some("technology hardware review")),
    ("Programming", Some("rust programming software engineering")),
    ("Podcasts", Some("podcast full episode")),
    ("Science", Some("science documentary explained")),
];

impl YtApp {
    pub fn new(cc: &eframe::CreationContext<'_>, backend: Backend) -> Self {
        theme::install(&cc.egui_ctx);

        let get_proc_address = cc.get_proc_address.clone();
        let (mpv, mpv_error) = match get_proc_address.clone() {
            Some(gpa) => match MpvPlayer::new(&backend.data_dir, gpa, cc.egui_ctx.clone()) {
                Ok(player) => (Some(player), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            },
            None => (None, Some("OpenGL get_proc_address unavailable".to_owned())),
        };

        let mut volume = 85.0f64;
        let mut sponsorblock_enabled = true;
        let mut sync_history = true;
        let mut autoplay_next = true;
        let mut preferred_quality = PreferredQuality::P1080;
        let mut sidebar_expanded = true;
        let mut history = Vec::new();
        let mut saved = Vec::new();
        let mut resume_positions = HashMap::new();

        if let Some(storage) = cc.storage {
            if let Some(v) = storage.get_string("volume").and_then(|s| s.parse().ok()) {
                volume = v;
            }
            if let Some(sb) = storage.get_string("sponsorblock") {
                sponsorblock_enabled = sb != "0";
            }
            if let Some(sh) = storage.get_string("sync_history") {
                sync_history = sh != "0";
            }
            if let Some(ap) = storage.get_string("autoplay") {
                autoplay_next = ap != "0";
            }
            if let Some(se) = storage.get_string("sidebar") {
                sidebar_expanded = se != "0";
            }
            if let Some(q) = storage.get_string("quality") {
                preferred_quality = match q.as_str() {
                    "4k" => PreferredQuality::Best4K,
                    "1440" => PreferredQuality::P1440,
                    "720" => PreferredQuality::P720,
                    "480" => PreferredQuality::P480,
                    _ => PreferredQuality::P1080,
                };
            }
            if let Some(h) = storage.get_string("history") {
                if let Ok(parsed) = serde_json::from_str::<Vec<VideoItem>>(&h) {
                    history = parsed;
                }
            }
            if let Some(s) = storage.get_string("saved") {
                if let Ok(parsed) = serde_json::from_str::<Vec<VideoItem>>(&s) {
                    saved = parsed;
                }
            }
            if let Some(r) = storage.get_string("resume_positions") {
                if let Ok(parsed) = serde_json::from_str::<HashMap<String, f64>>(&r) {
                    resume_positions = parsed;
                }
            }
        }

        if let Some(p) = &mpv {
            p.set_volume(volume);
        }

        let _ = backend.tx.send(Request::LoadHome {
            params: None,
            continuation: None,
        });

        Self {
            backend,
            mpv,
            mpv_error,
            get_proc_address,
            downloading_mpv: false,
            page: Page::Home,
            prev_page: Page::Home,
            sidebar_expanded,
            signed_in: false,
            signing_in: false,
            account_name: None,
            account_avatar: None,
            search_input: String::new(),
            active_query: String::new(),
            search_suggestions: Vec::new(),
            last_suggest_query: String::new(),
            last_suggest_edit: None,
            search_popup_open: false,
            guide_channels: Vec::new(),
            guide_playlists: Vec::new(),
            subs_sidebar_expanded: false,
            playlists_sidebar_expanded: false,
            active_playlist_id: None,
            active_playlist_title: String::new(),
            playlist_items: Vec::new(),
            playlist_continuation: None,
            playlist_loading: false,
            home_items: Vec::new(),
            home_chips: Vec::new(),
            home_is_fallback: false,
            home_continuation: None,
            home_loading: true,
            selected_topic_chip: 0,
            chips_scroll_x: 0.0,
            chips_dragging: false,
            chips_drag_dist: 0.0,
            subs_items: Vec::new(),
            subs_continuation: None,
            subs_loading: false,
            trending_items: Vec::new(),
            trending_loading: false,
            channel_id: None,
            channel_name: String::new(),
            channel_info: None,
            channel_tab_params: None,
            channel_items: Vec::new(),
            channel_continuation: None,
            channel_loading: false,
            search_items: Vec::new(),
            search_continuation: None,
            search_loading: false,
            history,
            remote_history: Vec::new(),
            remote_history_continuation: None,
            remote_history_loading: false,
            saved,
            queue: Vec::new(),
            resume_positions,
            pending_resume: None,
            watch_loading: false,
            watch: None,
            selected_format_idx: None,
            selected_audio_idx: 0,
            selected_caption_idx: None,
            used_ytdlp: false,
            sponsor_segments: Vec::new(),
            last_skipped_end: -1.0,
            comments: Vec::new(),
            comments_next_token: None,
            comments_loading: false,
            show_chapters: false,
            live_chat: Vec::new(),
            live_chat_token: None,
            live_chat_next_poll: None,
            right_tab: RightTab::UpNext,
            playback: PlaybackState::default(),
            seeking_pos: None,
            volume,
            muted: false,
            speed: 1.0,
            preferred_quality,
            sponsorblock_enabled,
            sync_history,
            autoplay_next,
            theater_mode: false,
            fullscreen: false,
            show_description: false,
            status_toast: None,
            last_error: None,
            playlist_modal_video: None,
            playlist_options: Vec::new(),
            playlist_options_loading: false,
            playlist_filter: String::new(),
            new_playlist_title: String::new(),
        }
    }

    fn toast(&mut self, msg: impl Into<String>) {
        self.status_toast = Some((msg.into(), Instant::now()));
    }

    fn start_sign_in(&mut self) {
        if self.signing_in {
            return;
        }
        self.signing_in = true;
        let _ = self.backend.tx.send(Request::SignIn);
    }

    fn open_channel(&mut self, channel_id: String, name: String) {
        self.page = Page::Channel;
        self.channel_id = Some(channel_id.clone());
        self.channel_name = name;
        self.channel_info = None;
        self.channel_tab_params = None;
        self.channel_items.clear();
        self.channel_continuation = None;
        self.channel_loading = true;
        self.last_error = None;
        let _ = self.backend.tx.send(Request::LoadChannel {
            channel_id,
            params: None,
            continuation: None,
        });
    }

    fn open_playlist(&mut self, browse_id: String, title: String) {
        self.page = Page::Playlist;
        self.active_playlist_id = Some(browse_id.clone());
        self.active_playlist_title = title;
        self.playlist_items.clear();
        self.playlist_continuation = None;
        self.playlist_loading = true;
        self.last_error = None;
        let _ = self.backend.tx.send(Request::LoadPlaylist {
            browse_id,
            continuation: None,
        });
    }

    fn open_video(&mut self, item: VideoItem) {
        if let Some(s) = item.resume_secs.filter(|&s| s > 2.0) {
            self.resume_positions.insert(item.id.clone(), s);
        }
        self.pending_resume = Some((
            item.id.clone(),
            item.resume_secs.unwrap_or(0.0),
            item.watched_percent,
        ));
        self.record_history(item.clone());
        self.open_video_id(&item.id);
    }

    fn open_video_id(&mut self, video_id: &str) {
        if self.page != Page::Watch {
            self.prev_page = self.page;
        }
        self.page = Page::Watch;
        self.watch_loading = true;
        self.last_error = None;
        self.sponsor_segments.clear();
        self.last_skipped_end = -1.0;
        self.seeking_pos = None;
        self.selected_audio_idx = 0;
        self.selected_caption_idx = None;
        self.comments.clear();
        self.comments_next_token = None;
        self.comments_loading = false;
        self.show_chapters = false;
        self.live_chat.clear();
        self.live_chat_token = None;
        self.live_chat_next_poll = None;
        self.right_tab = RightTab::UpNext;
        let _ = self.backend.tx.send(Request::Watch {
            video_id: video_id.to_owned(),
            quality: self.preferred_quality,
            sync_history: self.sync_history,
        });
    }

    fn record_history(&mut self, item: VideoItem) {
        self.history.retain(|x| x.id != item.id);
        self.history.insert(0, item);
        self.history.truncate(100);
    }

    fn submit_search(&mut self) {
        let q = self.search_input.trim().to_owned();
        if q.is_empty() {
            return;
        }
        if let Some(vid) = innertube::extract_video_id(&q) {
            self.open_video_id(&vid);
            return;
        }
        self.active_query = q.clone();
        self.page = Page::Search;
        self.search_items.clear();
        self.search_continuation = None;
        self.search_loading = true;
        self.last_error = None;
        let _ = self.backend.tx.send(Request::Search {
            query: q,
            continuation: None,
        });
    }

    fn switch_quality(&mut self, idx: usize) {
        let Some(w) = &self.watch else { return };
        let Some(fmt) = w.video_formats.get(idx) else { return };
        let video_url = self.backend.proxy_url(&fmt.url, fmt.content_length);
        let audio = if fmt.has_audio {
            None
        } else {
            self.selected_audio_stream(w)
        };
        let pos = self.playback.time_pos;
        if let Some(mpv) = &self.mpv {
            mpv.load_stream(&video_url, audio.as_deref(), (pos > 1.0).then_some(pos));
        }
        self.selected_format_idx = Some(idx);
        self.toast(format!("Quality: {}", fmt.label()));
    }

    fn selected_audio_stream(&self, w: &WatchData) -> Option<String> {
        if let Some(tr) = w.audio_tracks.get(self.selected_audio_idx) {
            return Some(self.backend.proxy_url(&tr.url, tr.content_length));
        }
        w.audio_url
            .as_deref()
            .map(|a| self.backend.proxy_url(a, w.audio_content_length))
    }

    fn switch_audio_track(&mut self, idx: usize) {
        let Some(w) = &self.watch else { return };
        let Some(tr) = w.audio_tracks.get(idx) else { return };
        let label = tr.label.clone();
        self.selected_audio_idx = idx;
        if let Some(fmt_idx) = self.selected_format_idx {
            if let Some(fmt) = w.video_formats.get(fmt_idx) {
                let video_url = self.backend.proxy_url(&fmt.url, fmt.content_length);
                let audio = self.backend.proxy_url(&tr.url, tr.content_length);
                let pos = self.playback.time_pos;
                if let Some(mpv) = &self.mpv {
                    mpv.load_stream(&video_url, Some(&audio), (pos > 1.0).then_some(pos));
                }
            }
        }
        self.toast(format!("Audio: {label}"));
    }

    fn ingest_resume_metadata(&mut self, items: &mut [VideoItem]) {
        for it in items {
            if let Some(s) = it.resume_secs.filter(|&s| s > 2.0) {
                self.resume_positions.insert(it.id.clone(), s);
            } else if it.watched_percent.is_none() {
                if let Some(&s) = self.resume_positions.get(&it.id) {
                    if let Some(dur) = parse_duration_text(&it.duration_text) {
                        if dur > 10.0 && s > 2.0 {
                            it.resume_secs = Some(s);
                            it.watched_percent = Some(((s / dur) as f32).clamp(0.02, 1.0));
                        }
                    }
                }
            }
        }
    }

    fn open_playlist_picker(&mut self, video: VideoItem) {
        self.playlist_filter.clear();
        self.new_playlist_title.clear();
        self.playlist_options = self
            .guide_playlists
            .iter()
            .map(|p| PlaylistOption {
                id: p.browse_id.strip_prefix("VL").unwrap_or(&p.browse_id).to_owned(),
                title: p.title.clone(),
                is_added: false,
                thumbnail_url: p.thumbnail_url.clone(),
            })
            .collect();
        self.playlist_options_loading = self.signed_in;
        if self.signed_in {
            let _ = self.backend.tx.send(Request::LoadPlaylistOptions {
                video_id: video.id.clone(),
            });
        }
        self.playlist_modal_video = Some(video);
    }

    fn handle_grid_action(
        &mut self,
        ctx: &egui::Context,
        action: GridAction,
        load_more: impl FnOnce(&mut Self),
    ) {
        match action {
            GridAction::Open(it) => {
                if let Some(bid) = it.playlist_browse_id.clone() {
                    self.open_playlist(bid, it.title);
                } else {
                    self.open_video(it);
                }
            }
            GridAction::OpenChannel { channel_id, name } => self.open_channel(channel_id, name),
            GridAction::AddQueue(it) => {
                if !self.queue.iter().any(|x| x.id == it.id) {
                    let title = it.title.clone();
                    self.queue.push(it);
                    self.toast(format!("Queued: {title}"));
                } else {
                    self.toast("Already in Queue");
                }
            }
            GridAction::ToggleWatchLater(it) => {
                let is_saved = self.saved.iter().any(|x| x.id == it.id);
                if self.signed_in {
                    let _ = self.backend.tx.send(Request::SetWatchLater {
                        video_id: it.id.clone(),
                        add: !is_saved,
                    });
                }
                if is_saved {
                    self.saved.retain(|x| x.id != it.id);
                    self.toast("Removed from Watch Later");
                } else {
                    self.saved.insert(0, it);
                    self.toast("Saved to Watch Later");
                }
            }
            GridAction::OpenPlaylistPicker(it) => {
                self.open_playlist_picker(it);
            }
            GridAction::Download { item, audio_only } => {
                let kind = if audio_only { "audio" } else { "video" };
                self.toast(format!("Downloading {kind}: {}", item.title));
                let _ = self.backend.tx.send(Request::DownloadVideo {
                    video_id: item.id,
                    title: item.title,
                    audio_only,
                    max_height: self.preferred_quality.max_height(),
                });
            }
            GridAction::CopyUrl(id) => {
                ctx.copy_text(format!("https://www.youtube.com/watch?v={id}"));
                self.toast("Copied YouTube URL");
            }
            GridAction::LoadMore => load_more(self),
        }
    }

    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
    }

    fn drain_backend(&mut self, ctx: &egui::Context) {
        while let Ok(resp) = self.backend.rx.try_recv() {
            match resp {
                Response::HomeLoaded {
                    mut items,
                    chips,
                    continuation,
                    append,
                    is_fallback_trending,
                } => {
                    self.home_loading = false;
                    self.home_is_fallback = is_fallback_trending;
                    self.ingest_resume_metadata(&mut items);
                    if !chips.is_empty() {
                        self.home_chips = chips;
                    }
                    if append {
                        for it in items {
                            if !self.home_items.iter().any(|x| x.id == it.id) {
                                self.home_items.push(it);
                            }
                        }
                    } else {
                        self.home_items = items;
                    }
                    self.home_continuation = continuation;
                }
                Response::SubscriptionsLoaded {
                    mut items,
                    continuation,
                    append,
                } => {
                    self.subs_loading = false;
                    self.ingest_resume_metadata(&mut items);
                    if append {
                        for it in items {
                            if !self.subs_items.iter().any(|x| x.id == it.id) {
                                self.subs_items.push(it);
                            }
                        }
                    } else {
                        self.subs_items = items;
                    }
                    self.subs_continuation = continuation;
                }
                Response::HistoryLoaded {
                    mut items,
                    continuation,
                    append,
                } => {
                    self.remote_history_loading = false;
                    self.ingest_resume_metadata(&mut items);
                    if append {
                        for it in items {
                            if !self.remote_history.iter().any(|x| x.id == it.id) {
                                self.remote_history.push(it);
                            }
                        }
                    } else {
                        self.remote_history = items;
                    }
                    self.remote_history_continuation = continuation;
                }
                Response::GuideLoaded {
                    channels,
                    playlists,
                } => {
                    self.guide_channels = channels;
                    self.guide_playlists = playlists;
                }
                Response::PlaylistLoaded {
                    browse_id,
                    title,
                    mut items,
                    continuation,
                    append,
                } => {
                    if self.active_playlist_id.as_deref() == Some(browse_id.as_str()) {
                        self.playlist_loading = false;
                        self.ingest_resume_metadata(&mut items);
                        if !title.is_empty() {
                            self.active_playlist_title = title;
                        }
                        if append {
                            for it in items {
                                if !self.playlist_items.iter().any(|x| x.id == it.id) {
                                    self.playlist_items.push(it);
                                }
                            }
                        } else {
                            self.playlist_items = items;
                        }
                        self.playlist_continuation = continuation;
                    }
                }
                Response::TrendingLoaded { mut items } => {
                    self.trending_loading = false;
                    self.ingest_resume_metadata(&mut items);
                    self.trending_items = items;
                }
                Response::ChannelLoaded {
                    channel_id,
                    info,
                    mut items,
                    continuation,
                    append,
                } => {
                    if self.channel_id.as_deref() == Some(channel_id.as_str()) {
                        self.channel_loading = false;
                        self.ingest_resume_metadata(&mut items);
                        if !info.name.is_empty() && info.name != "Channel" {
                            self.channel_name = info.name.clone();
                        }
                        if !append || self.channel_info.is_none() {
                            self.channel_info = Some(info);
                        }
                        if append {
                            for it in items {
                                if !self.channel_items.iter().any(|x| x.id == it.id) {
                                    self.channel_items.push(it);
                                }
                            }
                        } else {
                            self.channel_items = items;
                        }
                        self.channel_continuation = continuation;
                    }
                }
                Response::SearchLoaded {
                    query,
                    mut items,
                    continuation,
                    append,
                } => {
                    if query == self.active_query {
                        self.search_loading = false;
                        self.ingest_resume_metadata(&mut items);
                        if append {
                            for it in items {
                                if !self.search_items.iter().any(|x| x.id == it.id) {
                                    self.search_items.push(it);
                                }
                            }
                        } else {
                            self.search_items = items;
                        }
                        self.search_continuation = continuation;
                    }
                }
                Response::SearchSuggestions { query, suggestions } => {
                    if query == self.search_input.trim() {
                        self.search_suggestions = suggestions;
                    }
                }
                Response::WatchReady {
                    data,
                    selected_video_url,
                    selected_audio_url,
                    selected_format_idx,
                    used_ytdlp,
                } => {
                    self.watch_loading = false;
                    self.selected_format_idx = selected_format_idx;
                    self.selected_audio_idx = 0;
                    self.selected_caption_idx = None;
                    self.used_ytdlp = used_ytdlp;

                    let start_at = self
                        .pending_resume
                        .take()
                        .filter(|(id, _, _)| id == &data.id)
                        .and_then(|(_, secs, pct)| {
                            if secs > 2.0 {
                                Some(secs)
                            } else if let Some(p) =
                                pct.filter(|&p| p > 0.02 && p < 0.94 && data.duration_secs > 10.0)
                            {
                                Some(f64::from(p) * data.duration_secs)
                            } else {
                                None
                            }
                        })
                        .or(data.resume_secs)
                        .or_else(|| {
                            self.resume_positions.get(&data.id).copied().filter(|&s| {
                                s > 2.0 && (data.duration_secs <= 0.0 || s < data.duration_secs * 0.94)
                            })
                        });

                    if let Some(mpv) = &self.mpv {
                        mpv.load_stream(
                            &selected_video_url,
                            selected_audio_url.as_deref(),
                            start_at,
                        );
                        mpv.set_speed(self.speed);
                    }
                    if let Some(s) = start_at {
                        self.toast(format!("Resumed at {}", format_duration(s)));
                    }
                    let hist_item = VideoItem {
                        id: data.id.clone(),
                        title: data.title.clone(),
                        channel: data.channel.clone(),
                        channel_id: data.channel_id.clone(),
                        channel_avatar_url: data.channel_avatar_url.clone(),
                        views_text: data.views_text.clone(),
                        published_text: data.published_text.clone(),
                        duration_text: format_duration(data.duration_secs),
                        thumbnail_url: format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", data.id),
                        is_live: data.is_live,
                        watched_percent: start_at.and_then(|s| {
                            (data.duration_secs > 10.0)
                                .then_some((s / data.duration_secs) as f32)
                        }),
                        resume_secs: start_at,
                        playlist_browse_id: None,
                    };
                    self.record_history(hist_item);
                    if let Some(tok) = data.comments_token.clone() {
                        self.comments_loading = true;
                        let _ = self.backend.tx.send(Request::LoadComments {
                            video_id: data.id.clone(),
                            continuation: tok,
                            append: false,
                        });
                    }
                    if let Some(chat_tok) = data.live_chat_token.clone() {
                        self.live_chat_token = Some(chat_tok.clone());
                        self.live_chat_next_poll = Some(Instant::now() + Duration::from_secs(3));
                        self.right_tab = RightTab::LiveChat;
                        let _ = self.backend.tx.send(Request::LoadLiveChat {
                            video_id: data.id.clone(),
                            continuation: chat_tok,
                        });
                    }
                    self.watch = Some(*data);
                }
                Response::CommentsLoaded {
                    video_id,
                    comments,
                    next_token,
                    append,
                } => {
                    if self.watch.as_ref().is_some_and(|w| w.id == video_id) {
                        self.comments_loading = false;
                        if append {
                            self.comments.extend(comments);
                        } else {
                            self.comments = comments;
                        }
                        self.comments_next_token = next_token;
                    }
                }
                Response::LiveChatLoaded {
                    video_id,
                    messages,
                    next_token,
                } => {
                    if self.watch.as_ref().is_some_and(|w| w.id == video_id) {
                        for m in messages {
                            if !self.live_chat.iter().any(|x| x.id == m.id) {
                                self.live_chat.push(m);
                            }
                        }
                        if self.live_chat.len() > 250 {
                            let drop_n = self.live_chat.len() - 250;
                            self.live_chat.drain(..drop_n);
                        }
                        self.live_chat_token = next_token;
                        self.live_chat_next_poll = Some(Instant::now() + Duration::from_secs(3));
                    }
                }
                Response::LikeUpdated { video_id, liked } => {
                    if let Some(w) = &mut self.watch {
                        if w.id == video_id {
                            w.is_liked = liked;
                            if liked {
                                w.is_disliked = false;
                            }
                        }
                    }
                    self.toast(if liked { "Added to Liked videos" } else { "Removed like" });
                }
                Response::DislikeUpdated { video_id, disliked } => {
                    if let Some(w) = &mut self.watch {
                        if w.id == video_id {
                            w.is_disliked = disliked;
                            if disliked {
                                w.is_liked = false;
                            }
                        }
                    }
                    self.toast(if disliked { "Disliked video" } else { "Removed dislike" });
                }
                Response::WatchLaterUpdated { .. } => {}
                Response::PlaylistEdited {
                    playlist_title,
                    added,
                } => {
                    self.toast(if added {
                        format!("Saved to {playlist_title}")
                    } else {
                        format!("Removed from {playlist_title}")
                    });
                }
                Response::PlaylistOptionsLoaded {
                    video_id,
                    mut options,
                } => {
                    if self
                        .playlist_modal_video
                        .as_ref()
                        .is_some_and(|v| v.id == video_id)
                    {
                        self.playlist_options_loading = false;
                        for opt in &mut options {
                            if opt.thumbnail_url.is_none() {
                                opt.thumbnail_url = self
                                    .guide_playlists
                                    .iter()
                                    .find(|g| g.browse_id.strip_prefix("VL") == Some(opt.id.as_str()))
                                    .and_then(|g| g.thumbnail_url.clone());
                            }
                        }
                        if !options.is_empty() {
                            self.playlist_options = options;
                        }
                    }
                }
                Response::PlaylistCreated { title } => {
                    self.toast(format!("Created playlist \"{title}\""));
                }
                Response::SubscribeUpdated {
                    channel_id,
                    subscribed,
                } => {
                    if let Some(w) = &mut self.watch {
                        if w.channel_id.as_deref() == Some(channel_id.as_str()) {
                            w.is_subscribed = subscribed;
                        }
                    }
                    if let Some(ci) = &mut self.channel_info {
                        if ci.id == channel_id {
                            ci.is_subscribed = subscribed;
                        }
                    }
                    self.toast(if subscribed { "Subscribed" } else { "Unsubscribed" });
                }
                Response::VideoDownloaded { title, result, .. } => match result {
                    Ok(path) => {
                        let fname = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file");
                        self.toast(format!("Downloaded: {fname}"));
                    }
                    Err(e) => {
                        self.last_error = Some(format!("Download \"{title}\" failed: {e}"));
                    }
                },
                Response::SponsorSegments { video_id, segments } => {
                    if self.watch.as_ref().is_some_and(|w| w.id == video_id) {
                        self.sponsor_segments = segments;
                    }
                }
                Response::SignedIn { name, avatar } => {
                    self.signed_in = true;
                    self.signing_in = false;
                    self.account_name = name;
                    self.account_avatar = avatar;
                    self.home_is_fallback = false;
                    self.toast("Signed in to YouTube");
                }
                Response::SignedOut => {
                    self.signed_in = false;
                    self.signing_in = false;
                    self.account_name = None;
                    self.account_avatar = None;
                    self.subs_items.clear();
                    self.guide_channels.clear();
                    self.guide_playlists.clear();
                    self.playlist_items.clear();
                    self.active_playlist_id = None;
                    self.remote_history.clear();
                    self.remote_history_continuation = None;
                    self.home_chips.clear();
                    self.home_loading = true;
                    let _ = self.backend.tx.send(Request::LoadHome {
                        params: None,
                        continuation: None,
                    });
                    self.toast("Signed out");
                }
                Response::LibmpvDownloaded(res) => {
                    self.downloading_mpv = false;
                    match res {
                        Ok(path) => {
                            self.mpv_error = None;
                            if let Some(gpa) = self.get_proc_address.clone() {
                                match MpvPlayer::new(&self.backend.data_dir, gpa, ctx.clone()) {
                                    Ok(player) => {
                                        player.set_volume(self.volume);
                                        self.mpv = Some(player);
                                        self.toast(format!("libmpv ready ({})", path.display()));
                                    }
                                    Err(e) => self.mpv_error = Some(format!("{e:#}")),
                                }
                            }
                        }
                        Err(e) => self.mpv_error = Some(format!("Download failed: {e}")),
                    }
                }
                Response::Error(msg) => {
                    self.home_loading = false;
                    self.subs_loading = false;
                    self.playlist_loading = false;
                    self.remote_history_loading = false;
                    self.trending_loading = false;
                    self.channel_loading = false;
                    self.search_loading = false;
                    self.watch_loading = false;
                    self.comments_loading = false;
                    self.signing_in = false;
                    self.last_error = Some(msg);
                }
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        ctx.input(|i| {
            if i.key_pressed(Key::Escape) && self.fullscreen {
                self.fullscreen = false;
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
            }
            if self.page != Page::Watch {
                return;
            }
            if i.key_pressed(Key::Space) || i.key_pressed(Key::K) {
                if let Some(m) = &self.mpv {
                    m.toggle_pause();
                }
            }
            if i.key_pressed(Key::ArrowLeft) {
                if let Some(m) = &self.mpv {
                    m.seek_rel(-5.0);
                }
            }
            if i.key_pressed(Key::ArrowRight) {
                if let Some(m) = &self.mpv {
                    m.seek_rel(5.0);
                }
            }
            if i.key_pressed(Key::J) {
                if let Some(m) = &self.mpv {
                    m.seek_rel(-10.0);
                }
            }
            if i.key_pressed(Key::L) {
                if let Some(m) = &self.mpv {
                    m.seek_rel(10.0);
                }
            }
            if i.key_pressed(Key::M) {
                self.muted = !self.muted;
                if let Some(m) = &self.mpv {
                    m.set_mute(self.muted);
                }
            }
            if i.key_pressed(Key::T) && !self.fullscreen {
                self.theater_mode = !self.theater_mode;
            }
            if i.key_pressed(Key::F) {
                self.fullscreen = !self.fullscreen;
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
            }
            if i.key_pressed(Key::ArrowUp) {
                self.volume = (self.volume + 5.0).min(100.0);
                if let Some(m) = &self.mpv {
                    m.set_volume(self.volume);
                }
            }
            if i.key_pressed(Key::ArrowDown) {
                self.volume = (self.volume - 5.0).max(0.0);
                if let Some(m) = &self.mpv {
                    m.set_volume(self.volume);
                }
            }
        });
    }

    fn poll_playback_and_sponsorblock(&mut self, ctx: &egui::Context) {
        let Some(mpv) = &self.mpv else { return };
        if self.watch.is_none() {
            return;
        }
        let was_eof = self.playback.eof;
        self.playback = mpv.poll_state();
        if !self.watch_loading {
            self.backend
                .cursor
                .update(self.playback.time_pos, self.playback.paused);
            if self.playback.duration > 15.0 && self.playback.time_pos > 2.0 {
                if let Some(w) = self.watch.as_ref().map(|w| w.id.clone()) {
                    let pos = self.playback.time_pos;
                    let pct = (pos / self.playback.duration).clamp(0.01, 1.0) as f32;
                    let finished = pos >= self.playback.duration * 0.94;
                    if finished {
                        self.resume_positions.remove(&w);
                    } else {
                        self.resume_positions.insert(w.clone(), pos);
                    }
                    let update_list = |list: &mut [VideoItem]| {
                        for it in list.iter_mut().filter(|x| x.id == w) {
                            it.resume_secs = if finished { None } else { Some(pos) };
                            it.watched_percent = Some(if finished { 1.0 } else { pct });
                        }
                    };
                    update_list(&mut self.history);
                    update_list(&mut self.remote_history);
                    update_list(&mut self.home_items);
                    update_list(&mut self.subs_items);
                    update_list(&mut self.playlist_items);
                    update_list(&mut self.channel_items);
                    update_list(&mut self.search_items);
                    update_list(&mut self.saved);
                }
            }
        }

        if !self.playback.paused && self.page == Page::Watch {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

        // Poll Live Chat every ~3s when on Watch page for a livestream
        if self.page == Page::Watch && self.right_tab == RightTab::LiveChat {
            if let (Some(w), Some(tok), Some(due)) = (
                &self.watch,
                &self.live_chat_token,
                self.live_chat_next_poll,
            ) {
                if Instant::now() >= due {
                    self.live_chat_next_poll = Some(Instant::now() + Duration::from_secs(4));
                    let _ = self.backend.tx.send(Request::LoadLiveChat {
                        video_id: w.id.clone(),
                        continuation: tok.clone(),
                    });
                } else {
                    ctx.request_repaint_after(Duration::from_secs(1));
                }
            }
        }

        if self.sponsorblock_enabled && !self.playback.paused {
            let pos = self.playback.time_pos;
            let dur = self.playback.duration;
            if let Some(seg) = self
                .sponsor_segments
                .iter()
                .find(|s| s.fits(dur) && pos >= s.start && pos < s.end - 0.5)
            {
                if (seg.end - self.last_skipped_end).abs() > 0.5 {
                    self.last_skipped_end = seg.end;
                    mpv.seek_abs(seg.end);
                    let label = match seg.category {
                        SponsorCategory::Sponsor => "sponsor",
                        SponsorCategory::SelfPromo => "self-promo",
                        SponsorCategory::Interaction => "reminder",
                        SponsorCategory::Intro => "intro",
                        SponsorCategory::Outro => "outro",
                        SponsorCategory::Other => "segment",
                    };
                    self.toast(format!("Skipped {label}"));
                }
            }
        }

        if self.autoplay_next && self.playback.eof && !was_eof && self.playback.duration > 1.0 {
            if !self.queue.is_empty() {
                let next = self.queue.remove(0);
                self.open_video(next);
            } else if let Some(next) = self.watch.as_ref().and_then(|w| w.related.first()).cloned()
            {
                self.open_video(next);
            }
        }
    }
}

impl eframe::App for YtApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("volume", format!("{:.1}", self.volume));
        storage.set_string("sponsorblock", if self.sponsorblock_enabled { "1" } else { "0" }.into());
        storage.set_string("sync_history", if self.sync_history { "1" } else { "0" }.into());
        storage.set_string("autoplay", if self.autoplay_next { "1" } else { "0" }.into());
        storage.set_string("sidebar", if self.sidebar_expanded { "1" } else { "0" }.into());
        let q = match self.preferred_quality {
            PreferredQuality::Best4K => "4k",
            PreferredQuality::P1440 => "1440",
            PreferredQuality::P1080 => "1080",
            PreferredQuality::P720 => "720",
            PreferredQuality::P480 => "480",
        };
        storage.set_string("quality", q.into());
        if let Ok(h) = serde_json::to_string(&self.history) {
            storage.set_string("history", h);
        }
        if let Ok(s) = serde_json::to_string(&self.saved) {
            storage.set_string("saved", s);
        }
        if let Ok(r) = serde_json::to_string(&self.resume_positions) {
            storage.set_string("resume_positions", r);
        }
    }

    fn on_exit(&mut self, gl: Option<&glow::Context>) {
        self.backend.cursor.stop();
        if let Some(mpv) = &self.mpv {
            mpv.destroy_gl(gl);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain_backend(&ctx);
        self.handle_shortcuts(&ctx);
        self.poll_playback_and_sponsorblock(&ctx);

        if let Some((_, at)) = &self.status_toast {
            if at.elapsed() > Duration::from_secs(3) {
                self.status_toast = None;
            }
        }

        if !self.fullscreen {
            self.draw_top_bar(ui);
            self.draw_sidebar(ui);
        }

        egui::CentralPanel::default()
            .frame(
                Frame::central_panel(ui.style())
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(20, 14)),
            )
            .show(ui, |ui| {
                if let Some(err) = self.last_error.clone() {
                    Frame::new()
                        .fill(Color32::from_rgb(56, 20, 22))
                        .corner_radius(10)
                        .inner_margin(Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.colored_label(Color32::from_rgb(255, 165, 165), &err);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.small_button("Dismiss").clicked() {
                                        self.last_error = None;
                                    }
                                });
                            });
                        });
                    ui.add_space(10.0);
                }

                match self.page {
                    Page::Home => self.draw_home_page(ui),
                    Page::Subscriptions => self.draw_subscriptions_page(ui),
                    Page::Trending => {
                        let items = self.trending_items.clone();
                        let loading = self.trending_loading;
                        if let Some(act) =
                            draw_video_grid(ui, "Trending", &items, loading, false)
                        {
                            self.handle_grid_action(&ctx, act, |_| {});
                        }
                    }
                    Page::Playlist => {
                        let heading = self.active_playlist_title.clone();
                        let items = self.playlist_items.clone();
                        let loading = self.playlist_loading;
                        let cont = self.playlist_continuation.clone();
                        if let Some(act) =
                            draw_video_grid(ui, &heading, &items, loading, cont.is_some())
                        {
                            self.handle_grid_action(&ctx, act, |app| {
                                if let Some(bid) = app.active_playlist_id.clone() {
                                    app.playlist_loading = true;
                                    let _ = app.backend.tx.send(Request::LoadPlaylist {
                                        browse_id: bid,
                                        continuation: cont,
                                    });
                                }
                            });
                        }
                    }
                    Page::Channel => self.draw_channel_page(ui, &ctx),
                    Page::Search => {
                        let heading = format!("Results for \"{}\"", self.active_query);
                        let items = self.search_items.clone();
                        let loading = self.search_loading;
                        let cont = self.search_continuation.clone();
                        if let Some(act) =
                            draw_video_grid(ui, &heading, &items, loading, cont.is_some())
                        {
                            self.handle_grid_action(&ctx, act, |app| {
                                app.search_loading = true;
                                let _ = app.backend.tx.send(Request::Search {
                                    query: app.active_query.clone(),
                                    continuation: cont,
                                });
                            });
                        }
                    }
                    Page::History => {
                        if self.signed_in && self.sync_history {
                            let items = self.remote_history.clone();
                            let loading = self.remote_history_loading;
                            let cont = self.remote_history_continuation.clone();
                            if let Some(act) = draw_video_grid(
                                ui,
                                "Watch History (Synced with YouTube)",
                                &items,
                                loading,
                                cont.is_some(),
                            ) {
                                self.handle_grid_action(&ctx, act, |app| {
                                    app.remote_history_loading = true;
                                    let _ = app.backend.tx.send(Request::LoadHistory {
                                        continuation: cont,
                                    });
                                });
                            }
                        } else {
                            let items = self.history.clone();
                            if let Some(act) =
                                draw_video_grid(ui, "Watch History (Local)", &items, false, false)
                            {
                                self.handle_grid_action(&ctx, act, |_| {});
                            }
                        }
                    }
                    Page::Saved => {
                        let items = self.saved.clone();
                        if let Some(act) =
                            draw_video_grid(ui, "Watch Later", &items, false, false)
                        {
                            self.handle_grid_action(&ctx, act, |_| {});
                        }
                    }
                    Page::Settings => self.draw_settings_page(ui),
                    Page::Watch => self.draw_watch_page(ui, &ctx),
                }
            });

        if !self.fullscreen && self.page != Page::Watch && self.watch.is_some() {
            self.draw_mini_player(&ctx);
        }
        self.draw_playlist_picker_modal(&ctx);
    }
}

impl YtApp {
    fn draw_top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_bar")
            .default_size(58.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    // Hamburger menu button to collapse/expand sidebar
                    let (menu_rect, menu_resp) =
                        ui.allocate_exact_size(Vec2::splat(36.0), Sense::click());
                    if menu_resp.hovered() {
                        ui.painter()
                            .circle_filled(menu_rect.center(), 17.0, theme::CHIP);
                    }
                    for dy in [-5.0f32, 0.0, 5.0] {
                        let y = menu_rect.center().y + dy;
                        ui.painter().line_segment(
                            [
                                pos2(menu_rect.center().x - 8.5, y),
                                pos2(menu_rect.center().x + 8.5, y),
                            ],
                            Stroke::new(1.6, theme::TEXT),
                        );
                    }
                    if menu_resp.clicked() {
                        self.sidebar_expanded = !self.sidebar_expanded;
                    }

                    ui.add_space(6.0);

                    if self.page == Page::Watch {
                        if icon_circle_button(ui, icon::CHEVRON_LEFT, 18.0, "Back") {
                            self.page = self.prev_page;
                        }
                        ui.add_space(4.0);
                    }

                    // YouTube brand badge
                    let (logo_rect, logo_resp) =
                        ui.allocate_exact_size(Vec2::new(126.0, 28.0), Sense::click());
                    let badge = Rect::from_min_size(
                        pos2(logo_rect.left(), logo_rect.center().y - 10.0),
                        Vec2::new(28.0, 20.0),
                    );
                    theme::draw_youtube_logo(ui.painter(), badge);
                    ui.painter().text(
                        pos2(badge.right() + 7.0, logo_rect.center().y),
                        Align2::LEFT_CENTER,
                        "YouTube",
                        theme::bold(17.5),
                        Color32::WHITE,
                    );
                    ui.painter().text(
                        pos2(badge.right() + 85.0, logo_rect.center().y - 5.0),
                        Align2::LEFT_CENTER,
                        "rs",
                        theme::semibold(10.0),
                        theme::TEXT_DIM,
                    );
                    if logo_resp.clicked() {
                        self.page = Page::Home;
                    }

                    // Centered pill Search bar
                    let total_avail = ui.available_width();
                    let right_reserve = if self.status_toast.is_some() {
                        310.0
                    } else {
                        185.0
                    };
                    let search_w = (total_avail - right_reserve).clamp(180.0, 560.0);
                    let left_pad = ((total_avail - right_reserve - search_w) * 0.5).max(8.0);
                    ui.add_space(left_pad);

                    let pill_h = 36.0;
                    let (pill_rect, _) =
                        ui.allocate_exact_size(Vec2::new(search_w, pill_h), Sense::hover());
                    ui.painter().rect_filled(
                        pill_rect,
                        CornerRadius::same(18),
                        Color32::from_rgb(18, 18, 18),
                    );
                    ui.painter().rect_stroke(
                        pill_rect,
                        CornerRadius::same(18),
                        Stroke::new(1.0, theme::BORDER),
                        egui::StrokeKind::Inside,
                    );

                    // Search button cap on right of pill
                    let btn_w = 54.0;
                    let btn_rect = Rect::from_min_max(
                        pos2(pill_rect.right() - btn_w, pill_rect.top()),
                        pill_rect.max,
                    );
                    let btn_resp = ui.interact(btn_rect, ui.scope_id().with("search_btn"), Sense::click());
                    let btn_fill = if btn_resp.hovered() {
                        theme::CHIP_HOVER
                    } else {
                        Color32::from_rgb(34, 34, 34)
                    };
                    ui.painter().rect_filled(
                        btn_rect,
                        CornerRadius {
                            nw: 0,
                            sw: 0,
                            ne: 18,
                            se: 18,
                        },
                        btn_fill,
                    );
                    ui.painter().line_segment(
                        [btn_rect.left_top(), btn_rect.left_bottom()],
                        Stroke::new(1.0, theme::BORDER),
                    );
                    ui.painter().text(
                        btn_rect.center(),
                        Align2::CENTER_CENTER,
                        icon::SEARCH.to_string(),
                        theme::icon_font(16.0),
                        theme::TEXT,
                    );
                    if btn_resp.clicked() {
                        self.submit_search();
                    }

                    // Input vertically centered inside left portion of pill
                    let clear_w = if self.search_input.is_empty() { 0.0 } else { 28.0 };
                    let input_rect = Rect::from_min_max(
                        pos2(pill_rect.left() + 16.0, pill_rect.center().y - 10.0),
                        pos2(btn_rect.left() - clear_w - 4.0, pill_rect.center().y + 10.0),
                    );
                    let mut input_ui =
                        ui.new_child(egui::UiBuilder::new().max_rect(input_rect));
                    let edit_resp = input_ui.add_sized(
                        input_rect.size(),
                        egui::TextEdit::singleline(&mut self.search_input)
                            .frame(Frame::NONE)
                            .font(theme::regular(14.0))
                            .hint_text(
                                RichText::new("Search or paste a YouTube URL")
                                    .color(theme::TEXT_FAINT),
                            ),
                    );
                    if edit_resp.gained_focus() && !self.search_input.trim().is_empty() {
                        self.search_popup_open = true;
                    }
                    if edit_resp.changed() {
                        let q = self.search_input.trim().to_owned();
                        if q.is_empty() || innertube::extract_video_id(&q).is_some() {
                            self.search_suggestions.clear();
                            self.search_popup_open = false;
                            self.last_suggest_edit = None;
                        } else {
                            self.search_popup_open = true;
                            self.last_suggest_edit = Some(Instant::now());
                        }
                    }
                    if let Some(edited_at) = self.last_suggest_edit {
                        if edited_at.elapsed() >= Duration::from_millis(180) {
                            self.last_suggest_edit = None;
                            let q = self.search_input.trim().to_owned();
                            if !q.is_empty() && q != self.last_suggest_query {
                                self.last_suggest_query = q.clone();
                                let _ = self.backend.tx.send(Request::SearchSuggest { query: q });
                            }
                        } else {
                            ui.ctx().request_repaint_after(Duration::from_millis(60));
                        }
                    }
                    if edit_resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        self.search_popup_open = false;
                        self.submit_search();
                    }
                    if self.search_popup_open
                        && !self.search_suggestions.is_empty()
                        && !edit_resp.has_focus()
                        && ui.input(|i| i.key_pressed(Key::Escape))
                    {
                        self.search_popup_open = false;
                    }
                    if !self.search_input.is_empty() {
                        let clear_rect = Rect::from_min_max(
                            pos2(btn_rect.left() - clear_w, pill_rect.top()),
                            pos2(btn_rect.left(), pill_rect.bottom()),
                        );
                        let clr = ui.interact(clear_rect, ui.scope_id().with("clr_search"), Sense::click());
                        ui.painter().text(
                            clear_rect.center(),
                            Align2::CENTER_CENTER,
                            icon::X.to_string(),
                            theme::icon_font(14.0),
                            if clr.hovered() {
                                Color32::WHITE
                            } else {
                                theme::TEXT_DIM
                            },
                        );
                        if clr.clicked() {
                            self.search_input.clear();
                            self.search_suggestions.clear();
                            self.search_popup_open = false;
                        }
                    }

                    if self.search_popup_open && !self.search_suggestions.is_empty() {
                        let mut picked_suggestion = None;
                        let popup_pos = pos2(pill_rect.left() + 8.0, pill_rect.bottom() + 4.0);
                        let popup_w = (pill_rect.width() - 16.0).max(200.0);
                        egui::Area::new(egui::Id::unique("search_suggest_popup"))
                            .fixed_pos(popup_pos)
                            .order(egui::Order::Foreground)
                            .show(ui.ctx(), |ui| {
                                Frame::new()
                                    .fill(theme::SURFACE)
                                    .stroke(Stroke::new(1.0, theme::BORDER))
                                    .corner_radius(12)
                                    .shadow(egui::epaint::Shadow {
                                        offset: [0, 6],
                                        blur: 16,
                                        spread: 2,
                                        color: Color32::from_black_alpha(150),
                                    })
                                    .inner_margin(Margin::symmetric(6, 6))
                                    .show(ui, |ui| {
                                        ui.set_width(popup_w - 12.0);
                                        for s in &self.search_suggestions {
                                            let row = ui.add_sized(
                                                Vec2::new(popup_w - 16.0, 30.0),
                                                egui::Button::new(
                                                    RichText::new(format!("{}  {s}", icon::SEARCH))
                                                        .font(theme::regular(13.0))
                                                        .color(theme::TEXT),
                                                )
                                                .fill(Color32::TRANSPARENT),
                                            );
                                            if row.clicked() {
                                                picked_suggestion = Some(s.clone());
                                            }
                                        }
                                    });
                            });
                        if let Some(s) = picked_suggestion {
                            self.search_input = s;
                            self.search_popup_open = false;
                            self.submit_search();
                        }
                    }

                    // Right side: Toast, Account Sign-in / Avatar pill
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.signed_in {
                            let label = self
                                .account_name
                                .as_deref()
                                .unwrap_or("Account")
                                .to_owned();
                            let avatar = self.account_avatar.clone();
                            let acc_resp = ui.horizontal(|ui| {
                                draw_avatar(ui, avatar.as_deref(), &label, 24.0);
                                ui.menu_button(
                                    RichText::new(&label).font(theme::medium(13.0)),
                                    |ui| {
                                        if ui
                                            .button(format!("{}  Sign out", icon::LOG_OUT))
                                            .clicked()
                                        {
                                            ui.close();
                                            let _ = self.backend.tx.send(Request::SignOut);
                                        }
                                    },
                                )
                            });
                            let _ = acc_resp;
                        } else {
                            let btn_text = if self.signing_in {
                                "Signing in...".to_owned()
                            } else {
                                format!("{}  Sign in", icon::CIRCLE_USER)
                            };
                            let sign_btn = egui::Button::new(
                                RichText::new(btn_text)
                                    .font(theme::medium(13.0))
                                    .color(theme::YT_BLUE),
                            )
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, theme::BORDER))
                            .corner_radius(CornerRadius::same(18));
                            if ui.add_enabled(!self.signing_in, sign_btn).clicked() {
                                self.start_sign_in();
                            }
                        }

                        if let Some((msg, _)) = &self.status_toast {
                            theme::pill_frame(Color32::from_rgb(30, 52, 34)).show(ui, |ui| {
                                ui.label(
                                    RichText::new(msg)
                                        .font(theme::medium(12.0))
                                        .color(Color32::from_rgb(150, 235, 165)),
                                );
                            });
                        }
                    });
                });
            });
    }

    fn draw_sidebar(&mut self, ui: &mut egui::Ui) {
        let width = if self.sidebar_expanded { 210.0 } else { 72.0 };
        egui::Panel::left("sidebar")
            .resizable(false)
            .default_size(width)
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(10, 6)),
            )
            .show(ui, |ui| {
                ui.set_width(width - 20.0);

                ScrollArea::vertical()
                    .id_salt("sidebar_nav_scroll")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .max_height((ui.available_height() - 32.0).max(160.0))
                    .show(ui, |ui| {
                        let nav_items: &[(Page, char, &str)] = &[
                            (Page::Home, icon::HOUSE, "Home"),
                            (Page::Subscriptions, icon::RADIO, "Subscriptions"),
                            (Page::Trending, icon::COMPASS, "Trending"),
                            (Page::History, icon::HISTORY, "History"),
                        ];

                        for &(target, ico, label) in nav_items {
                            if sidebar_item(
                                ui,
                                ico,
                                label,
                                self.page == target,
                                self.sidebar_expanded,
                            ) {
                                self.page = target;
                                match target {
                                    Page::Home => {
                                        if self.home_items.is_empty() && !self.home_loading {
                                            self.home_loading = true;
                                            let _ = self.backend.tx.send(Request::LoadHome {
                                                params: None,
                                                continuation: None,
                                            });
                                        }
                                    }
                                    Page::Subscriptions => {
                                        if self.signed_in
                                            && self.subs_items.is_empty()
                                            && !self.subs_loading
                                        {
                                            self.subs_loading = true;
                                            let _ =
                                                self.backend.tx.send(Request::LoadSubscriptions {
                                                    continuation: None,
                                                });
                                        }
                                    }
                                    Page::Trending => {
                                        if self.trending_items.is_empty() && !self.trending_loading
                                        {
                                            self.trending_loading = true;
                                            let _ = self.backend.tx.send(Request::LoadTrending);
                                        }
                                    }
                                    Page::History => {
                                        if self.signed_in
                                            && self.sync_history
                                            && !self.remote_history_loading
                                        {
                                            self.remote_history_loading = true;
                                            let _ = self.backend.tx.send(Request::LoadHistory {
                                                continuation: None,
                                            });
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }

                        let wl_active = self.page == Page::Saved
                            || (self.page == Page::Playlist
                                && self.active_playlist_id.as_deref() == Some("VLWL"));
                        if sidebar_item(
                            ui,
                            icon::LIBRARY,
                            "Watch Later",
                            wl_active,
                            self.sidebar_expanded,
                        ) {
                            if self.signed_in {
                                self.open_playlist("VLWL".to_owned(), "Watch Later".to_owned());
                            } else {
                                self.page = Page::Saved;
                            }
                        }

                        if self.signed_in {
                            let liked_active = self.page == Page::Playlist
                                && self.active_playlist_id.as_deref() == Some("VLLL");
                            if sidebar_item(
                                ui,
                                icon::HEART,
                                "Liked Videos",
                                liked_active,
                                self.sidebar_expanded,
                            ) {
                                self.open_playlist("VLLL".to_owned(), "Liked Videos".to_owned());
                            }
                        }

                        if self.watch.is_some() {
                            ui.add_space(4.0);
                            if sidebar_item(
                                ui,
                                icon::PLAY,
                                "Now Playing",
                                self.page == Page::Watch,
                                self.sidebar_expanded,
                            ) {
                                self.page = Page::Watch;
                            }
                        }

                        if self.sidebar_expanded && self.signed_in && !self.guide_playlists.is_empty()
                        {
                            ui.add_space(6.0);
                            ui.separator();
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Playlists")
                                        .font(theme::semibold(12.5))
                                        .color(theme::TEXT_DIM),
                                );
                            });
                            ui.add_space(4.0);
                            let limit = if self.playlists_sidebar_expanded {
                                self.guide_playlists.len()
                            } else {
                                4.min(self.guide_playlists.len())
                            };
                            let pls: Vec<_> = self.guide_playlists[..limit].to_vec();
                            for pl in pls {
                                let active = self.page == Page::Playlist
                                    && self.active_playlist_id.as_deref()
                                        == Some(pl.browse_id.as_str());
                                if sidebar_item(ui, icon::LIST_MUSIC, &pl.title, active, true) {
                                    self.open_playlist(pl.browse_id, pl.title);
                                }
                            }
                            if self.guide_playlists.len() > 4 {
                                let toggle_lbl = if self.playlists_sidebar_expanded {
                                    "Show less"
                                } else {
                                    "Show more"
                                };
                                let toggle_ico = if self.playlists_sidebar_expanded {
                                    icon::CHEVRON_UP
                                } else {
                                    icon::CHEVRON_DOWN
                                };
                                if sidebar_item(ui, toggle_ico, toggle_lbl, false, true) {
                                    self.playlists_sidebar_expanded =
                                        !self.playlists_sidebar_expanded;
                                }
                            }
                        }

                        if self.sidebar_expanded && self.signed_in && !self.guide_channels.is_empty()
                        {
                            ui.add_space(6.0);
                            ui.separator();
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new("Subscriptions")
                                    .font(theme::semibold(12.5))
                                    .color(theme::TEXT_DIM),
                            );
                            ui.add_space(4.0);
                            let limit = if self.subs_sidebar_expanded {
                                self.guide_channels.len()
                            } else {
                                7.min(self.guide_channels.len())
                            };
                            let chans: Vec<_> = self.guide_channels[..limit].to_vec();
                            for ch in chans {
                                let active = self.page == Page::Channel
                                    && self.channel_id.as_deref() == Some(ch.browse_id.as_str());
                                if sidebar_channel_row(
                                    ui,
                                    &ch.name,
                                    ch.avatar_url.as_deref(),
                                    ch.is_live,
                                    active,
                                ) {
                                    self.open_channel(ch.browse_id, ch.name);
                                }
                            }
                            if self.guide_channels.len() > 7 {
                                let toggle_lbl = if self.subs_sidebar_expanded {
                                    "Show less"
                                } else {
                                    "Show more"
                                };
                                let toggle_ico = if self.subs_sidebar_expanded {
                                    icon::CHEVRON_UP
                                } else {
                                    icon::CHEVRON_DOWN
                                };
                                if sidebar_item(ui, toggle_ico, toggle_lbl, false, true) {
                                    self.subs_sidebar_expanded = !self.subs_sidebar_expanded;
                                }
                            }
                        }

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        if sidebar_item(
                            ui,
                            icon::SETTINGS,
                            "Settings",
                            self.page == Page::Settings,
                            self.sidebar_expanded,
                        ) {
                            self.page = Page::Settings;
                        }
                    });

                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    if self.sidebar_expanded {
                        if let Some(mpv) = &self.mpv {
                            ui.label(
                                RichText::new("● libmpv GPU ready")
                                    .font(theme::medium(11.5))
                                    .color(Color32::from_rgb(110, 215, 135)),
                            )
                            .on_hover_text(mpv.dll_path.display().to_string());
                        } else {
                            let r = ui.add(
                                egui::Button::new(
                                    RichText::new("● Install libmpv")
                                        .font(theme::medium(11.5))
                                        .color(Color32::from_rgb(255, 185, 85)),
                                )
                                .fill(Color32::from_rgb(38, 30, 18)),
                            );
                            if r.clicked() && !self.downloading_mpv {
                                self.downloading_mpv = true;
                                let _ = self.backend.tx.send(Request::DownloadLibmpv);
                            }
                        }
                    }
                });
            });
    }

    fn draw_home_page(&mut self, ui: &mut egui::Ui) {
        // Filter chips bar with < / > arrows, wheel scroll, and left-click hold & drag panning
        let drag_blocked_click = self.chips_drag_dist >= 6.0;
        let mut clicked_remote: Option<Option<String>> = None;
        let mut clicked_guest: Option<(usize, Option<&'static str>)> = None;

        ui.horizontal(|ui| {
            let left_clicked = icon_circle_button(ui, icon::CHEVRON_LEFT, 15.0, "Scroll left");
            let avail_bar_w = (ui.available_width() - 40.0).max(160.0);
            let bar_out = ui.allocate_ui_with_layout(
                Vec2::new(avail_bar_w, 34.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    let bar_clip = ui.clip_rect().intersect(ui.max_rect());
                    ui.set_clip_rect(bar_clip);
                    ScrollArea::horizontal()
                        .id_salt("home_chips")
                        .horizontal_scroll_offset(self.chips_scroll_x)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            if !self.home_chips.is_empty() {
                                for chip in &self.home_chips {
                                    let fill = if chip.selected {
                                        Color32::from_rgb(241, 241, 241)
                                    } else {
                                        theme::CHIP
                                    };
                                    let text_col = if chip.selected {
                                        Color32::from_rgb(15, 15, 15)
                                    } else {
                                        theme::TEXT
                                    };
                                    let btn = egui::Button::new(
                                        RichText::new(&chip.title)
                                            .font(theme::medium(13.0))
                                            .color(text_col),
                                    )
                                    .fill(fill)
                                    .corner_radius(CornerRadius::same(8));
                                    if ui.add(btn).clicked() && !drag_blocked_click {
                                        clicked_remote = Some(chip.params.clone());
                                    }
                                }
                            } else {
                                for (idx, &(title, query)) in GUEST_TOPICS.iter().enumerate() {
                                    let selected = self.selected_topic_chip == idx;
                                    let fill = if selected {
                                        Color32::from_rgb(241, 241, 241)
                                    } else {
                                        theme::CHIP
                                    };
                                    let text_col = if selected {
                                        Color32::from_rgb(15, 15, 15)
                                    } else {
                                        theme::TEXT
                                    };
                                    let btn = egui::Button::new(
                                        RichText::new(title)
                                            .font(theme::medium(13.0))
                                            .color(text_col),
                                    )
                                    .fill(fill)
                                    .corner_radius(CornerRadius::same(8));
                                    if ui.add(btn).clicked() && !drag_blocked_click {
                                        clicked_guest = Some((idx, query));
                                    }
                                }
                            }
                        })
                },
            );
            let scroll_out = bar_out.inner;
            let bar_rect = bar_out.response.rect;
            let max_scroll = (scroll_out.content_size.x - scroll_out.inner_rect.width()).max(0.0);
            let mut next_offset = scroll_out.state.clamped_offset().x;
            if left_clicked {
                next_offset -= 220.0;
            }

            let hovered_bar = ui.rect_contains_pointer(bar_rect);
            let (primary_down, primary_pressed, secondary_down, delta_x, wheel_y) =
                ui.ctx().input(|i| {
                    (
                        i.pointer.primary_down(),
                        i.pointer.primary_pressed(),
                        i.pointer.secondary_down(),
                        i.pointer.delta().x,
                        i.smooth_scroll_delta.y,
                    )
                });
            if hovered_bar && primary_pressed {
                self.chips_dragging = true;
                self.chips_drag_dist = 0.0;
            }
            if self.chips_dragging && primary_down {
                self.chips_drag_dist += delta_x.abs();
                next_offset -= delta_x;
                if self.chips_drag_dist >= 6.0 {
                    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                }
            } else if !primary_down {
                self.chips_dragging = false;
                self.chips_drag_dist = 0.0;
            }
            if hovered_bar {
                if secondary_down {
                    next_offset -= delta_x;
                }
                if wheel_y.abs() > 0.1 {
                    next_offset -= wheel_y;
                }
            }

            let right_clicked =
                icon_circle_button(ui, icon::CHEVRON_RIGHT, 15.0, "Scroll right (or drag)");
            if right_clicked {
                next_offset += 220.0;
            }
            self.chips_scroll_x = next_offset.clamp(0.0, max_scroll);
        });

        if let Some(params) = clicked_remote {
            self.home_loading = true;
            let _ = self.backend.tx.send(Request::LoadHome {
                params,
                continuation: None,
            });
        } else if let Some((idx, query)) = clicked_guest {
            self.selected_topic_chip = idx;
            match query {
                None => {
                    self.home_loading = true;
                    let _ = self.backend.tx.send(Request::LoadHome {
                        params: None,
                        continuation: None,
                    });
                }
                Some(q) => {
                    let _ = self.backend.tx.send(Request::Search {
                        query: q.to_owned(),
                        continuation: None,
                    });
                    self.active_query = q.to_owned();
                    self.page = Page::Search;
                    self.search_items.clear();
                    self.search_loading = true;
                    self.home_loading = false;
                }
            }
        }
        ui.add_space(10.0);

        if !self.signed_in && self.home_is_fallback {
            Frame::new()
                .fill(theme::SURFACE)
                .stroke(Stroke::new(1.0, theme::BORDER))
                .corner_radius(12)
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        theme::draw_icon(ui, icon::SPARKLES, 16.0, theme::YT_BLUE);
                        ui.label(
                            RichText::new(
                                "Showing Trending & Explore. Sign in with Google for your personalized YouTube Home feed & Subscriptions.",
                            )
                            .font(theme::regular(13.0))
                            .color(theme::TEXT_DIM),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Sign in")
                                            .font(theme::semibold(12.5))
                                            .color(Color32::from_rgb(15, 15, 15)),
                                    )
                                    .fill(theme::YT_BLUE)
                                    .corner_radius(16),
                                )
                                .clicked()
                            {
                                self.start_sign_in();
                            }
                        });
                    });
                });
            ui.add_space(12.0);
        }

        let items = self.home_items.clone();
        let loading = self.home_loading;
        let cont = self.home_continuation.clone();
        let title = if self.signed_in { "" } else { "Recommended & Trending" };
        let ctx = ui.ctx().clone();
        if let Some(act) = draw_video_grid(ui, title, &items, loading, cont.is_some()) {
            self.handle_grid_action(&ctx, act, |app| {
                app.home_loading = true;
                let _ = app.backend.tx.send(Request::LoadHome {
                    params: None,
                    continuation: cont,
                });
            });
        }
    }

    fn draw_subscriptions_page(&mut self, ui: &mut egui::Ui) {
        if !self.signed_in {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    theme::draw_icon(ui, icon::RADIO, 42.0, theme::TEXT_DIM);
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("Don't miss new videos")
                            .font(theme::bold(20.0))
                            .color(Color32::WHITE),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Sign in to see updates from your favorite YouTube channels")
                            .font(theme::regular(13.5))
                            .color(theme::TEXT_DIM),
                    );
                    ui.add_space(12.0);
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(format!("{}  Sign in with Google", icon::CIRCLE_USER))
                                    .font(theme::semibold(13.5))
                                    .color(theme::YT_BLUE),
                            )
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, theme::BORDER))
                            .corner_radius(18),
                        )
                        .clicked()
                    {
                        self.start_sign_in();
                    }
                });
            });
            return;
        }
        let items = self.subs_items.clone();
        let loading = self.subs_loading;
        let cont = self.subs_continuation.clone();
        let ctx = ui.ctx().clone();
        if let Some(act) =
            draw_video_grid(ui, "Subscriptions", &items, loading, cont.is_some())
        {
            self.handle_grid_action(&ctx, act, |app| {
                app.subs_loading = true;
                let _ = app.backend.tx.send(Request::LoadSubscriptions {
                    continuation: cont,
                });
            });
        }
    }

    fn draw_channel_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let info = self.channel_info.clone().unwrap_or_else(|| ChannelInfo {
            id: self.channel_id.clone().unwrap_or_default(),
            name: self.channel_name.clone(),
            ..Default::default()
        });

        if let Some(banner) = &info.banner_url {
            let bw = ui.available_width();
            let bh = (bw * 0.145).clamp(72.0, 140.0);
            let (brect, _) = ui.allocate_exact_size(Vec2::new(bw, bh), Sense::hover());
            egui::Image::new(banner)
                .corner_radius(CornerRadius::same(12))
                .fit_to_exact_size(Vec2::new(bw, bh))
                .paint_at(ui, brect);
            ui.add_space(12.0);
        }

        ui.horizontal(|ui| {
            draw_avatar(ui, info.avatar_url.as_deref(), &info.name, 72.0);
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(&info.name)
                        .font(theme::bold(22.0))
                        .color(Color32::WHITE),
                );
                let sub_line = match (info.handle.is_empty(), info.subscribers_text.is_empty()) {
                    (false, false) => format!("{} · {}", info.handle, info.subscribers_text),
                    (false, true) => info.handle.clone(),
                    (true, false) => info.subscribers_text.clone(),
                    (true, true) => String::new(),
                };
                if !sub_line.is_empty() {
                    ui.label(
                        RichText::new(sub_line)
                            .font(theme::regular(13.0))
                            .color(theme::TEXT_DIM),
                    );
                }
                ui.add_space(4.0);
                if let Some(cid) = self.channel_id.clone() {
                    let (sub_label, sub_fill, sub_text_col) = if info.is_subscribed {
                        ("Subscribed", theme::CHIP, theme::TEXT)
                    } else {
                        (
                            "Subscribe",
                            Color32::from_rgb(241, 241, 241),
                            Color32::from_rgb(15, 15, 15),
                        )
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(sub_label)
                                    .font(theme::semibold(13.0))
                                    .color(sub_text_col),
                            )
                            .fill(sub_fill)
                            .corner_radius(18),
                        )
                        .clicked()
                    {
                        let _ = self.backend.tx.send(Request::SetSubscribe {
                            channel_id: cid,
                            subscribe: !info.is_subscribed,
                        });
                    }
                }
            });
        });

        ui.add_space(10.0);
        let tabs = if !info.tabs.is_empty() {
            info.tabs
        } else {
            vec![
                crate::model::ChannelTab {
                    title: "Videos".to_owned(),
                    params: Some("EgZ2aWRlb3PyBgQKAjoA".to_owned()),
                    selected: self.channel_tab_params.is_none(),
                },
                crate::model::ChannelTab {
                    title: "Shorts".to_owned(),
                    params: Some("EgZzaG9ydHPyBgUKA5oBAA%3D%3D".to_owned()),
                    selected: false,
                },
                crate::model::ChannelTab {
                    title: "Live".to_owned(),
                    params: Some("EgdzdHJlYW1z8gYECgJ6AA%3D%3D".to_owned()),
                    selected: false,
                },
                crate::model::ChannelTab {
                    title: "Playlists".to_owned(),
                    params: Some("EglwbGF5bGlzdHPyBgQKAkIA".to_owned()),
                    selected: false,
                },
            ]
        };
        ui.horizontal(|ui| {
            for tab in tabs {
                let is_active = match (&self.channel_tab_params, &tab.params) {
                    (Some(cur), Some(p)) => cur == p,
                    (None, _) => tab.selected,
                    _ => false,
                };
                let resp = ui.add(
                    egui::Button::new(
                        RichText::new(&tab.title)
                            .font(if is_active {
                                theme::semibold(14.0)
                            } else {
                                theme::medium(14.0)
                            })
                            .color(if is_active {
                                Color32::WHITE
                            } else {
                                theme::TEXT_DIM
                            }),
                    )
                    .fill(Color32::TRANSPARENT),
                );
                if is_active {
                    let r = resp.rect;
                    ui.painter().line_segment(
                        [pos2(r.left() + 4.0, r.bottom()), pos2(r.right() - 4.0, r.bottom())],
                        Stroke::new(2.0, Color32::WHITE),
                    );
                }
                if resp.clicked() && !is_active {
                    if let Some(cid) = self.channel_id.clone() {
                        self.channel_tab_params = tab.params.clone();
                        self.channel_items.clear();
                        self.channel_continuation = None;
                        self.channel_loading = true;
                        let _ = self.backend.tx.send(Request::LoadChannel {
                            channel_id: cid,
                            params: tab.params,
                            continuation: None,
                        });
                    }
                }
                ui.add_space(6.0);
            }
        });
        ui.separator();
        ui.add_space(8.0);

        let items = self.channel_items.clone();
        let loading = self.channel_loading;
        let cont = self.channel_continuation.clone();
        let tab_params = self.channel_tab_params.clone();
        if let Some(act) = draw_video_grid(ui, "", &items, loading, cont.is_some()) {
            self.handle_grid_action(ctx, act, |app| {
                if let Some(cid) = app.channel_id.clone() {
                    app.channel_loading = true;
                    let _ = app.backend.tx.send(Request::LoadChannel {
                        channel_id: cid,
                        params: tab_params,
                        continuation: cont,
                    });
                }
            });
        }
    }

    fn draw_settings_page(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical().show(ui, |ui| {
            ui.set_max_width(680.0);
            ui.label(RichText::new("Settings").font(theme::bold(22.0)));
            ui.add_space(14.0);

            settings_card(ui, "Account", |ui| {
                if self.signed_in {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Signed in as {}",
                                self.account_name.as_deref().unwrap_or("Google Account")
                            ))
                            .font(theme::medium(14.0)),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Sign out").clicked() {
                                let _ = self.backend.tx.send(Request::SignOut);
                            }
                        });
                    });
                    ui.add_space(6.0);
                    ui.checkbox(
                        &mut self.sync_history,
                        "Sync watch history & playback progress with YouTube",
                    );
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Sign in with Google to access your Home feed & Subscriptions.");
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Sign in").clicked() {
                                self.start_sign_in();
                            }
                        });
                    });
                }
            });

            ui.add_space(12.0);
            settings_card(ui, "Playback", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Preferred max quality");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("settings_quality")
                            .selected_text(self.preferred_quality.label())
                            .show_ui(ui, |ui| {
                                for q in PreferredQuality::ALL {
                                    ui.selectable_value(&mut self.preferred_quality, q, q.label());
                                }
                            });
                    });
                });
                ui.add_space(6.0);
                ui.checkbox(&mut self.sponsorblock_enabled, "Auto-skip SponsorBlock segments");
                ui.checkbox(&mut self.autoplay_next, "Autoplay next Up Next video");
            });

            ui.add_space(12.0);
            settings_card(ui, "Video Engine (libmpv)", |ui| {
                if let Some(mpv) = &self.mpv {
                    ui.colored_label(
                        Color32::from_rgb(110, 215, 135),
                        format!("Active: {}", mpv.dll_path.display()),
                    );
                } else {
                    ui.colored_label(
                        Color32::from_rgb(255, 185, 85),
                        "libmpv-2.dll is not installed yet.",
                    );
                    if let Some(err) = &self.mpv_error {
                        ui.label(RichText::new(err).font(theme::regular(12.0)).color(theme::TEXT_DIM));
                    }
                    ui.add_space(6.0);
                    if self.downloading_mpv {
                        ui.spinner();
                    } else if ui.button("Download libmpv-2.dll automatically").clicked() {
                        self.downloading_mpv = true;
                        let _ = self.backend.tx.send(Request::DownloadLibmpv);
                    }
                }
            });
        });
    }

    fn draw_watch_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.fullscreen {
            let avail = ui.available_size();
            let player_h = (avail.y - 58.0).max(200.0);
            self.draw_video_surface(ui, ctx, Vec2::new(avail.x, player_h));
            self.draw_player_controls(ui, ctx);
            return;
        }

        let wide = ui.available_width() >= 920.0 && !self.theater_mode;
        if wide {
            let total_w = ui.available_width();
            let right_w = (total_w * 0.29).clamp(300.0, 380.0);
            let left_w = (total_w - right_w - 18.0).max(460.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(left_w, ui.available_height()),
                    Layout::top_down(Align::LEFT),
                    |ui| {
                        ScrollArea::vertical().id_salt("watch_left_scroll").show(ui, |ui| {
                            let video_h =
                                (left_w * 9.0 / 16.0).min(ctx.content_rect().height() * 0.72);
                            self.draw_video_surface(ui, ctx, Vec2::new(left_w, video_h));
                            self.draw_player_controls(ui, ctx);
                            ui.add_space(12.0);
                            self.draw_watch_metadata(ui);
                        });
                    },
                );
                ui.add_space(16.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(right_w, ui.available_height()),
                    Layout::top_down(Align::LEFT),
                    |ui| {
                        self.draw_related_panel(ui);
                    },
                );
            });
        } else {
            ScrollArea::vertical().id_salt("watch_theater_scroll").show(ui, |ui| {
                let w = ui.available_width();
                let video_h = if self.theater_mode {
                    (ctx.content_rect().height() * 0.74).max(320.0)
                } else {
                    (w * 9.0 / 16.0).min(560.0)
                };
                self.draw_video_surface(ui, ctx, Vec2::new(w, video_h));
                self.draw_player_controls(ui, ctx);
                ui.add_space(12.0);
                self.draw_watch_metadata(ui);
                ui.add_space(16.0);
                self.draw_related_panel(ui);
            });
        }
    }

    fn draw_video_surface(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, size: Vec2) {
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        ui.painter().rect_filled(rect, 12.0, Color32::BLACK);

        if let Some(mpv) = &self.mpv {
            if self.watch.is_some() {
                ui.painter().add(mpv.paint_callback(rect));
            }
            if resp.double_clicked() {
                self.toggle_fullscreen(ctx);
            } else if resp.clicked() {
                mpv.toggle_pause();
            }

            let is_initial_wait =
                self.watch.is_some() && self.playback.video_h == 0 && !self.playback.paused;
            if self.watch_loading || self.playback.buffering || is_initial_wait {
                ctx.request_repaint();
                let label = if self.watch_loading {
                    "Resolving stream...".to_owned()
                } else if self.playback.cache_duration > 0.05 {
                    format!("Buffering • {:.1}s", self.playback.cache_duration)
                } else {
                    "Buffering...".to_owned()
                };
                draw_buffering_overlay(ui, ctx, rect, &label);
            }
        } else {
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(24.0)), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        theme::draw_icon(ui, icon::DOWNLOAD, 34.0, theme::YT_BLUE);
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("libmpv-2.dll is required for hardware video playback")
                                .font(theme::semibold(16.0)),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!(
                                "Place libmpv-2.dll beside yt-rust.exe or in {}",
                                self.backend.data_dir.display()
                            ))
                            .font(theme::regular(12.5))
                            .color(theme::TEXT_DIM),
                        );
                        ui.add_space(12.0);
                        if self.downloading_mpv {
                            ui.spinner();
                            ui.label("Downloading & extracting libmpv-2.dll...");
                        } else if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Download libmpv-2.dll automatically")
                                        .font(theme::semibold(13.0))
                                        .color(Color32::from_rgb(15, 15, 15)),
                                )
                                .fill(theme::YT_BLUE)
                                .corner_radius(18),
                            )
                            .clicked()
                        {
                            self.downloading_mpv = true;
                            let _ = self.backend.tx.send(Request::DownloadLibmpv);
                        }
                    });
                });
            });
        }
    }

    fn draw_player_controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let dur = self
            .playback
            .duration
            .max(self.watch.as_ref().map_or(0.0, |w| w.duration_secs));
        let mut pos = self.seeking_pos.unwrap_or(self.playback.time_pos);

        Frame::new()
            .fill(Color32::from_rgb(20, 20, 22))
            .corner_radius(CornerRadius {
                nw: 0,
                ne: 0,
                sw: 10,
                se: 10,
            })
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                let bar_w = ui.available_width();
                let (bar_rect, bar_resp) =
                    ui.allocate_exact_size(Vec2::new(bar_w, 14.0), Sense::click_and_drag());
                let h = if bar_resp.hovered() || bar_resp.dragged() {
                    6.0
                } else {
                    4.0
                };
                let track = Rect::from_center_size(bar_rect.center(), Vec2::new(bar_rect.width(), h));
                ui.painter()
                    .rect_filled(track, 2.5, Color32::from_rgb(72, 72, 76));

                if dur > 0.0 {
                    // Buffered readahead bar (behind progress bar)
                    if self.playback.cache_duration > 0.0 {
                        let buf_frac = ((pos + self.playback.cache_duration) / dur).clamp(0.0, 1.0) as f32;
                        let buf_rect = Rect::from_min_max(
                            track.min,
                            pos2(track.left() + track.width() * buf_frac, track.bottom()),
                        );
                        ui.painter()
                            .rect_filled(buf_rect, 2.5, Color32::from_white_alpha(75));
                    }

                    for seg in &self.sponsor_segments {
                        if !seg.fits(dur) {
                            continue;
                        }
                        let x0 = track.left()
                            + (seg.start / dur).clamp(0.0, 1.0) as f32 * track.width();
                        let x1 = track.left()
                            + (seg.end / dur).clamp(0.0, 1.0) as f32 * track.width();
                        let seg_rect = Rect::from_min_max(
                            pos2(x0, track.top()),
                            pos2(x1.max(x0 + 2.0), track.bottom()),
                        );
                        let color = match seg.category {
                            SponsorCategory::Sponsor => Color32::from_rgb(0, 212, 0),
                            SponsorCategory::SelfPromo => Color32::from_rgb(255, 255, 0),
                            SponsorCategory::Interaction => Color32::from_rgb(204, 0, 255),
                            _ => Color32::from_rgb(0, 200, 230),
                        };
                        ui.painter().rect_filled(seg_rect, 1.5, color);
                    }

                    let frac = (pos / dur).clamp(0.0, 1.0) as f32;
                    let fill = Rect::from_min_max(
                        track.min,
                        pos2(track.left() + track.width() * frac, track.bottom()),
                    );
                    ui.painter().rect_filled(fill, 2.5, theme::YT_RED);

                    // Subtle chapter dividers along the seekbar
                    if let Some(w) = &self.watch {
                        for ch in &w.chapters {
                            if ch.start_secs > 1.0 && ch.start_secs < dur {
                                let cx = track.left() + (ch.start_secs / dur) as f32 * track.width();
                                ui.painter().line_segment(
                                    [pos2(cx, track.top()), pos2(cx, track.bottom())],
                                    Stroke::new(2.0, Color32::from_rgb(20, 20, 22)),
                                );
                            }
                        }
                    }

                    let knob_x = track.left() + track.width() * frac;
                    if bar_resp.hovered() || bar_resp.dragged() {
                        ui.painter().circle_filled(
                            pos2(knob_x, track.center().y),
                            6.5,
                            theme::YT_RED,
                        );
                        if let Some(ptr) = ui.input(|i| i.pointer.hover_pos()) {
                            let hf = ((ptr.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
                            let h_secs = hf * dur;
                            let ch_title = self.watch.as_ref().and_then(|w| {
                                w.chapters
                                    .iter()
                                    .rfind(|c| c.start_secs <= h_secs)
                                    .map(|c| c.title.as_str())
                            });
                            let tip = match ch_title {
                                Some(t) => format!("{} · {t}", format_duration(h_secs)),
                                None => format_duration(h_secs),
                            };
                            let galley = ui.painter().layout_no_wrap(
                                tip,
                                theme::medium(11.5),
                                Color32::WHITE,
                            );
                            let pad = vec2(8.0, 4.0);
                            let box_sz = galley.size() + pad * 2.0;
                            let tip_x = ptr.x.clamp(
                                track.left() + box_sz.x * 0.5,
                                (track.right() - box_sz.x * 0.5).max(track.left() + box_sz.x * 0.5),
                            );
                            let tip_rect = Rect::from_min_size(
                                pos2(tip_x - box_sz.x * 0.5, track.top() - box_sz.y - 8.0),
                                box_sz,
                            );
                            ui.painter().rect_filled(tip_rect, 6.0, Color32::from_black_alpha(220));
                            ui.painter().galley(tip_rect.min + pad, galley, Color32::WHITE);

                            if let Some((sheet_url, uv)) = self
                                .watch
                                .as_ref()
                                .and_then(|w| w.storyboard.as_ref())
                                .and_then(|sb| sb.frame_at(h_secs, dur))
                            {
                                let prev_sz = vec2(160.0, 90.0);
                                let prev_x = ptr.x.clamp(
                                    track.left() + prev_sz.x * 0.5,
                                    (track.right() - prev_sz.x * 0.5)
                                        .max(track.left() + prev_sz.x * 0.5),
                                );
                                let prev_rect = Rect::from_min_size(
                                    pos2(prev_x - prev_sz.x * 0.5, tip_rect.top() - prev_sz.y - 6.0),
                                    prev_sz,
                                );
                                ui.painter().rect_filled(prev_rect.expand(2.0), 8.0, Color32::BLACK);
                                egui::Image::new(sheet_url)
                                    .uv(Rect::from_min_max(
                                        pos2(uv[0], uv[1]),
                                        pos2(uv[2], uv[3]),
                                    ))
                                    .corner_radius(CornerRadius::same(6))
                                    .fit_to_exact_size(prev_sz)
                                    .paint_at(ui, prev_rect);
                                ui.painter().rect_stroke(
                                    prev_rect,
                                    6.0,
                                    Stroke::new(1.5, Color32::from_white_alpha(80)),
                                    egui::StrokeKind::Outside,
                                );
                            }
                        }
                    }

                    if bar_resp.dragged() || bar_resp.clicked() {
                        if let Some(ptr) = bar_resp.interact_pointer_pos() {
                            let f =
                                ((ptr.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
                            pos = f * dur;
                            self.seeking_pos = Some(pos);
                        }
                    }
                    if bar_resp.drag_stopped() || bar_resp.clicked() {
                        if let Some(target) = self.seeking_pos.take() {
                            if let Some(mpv) = &self.mpv {
                                mpv.seek_abs(target);
                            }
                        }
                    }
                }

                ui.add_space(4.0);
                // Split control row into non-overlapping left and right sub-regions
                let row_w = ui.available_width();
                let has_cc = self
                    .watch
                    .as_ref()
                    .is_some_and(|w| !w.captions.is_empty());
                let has_qual = self
                    .watch
                    .as_ref()
                    .is_some_and(|w| !w.video_formats.is_empty());
                let has_audio = self
                    .watch
                    .as_ref()
                    .is_some_and(|w| w.audio_tracks.len() > 1);
                let right_w: f32 = (if self.fullscreen { 38.0_f32 } else { 76.0_f32 }
                    + 54.0
                    + if has_qual { 124.0 } else { 0.0 }
                    + if has_audio { 112.0 } else { 0.0 }
                    + if has_cc { 64.0 } else { 0.0 })
                .min(row_w * 0.62);
                let left_w = (row_w - right_w - 6.0).max(160.0);

                let (row_rect, _) =
                    ui.allocate_exact_size(Vec2::new(row_w, 34.0), Sense::hover());
                let left_rect = Rect::from_min_max(
                    row_rect.min,
                    pos2(row_rect.left() + left_w, row_rect.max.y),
                );
                let right_rect = Rect::from_min_max(
                    pos2(row_rect.right() - right_w, row_rect.top()),
                    row_rect.max,
                );

                let mut left_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(left_rect)
                        .layout(Layout::left_to_right(Align::Center)),
                );
                let lclip = left_ui.clip_rect().intersect(left_rect);
                left_ui.set_clip_rect(lclip);
                left_ui.horizontal_centered(|ui| {
                    let play_ico = if self.playback.paused {
                        icon::PLAY
                    } else {
                        icon::PAUSE
                    };
                    if icon_circle_button(ui, play_ico, 17.0, "Play/Pause (Space)") {
                        if let Some(mpv) = &self.mpv {
                            mpv.toggle_pause();
                        }
                    }
                    if left_w > 240.0 {
                        if icon_circle_button(ui, icon::SKIP_BACK, 16.0, "Back 10s (J)") {
                            if let Some(mpv) = &self.mpv {
                                mpv.seek_rel(-10.0);
                            }
                        }
                        if icon_circle_button(ui, icon::SKIP_FORWARD, 16.0, "Forward 10s (L)") {
                            if let Some(mpv) = &self.mpv {
                                mpv.seek_rel(10.0);
                            }
                        }
                    }

                    let vol_ico = if self.muted || self.volume < 0.5 {
                        icon::VOLUME_X
                    } else if self.volume < 50.0 {
                        icon::VOLUME_1
                    } else {
                        icon::VOLUME_2
                    };
                    if icon_circle_button(ui, vol_ico, 16.0, "Mute (M)") {
                        self.muted = !self.muted;
                        if let Some(mpv) = &self.mpv {
                            mpv.set_mute(self.muted);
                        }
                    }
                    if left_w > 345.0 {
                        let mut vol = self.volume;
                        if ui
                            .add_sized(
                                [48.0, 18.0],
                                egui::Slider::new(&mut vol, 0.0..=100.0).show_value(false),
                            )
                            .changed()
                        {
                            self.volume = vol;
                            self.muted = false;
                            if let Some(mpv) = &self.mpv {
                                mpv.set_mute(false);
                                mpv.set_volume(vol);
                            }
                        }
                    }

                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "{} / {}",
                            format_duration(pos),
                            format_duration(dur)
                        ))
                        .font(theme::medium(12.0))
                        .color(theme::TEXT_DIM),
                    );
                    if let Some(ch_title) = self.watch.as_ref().and_then(|w| {
                        w.chapters
                            .iter()
                            .rfind(|c| c.start_secs <= pos)
                            .map(|c| c.title.clone())
                    }) {
                        let rem = ui.available_width();
                        if rem > 48.0 {
                            ui.add_sized(
                                [rem, 20.0],
                                egui::Label::new(
                                    RichText::new(format!("• {ch_title}"))
                                        .font(theme::regular(12.0))
                                        .color(theme::TEXT_DIM),
                                )
                                .truncate(),
                            );
                        }
                    }
                });

                let mut right_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(right_rect)
                        .layout(Layout::right_to_left(Align::Center)),
                );
                let rclip = right_ui.clip_rect().intersect(right_rect);
                right_ui.set_clip_rect(rclip);
                {
                    let ui = &mut right_ui;
                    let fs_ico = if self.fullscreen {
                        icon::SHRINK
                    } else {
                        icon::MAXIMIZE_2
                    };
                    if icon_circle_button(ui, fs_ico, 16.0, "Fullscreen (F)") {
                        self.toggle_fullscreen(ctx);
                    }
                    if !self.fullscreen {
                        if icon_circle_button(ui, icon::EXPAND, 16.0, "Theater Mode (T)") {
                            self.theater_mode = !self.theater_mode;
                        }
                    }

                    let mut picked_quality: Option<usize> = None;
                    let mut picked_audio: Option<usize> = None;
                    if let Some(w) = &self.watch {
                        if !w.captions.is_empty() {
                            let cc_label = self
                                .selected_caption_idx
                                .and_then(|i| w.captions.get(i))
                                .map(|c| format!("CC: {}", c.language_code.to_uppercase()))
                                .unwrap_or_else(|| "CC".to_owned());
                            let mut picked_cap: Option<Option<usize>> = None;
                            egui::ComboBox::from_id_salt("watch_cc")
                                .width(56.0)
                                .selected_text(cc_label)
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(self.selected_caption_idx.is_none(), "Off")
                                        .clicked()
                                    {
                                        picked_cap = Some(None);
                                    }
                                    for (i, c) in w.captions.iter().enumerate() {
                                        let suffix = if c.is_auto { " (auto)" } else { "" };
                                        if ui
                                            .selectable_label(
                                                self.selected_caption_idx == Some(i),
                                                format!("{}{suffix}", c.name),
                                            )
                                            .clicked()
                                        {
                                            picked_cap = Some(Some(i));
                                        }
                                    }
                                });
                            if let Some(choice) = picked_cap {
                                self.selected_caption_idx = choice;
                                let url = choice
                                    .and_then(|i| w.captions.get(i))
                                    .map(|c| c.vtt_url.as_str());
                                if let Some(mpv) = &self.mpv {
                                    mpv.set_subtitle(url);
                                }
                            }
                        }
                        if !w.video_formats.is_empty() {
                            let current_label = self
                                .selected_format_idx
                                .and_then(|i| w.video_formats.get(i))
                                .map(|f| f.label())
                                .unwrap_or_else(|| "Quality".to_owned());
                            egui::ComboBox::from_id_salt("watch_quality")
                                .width(114.0)
                                .selected_text(current_label)
                                .show_ui(ui, |ui| {
                                    for (i, f) in w.video_formats.iter().enumerate() {
                                        if ui
                                            .selectable_label(
                                                self.selected_format_idx == Some(i),
                                                f.label(),
                                            )
                                            .clicked()
                                        {
                                            picked_quality = Some(i);
                                        }
                                    }
                                });
                        }
                        if w.audio_tracks.len() > 1 {
                            let tr_label = w
                                .audio_tracks
                                .get(self.selected_audio_idx)
                                .map(|t| t.label.as_str())
                                .unwrap_or("Audio");
                            egui::ComboBox::from_id_salt("watch_audio_track")
                                .width(100.0)
                                .selected_text(format!("{}  {tr_label}", icon::HEADPHONES))
                                .show_ui(ui, |ui| {
                                    for (i, tr) in w.audio_tracks.iter().enumerate() {
                                        if ui
                                            .selectable_label(
                                                self.selected_audio_idx == i,
                                                &tr.label,
                                            )
                                            .clicked()
                                        {
                                            picked_audio = Some(i);
                                        }
                                    }
                                });
                        }
                    }
                    if let Some(idx) = picked_quality {
                        self.switch_quality(idx);
                    }
                    if let Some(idx) = picked_audio {
                        self.switch_audio_track(idx);
                    }

                    let mut new_speed = self.speed;
                    egui::ComboBox::from_id_salt("watch_speed")
                        .width(44.0)
                        .selected_text(format!("{}x", self.speed))
                        .show_ui(ui, |ui| {
                            for s in [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0] {
                                ui.selectable_value(&mut new_speed, s, format!("{s}x"));
                            }
                        });
                    if (new_speed - self.speed).abs() > 0.01 {
                        self.speed = new_speed;
                        if let Some(mpv) = &self.mpv {
                            mpv.set_speed(new_speed);
                        }
                    }
                }
            });
    }

    fn draw_watch_metadata(&mut self, ui: &mut egui::Ui) {
        let Some(w) = self.watch.clone() else {
            if self.watch_loading {
                ui.spinner();
            }
            return;
        };

        ui.label(RichText::new(&w.title).font(theme::bold(20.0)).color(Color32::WHITE));
        ui.add_space(10.0);

        let meta_w = ui.available_width();
        if meta_w >= 880.0 {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new((meta_w * 0.42).max(240.0), 42.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        self.draw_channel_row(ui, &w);
                    },
                );
                self.draw_actions_row(ui, &w, true);
            });
        } else {
            self.draw_channel_row(ui, &w);
            ui.add_space(8.0);
            self.draw_actions_row(ui, &w, false);
        }

        ui.add_space(10.0);

        Frame::new()
            .fill(theme::SURFACE)
            .corner_radius(12)
            .inner_margin(Margin::same(14))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if !w.views_text.is_empty() {
                        ui.label(RichText::new(&w.views_text).font(theme::semibold(13.0)));
                    }
                    if !w.published_text.is_empty() {
                        ui.label(RichText::new(&w.published_text).font(theme::semibold(13.0)));
                    }
                    if self.used_ytdlp {
                        ui.label(
                            RichText::new("· via yt-dlp")
                                .font(theme::regular(12.0))
                                .color(theme::YT_BLUE),
                        );
                    }
                    if !self.sponsor_segments.is_empty() {
                        ui.label(
                            RichText::new(format!(
                                "· {} SponsorBlock segments",
                                self.sponsor_segments.len()
                            ))
                            .font(theme::regular(12.0))
                            .color(Color32::from_rgb(120, 220, 140)),
                        );
                    }
                });
                if !w.description.is_empty() {
                    ui.add_space(6.0);
                    let desc = if self.show_description || w.description.len() <= 240 {
                        w.description.as_str()
                    } else {
                        let end = w
                            .description
                            .char_indices()
                            .nth(240)
                            .map_or(w.description.len(), |(i, _)| i);
                        &w.description[..end]
                    };
                    ui.label(
                        RichText::new(desc)
                            .font(theme::regular(13.0))
                            .color(Color32::from_rgb(215, 215, 215)),
                    );
                    if w.description.len() > 240 {
                        let btn = if self.show_description {
                            "Show less"
                        } else {
                            "...more"
                        };
                        if ui.link(RichText::new(btn).font(theme::semibold(12.5))).clicked() {
                            self.show_description = !self.show_description;
                        }
                    }
                }
            });

        if !w.chapters.is_empty() {
            ui.add_space(10.0);
            Frame::new()
                .fill(theme::SURFACE)
                .corner_radius(12)
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("Chapters ({})", w.chapters.len()))
                                .font(theme::semibold(14.0)),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let label = if self.show_chapters { "Hide" } else { "Show all" };
                            if ui.small_button(label).clicked() {
                                self.show_chapters = !self.show_chapters;
                            }
                        });
                    });
                    if self.show_chapters {
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            for ch in &w.chapters {
                                let active = self.playback.time_pos >= ch.start_secs
                                    && w.chapters
                                        .iter()
                                        .find(|n| n.start_secs > ch.start_secs)
                                        .is_none_or(|n| self.playback.time_pos < n.start_secs);
                                let fill = if active {
                                    theme::YT_BLUE.gamma_multiply(0.25)
                                } else {
                                    theme::CHIP
                                };
                                let btn = egui::Button::new(
                                    RichText::new(format!(
                                        "{}  {}",
                                        format_duration(ch.start_secs),
                                        ch.title
                                    ))
                                    .font(theme::medium(12.0)),
                                )
                                .fill(fill)
                                .corner_radius(14);
                                if ui.add(btn).clicked() {
                                    if let Some(mpv) = &self.mpv {
                                        mpv.seek_abs(ch.start_secs);
                                    }
                                }
                            }
                        });
                    }
                });
        }

        // Comments section
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if self.comments.is_empty() {
                    "Comments".to_owned()
                } else {
                    format!("Comments ({})", self.comments.len())
                })
                .font(theme::semibold(16.0)),
            );
            if self.comments_loading {
                ui.add_space(8.0);
                ui.spinner();
            }
        });
        ui.add_space(8.0);

        if self.comments.is_empty() && !self.comments_loading {
            if let Some(tok) = w.comments_token.clone() {
                if ui.button("Load comments").clicked() {
                    self.comments_loading = true;
                    let _ = self.backend.tx.send(Request::LoadComments {
                        video_id: w.id.clone(),
                        continuation: tok,
                        append: false,
                    });
                }
            } else {
                ui.label(
                    RichText::new("Comments are unavailable for this video.")
                        .font(theme::regular(13.0))
                        .color(theme::TEXT_DIM),
                );
            }
        } else {
            for c in &self.comments {
                ui.horizontal_top(|ui| {
                    draw_avatar(ui, c.avatar_url.as_deref(), &c.author, 32.0);
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&c.author).font(theme::semibold(12.5)));
                            if !c.published.is_empty() {
                                ui.label(
                                    RichText::new(&c.published)
                                        .font(theme::regular(11.5))
                                        .color(theme::TEXT_DIM),
                                );
                            }
                        });
                        ui.label(
                            RichText::new(&c.text)
                                .font(theme::regular(13.0))
                                .color(theme::TEXT),
                        );
                        if !c.likes.is_empty() {
                            ui.label(
                                RichText::new(format!("{} likes", c.likes))
                                    .font(theme::regular(11.5))
                                    .color(theme::TEXT_DIM),
                            );
                        }
                    });
                });
                ui.add_space(10.0);
            }
            if !self.comments_loading {
                if let Some(tok) = self.comments_next_token.clone() {
                    if pill_button(ui, icon::CHEVRON_DOWN, "Load more comments").clicked() {
                        self.comments_loading = true;
                        let _ = self.backend.tx.send(Request::LoadComments {
                            video_id: w.id.clone(),
                            continuation: tok,
                            append: true,
                        });
                    }
                }
            }
        }
    }

    fn draw_channel_row(&mut self, ui: &mut egui::Ui, w: &WatchData) {
        ui.horizontal(|ui| {
            draw_avatar(ui, w.channel_avatar_url.as_deref(), &w.channel, 38.0);
            ui.add_space(8.0);
            let name_max_w = (ui.available_width() - 110.0).clamp(110.0, 260.0);
            ui.vertical(|ui| {
                ui.set_max_width(name_max_w);
                let ch_resp = ui.add(
                    egui::Label::new(
                        RichText::new(&w.channel)
                            .font(theme::semibold(15.0))
                            .color(Color32::WHITE),
                    )
                    .truncate()
                    .sense(Sense::click()),
                );
                if ch_resp.hovered() && w.channel_id.is_some() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if ch_resp.clicked() {
                    if let Some(cid) = w.channel_id.clone() {
                        self.open_channel(cid, w.channel.clone());
                    }
                }
                if !w.subscribers_text.is_empty() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&w.subscribers_text)
                                .font(theme::regular(12.0))
                                .color(theme::TEXT_DIM),
                        )
                        .truncate(),
                    );
                }
            });

            if let Some(cid) = w.channel_id.clone() {
                ui.add_space(6.0);
                let (sub_label, sub_fill, sub_text_col) = if w.is_subscribed {
                    ("Subscribed", theme::CHIP, theme::TEXT)
                } else {
                    (
                        "Subscribe",
                        Color32::from_rgb(241, 241, 241),
                        Color32::from_rgb(15, 15, 15),
                    )
                };
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(sub_label)
                                .font(theme::semibold(13.0))
                                .color(sub_text_col),
                        )
                        .fill(sub_fill)
                        .corner_radius(18),
                    )
                    .clicked()
                {
                    let _ = self.backend.tx.send(Request::SetSubscribe {
                        channel_id: cid,
                        subscribe: !w.is_subscribed,
                    });
                }
            }
        });
    }

    fn draw_actions_row(&mut self, ui: &mut egui::Ui, w: &WatchData, rtl: bool) {
        let layout = if rtl {
            Layout::right_to_left(Align::Center)
        } else {
            Layout::left_to_right(Align::Center)
        };
        ui.with_layout(layout, |ui| {
            let watch_url = format!("https://www.youtube.com/watch?v={}", w.id);
            if rtl {
                if pill_button(ui, icon::COPY, "Copy URL").clicked() {
                    ui.ctx().copy_text(watch_url.clone());
                    self.toast("Copied YouTube URL");
                }
                if pill_button(ui, icon::LINK, "Browser").clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(&watch_url));
                }
            }
            let is_saved = self.saved.iter().any(|x| x.id == w.id);
            let like_label = if w.likes_text.is_empty() {
                if w.is_liked {
                    "Liked".to_owned()
                } else {
                    "Like".to_owned()
                }
            } else {
                format!("{} Likes", w.likes_text)
            };
            let make_like_btn = || {
                egui::Button::new(
                    RichText::new(if w.is_liked {
                        format!("{}  {like_label}", icon::CHECK)
                    } else {
                        like_label.clone()
                    })
                    .font(theme::medium(12.5))
                    .color(if w.is_liked {
                        theme::YT_BLUE
                    } else {
                        theme::TEXT
                    }),
                )
                .fill(if w.is_liked {
                    theme::YT_BLUE.gamma_multiply(0.18)
                } else {
                    theme::CHIP
                })
                .corner_radius(18)
            };
            let make_dislike_btn = || {
                egui::Button::new(
                    RichText::new(icon::THUMBS_DOWN.to_string())
                        .font(theme::icon_font(14.0))
                        .color(if w.is_disliked {
                            theme::YT_RED
                        } else {
                            theme::TEXT
                        }),
                )
                .fill(if w.is_disliked {
                    theme::YT_RED.gamma_multiply(0.18)
                } else {
                    theme::CHIP
                })
                .corner_radius(18)
            };
            if !rtl {
                if ui.add(make_like_btn()).clicked() {
                    let _ = self.backend.tx.send(Request::SetLike {
                        video_id: w.id.clone(),
                        like: !w.is_liked,
                    });
                }
                if ui.add(make_dislike_btn()).on_hover_text("Dislike").clicked() {
                    let _ = self.backend.tx.send(Request::SetDislike {
                        video_id: w.id.clone(),
                        dislike: !w.is_disliked,
                    });
                }
            }
            if pill_button(
                ui,
                if is_saved { icon::CHECK } else { icon::LIST_PLUS },
                if is_saved { "Saved" } else { "Save" },
            )
            .clicked()
            {
                self.open_playlist_picker(VideoItem {
                    id: w.id.clone(),
                    title: w.title.clone(),
                    channel: w.channel.clone(),
                    channel_id: w.channel_id.clone(),
                    channel_avatar_url: w.channel_avatar_url.clone(),
                    views_text: w.views_text.clone(),
                    published_text: w.published_text.clone(),
                    duration_text: format_duration(w.duration_secs),
                    thumbnail_url: format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", w.id),
                    is_live: w.is_live,
                    watched_percent: None,
                    resume_secs: None,
                    playlist_browse_id: None,
                });
            }
            if rtl {
                if ui.add(make_dislike_btn()).on_hover_text("Dislike").clicked() {
                    let _ = self.backend.tx.send(Request::SetDislike {
                        video_id: w.id.clone(),
                        dislike: !w.is_disliked,
                    });
                }
                if ui.add(make_like_btn()).clicked() {
                    let _ = self.backend.tx.send(Request::SetLike {
                        video_id: w.id.clone(),
                        like: !w.is_liked,
                    });
                }
            }
            if !rtl {
                if pill_button(ui, icon::LINK, "Browser").clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(&watch_url));
                }
                if pill_button(ui, icon::COPY, "Copy URL").clicked() {
                    ui.ctx().copy_text(watch_url);
                    self.toast("Copied YouTube URL");
                }
            }
        });
    }

    fn draw_related_panel(&mut self, ui: &mut egui::Ui) {
        let Some(w) = self.watch.as_ref() else {
            return;
        };
        let has_chat = w.live_chat_token.is_some() || !self.live_chat.is_empty();
        let has_queue = !self.queue.is_empty();
        if has_chat || has_queue {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(
                        self.right_tab == RightTab::UpNext,
                        RichText::new("Up Next").font(theme::semibold(14.0)),
                    )
                    .clicked()
                {
                    self.right_tab = RightTab::UpNext;
                }
                if has_queue
                    && ui
                        .selectable_label(
                            self.right_tab == RightTab::Queue,
                            RichText::new(format!("Queue ({})", self.queue.len()))
                                .font(theme::semibold(14.0)),
                        )
                        .clicked()
                {
                    self.right_tab = RightTab::Queue;
                }
                if has_chat
                    && ui
                        .selectable_label(
                            self.right_tab == RightTab::LiveChat,
                            RichText::new("Live Chat").font(theme::semibold(14.0)),
                        )
                        .clicked()
                {
                    self.right_tab = RightTab::LiveChat;
                }
            });
            ui.add_space(8.0);
        } else {
            ui.label(RichText::new("Up Next").font(theme::semibold(15.5)));
            ui.add_space(8.0);
        }

        if has_queue && self.right_tab == RightTab::Queue {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} queued", self.queue.len()))
                        .font(theme::regular(12.0))
                        .color(theme::TEXT_DIM),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .small_button(format!("{} Clear", icon::TRASH_2))
                        .clicked()
                    {
                        self.queue.clear();
                        self.right_tab = RightTab::UpNext;
                    }
                });
            });
            ui.add_space(6.0);
            let q_items = self.queue.clone();
            let mut play_idx = None;
            let mut remove_idx = None;
            ScrollArea::vertical().id_salt("queue_scroll").show(ui, |ui| {
                for (qi, item) in q_items.iter().enumerate() {
                    let (row_rect, row_resp) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 82.0),
                        Sense::click(),
                    );
                    if row_resp.hovered() {
                        ui.painter().rect_filled(row_rect, 8.0, theme::SURFACE);
                    }
                    let inner = row_rect.shrink(4.0);
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                    child.horizontal(|ui| {
                        let thumb_size = Vec2::new(132.0, 74.0);
                        draw_thumbnail(
                            ui,
                            &item.thumbnail_url,
                            &item.duration_text,
                            thumb_size,
                            8.0,
                            item.watched_percent,
                        );
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.set_max_width((inner.width() - 168.0).max(60.0));
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&item.title)
                                        .font(theme::medium(12.5))
                                        .color(theme::TEXT),
                                )
                                .truncate(),
                            );
                            ui.label(
                                RichText::new(&item.channel)
                                    .font(theme::regular(11.5))
                                    .color(theme::TEXT_DIM),
                            );
                        });
                        if ui.small_button(icon::X.to_string()).clicked() {
                            remove_idx = Some(qi);
                        }
                    });
                    if row_resp.clicked() {
                        play_idx = Some(qi);
                    }
                    ui.add_space(4.0);
                }
            });
            if let Some(i) = remove_idx {
                self.queue.remove(i);
                if self.queue.is_empty() {
                    self.right_tab = RightTab::UpNext;
                }
            } else if let Some(i) = play_idx {
                let next = self.queue.remove(i);
                self.open_video(next);
            }
            return;
        }

        if has_chat && self.right_tab == RightTab::LiveChat {
            Frame::new()
                .fill(theme::SURFACE)
                .stroke(Stroke::new(1.0, theme::BORDER))
                .corner_radius(12)
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ScrollArea::vertical()
                        .id_salt("live_chat_scroll")
                        .stick_to_bottom(true)
                        .max_height((ui.available_height() - 16.0).max(240.0))
                        .show(ui, |ui| {
                            if self.live_chat.is_empty() {
                                ui.label(
                                    RichText::new("Waiting for live chat messages...")
                                        .font(theme::regular(12.5))
                                        .color(theme::TEXT_DIM),
                                );
                            }
                            for m in &self.live_chat {
                                ui.horizontal_wrapped(|ui| {
                                    let author_col = if m.is_owner {
                                        Color32::from_rgb(255, 214, 0)
                                    } else if m.is_mod {
                                        theme::YT_BLUE
                                    } else {
                                        theme::TEXT_DIM
                                    };
                                    ui.label(
                                        RichText::new(format!("{}:", m.author))
                                            .font(theme::semibold(12.0))
                                            .color(author_col),
                                    );
                                    ui.label(
                                        RichText::new(&m.text)
                                            .font(theme::regular(12.5))
                                            .color(theme::TEXT),
                                    );
                                });
                                ui.add_space(4.0);
                            }
                        });
                });
            return;
        }

        let related = w.related.clone();
        ScrollArea::vertical().id_salt("related_scroll").show(ui, |ui| {
            for item in related {
                let (row_rect, row_resp) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), 92.0),
                    Sense::click(),
                );
                if row_resp.hovered() {
                    ui.painter().rect_filled(row_rect, 8.0, theme::SURFACE);
                }
                let inner = row_rect.shrink(4.0);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                child.horizontal(|ui| {
                    let thumb_size = Vec2::new(152.0, 84.0);
                    draw_thumbnail(
                        ui,
                        &item.thumbnail_url,
                        &item.duration_text,
                        thumb_size,
                        8.0,
                        item.watched_percent,
                    );
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&item.title)
                                    .font(theme::medium(13.0))
                                    .color(theme::TEXT),
                            )
                            .truncate(),
                        );
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(&item.channel)
                                .font(theme::regular(11.5))
                                .color(theme::TEXT_DIM),
                        );
                        ui.label(
                            RichText::new(join_meta(&item.views_text, &item.published_text))
                                .font(theme::regular(11.5))
                                .color(theme::TEXT_DIM),
                        );
                    });
                });
                if row_resp.clicked() {
                    self.open_video(item);
                }
                ui.add_space(4.0);
            }
        });
    }

    fn draw_mini_player(&mut self, ctx: &egui::Context) {
        let Some(w) = self.watch.as_ref() else { return };
        let title = w.title.clone();
        let channel = w.channel.clone();
        let dur = self.playback.duration.max(w.duration_secs).max(0.0);
        let mut pos = self.seeking_pos.unwrap_or(self.playback.time_pos).max(0.0);
        let is_live = w.is_live;

        let default_x = (ctx.content_rect().right() - 360.0).max(20.0);
        let default_y = (ctx.content_rect().bottom() - 280.0).max(60.0);

        egui::Area::new(egui::Id::unique("mini_player"))
            .default_pos(pos2(default_x, default_y))
            .movable(true)
            .constrain(true)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(12)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 8],
                        blur: 22,
                        spread: 2,
                        color: Color32::from_black_alpha(170),
                    })
                    .inner_margin(Margin::same(1))
                    .show(ui, |ui| {
                        let pip_w = 340.0;
                        let video_size = vec2(pip_w, 191.0);
                        ui.set_width(pip_w);

                        let (vrect, vresp) = ui.allocate_exact_size(video_size, Sense::click());
                        ui.painter().rect_filled(vrect, 0.0, Color32::BLACK);
                        if let Some(mpv) = &self.mpv {
                            ui.painter().add(mpv.paint_callback(vrect));
                        }
                        if vresp.clicked() {
                            self.page = Page::Watch;
                        }

                        // Interactive seek/progress bar directly under the PiP video
                        if !is_live && dur > 0.5 {
                            let (bar_rect, bar_resp) = ui.allocate_exact_size(
                                Vec2::new(pip_w, 12.0),
                                Sense::click_and_drag(),
                            );
                            let hot = bar_resp.hovered() || bar_resp.dragged();
                            let bar_h = if hot { 5.0 } else { 3.5 };
                            let track = Rect::from_min_max(
                                pos2(bar_rect.left(), bar_rect.center().y - bar_h * 0.5),
                                pos2(bar_rect.right(), bar_rect.center().y + bar_h * 0.5),
                            );
                            ui.painter()
                                .rect_filled(track, 0.0, Color32::from_white_alpha(45));
                            let cache_frac = ((self.playback.time_pos + self.playback.cache_duration)
                                / dur)
                                .clamp(0.0, 1.0) as f32;
                            ui.painter().rect_filled(
                                Rect::from_min_size(
                                    track.min,
                                    Vec2::new(track.width() * cache_frac, bar_h),
                                ),
                                0.0,
                                Color32::from_white_alpha(80),
                            );
                            if bar_resp.dragged() || bar_resp.clicked() {
                                if let Some(ptr) = bar_resp.interact_pointer_pos() {
                                    let f = ((ptr.x - track.left()) / track.width()).clamp(0.0, 1.0)
                                        as f64;
                                    pos = f * dur;
                                    self.seeking_pos = Some(pos);
                                }
                            }
                            if bar_resp.drag_stopped() || bar_resp.clicked() {
                                if let Some(target) = self.seeking_pos.take() {
                                    if let Some(mpv) = &self.mpv {
                                        mpv.seek_abs(target);
                                    }
                                }
                            }
                            let frac = (pos / dur).clamp(0.0, 1.0) as f32;
                            let fill_x = track.left() + track.width() * frac;
                            ui.painter().rect_filled(
                                Rect::from_min_max(track.min, pos2(fill_x, track.max.y)),
                                0.0,
                                theme::YT_RED,
                            );
                            if hot {
                                ui.painter().circle_filled(
                                    pos2(fill_x, track.center().y),
                                    5.5,
                                    theme::YT_RED,
                                );
                                if let Some(hpos) = bar_resp.hover_pos() {
                                    let hf = ((hpos.x - track.left()) / track.width())
                                        .clamp(0.0, 1.0)
                                        as f64;
                                    bar_resp.on_hover_text(format_duration(hf * dur));
                                }
                            }
                        } else if is_live {
                            let (bar_rect, _) =
                                ui.allocate_exact_size(Vec2::new(pip_w, 4.0), Sense::hover());
                            ui.painter().rect_filled(bar_rect, 0.0, theme::YT_RED);
                        }

                        // Compact bottom bar with strictly bounded text column + evenly spaced buttons
                        Frame::new()
                            .inner_margin(Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                let inner_w = pip_w - 20.0;
                                let btns_w = 108.0;
                                let text_w = (inner_w - btns_w - 8.0).max(120.0);
                                let sub_line = if is_live {
                                    format!("LIVE · {channel}")
                                } else if dur > 0.5 {
                                    format!(
                                        "{} / {} · {channel}",
                                        format_duration(pos),
                                        format_duration(dur)
                                    )
                                } else {
                                    channel.clone()
                                };

                                ui.horizontal(|ui| {
                                    ui.allocate_ui_with_layout(
                                        Vec2::new(text_w, 34.0),
                                        Layout::top_down(Align::LEFT),
                                        |ui| {
                                            let tclip = ui.clip_rect().intersect(ui.max_rect());
                                            ui.set_clip_rect(tclip);
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&title)
                                                        .font(theme::semibold(12.0))
                                                        .color(theme::TEXT),
                                                )
                                                .truncate(),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(sub_line)
                                                        .font(theme::regular(11.0))
                                                        .color(theme::TEXT_DIM),
                                                )
                                                .truncate(),
                                            );
                                        },
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if icon_circle_button(ui, icon::X, 15.0, "Close player") {
                                            self.backend.cursor.stop();
                                            if let Some(m) = &self.mpv {
                                                m.stop();
                                            }
                                            self.watch = None;
                                        }
                                        if icon_circle_button(
                                            ui,
                                            icon::MAXIMIZE_2,
                                            14.0,
                                            "Expand",
                                        ) {
                                            self.page = Page::Watch;
                                        }
                                        let play_ico = if self.playback.paused {
                                            icon::PLAY
                                        } else {
                                            icon::PAUSE
                                        };
                                        if icon_circle_button(ui, play_ico, 15.0, "Play/Pause") {
                                            if let Some(m) = &self.mpv {
                                                m.toggle_pause();
                                            }
                                        }
                                    });
                                });
                            });
                    });
            });
    }

    fn draw_playlist_picker_modal(&mut self, ctx: &egui::Context) {
        let Some(video) = self.playlist_modal_video.clone() else {
            return;
        };
        let mut open = true;
        let mut close_now = false;
        egui::Window::new("Save video to...")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(370.0, 410.0))
            .frame(
                Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(14)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 8],
                        blur: 20,
                        spread: 2,
                        color: Color32::from_black_alpha(160),
                    })
                    .inner_margin(Margin::same(16)),
            )
            .show(ctx, |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(&video.title)
                            .font(theme::medium(12.5))
                            .color(theme::TEXT_DIM),
                    )
                    .truncate(),
                );
                ui.add_space(10.0);

                let in_wl = self.saved.iter().any(|x| x.id == video.id);
                let wl_text = if in_wl {
                    format!("{}  Watch Later (Saved)", icon::CHECK)
                } else {
                    format!("{}  Watch Later", icon::LIBRARY)
                };
                if ui
                    .add_sized(
                        Vec2::new(ui.available_width(), 34.0),
                        egui::Button::new(RichText::new(wl_text).font(theme::medium(13.0)))
                            .fill(if in_wl {
                                theme::YT_BLUE.gamma_multiply(0.18)
                            } else {
                                theme::CHIP
                            })
                            .corner_radius(9),
                    )
                    .clicked()
                {
                    if self.signed_in {
                        let _ = self.backend.tx.send(Request::SetWatchLater {
                            video_id: video.id.clone(),
                            add: !in_wl,
                        });
                    }
                    if in_wl {
                        self.saved.retain(|x| x.id != video.id);
                        self.toast("Removed from Watch Later");
                    } else {
                        self.saved.insert(0, video.clone());
                        self.toast("Saved to Watch Later");
                    }
                }

                ui.add_space(8.0);
                if self.signed_in {
                    ui.add_sized(
                        Vec2::new(ui.available_width(), 28.0),
                        egui::TextEdit::singleline(&mut self.playlist_filter)
                            .hint_text("Search playlists..."),
                    );
                    ui.add_space(6.0);
                    if self.playlist_options_loading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                RichText::new("Loading your YouTube playlists...")
                                    .font(theme::regular(12.5))
                                    .color(theme::TEXT_DIM),
                            );
                        });
                        ui.add_space(6.0);
                    }
                    let q_filter = self.playlist_filter.trim().to_ascii_lowercase();
                    ScrollArea::vertical()
                        .id_salt("playlist_picker_list")
                        .max_height(195.0)
                        .show(ui, |ui| {
                            let non_wl: Vec<(usize, PlaylistOption)> = self
                                .playlist_options
                                .iter()
                                .enumerate()
                                .filter(|(_, p)| {
                                    p.id != "WL"
                                        && (q_filter.is_empty()
                                            || p.title.to_ascii_lowercase().contains(&q_filter))
                                })
                                .map(|(i, p)| (i, p.clone()))
                                .collect();
                            if non_wl.is_empty() && !self.playlist_options_loading {
                                ui.label(
                                    RichText::new("No matching playlists.")
                                        .font(theme::regular(12.5))
                                        .color(theme::TEXT_DIM),
                                );
                            }
                            for (idx, opt) in non_wl {
                                let row_w = ui.available_width();
                                let (rect, resp) = ui.allocate_exact_size(
                                    Vec2::new(row_w, 38.0),
                                    Sense::click(),
                                );
                                let fill = if opt.is_added {
                                    theme::YT_BLUE.gamma_multiply(0.18)
                                } else if resp.hovered() {
                                    theme::CHIP_HOVER
                                } else {
                                    theme::CHIP
                                };
                                ui.painter().rect_filled(rect, 8.0, fill);

                                let thumb_rect = Rect::from_min_size(
                                    pos2(rect.left() + 6.0, rect.center().y - 13.0),
                                    Vec2::new(46.0, 26.0),
                                );
                                ui.painter().rect_filled(
                                    thumb_rect,
                                    5.0,
                                    Color32::from_rgb(38, 38, 44),
                                );
                                if let Some(u) = &opt.thumbnail_url {
                                    egui::Image::new(u)
                                        .corner_radius(CornerRadius::same(5))
                                        .fit_to_exact_size(thumb_rect.size())
                                        .paint_at(ui, thumb_rect);
                                } else {
                                    ui.painter().text(
                                        thumb_rect.center(),
                                        Align2::CENTER_CENTER,
                                        icon::LIST_MUSIC.to_string(),
                                        theme::icon_font(13.0),
                                        theme::TEXT_DIM,
                                    );
                                }

                                let ico = if opt.is_added {
                                    icon::CHECK
                                } else {
                                    icon::LIST_PLUS
                                };
                                ui.painter().text(
                                    pos2(rect.right() - 12.0, rect.center().y),
                                    Align2::RIGHT_CENTER,
                                    ico.to_string(),
                                    theme::icon_font(15.0),
                                    if opt.is_added {
                                        theme::YT_BLUE
                                    } else {
                                        theme::TEXT_DIM
                                    },
                                );

                                let title_rect = Rect::from_min_max(
                                    pos2(thumb_rect.right() + 10.0, rect.top() + 4.0),
                                    pos2(rect.right() - 32.0, rect.bottom() - 4.0),
                                );
                                let mut t_ui =
                                    ui.new_child(egui::UiBuilder::new().max_rect(title_rect));
                                let clip = t_ui.clip_rect().intersect(title_rect);
                                t_ui.set_clip_rect(clip);
                                t_ui.centered_and_justified(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&opt.title)
                                                .font(theme::medium(13.0))
                                                .color(theme::TEXT),
                                        )
                                        .truncate(),
                                    );
                                });

                                if resp.clicked() {
                                    let new_added = !opt.is_added;
                                    self.playlist_options[idx].is_added = new_added;
                                    let _ = self.backend.tx.send(Request::EditPlaylist {
                                        playlist_id: opt.id,
                                        playlist_title: opt.title,
                                        video_id: video.id.clone(),
                                        add: new_added,
                                    });
                                }
                                ui.add_space(4.0);
                            }
                        });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Create new playlist")
                            .font(theme::semibold(12.5))
                            .color(theme::TEXT_DIM),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let resp = ui.add_sized(
                            Vec2::new((ui.available_width() - 78.0).max(120.0), 30.0),
                            egui::TextEdit::singleline(&mut self.new_playlist_title)
                                .hint_text("Playlist title"),
                        );
                        let submit = ui
                            .add_sized(
                                Vec2::new(68.0, 30.0),
                                egui::Button::new(
                                    RichText::new("Create")
                                        .font(theme::semibold(12.5))
                                        .color(Color32::from_rgb(15, 15, 15)),
                                )
                                .fill(theme::YT_BLUE)
                                .corner_radius(8),
                            )
                            .clicked()
                            || (resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
                        if submit {
                            let title = self.new_playlist_title.trim().to_owned();
                            if !title.is_empty() {
                                let _ = self.backend.tx.send(Request::CreatePlaylist {
                                    title,
                                    video_id: Some(video.id.clone()),
                                });
                                self.new_playlist_title.clear();
                                close_now = true;
                            }
                        }
                    });
                } else {
                    ui.label(
                        RichText::new("Sign in with Google to save to your YouTube playlists.")
                            .font(theme::regular(12.5))
                            .color(theme::TEXT_DIM),
                    );
                }
            });
        if !open || close_now {
            self.playlist_modal_video = None;
        }
    }
}

enum GridAction {
    Open(VideoItem),
    OpenChannel { channel_id: String, name: String },
    AddQueue(VideoItem),
    ToggleWatchLater(VideoItem),
    OpenPlaylistPicker(VideoItem),
    Download { item: VideoItem, audio_only: bool },
    CopyUrl(String),
    LoadMore,
}

fn draw_video_grid(
    ui: &mut egui::Ui,
    heading: &str,
    items: &[VideoItem],
    loading: bool,
    has_more: bool,
) -> Option<GridAction> {
    let mut action = None;
    if !heading.is_empty() || loading {
        ui.horizontal(|ui| {
            if !heading.is_empty() {
                ui.label(RichText::new(heading).font(theme::bold(20.0)));
            }
            if loading {
                ui.add_space(8.0);
                ui.spinner();
            }
        });
        ui.add_space(10.0);
    }

    if items.is_empty() && !loading {
        ui.label(
            RichText::new("No videos found.")
                .font(theme::regular(14.0))
                .color(theme::TEXT_DIM),
        );
        return None;
    }

    let scroll_out = ScrollArea::vertical().id_salt(heading).show(ui, |ui| {
        let avail = (ui.available_width() - 8.0).max(200.0);
        let cols = ((avail / 305.0).floor() as usize).clamp(1, 5);
        let spacing = 16.0;
        let card_w = ((avail - spacing * (cols as f32 - 1.0)) / cols as f32).max(180.0);
        let thumb_h = card_w * 9.0 / 16.0;

        for row in items.chunks(cols) {
            ui.horizontal_top(|ui| {
                for (col_idx, item) in row.iter().enumerate() {
                    let card_h = thumb_h + 92.0;
                    let (card_rect, card_resp) =
                        ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click());
                    let hovered = card_resp.hovered()
                        || ui.rect_contains_pointer(card_rect);
                    if hovered {
                        // Subtle warm ambient card highlight like modern YouTube
                        ui.painter().rect_filled(
                            card_rect.expand(5.0),
                            13.0,
                            Color32::from_rgb(36, 28, 25),
                        );
                    }

                    let mut card_ui =
                        ui.new_child(egui::UiBuilder::new().max_rect(card_rect));
                    let cclip = card_ui.clip_rect().intersect(card_rect);
                    card_ui.set_clip_rect(cclip);
                    let mut child_clicked_override = false;
                    card_ui.vertical(|ui| {
                        draw_thumbnail(
                            ui,
                            &item.thumbnail_url,
                            &item.duration_text,
                            Vec2::new(card_w, thumb_h),
                            12.0,
                            item.watched_percent,
                        );
                        ui.add_space(10.0);
                        ui.horizontal_top(|ui| {
                            let avatar_r = ui.horizontal(|ui| {
                                draw_avatar(
                                    ui,
                                    item.channel_avatar_url.as_deref(),
                                    &item.channel,
                                    34.0,
                                );
                            })
                            .response;
                            if item.channel_id.is_some()
                                && avatar_r.interact(Sense::click()).clicked()
                            {
                                if let Some(cid) = item.channel_id.clone() {
                                    action = Some(GridAction::OpenChannel {
                                        channel_id: cid,
                                        name: item.channel.clone(),
                                    });
                                    child_clicked_override = true;
                                }
                            }
                            ui.add_space(8.0);
                            ui.vertical(|ui| {
                                ui.set_max_width((card_w - 72.0).max(90.0));
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&item.title)
                                            .font(theme::semibold(13.8))
                                            .color(theme::TEXT),
                                    )
                                    .wrap(),
                                );
                                ui.add_space(2.0);
                                let ch_lbl = ui.add(
                                    egui::Label::new(
                                        RichText::new(&item.channel)
                                            .font(theme::regular(12.2))
                                            .color(theme::TEXT_DIM),
                                    )
                                    .truncate()
                                    .sense(Sense::click()),
                                );
                                if ch_lbl.hovered() && item.channel_id.is_some() {
                                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                                }
                                if ch_lbl.clicked() {
                                    if let Some(cid) = item.channel_id.clone() {
                                        action = Some(GridAction::OpenChannel {
                                            channel_id: cid,
                                            name: item.channel.clone(),
                                        });
                                        child_clicked_override = true;
                                    }
                                }
                                let meta = join_meta(&item.views_text, &item.published_text);
                                if !meta.is_empty() {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(meta)
                                                .font(theme::regular(12.0))
                                                .color(theme::TEXT_DIM),
                                        )
                                        .truncate(),
                                    );
                                }
                            });

                            ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                                let menu_id = ui.scope_id().with(&item.id);
                                ui.push_id(menu_id, |ui| {
                                    ui.style_mut().visuals.widgets.inactive.weak_bg_fill =
                                        Color32::TRANSPARENT;
                                    ui.style_mut().visuals.widgets.inactive.bg_stroke =
                                        Stroke::NONE;
                                    ui.style_mut().visuals.widgets.hovered.weak_bg_fill =
                                        Color32::from_white_alpha(22);
                                    let menu_resp = ui.menu_button(
                                        RichText::new(icon::ELLIPSIS_VERTICAL.to_string())
                                            .font(theme::icon_font(15.0))
                                            .color(theme::TEXT_DIM),
                                        |ui| {
                                            if ui
                                                .button(format!(
                                                    "{}  Add to Queue",
                                                    icon::LIST_PLUS
                                                ))
                                                .clicked()
                                            {
                                                action = Some(GridAction::AddQueue(item.clone()));
                                                ui.close();
                                            }
                                            if ui
                                                .button(format!(
                                                    "{}  Save to Watch Later",
                                                    icon::LIBRARY
                                                ))
                                                .clicked()
                                            {
                                                action = Some(GridAction::ToggleWatchLater(
                                                    item.clone(),
                                                ));
                                                ui.close();
                                            }
                                            if ui
                                                .button(format!(
                                                    "{}  Save to playlist...",
                                                    icon::LIST_MUSIC
                                                ))
                                                .clicked()
                                            {
                                                action = Some(GridAction::OpenPlaylistPicker(
                                                    item.clone(),
                                                ));
                                                ui.close();
                                            }
                                            if let Some(cid) = item.channel_id.clone() {
                                                if ui
                                                    .button(format!("{}  Go to channel", icon::USER))
                                                    .clicked()
                                                {
                                                    action = Some(GridAction::OpenChannel {
                                                        channel_id: cid,
                                                        name: item.channel.clone(),
                                                    });
                                                    ui.close();
                                                }
                                            }
                                            ui.separator();
                                            if ui
                                                .button(format!(
                                                    "{}  Download MP4",
                                                    icon::DOWNLOAD
                                                ))
                                                .clicked()
                                            {
                                                action = Some(GridAction::Download {
                                                    item: item.clone(),
                                                    audio_only: false,
                                                });
                                                ui.close();
                                            }
                                            if ui
                                                .button(format!(
                                                    "{}  Download Audio",
                                                    icon::HEADPHONES
                                                ))
                                                .clicked()
                                            {
                                                action = Some(GridAction::Download {
                                                    item: item.clone(),
                                                    audio_only: true,
                                                });
                                                ui.close();
                                            }
                                            if ui
                                                .button(format!("{}  Copy URL", icon::COPY))
                                                .clicked()
                                            {
                                                action =
                                                    Some(GridAction::CopyUrl(item.id.clone()));
                                                ui.close();
                                            }
                                        },
                                    );
                                    if menu_resp.response.hovered() || menu_resp.response.clicked()
                                    {
                                        child_clicked_override = true;
                                    }
                                });
                            });
                        });
                    });

                    if card_resp.clicked() && !child_clicked_override && action.is_none() {
                        action = Some(GridAction::Open(item.clone()));
                    }
                    if col_idx + 1 < row.len() {
                        ui.add_space(spacing);
                    }
                }
            });
            ui.add_space(18.0);
        }

        if has_more && !loading {
            ui.add_space(8.0);
            if ui
                .add(
                    egui::Button::new(RichText::new("Load more").font(theme::medium(13.0)))
                        .fill(theme::CHIP)
                        .corner_radius(18),
                )
                .clicked()
            {
                action = Some(GridAction::LoadMore);
            }
        }
    });

    if has_more && !loading && action.is_none() {
        let remaining = scroll_out.content_size.y
            - (scroll_out.state.clamped_offset().y + scroll_out.inner_rect.height());
        if remaining < 450.0 {
            action = Some(GridAction::LoadMore);
        }
    }

    action
}

fn sidebar_channel_row(
    ui: &mut egui::Ui,
    name: &str,
    avatar_url: Option<&str>,
    is_live: bool,
    active: bool,
) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 34.0), Sense::click());
    let fill = if active {
        theme::CHIP
    } else if resp.hovered() {
        Color32::from_rgb(28, 28, 30)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 9.0, fill);
    }
    let av_rect = Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), Vec2::splat(22.0));
    if let Some(u) = avatar_url {
        egui::Image::new(u)
            .corner_radius(CornerRadius::same(11))
            .fit_to_exact_size(Vec2::splat(22.0))
            .paint_at(ui, av_rect);
    } else {
        let initial = name.chars().next().unwrap_or('Y').to_ascii_uppercase();
        ui.painter()
            .circle_filled(av_rect.center(), 11.0, Color32::from_rgb(56, 56, 64));
        ui.painter().text(
            av_rect.center(),
            Align2::CENTER_CENTER,
            initial.to_string(),
            theme::semibold(10.0),
            Color32::WHITE,
        );
    }
    let max_text_w = (rect.width() - 58.0).max(40.0);
    let mut display = name.to_owned();
    while display.chars().count() > 3
        && ui
            .painter()
            .layout_no_wrap(display.clone(), theme::regular(12.8), theme::TEXT)
            .size()
            .x
            > max_text_w
    {
        display.pop();
        if display.ends_with(' ') {
            display.pop();
        }
    }
    if display.len() < name.len() {
        display.push('…');
    }
    ui.painter().text(
        pos2(rect.left() + 40.0, rect.center().y),
        Align2::LEFT_CENTER,
        display,
        if active {
            theme::semibold(12.8)
        } else {
            theme::regular(12.8)
        },
        if active { Color32::WHITE } else { theme::TEXT },
    );
    if is_live {
        ui.painter()
            .circle_filled(pos2(rect.right() - 10.0, rect.center().y), 3.0, theme::YT_RED);
    }
    resp.clicked()
}

fn sidebar_item(
    ui: &mut egui::Ui,
    ico: char,
    label: &str,
    active: bool,
    expanded: bool,
) -> bool {
    let w = ui.available_width();
    let h = if expanded { 38.0 } else { 44.0 };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let fill = if active {
        theme::CHIP
    } else if resp.hovered() {
        Color32::from_rgb(28, 28, 30)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 10.0, fill);
    }

    if expanded {
        ui.painter().text(
            pos2(rect.left() + 14.0, rect.center().y),
            Align2::LEFT_CENTER,
            ico.to_string(),
            theme::icon_font(17.0),
            if active { Color32::WHITE } else { theme::TEXT },
        );
        ui.painter().text(
            pos2(rect.left() + 46.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            if active {
                theme::semibold(13.5)
            } else {
                theme::regular(13.5)
            },
            if active { Color32::WHITE } else { theme::TEXT },
        );
    } else {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            ico.to_string(),
            theme::icon_font(18.5),
            if active { Color32::WHITE } else { theme::TEXT },
        );
        resp.clone().on_hover_text(label);
    }
    resp.clicked()
}

fn icon_circle_button(ui: &mut egui::Ui, ico: char, size: f32, tooltip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
    if resp.hovered() {
        ui.painter()
            .circle_filled(rect.center(), 16.0, Color32::from_white_alpha(20));
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        ico.to_string(),
        theme::icon_font(size),
        Color32::WHITE,
    );
    resp.on_hover_text(tooltip).clicked()
}

fn pill_button(ui: &mut egui::Ui, ico: char, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(format!("{ico}  {label}"))
                .font(theme::medium(12.5)),
        )
        .fill(theme::CHIP)
        .corner_radius(18),
    )
}

fn settings_card(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(12)
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).font(theme::semibold(15.5)));
            ui.add_space(10.0);
            add(ui);
        });
}

fn draw_buffering_overlay(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    rect: Rect,
    label: &str,
) {
    // Subtle dark vignette over the video while buffering
    ui.painter()
        .rect_filled(rect, 12.0, Color32::from_black_alpha(110));

    let c = rect.center();
    let pill = Rect::from_center_size(c, vec2(190.0, 74.0));
    ui.painter()
        .rect_filled(pill, 14.0, Color32::from_black_alpha(205));
    ui.painter().rect_stroke(
        pill,
        14.0,
        Stroke::new(1.0, Color32::from_white_alpha(28)),
        egui::StrokeKind::Inside,
    );

    // Smooth rotating YouTube-style arc spinner
    let t = ctx.input(|i| i.time) as f32;
    let spin_center = pos2(c.x, c.y - 11.0);
    let radius = 14.0;
    let start_angle = t * 5.2;
    let sweep = std::f32::consts::PI * 1.35;

    ui.painter().circle_stroke(
        spin_center,
        radius,
        Stroke::new(3.0, Color32::from_white_alpha(35)),
    );

    let steps = 20;
    let pts: Vec<egui::Pos2> = (0..=steps)
        .map(|k| {
            let a = start_angle + sweep * (k as f32 / steps as f32);
            pos2(
                spin_center.x + radius * a.cos(),
                spin_center.y + radius * a.sin(),
            )
        })
        .collect();
    ui.painter().add(egui::Shape::line(
        pts,
        Stroke::new(3.0, Color32::WHITE),
    ));

    ui.painter().text(
        pos2(c.x, c.y + 18.0),
        Align2::CENTER_CENTER,
        label,
        theme::medium(12.5),
        Color32::from_rgb(230, 230, 230),
    );
}

fn draw_avatar(ui: &mut egui::Ui, url: Option<&str>, name: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if let Some(u) = url {
        egui::Image::new(u)
            .corner_radius(CornerRadius::same((size / 2.0) as u8))
            .fit_to_exact_size(Vec2::splat(size))
            .paint_at(ui, rect);
    } else {
        let initial = name.chars().next().unwrap_or('Y').to_ascii_uppercase();
        ui.painter()
            .circle_filled(rect.center(), size * 0.5, Color32::from_rgb(56, 56, 64));
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            initial.to_string(),
            theme::semibold(size * 0.44),
            Color32::WHITE,
        );
    }
}

fn draw_thumbnail(
    ui: &mut egui::Ui,
    url: &str,
    duration: &str,
    size: Vec2,
    radius: f32,
    watched_percent: Option<f32>,
) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_filled(rect, radius, Color32::from_rgb(32, 32, 36));

    egui::Image::new(url)
        .corner_radius(CornerRadius::same(radius as u8))
        .fit_to_exact_size(size)
        .paint_at(ui, rect);

    if let Some(pct) = watched_percent.filter(|&p| p > 0.01) {
        let bar_h = 4.0;
        let track = Rect::from_min_max(
            pos2(rect.left(), rect.bottom() - bar_h),
            pos2(rect.right(), rect.bottom()),
        );
        ui.painter()
            .rect_filled(track, 0.0, Color32::from_white_alpha(75));
        let fill_w = (size.x * pct.clamp(0.0, 1.0)).max(3.0);
        let fill = Rect::from_min_size(track.min, Vec2::new(fill_w, bar_h));
        ui.painter().rect_filled(fill, 0.0, Color32::from_rgb(255, 0, 0));
    }

    if !duration.is_empty() {
        let is_live = duration.eq_ignore_ascii_case("LIVE");
        let bg = if is_live {
            theme::YT_RED
        } else {
            Color32::from_black_alpha(210)
        };
        let galley = ui.painter().layout_no_wrap(
            duration.to_owned(),
            theme::semibold(11.0),
            Color32::WHITE,
        );
        let pad = vec2(6.0, 2.5);
        let badge_size = galley.size() + pad * 2.0;
        let badge_rect = Rect::from_min_size(
            rect.max - badge_size - vec2(7.0, 7.0),
            badge_size,
        );
        ui.painter().rect_filled(badge_rect, 4.0, bg);
        ui.painter()
            .galley(badge_rect.min + pad, galley, Color32::WHITE);
    }
}

fn join_meta(views: &str, published: &str) -> String {
    match (views.is_empty(), published.is_empty()) {
        (false, false) => format!("{views} · {published}"),
        (false, true) => views.to_owned(),
        (true, false) => published.to_owned(),
        (true, true) => String::new(),
    }
}

fn parse_duration_text(s: &str) -> Option<f64> {
    let parts: Vec<u64> = s.trim().split(':').filter_map(|p| p.parse().ok()).collect();
    match parts.as_slice() {
        [m, sec] => Some((m * 60 + sec) as f64),
        [h, m, sec] => Some((h * 3600 + m * 60 + sec) as f64),
        _ => None,
    }
}

pub fn format_duration(secs: f64) -> String {
    if secs <= 0.0 || !secs.is_finite() {
        return "0:00".to_owned();
    }
    let total = secs.round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
