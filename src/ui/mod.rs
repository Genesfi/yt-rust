mod cards;
mod pages;
mod sidebar;
mod topbar;
mod watch;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::glow;
use egui::{Align, Color32, Frame, Key, Layout, Margin, ViewportCommand};

use crate::backend::{Backend, Request, Response};
use crate::innertube;
use crate::model::{
    ChannelInfo, CommentItem, FeedChip, HwdecMode, LastWatched, LiveChatEmote, LiveChatMessage,
    PlaylistOption, PlaylistSummary, PreferredQuality, SidebarChannel, SponsorCategory,
    SponsorSegment, VideoItem, WatchData,
};
use crate::mpv::{GlProcAddrFn, MpvPlayer, PlaybackState};
use crate::os_media::OsMedia;
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
pub(super) enum SubsFilter {
    All,
    Unwatched,
    Live,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum NavEntry {
    Home,
    Subscriptions,
    Trending,
    Playlists,
    Playlist { browse_id: String, title: String },
    Channel { channel_id: String, name: String },
    Search { query: String },
    Watch { video_id: String },
    History,
    Saved,
    Downloads,
    Settings,
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

#[derive(Clone, Debug)]
pub(super) struct ActiveDownload {
    pub video_id: String,
    pub title: String,
    pub audio_only: bool,
    pub percent: f32,
    pub speed: String,
    pub total_size: String,
    pub status: String,
}

pub struct YtApp {
    pub(super) backend: Backend,
    pub(super) mpv: Option<MpvPlayer>,
    pub(super) os_media: Option<OsMedia>,
    pub(super) mpv_error: Option<String>,
    pub(super) get_proc_address: Option<GlProcAddrFn>,
    pub(super) downloading_mpv: bool,

    pub(super) page: Page,
    pub(super) prev_page: Page,
    pub(super) active_watch_id: Option<String>,
    pub(super) nav_back: Vec<NavEntry>,
    pub(super) nav_forward: Vec<NavEntry>,
    pub(super) last_nav: NavEntry,
    pub(super) nav_applying: bool,
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
    pub(super) playlist_is_editable: bool,
    pub(super) playlist_manage_mode: bool,
    pub(super) playlist_drag_idx: Option<usize>,
    pub(super) playlist_drag_origin: Option<(egui::Pos2, Instant)>,
    pub(super) playlist_drag_active: bool,

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
    pub(super) subs_filter: SubsFilter,

    pub(super) trending_items: Vec<VideoItem>,
    pub(super) trending_loading: bool,

    pub(super) channel_id: Option<String>,
    pub(super) channel_name: String,
    pub(super) channel_info: Option<ChannelInfo>,
    pub(super) channel_tab_params: Option<String>,
    pub(super) channel_items: Vec<VideoItem>,
    pub(super) channel_continuation: Option<String>,
    pub(super) channel_loading: bool,
    pub(super) channel_search_input: String,
    pub(super) channel_search_active: String,

    pub(super) search_items: Vec<VideoItem>,
    pub(super) search_continuation: Option<String>,
    pub(super) search_loading: bool,

    pub(super) history: Vec<VideoItem>,
    pub(super) remote_history: Vec<VideoItem>,
    pub(super) remote_history_continuation: Option<String>,
    pub(super) remote_history_loading: bool,
    pub(super) saved: Vec<VideoItem>,
    pub(super) queue: Vec<VideoItem>,
    pub(super) queue_drag_idx: Option<usize>,
    pub(super) queue_drag_origin: Option<(egui::Pos2, Instant)>,
    pub(super) queue_drag_active: bool,
    pub(super) save_queue_open: bool,
    pub(super) save_queue_title: String,
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
    pub(super) comment_sort_options: Vec<(String, String)>,
    pub(super) selected_comment_sort: usize,
    pub(super) comment_create_params: Option<String>,
    pub(super) comment_draft: String,
    pub(super) comment_reply_target_id: Option<String>,
    pub(super) comment_reply_target_author: String,
    pub(super) comment_reply_draft: String,
    pub(super) show_chapters: bool,

    pub(super) live_chat: Vec<LiveChatMessage>,
    pub(super) live_chat_token: Option<String>,
    pub(super) live_chat_next_poll: Option<Instant>,
    pub(super) live_chat_send_params: Option<String>,
    pub(super) live_chat_draft: String,
    pub(super) live_chat_emotes: Vec<LiveChatEmote>,
    pub(super) live_chat_is_replay: bool,
    pub(super) live_chat_replay_initial_token: Option<String>,
    pub(super) live_chat_last_replay_pos: f64,
    pub(super) live_chat_emote_picker_open: bool,
    pub(super) right_tab: RightTab,
    pub(super) shorts_mode: bool,
    pub(super) shorts_queue: Vec<VideoItem>,
    pub(super) shorts_idx: usize,
    pub(super) history_feedback_tokens: HashMap<String, String>,
    pub(super) active_downloads: Vec<ActiveDownload>,

    pub(super) playback: PlaybackState,
    pub(super) watch_started_at: Option<Instant>,
    pub(super) active_stream_urls: (String, Option<String>),
    pub(super) stream_recovery_attempts: u8,
    pub(super) stream_recovering: Option<String>,
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
    pub(super) channel_avatars: HashMap<String, String>,
    pub(super) avatar_requested: HashSet<String>,
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

        let wake_ctx = cc.egui_ctx.clone();
        let os_media = OsMedia::new(cc, Arc::new(move || wake_ctx.request_repaint()));

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

        let mut queue = load_queue_file(&backend.data_dir);
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
            os_media,
            mpv_error,
            get_proc_address,
            downloading_mpv: false,
            page: Page::Home,
            prev_page: Page::Home,
            active_watch_id: None,
            nav_back: Vec::new(),
            nav_forward: Vec::new(),
            last_nav: NavEntry::Home,
            nav_applying: false,
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
            playlist_is_editable: false,
            playlist_manage_mode: false,
            playlist_drag_idx: None,
            playlist_drag_origin: None,
            playlist_drag_active: false,
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
            subs_filter: SubsFilter::All,
            trending_items: Vec::new(),
            trending_loading: false,
            channel_id: None,
            channel_name: String::new(),
            channel_info: None,
            channel_tab_params: None,
            channel_items: Vec::new(),
            channel_continuation: None,
            channel_loading: false,
            channel_search_input: String::new(),
            channel_search_active: String::new(),
            search_items: Vec::new(),
            search_continuation: None,
            search_loading: false,
            history,
            remote_history: Vec::new(),
            remote_history_continuation: None,
            remote_history_loading: false,
            saved,
            queue: Vec::new(),
            queue_drag_idx: None,
            queue_drag_origin: None,
            queue_drag_active: false,
            save_queue_open: false,
            save_queue_title: String::new(),
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
            comment_sort_options: Vec::new(),
            selected_comment_sort: 0,
            comment_create_params: None,
            comment_draft: String::new(),
            comment_reply_target_id: None,
            comment_reply_target_author: String::new(),
            comment_reply_draft: String::new(),
            show_chapters: false,
            live_chat: Vec::new(),
            live_chat_token: None,
            live_chat_next_poll: None,
            live_chat_send_params: None,
            live_chat_draft: String::new(),
            live_chat_emotes: Vec::new(),
            live_chat_is_replay: false,
            live_chat_replay_initial_token: None,
            live_chat_last_replay_pos: 0.0,
            live_chat_emote_picker_open: false,
            shorts_mode: false,
            shorts_queue: Vec::new(),
            shorts_idx: 0,
            history_feedback_tokens: HashMap::new(),
            active_downloads: Vec::new(),
            channel_avatars: HashMap::new(),
            avatar_requested: HashSet::new(),
            right_tab: RightTab::UpNext,
            playback: PlaybackState::default(),
            watch_started_at: None,
            active_stream_urls: (String::new(), None),
            stream_recovery_attempts: 0,
            stream_recovering: None,
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
        if !queue.is_empty() {
            app.ingest_and_request_avatars(&mut queue);
            app.queue = queue;
        }
        if app.auto_resume_on_start && app.last_watched.is_some() {
            app.resume_last_watched();
        }
        app.last_nav = app.current_nav_entry();
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
        if !self.queue.is_empty() {
            self.right_tab = RightTab::Queue;
        }
    }

    pub(super) fn persist_queue(&self) {
        save_queue_file(&self.backend.data_dir, &self.queue);
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
        self.channel_search_input.clear();
        self.channel_search_active.clear();
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
        self.playlist_is_editable = false;
        self.playlist_manage_mode = false;
        self.playlist_drag_idx = None;
        self.playlist_drag_origin = None;
        self.playlist_drag_active = false;
        self.last_error = None;
        let _ = self.backend.tx.send(Request::LoadPlaylist {
            browse_id,
            continuation: None,
        });
    }

    pub(super) fn play_all(&mut self, items: &[VideoItem], shuffle: bool, playlist_cont: Option<(String, String)>) {
        let mut list: Vec<VideoItem> = items
            .iter()
            .filter(|it| it.playlist_browse_id.is_none())
            .cloned()
            .collect();
        if list.is_empty() {
            return;
        }
        if shuffle {
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0x9e37_79b9_7f4a_7c15, |d| d.as_nanos() as u64);
            shuffle_vec(&mut list, seed);
        }
        let total_loaded = list.len();
        let first = list.remove(0);
        self.queue = list;
        self.persist_queue();
        self.right_tab = RightTab::Queue;
        self.open_video(first);
        if let Some((browse_id, continuation)) = playlist_cont {
            let _ = self.backend.tx.send(Request::LoadFullPlaylistForQueue {
                browse_id,
                continuation,
                shuffle,
            });
        }
        self.toast(if shuffle {
            format!("Shuffling {total_loaded} videos")
        } else {
            format!("Playing {total_loaded} videos")
        });
    }

