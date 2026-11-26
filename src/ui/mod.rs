mod cards;
mod pages;
mod sidebar;
mod topbar;
mod watch;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::glow;
use egui::{Align, Color32, Frame, Key, Layout, Margin, ViewportCommand};

use crate::backend::{Backend, Request, Response};
use crate::innertube;
use crate::model::{
    ChannelInfo, CommentItem, FeedChip, HwdecMode, LastWatched, LiveChatMessage, PlaylistOption,
    PlaylistSummary, PreferredQuality, SidebarChannel, SponsorCategory, SponsorSegment, VideoItem,
    WatchData,
};
use crate::mpv::{GlProcAddrFn, MpvPlayer, PlaybackState};
use crate::theme;
use crate::ytdlp;

use cards::{GridAction, draw_video_grid};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    Home,
    Subscriptions,
    Trending,
    Playlists,
    Playlist,
    Channel,
    Search,
    Watch,
    History,
    Saved,
    Downloads,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RightTab {
    UpNext,
    Queue,
    LiveChat,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PlayerMenu {
    None,
    Settings,
    Captions,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum HudIcon {
    Play,
    Pause,
    SeekBack,
    SeekForward,
    VolumeOn,
    VolumeMuted,
}

pub struct YtApp {
    pub(super) backend: Backend,
    pub(super) mpv: Option<MpvPlayer>,
    pub(super) mpv_error: Option<String>,
    pub(super) get_proc_address: Option<GlProcAddrFn>,
    pub(super) downloading_mpv: bool,

    pub(super) page: Page,
    pub(super) prev_page: Page,
    pub(super) sidebar_expanded: bool,

    pub(super) signed_in: bool,
    pub(super) signing_in: bool,
    pub(super) account_name: Option<String>,
    pub(super) account_avatar: Option<String>,

    pub(super) search_input: String,
    pub(super) active_query: String,
    pub(super) search_suggestions: Vec<String>,
    pub(super) last_suggest_query: String,
    pub(super) last_suggest_edit: Option<Instant>,
    pub(super) search_popup_open: bool,

    pub(super) guide_channels: Vec<SidebarChannel>,
    pub(super) guide_playlists: Vec<PlaylistSummary>,
    pub(super) subs_list_open: bool,
    pub(super) you_section_open: bool,
    pub(super) playlists_section_open: bool,
    pub(super) subs_sidebar_expanded: bool,
    pub(super) playlists_sidebar_expanded: bool,

    pub(super) active_playlist_id: Option<String>,
    pub(super) active_playlist_title: String,
    pub(super) playlist_items: Vec<VideoItem>,
    pub(super) playlist_continuation: Option<String>,
    pub(super) playlist_loading: bool,

    pub(super) home_items: Vec<VideoItem>,
    pub(super) home_chips: Vec<FeedChip>,
    pub(super) home_is_fallback: bool,
    pub(super) home_continuation: Option<String>,
    pub(super) home_loading: bool,
    pub(super) selected_topic_chip: usize,
    pub(super) chips_scroll_x: f32,
    pub(super) chips_scroll_target: f32,
    pub(super) chips_dragging: bool,
    pub(super) chips_drag_dist: f32,

    pub(super) subs_items: Vec<VideoItem>,
    pub(super) subs_continuation: Option<String>,
    pub(super) subs_loading: bool,

    pub(super) trending_items: Vec<VideoItem>,
    pub(super) trending_loading: bool,

    pub(super) channel_id: Option<String>,
    pub(super) channel_name: String,
    pub(super) channel_info: Option<ChannelInfo>,
    pub(super) channel_tab_params: Option<String>,
    pub(super) channel_items: Vec<VideoItem>,
    pub(super) channel_continuation: Option<String>,
    pub(super) channel_loading: bool,

    pub(super) search_items: Vec<VideoItem>,
    pub(super) search_continuation: Option<String>,
    pub(super) search_loading: bool,

    pub(super) history: Vec<VideoItem>,
    pub(super) remote_history: Vec<VideoItem>,
    pub(super) remote_history_continuation: Option<String>,
    pub(super) remote_history_loading: bool,
    pub(super) saved: Vec<VideoItem>,
    pub(super) queue: Vec<VideoItem>,
    pub(super) resume_positions: HashMap<String, f64>,
    pub(super) pending_resume: Option<(String, f64, Option<f32>)>,

    pub(super) watch_loading: bool,
    pub(super) watch: Option<WatchData>,
    pub(super) selected_format_idx: Option<usize>,
    pub(super) selected_audio_idx: usize,
    pub(super) selected_caption_idx: Option<usize>,
    pub(super) used_ytdlp: bool,
    pub(super) sponsor_segments: Vec<SponsorSegment>,
    pub(super) last_skipped_end: f64,

    pub(super) comments: Vec<CommentItem>,
    pub(super) comments_next_token: Option<String>,
    pub(super) comments_loading: bool,
    pub(super) show_chapters: bool,

    pub(super) live_chat: Vec<LiveChatMessage>,
    pub(super) live_chat_token: Option<String>,
    pub(super) live_chat_next_poll: Option<Instant>,
    pub(super) right_tab: RightTab,

    pub(super) playback: PlaybackState,
    pub(super) seeking_pos: Option<f64>,
    pub(super) volume: f64,
    pub(super) muted: bool,
    pub(super) speed: f64,
    pub(super) preferred_quality: PreferredQuality,
    pub(super) hwdec_mode: HwdecMode,
    pub(super) download_dir: String,
    pub(super) sponsorblock_enabled: bool,
    pub(super) sync_history: bool,
    pub(super) autoplay_next: bool,
    pub(super) auto_resume_on_start: bool,
    pub(super) theater_mode: bool,
    pub(super) fullscreen: bool,
    pub(super) watch_scrolled_into_pip: bool,
    pub(super) scroll_watch_to_top: bool,
    pub(super) show_description: bool,

    pub(super) last_watched: Option<LastWatched>,
    pub(super) last_watch_saved_at: Instant,

    pub(super) last_pointer_move: Instant,
    pub(super) last_pointer_pos: Option<egui::Pos2>,
    pub(super) player_menu: PlayerMenu,
    pub(super) show_stats_for_nerds: bool,
    pub(super) hud_feedback: Option<(HudIcon, String, Instant)>,
    pub(super) pip_pos: Option<egui::Pos2>,
    pub(super) pip_snap_target: Option<egui::Pos2>,

    pub(super) status_toast: Option<(String, Instant)>,
    pub(super) last_error: Option<String>,
    pub(super) playlist_modal_video: Option<VideoItem>,
    pub(super) playlist_modal_opened_at: Option<Instant>,
    pub(super) playlist_options: Vec<PlaylistOption>,
    pub(super) playlist_options_loading: bool,
    pub(super) playlist_filter: String,
    pub(super) new_playlist_title: String,
}

pub(super) const GUEST_TOPICS: &[(&str, Option<&str>)] = &[
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
        let mut auto_resume_on_start = false;
        let mut preferred_quality = PreferredQuality::P1080;
        let mut hwdec_mode = HwdecMode::AutoCopy;
        let mut download_dir = String::new();
        let mut sidebar_expanded = true;
        let mut subs_list_open = true;
        let mut you_section_open = true;
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
            if let Some(ar) = storage.get_string("auto_resume_on_start") {
                auto_resume_on_start = ar == "1";
            }
            if let Some(se) = storage.get_string("sidebar") {
                sidebar_expanded = se != "0";
            }
            if let Some(so) = storage.get_string("subs_list_open") {
                subs_list_open = so != "0";
            }
            if let Some(yo) = storage.get_string("you_section_open") {
                you_section_open = yo != "0";
            }
            if let Some(hw) = storage.get_string("hwdec") {
                hwdec_mode = if hw == "no" {
                    HwdecMode::Software
                } else {
                    HwdecMode::AutoCopy
                };
            }
            if let Some(dd) = storage.get_string("download_dir") {
                download_dir = dd;
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
            p.set_hwdec(hwdec_mode);
        }

        let last_watched = load_last_watched_file(&backend.data_dir);
        if let Some(ref lw) = last_watched {
            if lw.position_secs > 2.0 {
                resume_positions.insert(lw.id.clone(), lw.position_secs);
            }
        }

        let _ = backend.tx.send(Request::LoadHome {
            params: None,
            continuation: None,
        });

        let mut app = Self {
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
            subs_list_open,
            you_section_open,
            playlists_section_open: false,
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
            chips_scroll_target: 0.0,
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
            hwdec_mode,
            download_dir,
            sponsorblock_enabled,
            sync_history,
            autoplay_next,
            auto_resume_on_start,
            theater_mode: false,
            fullscreen: false,
            watch_scrolled_into_pip: false,
            scroll_watch_to_top: false,
            show_description: false,
            last_watched,
            last_watch_saved_at: Instant::now(),
            last_pointer_move: Instant::now(),
            last_pointer_pos: None,
            player_menu: PlayerMenu::None,
            show_stats_for_nerds: false,
            hud_feedback: None,
            pip_pos: None,
            pip_snap_target: None,
            status_toast: None,
            last_error: None,
            playlist_modal_video: None,
            playlist_modal_opened_at: None,
            playlist_options: Vec::new(),
            playlist_options_loading: false,
            playlist_filter: String::new(),
            new_playlist_title: String::new(),
        };
        if app.auto_resume_on_start && app.last_watched.is_some() {
            app.resume_last_watched();
        }
        app
    }

    pub(super) fn toast(&mut self, msg: impl Into<String>) {
        self.status_toast = Some((msg.into(), Instant::now()));
    }

    pub(super) fn trigger_hud(&mut self, ico: HudIcon, msg: impl Into<String>) {
        self.hud_feedback = Some((ico, msg.into(), Instant::now()));
    }

    pub(super) fn resume_last_watched(&mut self) {
        let Some(lw) = self.last_watched.clone() else {
            return;
        };
        let local_path = lw.id.strip_prefix("local_path:").map(PathBuf::from);
        if let Some(path) = local_path {
            if path.is_file() {
                self.open_local_file(&path);
                if lw.position_secs > 2.0 {
                    if let Some(mpv) = &self.mpv {
                        mpv.load_stream(&path.to_string_lossy(), None, Some(lw.position_secs));
                    }
                    self.toast(format!("Resumed at {}", format_duration(lw.position_secs)));
                }
                return;
            }
        }
        self.pending_resume = Some((lw.id.clone(), lw.position_secs, None));
        if lw.position_secs > 2.0 {
            self.resume_positions.insert(lw.id.clone(), lw.position_secs);
        }
        self.open_video_id(&lw.id);
    }

    pub(super) fn clear_last_watched(&mut self) {
        self.last_watched = None;
        let _ = std::fs::remove_file(last_watched_path(&self.backend.data_dir));
    }

    fn flush_last_watched_now(&mut self) {
        let Some(w) = &self.watch else { return };
        if w.is_live {
            return;
        }
        let pos = self.playback.time_pos;
        let dur = self.playback.duration.max(w.duration_secs);
        if dur > 15.0 && pos >= dur * 0.94 {
            self.clear_last_watched();
            return;
        }
        let id = if w.id.starts_with("local:") {
            format!("local_path:{}", w.published_text)
        } else {
            w.id.clone()
        };
        let lw = LastWatched {
            id: id.clone(),
            title: w.title.clone(),
            channel: w.channel.clone(),
            thumbnail_url: if w.id.starts_with("local:") {
                String::new()
            } else {
                format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", w.id)
            },
            position_secs: pos.max(0.0),
            duration_secs: dur.max(0.0),
        };
        self.last_watched = Some(lw.clone());
        save_last_watched_file(&self.backend.data_dir, &lw);
        self.last_watch_saved_at = Instant::now();
    }

    pub(super) fn custom_download_dir(&self) -> Option<PathBuf> {
        let trimmed = self.download_dir.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        }
    }

    pub(super) fn effective_download_dir(&self) -> PathBuf {
        self.custom_download_dir()
            .unwrap_or_else(|| ytdlp::default_downloads_dir(&self.backend.data_dir))
    }

    pub(super) fn start_sign_in(&mut self) {
        if self.signing_in {
            return;
        }
        self.signing_in = true;
        let _ = self.backend.tx.send(Request::SignIn);
    }

    pub(super) fn open_channel(&mut self, channel_id: String, name: String) {
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

    pub(super) fn open_playlist(&mut self, browse_id: String, title: String) {
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

    pub(super) fn open_video(&mut self, item: VideoItem) {
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

    pub(super) fn open_local_file(&mut self, path: &std::path::Path) {
        if self.page != Page::Watch {
            self.prev_page = self.page;
        }
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Local Media")
            .to_owned();
        let path_str = path.to_string_lossy().into_owned();
        self.page = Page::Watch;
        self.watch_scrolled_into_pip = false;
        self.scroll_watch_to_top = true;
        self.watch_loading = false;
        self.last_error = None;
        self.sponsor_segments.clear();
        self.last_skipped_end = -1.0;
        self.seeking_pos = None;
        self.selected_format_idx = None;
        self.selected_audio_idx = 0;
        self.selected_caption_idx = None;
        self.used_ytdlp = false;
        self.player_menu = PlayerMenu::None;
        self.last_pointer_move = Instant::now();
        self.comments.clear();
        self.comments_next_token = None;
        self.comments_loading = false;
        self.show_chapters = false;
        self.live_chat.clear();
        self.live_chat_token = None;
        self.live_chat_next_poll = None;
        self.right_tab = RightTab::UpNext;
        self.watch = Some(WatchData {
            id: format!("local:{}", title),
            title,
            channel: "Offline Download".to_owned(),
            channel_id: None,
            channel_avatar_url: None,
            subscribers_text: String::new(),
            views_text: "Local file".to_owned(),
            published_text: path_str.clone(),
            likes_text: String::new(),
            is_liked: false,
            is_disliked: false,
            is_subscribed: false,
            description: path_str.clone(),
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
        if let Some(mpv) = &self.mpv {
            mpv.load_stream(&path_str, None, None);
            mpv.set_speed(self.speed);
        }
    }

    pub(super) fn open_video_id(&mut self, video_id: &str) {
        if self.page != Page::Watch {
            self.prev_page = self.page;
        }
        self.page = Page::Watch;
        self.watch_scrolled_into_pip = false;
        self.scroll_watch_to_top = true;
        self.watch_loading = true;
        self.last_error = None;
        self.sponsor_segments.clear();
        self.last_skipped_end = -1.0;
        self.seeking_pos = None;
        self.selected_audio_idx = 0;
        self.selected_caption_idx = None;
        self.player_menu = PlayerMenu::None;
        self.last_pointer_move = Instant::now();
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

    pub(super) fn submit_search(&mut self) {
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

    pub(super) fn switch_quality(&mut self, idx: usize) {
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

    pub(super) fn switch_audio_track(&mut self, idx: usize) {
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

    pub(super) fn open_playlist_picker(&mut self, video: VideoItem) {
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
        self.playlist_modal_opened_at = Some(Instant::now());
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
            GridAction::PlayNext(it) => {
                self.queue.retain(|x| x.id != it.id);
                let title = it.title.clone();
                self.queue.insert(0, it);
                self.toast(format!("Playing next: {title}"));
            }
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
                    output_dir: self.custom_download_dir(),
                });
            }
            GridAction::CopyUrl(id) => {
                ctx.copy_text(format!("https://www.youtube.com/watch?v={id}"));
                self.toast("Copied YouTube URL");
            }
            GridAction::LoadMore => load_more(self),
        }
    }

    pub(super) fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
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
                    if let Some(first_thumb) = items.first().map(|it| it.thumbnail_url.clone()) {
                        if let Some(gp) = self
                            .guide_playlists
                            .iter_mut()
                            .find(|p| p.browse_id == browse_id)
                        {
                            if gp.thumbnail_url.is_none() {
                                gp.thumbnail_url = Some(first_thumb);
                            }
                        }
                    }
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
                    self.toast(if liked {
                        "Added to Liked videos"
                    } else {
                        "Removed like"
                    });
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
                    self.toast(if disliked {
                        "Disliked video"
                    } else {
                        "Removed dislike"
                    });
                }
                Response::WatchLaterUpdated => {}
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
                                    .find(|g| {
                                        g.browse_id
                                            .strip_prefix("VL")
                                            .unwrap_or(&g.browse_id)
                                            == opt.id.as_str()
                                    })
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
                    self.toast(if subscribed {
                        "Subscribed"
                    } else {
                        "Unsubscribed"
                    });
                }
                Response::VideoDownloaded { title, result } => match result {
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
                                        player.set_hwdec(self.hwdec_mode);
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
        let mut hud_msg: Option<(HudIcon, &'static str)> = None;
        let mut wake_controls = false;
        ctx.input(|i| {
            if i.key_pressed(Key::Escape) {
                if self.player_menu != PlayerMenu::None {
                    self.player_menu = PlayerMenu::None;
                } else if self.fullscreen {
                    self.fullscreen = false;
                    ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
                }
            }
            if self.page != Page::Watch {
                return;
            }
            if i.modifiers.ctrl && i.key_pressed(Key::I) {
                self.show_stats_for_nerds = !self.show_stats_for_nerds;
            }
            if i.key_pressed(Key::Space) || i.key_pressed(Key::K) {
                if let Some(m) = &self.mpv {
                    m.toggle_pause();
                    hud_msg = Some(if self.playback.paused {
                        (HudIcon::Play, "Playing")
                    } else {
                        (HudIcon::Pause, "Paused")
                    });
                    wake_controls = true;
                }
            }
            if i.key_pressed(Key::ArrowLeft) {
                if let Some(m) = &self.mpv {
                    m.seek_rel(-5.0);
                    hud_msg = Some((HudIcon::SeekBack, "-5s"));
                    wake_controls = true;
                }
            }
            if i.key_pressed(Key::ArrowRight) {
                if let Some(m) = &self.mpv {
                    m.seek_rel(5.0);
                    hud_msg = Some((HudIcon::SeekForward, "+5s"));
                    wake_controls = true;
                }
            }
            if i.key_pressed(Key::J) {
                if let Some(m) = &self.mpv {
                    m.seek_rel(-10.0);
                    hud_msg = Some((HudIcon::SeekBack, "-10s"));
                    wake_controls = true;
                }
            }
            if i.key_pressed(Key::L) {
                if let Some(m) = &self.mpv {
                    m.seek_rel(10.0);
                    hud_msg = Some((HudIcon::SeekForward, "+10s"));
                    wake_controls = true;
                }
            }
            if i.key_pressed(Key::M) {
                self.muted = !self.muted;
                if let Some(m) = &self.mpv {
                    m.set_mute(self.muted);
                }
                hud_msg = Some(if self.muted {
                    (HudIcon::VolumeMuted, "Muted")
                } else {
                    (HudIcon::VolumeOn, "Unmuted")
                });
                wake_controls = true;
            }
            if i.key_pressed(Key::T) && !self.fullscreen {
                self.theater_mode = !self.theater_mode;
            }
            if i.key_pressed(Key::F) {
                self.fullscreen = !self.fullscreen;
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
                wake_controls = true;
            }
            if i.key_pressed(Key::ArrowUp) {
                self.volume = (self.volume + 5.0).min(100.0);
                if let Some(m) = &self.mpv {
                    m.set_volume(self.volume);
                }
                wake_controls = true;
            }
            if i.key_pressed(Key::ArrowDown) {
                self.volume = (self.volume - 5.0).max(0.0);
                if let Some(m) = &self.mpv {
                    m.set_volume(self.volume);
                }
                wake_controls = true;
            }
        });
        if let Some((ico, msg)) = hud_msg {
            self.trigger_hud(ico, msg);
        }
        if wake_controls {
            self.last_pointer_move = Instant::now();
        }
    }

    fn poll_playback_and_sponsorblock(&mut self, ctx: &egui::Context) {
        if self.watch.is_none() || self.mpv.is_none() {
            return;
        }
        let was_eof = self.playback.eof;
        if let Some(mpv) = &self.mpv {
            self.playback = mpv.poll_state();
        }
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
            if self.last_watch_saved_at.elapsed() >= Duration::from_secs(4) {
                self.flush_last_watched_now();
            }
        }

        if !self.playback.paused && self.page == Page::Watch {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

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
                    if let Some(mpv) = &self.mpv {
                        mpv.seek_abs(seg.end);
                    }
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
        storage.set_string(
            "sponsorblock",
            if self.sponsorblock_enabled { "1" } else { "0" }.into(),
        );
        storage.set_string(
            "sync_history",
            if self.sync_history { "1" } else { "0" }.into(),
        );
        storage.set_string(
            "autoplay",
            if self.autoplay_next { "1" } else { "0" }.into(),
        );
        storage.set_string(
            "auto_resume_on_start",
            if self.auto_resume_on_start { "1" } else { "0" }.into(),
        );
        storage.set_string(
            "sidebar",
            if self.sidebar_expanded { "1" } else { "0" }.into(),
        );
        storage.set_string(
            "subs_list_open",
            if self.subs_list_open { "1" } else { "0" }.into(),
        );
        storage.set_string(
            "you_section_open",
            if self.you_section_open { "1" } else { "0" }.into(),
        );
        storage.set_string("hwdec", self.hwdec_mode.mpv_value().into());
        storage.set_string("download_dir", self.download_dir.clone());
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
        self.flush_last_watched_now();
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

        if !self.fullscreen {
            self.draw_top_bar(ui);
            self.draw_sidebar(ui);
        }

        let central_margin = if self.fullscreen && self.page == Page::Watch {
            Margin::ZERO
        } else if self.page == Page::Watch {
            Margin {
                left: 6,
                right: 16,
                top: 10,
                bottom: 10,
            }
        } else {
            Margin {
                left: 12,
                right: 18,
                top: 12,
                bottom: 14,
            }
        };

        egui::CentralPanel::default()
            .frame(
                Frame::central_panel(ui.style())
                    .fill(theme::BG)
                    .inner_margin(central_margin),
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
                    Page::Playlists => self.draw_playlists_library_page(ui),
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
                    Page::Downloads => self.draw_downloads_page(ui),
                    Page::Settings => self.draw_settings_page(ui),
                    Page::Watch => self.draw_watch_page(ui, &ctx),
                }
            });

        if !self.fullscreen
            && self.watch.is_some()
            && (self.page != Page::Watch || self.watch_scrolled_into_pip)
        {
            self.draw_mini_player(&ctx);
        }
        self.draw_playlist_picker_modal(&ctx);
        self.draw_toast_snackbar(&ctx);
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

fn last_watched_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("last_watch.json")
}

fn load_last_watched_file(data_dir: &std::path::Path) -> Option<LastWatched> {
    let raw = std::fs::read_to_string(last_watched_path(data_dir)).ok()?;
    serde_json::from_str::<LastWatched>(&raw).ok()
}

fn save_last_watched_file(data_dir: &std::path::Path, lw: &LastWatched) {
    if let Ok(json) = serde_json::to_string(lw) {
        let dest = last_watched_path(data_dir);
        let tmp = data_dir.join("last_watch.json.tmp");
        if std::fs::write(&tmp, &json).is_ok() {
            if std::fs::rename(&tmp, &dest).is_err() {
                let _ = std::fs::write(&dest, &json);
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }
}