    pub(super) fn move_playlist_item(&mut self, from: usize, to: usize) {
        if from >= self.playlist_items.len() || to >= self.playlist_items.len() || from == to {
            return;
        }
        let Some(pid) = self.active_playlist_id.clone() else {
            return;
        };
        let Some(set_id) = self.playlist_items[from].playlist_set_video_id.clone() else {
            return;
        };
        let item = self.playlist_items.remove(from);
        self.playlist_items.insert(to, item);
        let successor = self
            .playlist_items
            .get(to + 1)
            .and_then(|x| x.playlist_set_video_id.clone());
        let _ = self.backend.tx.send(Request::PlaylistMoveItem {
            playlist_id: pid,
            set_video_id: set_id,
            successor_set_video_id: successor,
        });
    }

    pub(super) fn remove_playlist_item(&mut self, item: &VideoItem) {
        let Some(pid) = self.active_playlist_id.clone() else {
            return;
        };
        if let Some(sid) = item.playlist_set_video_id.as_deref() {
            self.playlist_items
                .retain(|x| x.playlist_set_video_id.as_deref() != Some(sid));
        } else {
            self.playlist_items.retain(|x| x.id != item.id);
        }
        let _ = self.backend.tx.send(Request::PlaylistRemoveItem {
            playlist_id: pid,
            video_id: item.id.clone(),
            set_video_id: item.playlist_set_video_id.clone(),
        });
        self.toast(format!("Removed \"{}\"", item.title));
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

    pub(super) fn open_short(&mut self, item: VideoItem, list: Vec<VideoItem>) {
        self.shorts_idx = list.iter().position(|x| x.id == item.id).unwrap_or(0);
        self.shorts_queue = list;
        self.open_video(item);
        self.shorts_mode = true;
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
        self.active_watch_id = Some(format!("local:{}", title));
        self.watch_scrolled_into_pip = false;
        self.scroll_watch_to_top = true;
        self.watch_loading = false;
        self.watch_started_at = None;
        self.active_stream_urls = (path_str.clone(), None);
        self.stream_recovery_attempts = 0;
        self.stream_recovering = None;
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
        self.comment_sort_options.clear();
        self.selected_comment_sort = 0;
        self.comment_create_params = None;
        self.comment_draft.clear();
        self.comment_reply_target_id = None;
        self.comment_reply_target_author.clear();
        self.comment_reply_draft.clear();
        self.shorts_mode = false;
        self.show_chapters = false;
        self.live_chat.clear();
        self.live_chat_token = None;
        self.live_chat_next_poll = None;
        self.live_chat_send_params = None;
        self.live_chat_draft.clear();
        self.live_chat_emotes.clear();
        self.live_chat_is_replay = false;
        self.live_chat_replay_initial_token = None;
        self.live_chat_last_replay_pos = 0.0;
        self.live_chat_emote_picker_open = false;
        if !(self.right_tab == RightTab::Queue && !self.queue.is_empty()) {
            self.right_tab = RightTab::UpNext;
        }
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
        self.active_watch_id = Some(video_id.to_owned());
        self.watch_scrolled_into_pip = false;
        self.scroll_watch_to_top = true;
        self.watch_loading = true;
        self.watch_started_at = None;
        self.active_stream_urls = (String::new(), None);
        self.stream_recovery_attempts = 0;
        self.stream_recovering = None;
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
        self.comment_sort_options.clear();
        self.selected_comment_sort = 0;
        self.comment_create_params = None;
        self.comment_draft.clear();
        self.comment_reply_target_id = None;
        self.comment_reply_target_author.clear();
        self.comment_reply_draft.clear();
        self.shorts_mode = false;
        self.show_chapters = false;
        self.live_chat.clear();
        self.live_chat_token = None;
        self.live_chat_next_poll = None;
        self.live_chat_send_params = None;
        self.live_chat_draft.clear();
        self.live_chat_emotes.clear();
        self.live_chat_is_replay = false;
        self.live_chat_replay_initial_token = None;
        self.live_chat_last_replay_pos = 0.0;
        self.live_chat_emote_picker_open = false;
        if !(self.right_tab == RightTab::Queue && !self.queue.is_empty()) {
            self.right_tab = RightTab::UpNext;
        }
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
            let is_short = q.contains("/shorts/");
            self.open_video_id(&vid);
            if is_short {
                self.shorts_mode = true;
            }
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
            mpv.load_stream(
                &video_url,
                audio.as_deref(),
                (pos > 1.0 && !w.is_live).then_some(pos),
            );
        }
        self.active_stream_urls = (video_url, audio);
        self.selected_format_idx = Some(idx);
        self.toast(format!("Quality: {}", fmt.label()));
    }

    pub(super) fn switch_quality_auto(&mut self) {
        let Some(w) = &self.watch else { return };
        let Some(hls) = w.hls_url.as_deref() else { return };
        let pos = self.playback.time_pos;
        if let Some(mpv) = &self.mpv {
            mpv.load_stream(hls, None, (pos > 1.0 && !w.is_live).then_some(pos));
        }
        self.active_stream_urls = (hls.to_owned(), None);
        self.selected_format_idx = None;
        self.toast("Quality: Auto");
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
                self.active_stream_urls = (video_url, Some(audio));
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

    fn ingest_and_request_avatars(&mut self, items: &mut [VideoItem]) {
        let mut missing = Vec::new();
        for it in items.iter_mut() {
            if let Some(cid) = it.channel_id.as_ref().filter(|c| c.starts_with("UC")) {
                if let Some(av) = &it.channel_avatar_url {
                    self.channel_avatars.insert(cid.clone(), av.clone());
                } else if let Some(cached) = self.channel_avatars.get(cid) {
                    it.channel_avatar_url = Some(cached.clone());
                } else if !self.avatar_requested.contains(cid) && missing.len() < 36 {
                    self.avatar_requested.insert(cid.clone());
                    missing.push(cid.clone());
                }
            }
        }
        if !missing.is_empty() {
            let _ = self
                .backend
                .tx
                .send(Request::ResolveChannelAvatars { channel_ids: missing });
        }
    }

    fn patch_all_channel_avatars(&mut self) {
        let map = &self.channel_avatars;
        let patch = |list: &mut [VideoItem]| {
            for it in list {
                if it.channel_avatar_url.is_none() {
                    if let Some(cid) = &it.channel_id {
                        if let Some(av) = map.get(cid) {
                            it.channel_avatar_url = Some(av.clone());
                        }
                    }
                }
            }
        };
        patch(&mut self.home_items);
        patch(&mut self.subs_items);
        patch(&mut self.playlist_items);
        patch(&mut self.trending_items);
        patch(&mut self.channel_items);
        patch(&mut self.search_items);
        patch(&mut self.remote_history);
        patch(&mut self.history);
        patch(&mut self.saved);
        patch(&mut self.queue);
        if let Some(w) = &mut self.watch {
            patch(&mut w.related);
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

    pub(super) fn start_download(&mut self, video_id: String, title: String, audio_only: bool) {
        let kind = if audio_only { "audio" } else { "video" };
        self.toast(format!("Downloading {kind}: {title}"));
        if let Some(existing) = self
            .active_downloads
            .iter_mut()
            .find(|d| d.video_id == video_id && d.audio_only == audio_only)
        {
            existing.title = title.clone();
            existing.percent = 0.0;
            existing.speed.clear();
            existing.total_size.clear();
            existing.status = "Starting...".to_owned();
        } else {
            self.active_downloads.push(ActiveDownload {
                video_id: video_id.clone(),
                title: title.clone(),
                audio_only,
                percent: 0.0,
                speed: String::new(),
                total_size: String::new(),
                status: "Starting...".to_owned(),
            });
        }
        let _ = self.backend.tx.send(Request::DownloadVideo {
            video_id,
            title,
            audio_only,
            max_height: self.preferred_quality.max_height(),
            output_dir: self.custom_download_dir(),
        });
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
                self.persist_queue();
                self.toast(format!("Playing next: {title}"));
            }
            GridAction::AddQueue(it) => {
                if !self.queue.iter().any(|x| x.id == it.id) {
                    let title = it.title.clone();
                    self.queue.push(it);
                    self.persist_queue();
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
                self.start_download(item.id, item.title, audio_only);
            }
            GridAction::CopyUrl(id) => {
                ctx.copy_text(format!("https://www.youtube.com/watch?v={id}"));
                self.toast("Copied YouTube URL");
            }
            GridAction::RemoveFromHistory(it) => {
                self.history.retain(|x| x.id != it.id);
                self.remote_history.retain(|x| x.id != it.id);
                self.resume_positions.remove(&it.id);
                if self.last_watched.as_ref().is_some_and(|lw| lw.id == it.id) {
                    self.clear_last_watched();
                }
                if self.signed_in && self.sync_history {
                    if let Some(tok) = self.history_feedback_tokens.remove(&it.id) {
                        let _ = self.backend.tx.send(Request::RemoveFromHistory {
                            video_id: it.id,
                            feedback_token: tok,
                        });
                    } else {
                        self.toast("Removed from local view");
                    }
                } else {
                    self.toast("Removed from Watch History");
                }
            }
            GridAction::RemoveFromPlaylist(it) => {
                self.remove_playlist_item(&it);
            }
            GridAction::LoadMore => load_more(self),
        }
    }

    pub(super) fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
    }

    fn current_nav_entry(&self) -> NavEntry {
        match self.page {
            Page::Home => NavEntry::Home,
            Page::Subscriptions => NavEntry::Subscriptions,
            Page::Trending => NavEntry::Trending,
            Page::Playlists => NavEntry::Playlists,
            Page::Playlist => NavEntry::Playlist {
                browse_id: self.active_playlist_id.clone().unwrap_or_default(),
                title: self.active_playlist_title.clone(),
            },
            Page::Channel => NavEntry::Channel {
                channel_id: self.channel_id.clone().unwrap_or_default(),
                name: self.channel_name.clone(),
            },
            Page::Search => NavEntry::Search {
                query: self.active_query.clone(),
            },
            Page::Watch => NavEntry::Watch {
                video_id: self
                    .active_watch_id
                    .clone()
                    .or_else(|| self.watch.as_ref().map(|w| w.id.clone()))
                    .unwrap_or_default(),
            },
            Page::History => NavEntry::History,
            Page::Saved => NavEntry::Saved,
            Page::Downloads => NavEntry::Downloads,
            Page::Settings => NavEntry::Settings,
        }
    }

    fn apply_nav_entry(&mut self, entry: NavEntry) {
        match entry {
            NavEntry::Home => {
                self.page = Page::Home;
                if self.home_items.is_empty() && !self.home_loading {
                    self.home_loading = true;
                    let _ = self.backend.tx.send(Request::LoadHome {
                        params: None,
                        continuation: None,
                    });
                }
            }
            NavEntry::Subscriptions => {
                self.page = Page::Subscriptions;
                if self.signed_in && self.subs_items.is_empty() && !self.subs_loading {
                    self.subs_loading = true;
                    let _ = self
                        .backend
                        .tx
                        .send(Request::LoadSubscriptions { continuation: None });
                }
            }
            NavEntry::Trending => {
                self.page = Page::Trending;
                if self.trending_items.is_empty() && !self.trending_loading {
                    self.trending_loading = true;
                    let _ = self.backend.tx.send(Request::LoadTrending);
                }
            }
            NavEntry::Playlists => {
                self.page = Page::Playlists;
            }
            NavEntry::Playlist { browse_id, title } => {
                if self.active_playlist_id.as_deref() == Some(browse_id.as_str())
                    && !self.playlist_items.is_empty()
                {
                    self.page = Page::Playlist;
                } else if !browse_id.is_empty() {
                    self.open_playlist(browse_id, title);
                }
            }
            NavEntry::Channel { channel_id, name } => {
                if self.channel_id.as_deref() == Some(channel_id.as_str())
                    && !self.channel_items.is_empty()
                {
                    self.page = Page::Channel;
                } else if !channel_id.is_empty() {
                    self.open_channel(channel_id, name);
                }
            }
            NavEntry::Search { query } => {
                if self.active_query == query && !self.search_items.is_empty() {
                    self.page = Page::Search;
                } else if !query.is_empty() {
                    self.search_input = query.clone();
                    self.active_query = query.clone();
                    self.page = Page::Search;
                    self.search_items.clear();
                    self.search_continuation = None;
                    self.search_loading = true;
                    let _ = self.backend.tx.send(Request::Search {
                        query,
                        continuation: None,
                    });
                }
            }
            NavEntry::Watch { video_id } => {
                if self.watch.as_ref().is_some_and(|w| w.id == video_id)
                    && self.active_watch_id.as_deref() == Some(video_id.as_str())
                {
                    self.page = Page::Watch;
                    self.watch_scrolled_into_pip = false;
                    self.scroll_watch_to_top = true;
                } else if !video_id.is_empty() && !video_id.starts_with("local:") {
                    self.open_video_id(&video_id);
                } else if self.watch.is_some() {
                    self.page = Page::Watch;
                }
            }
            NavEntry::History => {
                self.page = Page::History;
                if self.signed_in
                    && self.sync_history
                    && self.remote_history.is_empty()
                    && !self.remote_history_loading
                {
                    self.remote_history_loading = true;
                    let _ = self
                        .backend
                        .tx
                        .send(Request::LoadHistory { continuation: None });
                }
            }
            NavEntry::Saved => self.page = Page::Saved,
            NavEntry::Downloads => self.page = Page::Downloads,
            NavEntry::Settings => self.page = Page::Settings,
        }
    }

    pub(super) fn nav_go_back(&mut self) {
        if let Some(target) = self.nav_back.pop() {
            let cur = self.current_nav_entry();
            self.nav_forward.push(cur);
            self.nav_applying = true;
            self.apply_nav_entry(target);
        } else if self.page == Page::Watch {
            self.page = self.prev_page;
        }
    }

    pub(super) fn nav_go_forward(&mut self) {
        if let Some(target) = self.nav_forward.pop() {
            let cur = self.current_nav_entry();
            self.nav_back.push(cur);
            self.nav_applying = true;
            self.apply_nav_entry(target);
        }
    }

    fn record_nav_transition(&mut self) {
        let cur = self.current_nav_entry();
        if cur != self.last_nav {
            if !self.nav_applying {
                if self.nav_back.len() >= 50 {
                    self.nav_back.remove(0);
                }
                self.nav_back.push(self.last_nav.clone());
                self.nav_forward.clear();
            }
            self.last_nav = cur;
        }
        self.nav_applying = false;
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
                    self.ingest_and_request_avatars(&mut items);
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
                    self.ingest_and_request_avatars(&mut items);
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
                    feedback_tokens,
                } => {
                    self.remote_history_loading = false;
                    self.ingest_resume_metadata(&mut items);
                    self.ingest_and_request_avatars(&mut items);
                    if !append {
                        self.history_feedback_tokens = feedback_tokens;
                    } else {
                        self.history_feedback_tokens.extend(feedback_tokens);
                    }
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
                    for ch in &channels {
                        if let Some(av) = &ch.avatar_url {
                            self.channel_avatars.insert(ch.browse_id.clone(), av.clone());
                        }
                    }
                    self.guide_channels = channels;
                    self.guide_playlists = playlists;
                    self.patch_all_channel_avatars();
                }
                Response::PlaylistLoaded {
                    browse_id,
                    title,
                    is_editable,
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
                        self.playlist_is_editable = is_editable;
                        self.ingest_resume_metadata(&mut items);
                        self.ingest_and_request_avatars(&mut items);
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
                Response::FullPlaylistQueueLoaded { mut items, shuffle } => {
                    self.ingest_resume_metadata(&mut items);
                    self.ingest_and_request_avatars(&mut items);
                    let cur_id = self.watch.as_ref().map(|w| w.id.as_str());
                    items.retain(|it| {
                        it.playlist_browse_id.is_none()
                            && cur_id != Some(it.id.as_str())
                            && !self.queue.iter().any(|q| q.id == it.id)
                    });
                    if shuffle {
                        let seed = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0x9e37_79b9_7f4a_7c15, |d| d.as_nanos() as u64);
                        shuffle_vec(&mut items, seed);
                    }
                    if !items.is_empty() {
                        self.queue.extend(items);
                        self.persist_queue();
                    }
                }
                Response::PlaylistItemRemoved | Response::PlaylistReordered => {}
                Response::TrendingLoaded { mut items } => {
                    self.trending_loading = false;
                    self.ingest_resume_metadata(&mut items);
                    self.ingest_and_request_avatars(&mut items);
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
                Response::ChannelSearchLoaded {
                    channel_id,
                    query,
                    mut items,
                    continuation,
                    append,
                } => {
                    if self.channel_id.as_deref() == Some(channel_id.as_str())
                        && self.channel_search_active == query
                    {
                        self.channel_loading = false;
                        self.ingest_resume_metadata(&mut items);
                        self.ingest_and_request_avatars(&mut items);
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
                        self.ingest_and_request_avatars(&mut items);
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
                    mut data,
                    selected_video_url,
                    selected_audio_url,
                    selected_format_idx,
                    used_ytdlp,
                } => {
                    if let (Some(cid), Some(av)) = (&data.channel_id, &data.channel_avatar_url) {
                        self.channel_avatars.insert(cid.clone(), av.clone());
                    }
                    self.ingest_and_request_avatars(&mut data.related);
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
                    self.active_stream_urls = (selected_video_url, selected_audio_url);
                    self.watch_started_at = Some(Instant::now());
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
                        playlist_set_video_id: None,
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
                        self.live_chat_replay_initial_token = Some(chat_tok.clone());
                        self.live_chat_is_replay = !data.is_live;
                        self.live_chat_last_replay_pos = start_at.unwrap_or(0.0);
                        self.live_chat_next_poll = Some(Instant::now() + Duration::from_secs(3));
                        if data.is_live {
                            self.right_tab = RightTab::LiveChat;
                        }
                        let replay_offset_ms = (!data.is_live)
                            .then(|| (start_at.unwrap_or(0.0).max(0.0) * 1000.0) as u64);
                        let _ = self.backend.tx.send(Request::LoadLiveChat {
                            video_id: data.id.clone(),
                            continuation: chat_tok,
                            replay_offset_ms,
                        });
                    }
                    self.watch = Some(*data);
                }
                Response::StreamRecovered {
                    video_id,
                    video_formats,
                    audio_url,
                    audio_content_length,
                    audio_tracks,
                    hls_url,
                    selected_video_url,
                    selected_audio_url,
                    selected_format_idx,
                    selected_audio_idx,
                    used_ytdlp,
                    resume_secs,
                } => {
                    self.stream_recovering = None;
                    if let Some(w) = self.watch.as_mut().filter(|w| w.id == video_id) {
                        w.video_formats = video_formats;
                        w.audio_url = audio_url;
                        w.audio_content_length = audio_content_length;
                        w.audio_tracks = audio_tracks;
                        w.hls_url = hls_url;
                        self.selected_format_idx = selected_format_idx;
                        self.selected_audio_idx = selected_audio_idx;
                        self.used_ytdlp = used_ytdlp;
                        self.last_error = None;
                        let start_at = (resume_secs > 1.0 && !w.is_live).then_some(resume_secs);
                        if let Some(mpv) = &self.mpv {
                            mpv.load_stream(
                                &selected_video_url,
                                selected_audio_url.as_deref(),
                                start_at,
                            );
                            mpv.set_speed(self.speed);
                        }
                        self.active_stream_urls = (selected_video_url, selected_audio_url);
                        self.watch_started_at = Some(Instant::now());
                        self.toast("Refreshed expired stream");
                    }
                }
                Response::StreamRecoverFailed { video_id, message } => {
                    self.stream_recovering = None;
                    if self.watch.as_ref().is_some_and(|w| w.id == video_id) {
                        self.last_error = Some(message);
                    }
                }
                Response::CommentsLoaded {
                    video_id,
                    comments,
                    next_token,
                    sort_options,
                    create_params,
                    append,
                } => {
                    if self.watch.as_ref().is_some_and(|w| w.id == video_id) {
                        self.comments_loading = false;
                        if !sort_options.is_empty() {
                            self.comment_sort_options = sort_options;
                        }
                        if create_params.is_some() {
                            self.comment_create_params = create_params;
                        }
                        if append {
                            self.comments.extend(comments);
                        } else {
                            self.comments = comments;
                        }
                        self.comments_next_token = next_token;
                    }
                }
                Response::CommentRepliesLoaded {
                    video_id,
                    comment_id,
                    replies,
                    next_token,
                } => {
                    if self.watch.as_ref().is_some_and(|w| w.id == video_id) {
                        if let Some(c) = self.comments.iter_mut().find(|c| c.id == comment_id) {
                            c.replies.extend(replies);
                            c.replies_token = next_token;
                            c.replies_loading = false;
                        }
                    }
                }
                Response::CommentPosted { video_id } => {
                    if self.watch.as_ref().is_some_and(|w| w.id == video_id) {
                        self.toast("Comment posted");
                    }
                }
                Response::CommentReplyPosted {
                    video_id,
                    comment_id,
                    text,
                } => {
                    if self.watch.as_ref().is_some_and(|w| w.id == video_id) {
                        if let Some(c) = self.comments.iter_mut().find(|c| c.id == comment_id) {
                            c.replies.push(CommentItem {
                                id: format!("local-reply-{}", c.replies.len()),
                                author: self
                                    .account_name
                                    .clone()
                                    .unwrap_or_else(|| "You".to_owned()),
                                avatar_url: self.account_avatar.clone(),
                                published: "Just now".to_owned(),
                                likes: String::new(),
                                text,
                                reply_count_text: String::new(),
                                replies_token: None,
                                reply_params: None,
                                replies: Vec::new(),
                                replies_loading: false,
                                is_liked: false,
                                like_action_params: None,
                                unlike_action_params: None,
                                is_pinned: false,
                                has_creator_heart: false,
                            });
                        }
                        self.toast("Reply posted");
                    }
                }
                Response::CommentActionUpdated {
                    video_id,
                    comment_id,
                    liked,
                } => {
                    if self.watch.as_ref().is_some_and(|w| w.id == video_id) {
                        for c in &mut self.comments {
                            if c.id == comment_id {
                                c.is_liked = liked;
                            }
                            for r in &mut c.replies {
                                if r.id == comment_id {
                                    r.is_liked = liked;
                                }
                            }
                        }
                    }
                }
                Response::LiveChatLoaded {
                    video_id,
                    messages,
                    next_token,
                    send_params,
                    emotes,
                    is_replay,
                } => {
                    if self.watch.as_ref().is_some_and(|w| w.id == video_id) {
                        if send_params.is_some() {
                            self.live_chat_send_params = send_params;
                        }
                        self.live_chat_is_replay |= is_replay;
                        for em in emotes {
                            if !self
                                .live_chat_emotes
                                .iter()
                                .any(|x| x.emoji_id == em.emoji_id)
                            {
                                self.live_chat_emotes.push(em);
                            }
                        }
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
                Response::LiveChatSent { video_id } => {
                    if self.watch.as_ref().is_some_and(|w| w.id == video_id) {
                        self.live_chat_next_poll = Some(Instant::now());
                    }
                }
                Response::HistoryRemoved => {
                    self.toast("Removed from Watch History");
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
                Response::PlaylistThumbsResolved(resolved) => {
                    for gp in &mut self.guide_playlists {
                        let key = gp.browse_id.strip_prefix("VL").unwrap_or(&gp.browse_id);
                        if gp.thumbnail_url.is_none() {
                            if let Some(url) = resolved.get(key) {
                                gp.thumbnail_url = Some(url.clone());
                            }
                        }
                    }
                    for opt in &mut self.playlist_options {
                        if opt.thumbnail_url.is_none() {
                            if let Some(url) = resolved.get(&opt.id) {
                                opt.thumbnail_url = Some(url.clone());
                            }
                        }
                    }
                }
                Response::ChannelAvatarsResolved(resolved) => {
                    for (cid, av) in resolved {
                        self.channel_avatars.insert(cid, av);
                    }
                    self.patch_all_channel_avatars();
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
                Response::DownloadProgress {
                    video_id,
                    audio_only,
                    progress,
                } => {
                    if let Some(dl) = self
                        .active_downloads
                        .iter_mut()
                        .find(|d| d.video_id == video_id && d.audio_only == audio_only)
                    {
                        dl.percent = progress.percent.max(dl.percent);
                        if !progress.speed.is_empty() {
                            dl.speed = progress.speed;
                        }
                        if !progress.total_size.is_empty() {
                            dl.total_size = progress.total_size;
                        }
                        if !progress.status.is_empty() {
                            dl.status = progress.status;
                        }
                    }
                }
                Response::VideoDownloaded {
                    video_id,
                    audio_only,
                    title,
                    result,
                } => {
                    self.active_downloads
                        .retain(|d| !(d.video_id == video_id && d.audio_only == audio_only));
                    match result {
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
                    }
                }
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
        let (mouse_back, mouse_forward, alt_back, alt_forward) = ctx.input(|i| {
            (
                i.pointer.button_pressed(egui::PointerButton::Extra1),
                i.pointer.button_pressed(egui::PointerButton::Extra2),
                i.modifiers.alt && i.key_pressed(Key::ArrowLeft),
                i.modifiers.alt && i.key_pressed(Key::ArrowRight),
            )
        });
        if mouse_back {
            self.nav_go_back();
            return;
        }
        if mouse_forward {
            self.nav_go_forward();
            return;
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        if alt_back {
            self.nav_go_back();
            return;
        }
        if alt_forward {
            self.nav_go_forward();
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

    fn handle_os_media(&mut self) {
        #[cfg(windows)]
        {
            use souvlaki::MediaControlEvent;
            let mut events = Vec::new();
            if let Some(om) = &self.os_media {
                while let Some(ev) = om.try_recv() {
                    events.push(ev);
                }
            }
            for ev in events {
                match ev {
                    MediaControlEvent::Play => {
                        if let Some(m) = &self.mpv {
                            m.set_paused(false);
                            self.trigger_hud(HudIcon::Play, "Playing");
                        }
                    }
                    MediaControlEvent::Pause | MediaControlEvent::Stop => {
                        if let Some(m) = &self.mpv {
                            m.set_paused(true);
                            self.trigger_hud(HudIcon::Pause, "Paused");
                        }
                    }
                    MediaControlEvent::Toggle => {
                        if let Some(m) = &self.mpv {
                            m.toggle_pause();
                            self.trigger_hud(
                                if self.playback.paused {
                                    HudIcon::Play
                                } else {
                                    HudIcon::Pause
                                },
                                if self.playback.paused {
                                    "Playing"
                                } else {
                                    "Paused"
                                },
                            );
                        }
                    }
                    MediaControlEvent::Next => {
                        if !self.queue.is_empty() {
                            let next = self.queue.remove(0);
                            self.persist_queue();
                            self.open_video(next);
                        } else if let Some(next) =
                            self.watch.as_ref().and_then(|w| w.related.first()).cloned()
                        {
                            self.open_video(next);
                        }
                    }
                    MediaControlEvent::Previous => {
                        if self.playback.time_pos > 5.0 || self.history.len() < 2 {
                            if let Some(m) = &self.mpv {
                                m.seek_abs(0.0);
                                self.trigger_hud(HudIcon::SeekBack, "Restart");
                            }
                        } else {
                            let prev = self.history[1].clone();
                            self.open_video(prev);
                        }
                    }
                    _ => {
                        if let Some(target) = crate::os_media::seek_target(
                            &ev,
                            self.playback.time_pos,
                            self.playback.duration,
                        ) {
                            if let Some(m) = &self.mpv {
                                let icon = if target >= self.playback.time_pos {
                                    HudIcon::SeekForward
                                } else {
                                    HudIcon::SeekBack
                                };
                                m.seek_abs(target);
                                self.trigger_hud(icon, format_duration(target));
                            }
                        }
                    }
                }
            }
        }
    }

    fn poll_playback_and_sponsorblock(&mut self, ctx: &egui::Context) {
        if self.watch.is_none() || self.mpv.is_none() {
            if let Some(om) = &mut self.os_media {
                om.sync_playback(false, true, 0.0);
            }
            return;
        }
        let was_eof = self.playback.eof;
        if let Some(mpv) = &self.mpv {
            self.playback = mpv.poll_state();
        }
        let is_live = self.watch.as_ref().is_some_and(|w| w.is_live);
        let is_remote_watch = self
            .watch
            .as_ref()
            .is_some_and(|w| !w.id.starts_with("local:"));
        let proxy_403 = self.backend.is_proxy_forbidden(&self.active_stream_urls.0)
            || self
                .active_stream_urls
                .1
                .as_deref()
                .is_some_and(|u| self.backend.is_proxy_forbidden(u));
        let proxy_failed = proxy_403
            || self.backend.is_proxy_failed(&self.active_stream_urls.0)
            || self
                .active_stream_urls
                .1
                .as_deref()
                .is_some_and(|u| self.backend.is_proxy_failed(u));
        let start_stalled = self.playback.idle
            && !self.playback.eof
            && self.playback.duration <= 0.0
            && self.playback.video_h == 0
            && self
                .watch_started_at
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(2));
        let mid_eof_drop = !is_live
            && self.playback.eof
            && !was_eof
            && self.playback.duration > 20.0
            && self.playback.time_pos > 1.0
            && self.playback.time_pos + 10.0 < self.playback.duration
            && proxy_failed;

        if !self.watch_loading
            && is_remote_watch
            && self.stream_recovering.is_none()
            && self.last_error.is_none()
            && (proxy_403 || start_stalled || mid_eof_drop)
        {
            if self.stream_recovery_attempts < 2 {
                if let Some(w) = &self.watch {
                    self.stream_recovery_attempts += 1;
                    self.stream_recovering = Some(w.id.clone());
                    self.watch_started_at = None;
                    let preferred_height = self
                        .selected_format_idx
                        .and_then(|i| w.video_formats.get(i))
                        .map(|f| f.height);
                    let preferred_audio_id = w
                        .audio_tracks
                        .get(self.selected_audio_idx)
                        .map(|t| t.id.clone());
                    let _ = self.backend.tx.send(Request::RecoverStream {
                        video_id: w.id.clone(),
                        quality: self.preferred_quality,
                        preferred_height,
                        preferred_audio_id,
                        resume_secs: self.playback.time_pos.max(0.0),
                    });
                    self.toast("Stream expired — refreshing...");
                }
            } else if start_stalled || proxy_403 || mid_eof_drop {
                self.watch_started_at = None;
                self.last_error = Some(
                    "Stream stopped (HTTP 403 or network error). Try switching quality or reopening."
                        .to_owned(),
                );
            }
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
            if let Some(w) = &self.watch {
                if self.live_chat_is_replay {
                    let pos = self.playback.time_pos.max(0.0);
                    let jumped = (pos - self.live_chat_last_replay_pos).abs() > 10.0;
                    if jumped {
                        if let Some(init_tok) = self.live_chat_replay_initial_token.clone() {
                            self.live_chat.clear();
                            self.live_chat_token = Some(init_tok.clone());
                            self.live_chat_last_replay_pos = pos;
                            self.live_chat_next_poll =
                                Some(Instant::now() + Duration::from_secs(3));
                            let _ = self.backend.tx.send(Request::LoadLiveChat {
                                video_id: w.id.clone(),
                                continuation: init_tok,
                                replay_offset_ms: Some((pos * 1000.0) as u64),
                            });
                        }
                    } else if !self.playback.paused {
                        if let (Some(tok), Some(due)) =
                            (&self.live_chat_token, self.live_chat_next_poll)
                        {
                            if Instant::now() >= due {
                                self.live_chat_last_replay_pos = pos;
                                self.live_chat_next_poll =
                                    Some(Instant::now() + Duration::from_secs(3));
                                let _ = self.backend.tx.send(Request::LoadLiveChat {
                                    video_id: w.id.clone(),
                                    continuation: tok.clone(),
                                    replay_offset_ms: Some((pos * 1000.0) as u64),
                                });
                            } else {
                                ctx.request_repaint_after(Duration::from_secs(1));
                            }
                        }
                    }
                } else if let (Some(tok), Some(due)) =
                    (&self.live_chat_token, self.live_chat_next_poll)
                {
                    if Instant::now() >= due {
                        self.live_chat_next_poll = Some(Instant::now() + Duration::from_secs(4));
                        let _ = self.backend.tx.send(Request::LoadLiveChat {
                            video_id: w.id.clone(),
                            continuation: tok.clone(),
                            replay_offset_ms: None,
                        });
                    } else {
                        ctx.request_repaint_after(Duration::from_secs(1));
                    }
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

        let is_live = self.watch.as_ref().is_some_and(|w| w.is_live);
        if !is_live
            && self.autoplay_next
            && self.playback.eof
            && !was_eof
            && self.playback.duration > 1.0
        {
            if !self.queue.is_empty() {
                let next = self.queue.remove(0);
                self.persist_queue();
                self.open_video(next);
            } else if let Some(next) = self.watch.as_ref().and_then(|w| w.related.first()).cloned()
            {
                self.open_video(next);
            }
        }

        if let Some(om) = &mut self.os_media {
            if let Some(w) = &self.watch {
                let cover = (!w.id.starts_with("local:"))
                    .then(|| format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", w.id));
                let dur = if w.duration_secs > 0.0 {
                    w.duration_secs
                } else {
                    self.playback.duration
                };
                om.set_track(&w.id, &w.title, &w.channel, cover.as_deref(), dur);
                om.sync_duration_if_unset(self.playback.duration);
                om.sync_playback(true, self.playback.paused, self.playback.time_pos);
            } else {
                om.sync_playback(false, true, 0.0);
            }
        }
    }
}

impl eframe::App for YtApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.persist_queue();
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
        self.persist_queue();
        self.backend.cursor.stop();
        if let Some(mpv) = &self.mpv {
            mpv.destroy_gl(gl);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain_backend(&ctx);
        self.handle_shortcuts(&ctx);
        self.handle_os_media();
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
                    Page::Playlist => self.draw_playlist_page(ui, &ctx),
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
                            if let Some(act) = cards::draw_video_grid_opts(
                                ui,
                                "Watch History (Synced with YouTube)",
                                &items,
                                loading,
                                cont.is_some(),
                                true,
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
                            if let Some(act) = cards::draw_video_grid_opts(
                                ui,
                                "Watch History (Local)",
                                &items,
                                false,
                                false,
                                true,
                            ) {
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
        self.record_nav_transition();
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

fn queue_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("queue.json")
}

fn load_queue_file(data_dir: &std::path::Path) -> Vec<VideoItem> {
    let Ok(raw) = std::fs::read_to_string(queue_path(data_dir)) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<VideoItem>>(&raw).unwrap_or_default()
}

fn save_queue_file(data_dir: &std::path::Path, queue: &[VideoItem]) {
    let dest = queue_path(data_dir);
    if queue.is_empty() {
        let _ = std::fs::remove_file(&dest);
        return;
    }
    if let Ok(json) = serde_json::to_string(queue) {
        let tmp = data_dir.join("queue.json.tmp");
        if std::fs::write(&tmp, &json).is_ok() {
            if std::fs::rename(&tmp, &dest).is_err() {
                let _ = std::fs::write(&dest, &json);
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }
}

pub(super) fn shuffle_vec<T>(items: &mut [T], mut seed: u64) {
    if items.len() <= 1 {
        return;
    }
    if seed == 0 {
        seed = 0x9e37_79b9_7f4a_7c15;
    }
    for i in (1..items.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let j = (seed as usize) % (i + 1);
        items.swap(i, j);
    }
}

#[cfg(test)]
mod ui_tests {
    use super::shuffle_vec;

    #[test]
    fn shuffle_preserves_all_elements() {
        let mut v = vec![1, 2, 3, 4, 5, 6, 7, 8];
        shuffle_vec(&mut v, 12345);
        v.sort();
        assert_eq!(v, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }
}
