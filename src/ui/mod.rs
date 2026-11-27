mod cards;
mod pages;
mod sidebar;
mod topbar;
mod watch;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::glow;
use egui::{Align, Color32, Frame, Key, Layout, Margin, ViewportCommand};

use crate::backend::{Backend, Request, Response};
use crate::innertube;
use crate::model::{
    AmbientMode, ChannelInfo, ChannelNotificationParams, CommentItem, FeedChip,
    HoverPreviewQuality, HwdecMode, KeyBinding, LastWatched, LiveChatEmote, LiveChatMessage,
    NotificationMenuItem, PlaylistOption, PlaylistSummary, PoTokenMode, PreferredQuality,
    SearchChannelResult, SearchFilter, ShortcutConfig, ShortcutSlot, SidebarChannel,
    SponsorCategory, SponsorSegment, UpNextFilterMode, VideoItem, WatchData, WindowMaterial,
};
use crate::mpv::{GlProcAddrFn, MpvPlayer, PlaybackState};
use crate::os_media::OsMedia;
use crate::theme;
use crate::win_shell::{ShellEvent, WinShell};
use crate::ytdlp;

use cards::{GridAction, draw_video_grid};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    Home,
    Shorts,
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

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum NotifFilter {
    #[default]
    All,
    Live,
    Videos,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum HistoryFilter {
    #[default]
    All,
    Videos,
    Shorts,
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
    Shorts,
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
    SleepTimer,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum SleepTimer {
    Off,
    EndOfVideo,
    Until(Instant),
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum SettingsTab {
    #[default]
    Account,
    Playback,
    Appearance,
    Engine,
    Connections,
    Shortcuts,
}

impl SettingsTab {
    pub const ALL: [Self; 6] = [
        Self::Account,
        Self::Playback,
        Self::Appearance,
        Self::Engine,
        Self::Connections,
        Self::Shortcuts,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Account => "Account",
            Self::Playback => "Playback",
            Self::Appearance => "Appearance",
            Self::Engine => "Downloads & Engine",
            Self::Connections => "Connections",
            Self::Shortcuts => "Keyboard shortcuts",
        }
    }
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

#[derive(Clone, Debug)]
pub(super) struct ShortsOutgoing {
    pub thumbnail_url: String,
    pub title: String,
    pub channel: String,
}

pub(super) struct PlayerSlot {
    pub mpv: Option<MpvPlayer>,
    pub watch: Option<WatchData>,
    pub active_watch_id: Option<String>,
    pub watch_loading: bool,
    pub playback: PlaybackState,
    pub watch_started_at: Option<Instant>,
    pub active_stream_urls: (String, Option<String>),
    pub playing_hls_master: bool,
    pub pending_hls_target_height: Option<u32>,
    pub stream_recovery_attempts: u8,
    pub stream_recovering: Option<String>,
    pub buffering_since: Option<Instant>,
    pub last_abr_switch_at: Option<Instant>,
    pub stable_playback_since: Option<Instant>,
    pub last_abr_progress_pos: f64,
    pub abr_initial_format_idx: Option<usize>,
    pub abr_target_height: Option<u32>,
    pub abr_downgraded: bool,
    pub abr_upgrade_attempts: u8,
    pub abr_tried_formats: HashSet<usize>,
    pub seeking_pos: Option<f64>,
    pub muted: bool,
    pub speed: f64,
    pub selected_format_idx: Option<usize>,
    pub selected_audio_idx: usize,
    pub selected_caption_idx: Option<usize>,
    pub used_ytdlp: bool,
    pub sponsor_segments: Vec<SponsorSegment>,
    pub last_skipped_end: f64,
    pub pending_resume: Option<(String, f64, Option<f32>)>,
    pub comments: Vec<CommentItem>,
    pub comments_next_token: Option<String>,
    pub comments_loading: bool,
    pub comment_sort_options: Vec<(String, String)>,
    pub selected_comment_sort: usize,
    pub comment_create_params: Option<String>,
    pub comment_draft: String,
    pub comment_reply_target_id: Option<String>,
    pub comment_reply_target_author: String,
    pub comment_reply_draft: String,
    pub show_chapters: bool,
    pub show_description: bool,
    pub watch_scrolled_into_pip: bool,
    pub scroll_watch_to_top: bool,
    pub player_menu: PlayerMenu,
    pub live_chat: Vec<LiveChatMessage>,
    pub live_chat_token: Option<String>,
    pub live_chat_next_poll: Option<Instant>,
    pub live_chat_send_params: Option<String>,
    pub live_chat_draft: String,
    pub live_chat_emotes: Vec<LiveChatEmote>,
    pub live_chat_is_replay: bool,
    pub live_chat_replay_initial_token: Option<String>,
    pub live_chat_last_replay_pos: f64,
    pub live_chat_emote_picker_open: bool,
    pub right_tab: RightTab,
    pub start_paused: bool,
    pub related_base: Vec<VideoItem>,
    pub up_next_active_chip: usize,
    pub up_next_loading: bool,
}

impl PlayerSlot {
    pub fn new(speed: f64) -> Self {
        Self {
            mpv: None,
            watch: None,
            active_watch_id: None,
            watch_loading: false,
            playback: PlaybackState::default(),
            watch_started_at: None,
            active_stream_urls: (String::new(), None),
            playing_hls_master: false,
            pending_hls_target_height: None,
            stream_recovery_attempts: 0,
            stream_recovering: None,
            buffering_since: None,
            last_abr_switch_at: None,
            stable_playback_since: None,
            last_abr_progress_pos: 0.0,
            abr_initial_format_idx: None,
            abr_target_height: None,
            abr_downgraded: false,
            abr_upgrade_attempts: 0,
            abr_tried_formats: HashSet::new(),
            seeking_pos: None,
            muted: false,
            speed,
            selected_format_idx: None,
            selected_audio_idx: 0,
            selected_caption_idx: None,
            used_ytdlp: false,
            sponsor_segments: Vec::new(),
            last_skipped_end: -1.0,
            pending_resume: None,
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
            show_description: false,
            watch_scrolled_into_pip: false,
            scroll_watch_to_top: false,
            player_menu: PlayerMenu::None,
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
            right_tab: RightTab::UpNext,
            start_paused: false,
            related_base: Vec::new(),
            up_next_active_chip: 0,
            up_next_loading: false,
        }
    }

    pub fn matches_video(&self, video_id: &str) -> bool {
        self.active_watch_id.as_deref() == Some(video_id)
            || self.watch.as_ref().is_some_and(|w| w.id == video_id)
    }

    pub fn clear_for_new_video(&mut self, video_id: &str) {
        self.active_watch_id = Some(video_id.to_owned());
        self.watch_scrolled_into_pip = false;
        self.scroll_watch_to_top = true;
        self.watch_loading = true;
        self.start_paused = false;
        self.watch_started_at = None;
        self.active_stream_urls = (String::new(), None);
        self.playing_hls_master = false;
        self.pending_hls_target_height = None;
        self.stream_recovery_attempts = 0;
        self.stream_recovering = None;
        self.buffering_since = None;
        self.last_abr_switch_at = None;
        self.stable_playback_since = None;
        self.last_abr_progress_pos = 0.0;
        self.abr_initial_format_idx = None;
        self.abr_target_height = None;
        self.abr_downgraded = false;
        self.abr_upgrade_attempts = 0;
        self.abr_tried_formats.clear();
        self.sponsor_segments.clear();
        self.last_skipped_end = -1.0;
        self.seeking_pos = None;
        self.selected_audio_idx = 0;
        self.selected_caption_idx = None;
        self.player_menu = PlayerMenu::None;
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
        self.related_base.clear();
        self.up_next_active_chip = usize::MAX;
        self.up_next_loading = false;
    }

    pub fn ensure_mpv(
        &mut self,
        data_dir: &std::path::Path,
        gpa: &Option<GlProcAddrFn>,
        ctx: &egui::Context,
        volume: f64,
        hwdec: HwdecMode,
        sub_font_size: u32,
        sub_bg_opacity: f32,
    ) {
        if self.mpv.is_some() {
            return;
        }
        if let Some(gpa) = gpa.clone() {
            if let Ok(player) = MpvPlayer::new(data_dir, gpa, ctx.clone()) {
                player.set_volume(volume);
                player.set_mute(self.muted);
                player.set_hwdec(hwdec);
                player.set_subtitle_style(sub_font_size, sub_bg_opacity);
                self.mpv = Some(player);
            }
        }
    }
}

pub struct YtApp {
    pub(super) backend: Backend,
    pub(super) mpv: Option<MpvPlayer>,
    pub(super) os_media: Option<OsMedia>,
    pub(super) win_shell: Option<WinShell>,
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

    pub(super) notifications: Vec<NotificationMenuItem>,
    pub(super) notifications_loading: bool,
    pub(super) notifications_open: bool,
    pub(super) unread_notifications: u32,
    pub(super) notifications_seen_params: Option<String>,
    pub(super) notifications_loaded_at: Option<Instant>,
    pub(super) seen_fallback_notif_ids: std::collections::HashSet<String>,

    pub(super) search_input: String,
    pub(super) active_query: String,
    pub(super) search_suggestions: Vec<String>,
    pub(super) search_history: Vec<String>,
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

    pub(super) shorts_items: Vec<VideoItem>,
    pub(super) shorts_loading: bool,

    pub(super) channel_id: Option<String>,
    pub(super) channel_name: String,
    pub(super) channel_info: Option<ChannelInfo>,
    pub(super) channel_tab_params: Option<String>,
    pub(super) channel_chips: Vec<FeedChip>,
    pub(super) channel_items: Vec<VideoItem>,
    pub(super) channel_continuation: Option<String>,
    pub(super) channel_loading: bool,
    pub(super) channel_search_input: String,
    pub(super) channel_search_active: String,

    pub(super) search_channels: Vec<SearchChannelResult>,
    pub(super) search_items: Vec<VideoItem>,
    pub(super) search_filter: SearchFilter,
    pub(super) search_filters_open: bool,
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
    pub(super) related_base: Vec<VideoItem>,
    pub(super) up_next_mode: UpNextFilterMode,
    pub(super) up_next_active_chip: usize,
    pub(super) up_next_loading: bool,
    pub(super) dropboard_online: bool,
    pub(super) dropboard_last_check: Option<Instant>,
    pub(super) active_player_tab: usize,
    pub(super) bg_player_tab: Option<Box<PlayerSlot>>,
    pub(super) shorts_slot: Box<PlayerSlot>,
    pub(super) shorts_queue: Vec<VideoItem>,
    pub(super) shorts_idx: usize,
    pub(super) shorts_slide_anim: Option<(i8, Instant)>,
    pub(super) shorts_outgoing: Option<ShortsOutgoing>,
    pub(super) shorts_opened_at: Option<Instant>,
    pub(super) shorts_last_wheel_at: Option<Instant>,
    pub(super) shorts_comments_open: bool,
    pub(super) shorts_grid_view: bool,
    pub(super) shorts_paused_video: bool,
    pub(super) history_feedback_tokens: HashMap<String, String>,
    pub(super) active_downloads: Vec<ActiveDownload>,

    pub(super) playback: PlaybackState,
    pub(super) watch_started_at: Option<Instant>,
    pub(super) active_stream_urls: (String, Option<String>),
    pub(super) playing_hls_master: bool,
    pub(super) pending_hls_target_height: Option<u32>,
    pub(super) stream_recovery_attempts: u8,
    pub(super) stream_recovering: Option<String>,
    pub(super) buffering_since: Option<Instant>,
    pub(super) last_abr_switch_at: Option<Instant>,
    pub(super) stable_playback_since: Option<Instant>,
    pub(super) last_abr_progress_pos: f64,
    pub(super) abr_initial_format_idx: Option<usize>,
    pub(super) abr_target_height: Option<u32>,
    pub(super) abr_downgraded: bool,
    pub(super) abr_upgrade_attempts: u8,
    pub(super) abr_tried_formats: HashSet<usize>,
    pub(super) seeking_pos: Option<f64>,
    pub(super) volume: f64,
    pub(super) muted: bool,
    pub(super) speed: f64,
    pub(super) preferred_quality: PreferredQuality,
    pub(super) hover_preview_quality: HoverPreviewQuality,
    pub(super) hover_preview_muted: bool,
    pub(super) hover_preview_mpv: Option<MpvPlayer>,
    pub(super) hover_preview_playback: PlaybackState,
    pub(super) hover_preview_candidate: Option<(String, Instant)>,
    pub(super) hover_preview_loading_id: Option<String>,
    pub(super) hover_preview_active_id: Option<String>,
    pub(super) hover_preview_duration: f64,
    pub(super) hover_preview_last_hover_at: Option<Instant>,
    pub(super) hover_preview_stopped_at: Option<Instant>,
    pub(super) retired_mpv: Arc<Mutex<Vec<MpvPlayer>>>,
    pub(super) hwdec_mode: HwdecMode,
    pub(super) download_dir: String,
    pub(super) sponsorblock_enabled: bool,
    pub(super) sponsorblock_categories: HashSet<SponsorCategory>,
    pub(super) dearrow_enabled: bool,
    pub(super) ryd_enabled: bool,
    pub(super) sync_history: bool,
    pub(super) autoplay_next: bool,
    pub(super) auto_resume_on_start: bool,
    pub(super) theater_mode: bool,
    pub(super) fullscreen: bool,
    pub(super) win_pip_mode: bool,
    pub(super) win_pip_saved_rect: Option<egui::Rect>,
    pub(super) win_pip_saved_maximized: bool,
    pub(super) win_pip_saved_aot: bool,
    pub(super) win_pip_saved_page: Page,
    pub(super) always_on_top: bool,
    pub(super) sleep_timer: SleepTimer,
    pub(super) sleep_timer_input: String,
    pub(super) start_paused: bool,
    pub(super) close_to_tray: bool,
    pub(super) minimize_to_tray: bool,
    pub(super) hidden_to_tray: bool,
    pub(super) was_minimized: bool,
    pub(super) quitting_from_tray: bool,
    pub(super) ambient_glow: bool,
    pub(super) po_token_mode: PoTokenMode,
    pub(super) window_material: WindowMaterial,
    pub(super) ambient_mode: AmbientMode,
    pub(super) ambient_opacity: f32,
    pub(super) ambient_requested_key: Option<String>,
    pub(super) ambient_tex_soft: Option<(String, egui::TextureHandle)>,
    pub(super) ambient_tex_tiled: Option<(String, egui::TextureHandle)>,
    pub(super) ambient_prev_soft: Option<egui::TextureHandle>,
    pub(super) ambient_changed_at: Option<Instant>,
    pub(super) settings_tab: SettingsTab,
    pub(super) settings_query: String,
    pub(super) settings_search_in_group: bool,
    pub(super) notif_filter: NotifFilter,
    pub(super) history_filter: HistoryFilter,
    pub(super) sub_font_size: u32,
    pub(super) sub_bg_opacity: f32,
    pub(super) shortcuts: ShortcutConfig,
    pub(super) rebinding_shortcut: Option<ShortcutSlot>,
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
    pub(super) pip_pre_queue_pos: Option<egui::Pos2>,
    pub(super) pip_queue_open: bool,

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
        let tray_wake_ctx = cc.egui_ctx.clone();
        let win_shell = WinShell::new(cc, Arc::new(move || tray_wake_ctx.request_repaint()));

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
        let mut sponsorblock_categories: HashSet<SponsorCategory> = [
            SponsorCategory::Sponsor,
            SponsorCategory::SelfPromo,
            SponsorCategory::Interaction,
            SponsorCategory::Intro,
            SponsorCategory::Outro,
        ]
        .into_iter()
        .collect();
        let mut dearrow_enabled = false;
        let mut ryd_enabled = false;
        let mut sync_history = true;
        let mut autoplay_next = true;
        let mut auto_resume_on_start = false;
        let mut theater_mode = false;
        let mut up_next_mode = UpNextFilterMode::All;
        let mut always_on_top = false;
        let mut close_to_tray = false;
        let mut minimize_to_tray = false;
        let mut ambient_glow = true;
        let mut po_token_mode = PoTokenMode::Auto;
        let mut window_material = WindowMaterial::None;
        let mut ambient_mode = AmbientMode::SoftBlur;
        let mut ambient_opacity = 0.28f32;
        let mut sub_font_size = 46u32;
        let mut sub_bg_opacity = 0.70f32;
        let mut shortcuts = ShortcutConfig::default();
        let mut preferred_quality = PreferredQuality::P1080;
        let mut hover_preview_quality = HoverPreviewQuality::P480;
        let mut hover_preview_muted = true;
        let mut hwdec_mode = HwdecMode::AutoCopy;
        let mut download_dir = String::new();
        let mut sidebar_expanded = true;
        let mut subs_list_open = true;
        let mut you_section_open = true;
        let mut history = Vec::new();
        let mut saved = Vec::new();
        let mut resume_positions = HashMap::new();
        let mut search_history: Vec<String> = Vec::new();

        if let Some(storage) = cc.storage {
            if let Some(v) = storage.get_string("volume").and_then(|s| s.parse().ok()) {
                volume = v;
            }
            if let Some(sb) = storage.get_string("sponsorblock") {
                sponsorblock_enabled = sb != "0";
            }
            if let Some(sbc) = storage.get_string("sb_categories") {
                if let Ok(parsed) = serde_json::from_str::<HashSet<SponsorCategory>>(&sbc) {
                    sponsorblock_categories = parsed;
                }
            }
            if let Some(da) = storage.get_string("dearrow") {
                dearrow_enabled = da == "1";
            }
            if let Some(ryd) = storage.get_string("ryd") {
                ryd_enabled = ryd == "1";
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
            if let Some(tm) = storage.get_string("theater_mode") {
                theater_mode = tm == "1";
            }
            if let Some(um) = storage.get_string("up_next_mode") {
                up_next_mode = UpNextFilterMode::from_key(&um);
            }
            if let Some(aot) = storage.get_string("always_on_top") {
                always_on_top = aot == "1";
            }
            if let Some(ct) = storage.get_string("close_to_tray") {
                close_to_tray = ct == "1";
            }
            if let Some(mt) = storage.get_string("minimize_to_tray") {
                minimize_to_tray = mt == "1";
            }
            if let Some(ag) = storage.get_string("ambient_glow") {
                ambient_glow = ag != "0";
            }
            if let Some(pt) = storage.get_string("po_token_mode") {
                po_token_mode = match pt.as_str() {
                    "on" => PoTokenMode::On,
                    "off" => PoTokenMode::Off,
                    _ => PoTokenMode::Auto,
                };
            }
            if let Some(wm) = storage.get_string("window_material") {
                window_material = WindowMaterial::from_key(&wm);
            }
            if let Some(am) = storage.get_string("ambient_mode") {
                ambient_mode = AmbientMode::from_key(&am);
            }
            if let Some(ao) = storage.get_string("ambient_opacity").and_then(|s| s.parse().ok()) {
                ambient_opacity = ao;
            }
            if let Some(sf) = storage.get_string("sub_font_size").and_then(|s| s.parse().ok()) {
                sub_font_size = sf;
            }
            if let Some(sb) = storage.get_string("sub_bg_opacity").and_then(|s| s.parse().ok()) {
                sub_bg_opacity = sb;
            }
            if let Some(sc) = storage.get_string("shortcuts") {
                if let Ok(parsed) = serde_json::from_str::<ShortcutConfig>(&sc) {
                    shortcuts = parsed;
                }
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
            if let Some(hq) = storage.get_string("hover_preview_quality") {
                hover_preview_quality = HoverPreviewQuality::from_key(&hq);
            }
            if let Some(hm) = storage.get_string("hover_preview_muted") {
                hover_preview_muted = hm != "0";
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
            if let Some(sh) = storage.get_string("search_history") {
                if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&sh) {
                    search_history = parsed;
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
            if let Some(ref bg) = lw.second_tab {
                if bg.position_secs > 2.0 {
                    resume_positions.insert(bg.id.clone(), bg.position_secs);
                }
            }
        }

        let _ = backend.tx.send(Request::SetPoTokenMode(po_token_mode));
        let _ = backend.tx.send(Request::LoadHome {
            params: None,
            continuation: None,
            append: false,
        });

        let mut app = Self {
            backend,
            mpv,
            os_media,
            win_shell,
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
            notifications: Vec::new(),
            notifications_loading: false,
            notifications_open: false,
            unread_notifications: 0,
            notifications_seen_params: None,
            notifications_loaded_at: None,
            seen_fallback_notif_ids: std::collections::HashSet::new(),
            search_input: String::new(),
            active_query: String::new(),
            search_suggestions: Vec::new(),
            search_history,
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
            shorts_items: Vec::new(),
            shorts_loading: false,
            channel_id: None,
            channel_name: String::new(),
            channel_info: None,
            channel_tab_params: None,
            channel_chips: Vec::new(),
            channel_items: Vec::new(),
            channel_continuation: None,
            channel_loading: false,
            channel_search_input: String::new(),
            channel_search_active: String::new(),
            search_channels: Vec::new(),
            search_items: Vec::new(),
            search_filter: SearchFilter::default(),
            search_filters_open: false,
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
            shorts_queue: Vec::new(),
            shorts_idx: 0,
            shorts_slide_anim: None,
            shorts_outgoing: None,
            shorts_opened_at: None,
            shorts_last_wheel_at: None,
            shorts_comments_open: false,
            shorts_grid_view: false,
            shorts_paused_video: false,
            active_player_tab: 0,
            bg_player_tab: None,
            shorts_slot: Box::new(PlayerSlot::new(1.0)),
            history_feedback_tokens: HashMap::new(),
            active_downloads: Vec::new(),
            channel_avatars: HashMap::new(),
            avatar_requested: HashSet::new(),
            right_tab: RightTab::UpNext,
            related_base: Vec::new(),
            up_next_mode,
            up_next_active_chip: 0,
            up_next_loading: false,
            dropboard_online: false,
            dropboard_last_check: None,
            playback: PlaybackState::default(),
            watch_started_at: None,
            active_stream_urls: (String::new(), None),
            playing_hls_master: false,
            pending_hls_target_height: None,
            stream_recovery_attempts: 0,
            stream_recovering: None,
            buffering_since: None,
            last_abr_switch_at: None,
            stable_playback_since: None,
            last_abr_progress_pos: 0.0,
            abr_initial_format_idx: None,
            abr_target_height: None,
            abr_downgraded: false,
            abr_upgrade_attempts: 0,
            abr_tried_formats: HashSet::new(),
            seeking_pos: None,
            volume,
            muted: false,
            speed: 1.0,
            preferred_quality,
            hover_preview_quality,
            hover_preview_muted,
            hover_preview_mpv: None,
            hover_preview_playback: PlaybackState::default(),
            hover_preview_candidate: None,
            hover_preview_loading_id: None,
            hover_preview_active_id: None,
            hover_preview_duration: 0.0,
            hover_preview_last_hover_at: None,
            hover_preview_stopped_at: None,
            retired_mpv: Arc::new(Mutex::new(Vec::new())),
            hwdec_mode,
            download_dir,
            sponsorblock_enabled,
            sponsorblock_categories,
            dearrow_enabled,
            ryd_enabled,
            sync_history,
            autoplay_next,
            auto_resume_on_start,
            theater_mode,
            fullscreen: false,
            win_pip_mode: false,
            win_pip_saved_rect: None,
            win_pip_saved_maximized: false,
            win_pip_saved_aot: always_on_top,
            win_pip_saved_page: Page::Home,
            always_on_top,
            sleep_timer: SleepTimer::Off,
            sleep_timer_input: String::new(),
            start_paused: false,
            close_to_tray,
            minimize_to_tray,
            hidden_to_tray: false,
            was_minimized: false,
            quitting_from_tray: false,
            ambient_glow,
            po_token_mode,
            window_material,
            ambient_mode,
            ambient_opacity,
            ambient_requested_key: None,
            ambient_tex_soft: None,
            ambient_tex_tiled: None,
            ambient_prev_soft: None,
            ambient_changed_at: None,
            settings_tab: SettingsTab::Account,
            settings_query: String::new(),
            settings_search_in_group: false,
            notif_filter: NotifFilter::All,
            history_filter: HistoryFilter::All,
            sub_font_size,
            sub_bg_opacity,
            shortcuts,
            rebinding_shortcut: None,
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
            pip_pre_queue_pos: None,
            pip_queue_open: false,
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
            let ctx = cc.egui_ctx.clone();
            app.resume_last_watched(&ctx);
        }
        if app.always_on_top {
            cc.egui_ctx
                .send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        }
        if let Some(mpv) = &app.mpv {
            mpv.set_subtitle_style(app.sub_font_size, app.sub_bg_opacity);
        }
        app.last_nav = app.current_nav_entry();
        app
    }

    pub(super) fn toast(&mut self, msg: impl Into<String>) {
        self.status_toast = Some((msg.into(), Instant::now()));
    }

    fn reset_signed_out_state(&mut self) {
        self.signed_in = false;
        self.signing_in = false;
        self.account_name = None;
        self.account_avatar = None;
        self.notifications.clear();
        self.notifications_open = false;
        self.unread_notifications = 0;
        self.notifications_seen_params = None;
        self.notifications_loaded_at = None;
        self.seen_fallback_notif_ids.clear();
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
            append: false,
        });
    }

    pub(super) fn trigger_hud(&mut self, ico: HudIcon, msg: impl Into<String>) {
        self.hud_feedback = Some((ico, msg.into(), Instant::now()));
    }

    pub(super) fn resume_last_watched(&mut self, ctx: &egui::Context) {
        let Some(lw) = self.last_watched.clone() else {
            return;
        };
        if let Some(bg) = self.bg_player_tab.take() {
            if let Some(m) = &bg.mpv {
                m.stop();
            }
        }
        self.active_player_tab = 0;

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
            }
        } else {
            self.pending_resume = Some((lw.id.clone(), lw.position_secs, None));
            if lw.position_secs > 2.0 {
                self.resume_positions.insert(lw.id.clone(), lw.position_secs);
            }
            self.open_video_id(&lw.id);
        }

        if let Some(bg_lw) = lw.second_tab {
            let mut slot = Box::new(PlayerSlot::new(self.speed));
            slot.ensure_mpv(
                &self.backend.data_dir,
                &self.get_proc_address,
                ctx,
                self.volume,
                self.hwdec_mode,
                self.sub_font_size,
                self.sub_bg_opacity,
            );
            if let Some(path) = bg_lw.id.strip_prefix("local_path:").map(PathBuf::from) {
                if path.is_file() {
                    let title = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Local Media")
                        .to_owned();
                    let path_str = path.to_string_lossy().into_owned();
                    slot.active_watch_id = Some(format!("local:{title}"));
                    slot.active_stream_urls = (path_str.clone(), None);
                    slot.watch = Some(WatchData {
                        id: format!("local:{title}"),
                        title,
                        channel: "Offline Download".to_owned(),
                        channel_id: None,
                        channel_avatar_url: None,
                        subscribers_text: String::new(),
                        views_text: "Local file".to_owned(),
                        published_text: path_str.clone(),
                        likes_text: String::new(),
                        dislikes_text: None,
                        is_liked: false,
                        is_disliked: false,
                        is_subscribed: false,
                        notifications: ChannelNotificationParams::default(),
                        description: String::new(),
                        duration_secs: bg_lw.duration_secs.max(0.0),
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
                        related_chips: Vec::new(),
                        is_live: false,
                        is_members_only: false,
                        resume_secs: (bg_lw.position_secs > 2.0).then_some(bg_lw.position_secs),
                    });
                    if let Some(mpv) = &slot.mpv {
                        let start = (bg_lw.position_secs > 2.0).then_some(bg_lw.position_secs);
                        mpv.load_stream(&path_str, None, start);
                        mpv.set_paused(true);
                    }
                    self.bg_player_tab = Some(slot);
                }
            } else {
                if bg_lw.position_secs > 2.0 {
                    self.resume_positions
                        .insert(bg_lw.id.clone(), bg_lw.position_secs);
                }
                slot.active_watch_id = Some(bg_lw.id.clone());
                slot.watch_loading = true;
                slot.start_paused = true;
                slot.pending_resume = Some((bg_lw.id.clone(), bg_lw.position_secs, None));
                self.bg_player_tab = Some(slot);
                let _ = self.backend.tx.send(Request::Watch {
                    video_id: bg_lw.id,
                    quality: self.preferred_quality,
                    sync_history: false,
                });
            }
            self.active_player_tab = (lw.active_tab as usize).min(1);
        }

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
        fn build_lw(w: &WatchData, playback: &PlaybackState) -> Option<LastWatched> {
            let pos = playback.time_pos.max(0.0);
            let dur = playback.duration.max(w.duration_secs).max(0.0);
            if !w.is_live && dur > 15.0 && pos >= dur * 0.94 {
                return None;
            }
            let id = if w.id.starts_with("local:") {
                format!("local_path:{}", w.published_text)
            } else {
                w.id.clone()
            };
            Some(LastWatched {
                id,
                title: w.title.clone(),
                channel: w.channel.clone(),
                thumbnail_url: if w.id.starts_with("local:") {
                    String::new()
                } else {
                    format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", w.id)
                },
                position_secs: if w.is_live { 0.0 } else { pos },
                duration_secs: dur,
                second_tab: None,
                active_tab: 0,
            })
        }

        let active_lw = self
            .watch
            .as_ref()
            .and_then(|w| build_lw(w, &self.playback));
        let bg_lw = self
            .bg_player_tab
            .as_ref()
            .and_then(|bg| bg.watch.as_ref().and_then(|w| build_lw(w, &bg.playback)));

        let Some(mut primary) = active_lw.or(bg_lw.clone()) else {
            if self.watch.is_some() {
                self.clear_last_watched();
            }
            return;
        };
        if self.watch.is_some() {
            primary.second_tab = bg_lw.filter(|b| b.id != primary.id).map(Box::new);
            primary.active_tab = if primary.second_tab.is_some() {
                self.active_player_tab as u8
            } else {
                0
            };
        }

        self.last_watched = Some(primary.clone());
        save_last_watched_file(&self.backend.data_dir, &primary);
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
        self.channel_chips.clear();
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
            append: false,
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

    pub(super) fn open_video(&mut self, mut item: VideoItem) {
        if self.hover_preview_active_id.as_deref() == Some(item.id.as_str())
            && !item.is_live
            && !item.is_short()
            && item.resume_secs.is_none()
            && self.hover_preview_playback.time_pos > 2.0
        {
            let dur = self.hover_preview_duration.max(self.hover_preview_playback.duration);
            if dur <= 0.0 || self.hover_preview_playback.time_pos + 5.0 < dur {
                item.resume_secs = Some(self.hover_preview_playback.time_pos);
            }
        }
        self.stop_hover_preview();
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

    pub(super) fn swap_active_with(&mut self, slot: &mut PlayerSlot) {
        std::mem::swap(&mut self.mpv, &mut slot.mpv);
        std::mem::swap(&mut self.watch, &mut slot.watch);
        std::mem::swap(&mut self.active_watch_id, &mut slot.active_watch_id);
        std::mem::swap(&mut self.watch_loading, &mut slot.watch_loading);
        std::mem::swap(&mut self.playback, &mut slot.playback);
        std::mem::swap(&mut self.watch_started_at, &mut slot.watch_started_at);
        std::mem::swap(&mut self.active_stream_urls, &mut slot.active_stream_urls);
        std::mem::swap(&mut self.playing_hls_master, &mut slot.playing_hls_master);
        std::mem::swap(
            &mut self.pending_hls_target_height,
            &mut slot.pending_hls_target_height,
        );
        std::mem::swap(
            &mut self.stream_recovery_attempts,
            &mut slot.stream_recovery_attempts,
        );
        std::mem::swap(&mut self.stream_recovering, &mut slot.stream_recovering);
        std::mem::swap(&mut self.buffering_since, &mut slot.buffering_since);
        std::mem::swap(&mut self.last_abr_switch_at, &mut slot.last_abr_switch_at);
        std::mem::swap(&mut self.stable_playback_since, &mut slot.stable_playback_since);
        std::mem::swap(&mut self.last_abr_progress_pos, &mut slot.last_abr_progress_pos);
        std::mem::swap(
            &mut self.abr_initial_format_idx,
            &mut slot.abr_initial_format_idx,
        );
        std::mem::swap(&mut self.abr_target_height, &mut slot.abr_target_height);
        std::mem::swap(&mut self.abr_downgraded, &mut slot.abr_downgraded);
        std::mem::swap(&mut self.abr_upgrade_attempts, &mut slot.abr_upgrade_attempts);
        std::mem::swap(&mut self.abr_tried_formats, &mut slot.abr_tried_formats);
        std::mem::swap(&mut self.seeking_pos, &mut slot.seeking_pos);
        std::mem::swap(&mut self.muted, &mut slot.muted);
        std::mem::swap(&mut self.speed, &mut slot.speed);
        std::mem::swap(&mut self.selected_format_idx, &mut slot.selected_format_idx);
        std::mem::swap(&mut self.selected_audio_idx, &mut slot.selected_audio_idx);
        std::mem::swap(
            &mut self.selected_caption_idx,
            &mut slot.selected_caption_idx,
        );
        std::mem::swap(&mut self.used_ytdlp, &mut slot.used_ytdlp);
        std::mem::swap(&mut self.sponsor_segments, &mut slot.sponsor_segments);
        std::mem::swap(&mut self.last_skipped_end, &mut slot.last_skipped_end);
        std::mem::swap(&mut self.pending_resume, &mut slot.pending_resume);
        std::mem::swap(&mut self.comments, &mut slot.comments);
        std::mem::swap(&mut self.comments_next_token, &mut slot.comments_next_token);
        std::mem::swap(&mut self.comments_loading, &mut slot.comments_loading);
        std::mem::swap(&mut self.comment_sort_options, &mut slot.comment_sort_options);
        std::mem::swap(
            &mut self.selected_comment_sort,
            &mut slot.selected_comment_sort,
        );
        std::mem::swap(
            &mut self.comment_create_params,
            &mut slot.comment_create_params,
        );
        std::mem::swap(&mut self.comment_draft, &mut slot.comment_draft);
        std::mem::swap(
            &mut self.comment_reply_target_id,
            &mut slot.comment_reply_target_id,
        );
        std::mem::swap(
            &mut self.comment_reply_target_author,
            &mut slot.comment_reply_target_author,
        );
        std::mem::swap(&mut self.comment_reply_draft, &mut slot.comment_reply_draft);
        std::mem::swap(&mut self.show_chapters, &mut slot.show_chapters);
        std::mem::swap(&mut self.show_description, &mut slot.show_description);
        std::mem::swap(
            &mut self.watch_scrolled_into_pip,
            &mut slot.watch_scrolled_into_pip,
        );
        std::mem::swap(&mut self.scroll_watch_to_top, &mut slot.scroll_watch_to_top);
        std::mem::swap(&mut self.player_menu, &mut slot.player_menu);
        std::mem::swap(&mut self.live_chat, &mut slot.live_chat);
        std::mem::swap(&mut self.live_chat_token, &mut slot.live_chat_token);
        std::mem::swap(&mut self.live_chat_next_poll, &mut slot.live_chat_next_poll);
        std::mem::swap(&mut self.live_chat_send_params, &mut slot.live_chat_send_params);
        std::mem::swap(&mut self.live_chat_draft, &mut slot.live_chat_draft);
        std::mem::swap(&mut self.live_chat_emotes, &mut slot.live_chat_emotes);
        std::mem::swap(&mut self.live_chat_is_replay, &mut slot.live_chat_is_replay);
        std::mem::swap(
            &mut self.live_chat_replay_initial_token,
            &mut slot.live_chat_replay_initial_token,
        );
        std::mem::swap(
            &mut self.live_chat_last_replay_pos,
            &mut slot.live_chat_last_replay_pos,
        );
        std::mem::swap(
            &mut self.live_chat_emote_picker_open,
            &mut slot.live_chat_emote_picker_open,
        );
        std::mem::swap(&mut self.right_tab, &mut slot.right_tab);
        std::mem::swap(&mut self.start_paused, &mut slot.start_paused);
        std::mem::swap(&mut self.related_base, &mut slot.related_base);
        std::mem::swap(&mut self.up_next_active_chip, &mut slot.up_next_active_chip);
        std::mem::swap(&mut self.up_next_loading, &mut slot.up_next_loading);
    }

    pub(super) fn switch_player_tab(&mut self) {
        let Some(mut bg) = self.bg_player_tab.take() else {
            return;
        };
        self.swap_active_with(&mut bg);
        self.bg_player_tab = Some(bg);
        self.active_player_tab = 1 - self.active_player_tab;
        self.watch_scrolled_into_pip = false;
        self.scroll_watch_to_top = true;
        self.flush_last_watched_now();
    }

    pub(super) fn open_video_in_second_tab(&mut self, ctx: &egui::Context, item: VideoItem) {
        if self.watch.is_none() && self.bg_player_tab.is_none() {
            self.open_video(item);
            return;
        }
        if self.bg_player_tab.is_none() {
            let mut slot = Box::new(PlayerSlot::new(1.0));
            slot.ensure_mpv(
                &self.backend.data_dir,
                &self.get_proc_address,
                ctx,
                self.volume,
                self.hwdec_mode,
                self.sub_font_size,
                self.sub_bg_opacity,
            );
            self.swap_active_with(&mut slot);
            self.bg_player_tab = Some(slot);
            self.active_player_tab = 1;
        } else if self.active_player_tab == 0 {
            self.switch_player_tab();
        }
        let title = item.title.clone();
        self.open_video(item);
        self.toast(format!("Opened in Tab 2: {title}"));
    }

    /// Queue a dropped `MpvPlayer` so its OpenGL FBO/texture/render_context are freed
    /// with an active GL context during the next paint pass (or `on_exit`).
    pub(super) fn retire_mpv(&self, mpv: Option<MpvPlayer>) {
        if let Some(m) = mpv {
            m.stop();
            if let Ok(mut q) = self.retired_mpv.lock() {
                q.push(m);
            }
        }
    }

    fn schedule_retired_mpv_gl_cleanup(&self, ctx: &egui::Context) {
        let has_retired = self
            .retired_mpv
            .lock()
            .is_ok_and(|q| !q.is_empty());
        if !has_retired {
            return;
        }
        let queue = self.retired_mpv.clone();
        let cb = egui::PaintCallback {
            rect: egui::Rect::NOTHING,
            callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |_info, painter| {
                let gl = painter.gl();
                if let Ok(mut q) = queue.lock() {
                    for mpv in q.drain(..) {
                        mpv.destroy_gl(Some(gl));
                    }
                }
            })),
        };
        ctx.layer_painter(egui::LayerId::background()).add(cb);
    }

    pub(super) fn close_player_tab(&mut self, close_active: bool) {
        if close_active {
            if let Some(mut bg) = self.bg_player_tab.take() {
                self.swap_active_with(&mut bg);
                self.retire_mpv(bg.mpv.take());
                drop(bg);
                self.active_player_tab = 0;
            } else {
                let old_mpv = self.mpv.take();
                self.retire_mpv(old_mpv);
                self.watch = None;
                self.active_watch_id = None;
                self.watch_loading = false;
                self.watch_scrolled_into_pip = false;
                if self.page == Page::Watch {
                    self.page = self.prev_page;
                }
            }
        } else if let Some(mut bg) = self.bg_player_tab.take() {
            self.retire_mpv(bg.mpv.take());
            drop(bg);
            self.active_player_tab = 0;
        }
        if self.watch.is_some() || self.bg_player_tab.is_some() {
            self.flush_last_watched_now();
        } else {
            self.clear_last_watched();
        }
    }

    pub(super) fn prioritized_home_shorts(&self) -> Vec<VideoItem> {
        let sub_ids: HashSet<&str> = self
            .guide_channels
            .iter()
            .map(|c| c.browse_id.as_str())
            .collect();
        let sub_names: HashSet<String> = self
            .guide_channels
            .iter()
            .map(|c| c.name.trim().to_ascii_lowercase())
            .filter(|n| !n.is_empty())
            .collect();
        let is_sub_item = |it: &VideoItem| -> bool {
            it.channel_id
                .as_deref()
                .is_some_and(|cid| sub_ids.contains(cid))
                || (!it.channel.is_empty()
                    && sub_names.contains(&it.channel.trim().to_ascii_lowercase()))
        };

        let mut out = Vec::new();
        let mut seen = HashSet::new();
        // 1. Shorts from Subscriptions feed first (fresh subscribed Shorts get "New" badge)
        for it in &self.subs_items {
            if it.is_short() && seen.insert(it.id.clone()) {
                out.push(it.clone());
            }
        }
        // 2. Home Shorts whose channel is in Subscriptions
        for it in &self.home_items {
            if it.is_short() && is_sub_item(it) && seen.insert(it.id.clone()) {
                out.push(it.clone());
            }
        }
        // 3. Remaining Home Shorts
        for it in &self.home_items {
            if it.is_short() && seen.insert(it.id.clone()) {
                out.push(it.clone());
            }
        }
        out
    }

    pub(super) fn reload_home_feed(&mut self) {
        let params = if !self.home_chips.is_empty() {
            self.home_chips
                .iter()
                .find(|c| c.selected)
                .and_then(|c| c.params.clone())
        } else {
            None
        };
        let continuation = if !self.home_chips.is_empty() {
            self.home_chips
                .iter()
                .find(|c| c.selected)
                .and_then(|c| c.continuation.clone())
        } else {
            None
        };
        self.home_items.clear();
        self.home_continuation = None;
        self.home_loading = true;
        let _ = self.backend.tx.send(Request::LoadHome {
            params,
            continuation,
            append: false,
        });
        if self.signed_in && !self.subs_loading {
            let _ = self
                .backend
                .tx
                .send(Request::LoadSubscriptions { continuation: None });
        }
    }

    pub(super) fn open_shorts_page(&mut self) {
        self.shorts_grid_view = false;
        if self.shorts_items.is_empty() {
            let from_home = self.prioritized_home_shorts();
            if !from_home.is_empty() {
                self.shorts_items = from_home;
            }
        }
        if !self.shorts_loading && self.shorts_items.is_empty() {
            self.shorts_loading = true;
            let _ = self.backend.tx.send(Request::LoadShorts);
        }
        if self.shorts_slot.watch.is_some() || self.shorts_slot.active_watch_id.is_some() {
            if self.page != Page::Shorts {
                self.prev_page = self.page;
            }
            if self.watch.is_some() && !self.playback.paused {
                if let Some(m) = &self.mpv {
                    m.set_paused(true);
                }
                self.shorts_paused_video = true;
            }
            self.page = Page::Shorts;
            return;
        }
        if let Some(first) = self.shorts_items.first().cloned() {
            let list = self.shorts_items.clone();
            self.open_short(first, list);
        } else {
            if self.page != Page::Shorts {
                self.prev_page = self.page;
            }
            self.page = Page::Shorts;
        }
    }

    fn shorts_quality(&self) -> PreferredQuality {
        let cap = match self.preferred_quality {
            PreferredQuality::Best4K | PreferredQuality::P1440 | PreferredQuality::P1080 => {
                PreferredQuality::P1080
            }
            q => q,
        };
        let bw = self
            .backend
            .estimated_bandwidth_bps()
            .map(|(bps, _)| bps)
            .unwrap_or(0);
        let net_q = if bw >= 15_000_000 {
            PreferredQuality::P1080
        } else if bw == 0 || bw >= 4_000_000 {
            PreferredQuality::P720
        } else {
            PreferredQuality::P480
        };
        if net_q.max_height() < cap.max_height() {
            net_q
        } else {
            cap
        }
    }

    fn trigger_shorts_prefetch(&self) {
        if self.page != Page::Shorts || self.shorts_grid_view || self.shorts_queue.is_empty() {
            return;
        }
        let quality = self.shorts_quality();
        for offset in 1..=2 {
            if let Some(next_item) = self.shorts_queue.get(self.shorts_idx + offset) {
                let head_blocks = if offset == 1 { 10 } else { 2 };
                let _ = self.backend.tx.send(Request::PrefetchShort {
                    video_id: next_item.id.clone(),
                    quality,
                    head_blocks,
                });
            }
        }
    }

    pub(super) fn open_short(&mut self, item: VideoItem, list: Vec<VideoItem>) {
        if self.page != Page::Shorts {
            self.prev_page = self.page;
        }
        if self.watch.is_some() && !self.playback.paused {
            if let Some(m) = &self.mpv {
                m.set_paused(true);
            }
            self.shorts_paused_video = true;
        }
        self.page = Page::Shorts;
        self.shorts_grid_view = false;

        let mut shorts_only: Vec<VideoItem> = list
            .into_iter()
            .filter(|x| x.is_short() || x.id == item.id)
            .collect();
        if shorts_only.is_empty() {
            shorts_only.push(item.clone());
        }
        self.shorts_idx = shorts_only
            .iter()
            .position(|x| x.id == item.id)
            .unwrap_or(0);
        self.shorts_queue = shorts_only;
        let need_seq = self.shorts_queue.len() <= 2;
        let seed = item.id.clone();

        self.record_history(item.clone());
        if let Some(m) = &self.shorts_slot.mpv {
            m.set_paused(true);
        }
        self.shorts_opened_at = Some(Instant::now());
        self.shorts_slot.clear_for_new_video(&item.id);
        self.shorts_slot.pending_resume = None;
        let _ = self.backend.tx.send(Request::Watch {
            video_id: item.id,
            quality: self.shorts_quality(),
            sync_history: self.sync_history,
        });
        self.trigger_shorts_prefetch();
        if need_seq {
            let _ = self
                .backend
                .tx
                .send(Request::LoadShortsQueue { seed_video_id: seed });
        }
    }

    fn capture_shorts_outgoing(&mut self) {
        let queue_item = self.shorts_queue.get(self.shorts_idx);
        let watch = self.shorts_slot.watch.as_ref();
        let id = watch
            .map(|w| w.id.as_str())
            .or_else(|| queue_item.map(|q| q.id.as_str()))
            .or(self.shorts_slot.active_watch_id.as_deref());
        let Some(id) = id else {
            return;
        };
        let thumb = queue_item
            .map(|q| q.thumbnail_url.clone())
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg"));
        let title = watch
            .map(|w| w.title.clone())
            .or_else(|| queue_item.map(|q| q.title.clone()))
            .unwrap_or_default();
        let channel = watch
            .map(|w| w.channel.clone())
            .or_else(|| queue_item.map(|q| q.channel.clone()))
            .unwrap_or_default();
        self.shorts_outgoing = Some(ShortsOutgoing {
            thumbnail_url: thumb,
            title,
            channel,
        });
    }

    pub(super) fn step_short(&mut self, delta: isize) {
        if delta < 0 && self.shorts_idx > 0 {
            let next_i = self.shorts_idx - 1;
            if let Some(item) = self.shorts_queue.get(next_i).cloned() {
                self.capture_shorts_outgoing();
                self.shorts_idx = next_i;
                self.shorts_slide_anim = Some((-1, Instant::now()));
                let list = self.shorts_queue.clone();
                self.open_short(item, list);
            }
        } else if delta > 0 {
            if !self.shorts_queue.is_empty() && self.shorts_idx + 1 < self.shorts_queue.len() {
                let next_i = self.shorts_idx + 1;
                if let Some(item) = self.shorts_queue.get(next_i).cloned() {
                    self.capture_shorts_outgoing();
                    self.shorts_idx = next_i;
                    self.shorts_slide_anim = Some((1, Instant::now()));
                    let list = self.shorts_queue.clone();
                    self.open_short(item, list);
                }
            } else if let Some(next) = self
                .shorts_slot
                .watch
                .as_ref()
                .and_then(|w| w.related.first())
                .cloned()
            {
                self.capture_shorts_outgoing();
                self.shorts_slide_anim = Some((1, Instant::now()));
                self.open_short(next, Vec::new());
            }
        }
    }

    pub(super) fn return_from_shorts_to_video(&mut self) {
        if let Some(mpv) = &self.shorts_slot.mpv {
            mpv.set_paused(true);
        }
        self.page = Page::Watch;
        self.watch_scrolled_into_pip = false;
        self.scroll_watch_to_top = true;
        if self.shorts_paused_video {
            if let Some(mpv) = &self.mpv {
                mpv.set_paused(false);
            }
            self.shorts_paused_video = false;
        }
    }

    /// Run `f(app, is_shorts)` with the player slot matching `video_id` temporarily swapped into `self`.
    fn with_matching_slot<R>(
        &mut self,
        video_id: &str,
        f: impl FnOnce(&mut Self, bool) -> R,
    ) -> Option<R> {
        if self.active_watch_id.as_deref() == Some(video_id)
            || self.watch.as_ref().is_some_and(|w| w.id == video_id)
        {
            return Some(f(self, false));
        }
        if self
            .bg_player_tab
            .as_ref()
            .is_some_and(|s| s.matches_video(video_id))
        {
            if let Some(mut bg) = self.bg_player_tab.take() {
                self.swap_active_with(&mut bg);
                let out = f(self, false);
                self.swap_active_with(&mut bg);
                self.bg_player_tab = Some(bg);
                return Some(out);
            }
        }
        if self.shorts_slot.matches_video(video_id) {
            let mut slot = Box::new(PlayerSlot::new(1.0));
            std::mem::swap(&mut self.shorts_slot, &mut slot);
            self.swap_active_with(&mut slot);
            let out = f(self, true);
            self.swap_active_with(&mut slot);
            std::mem::swap(&mut self.shorts_slot, &mut slot);
            return Some(out);
        }
        None
    }

    fn apply_watch_ready(
        &mut self,
        mut data: Box<WatchData>,
        selected_video_url: String,
        selected_audio_url: Option<String>,
        selected_format_idx: Option<usize>,
        used_ytdlp: bool,
        is_shorts: bool,
    ) {
        if let (Some(cid), Some(av)) = (&data.channel_id, &data.channel_avatar_url) {
            self.channel_avatars.insert(cid.clone(), av.clone());
        } else if data.channel_avatar_url.is_none() {
            if let Some(cid) = data.channel_id.as_ref().filter(|c| c.starts_with("UC")) {
                if let Some(cached) = self.channel_avatars.get(cid) {
                    data.channel_avatar_url = Some(cached.clone());
                } else if self.avatar_requested.insert(cid.clone()) {
                    let _ = self.backend.tx.send(Request::ResolveChannelAvatars {
                        channel_ids: vec![cid.clone()],
                    });
                }
            } else {
                let ch_low = data.channel.trim().to_ascii_lowercase();
                if !ch_low.is_empty() {
                    let fallback = self
                        .home_items
                        .iter()
                        .chain(self.subs_items.iter())
                        .chain(self.history.iter())
                        .find(|it| {
                            it.channel_avatar_url.is_some()
                                && it.channel.trim().to_ascii_lowercase() == ch_low
                        })
                        .and_then(|it| it.channel_avatar_url.clone());
                    if fallback.is_some() {
                        data.channel_avatar_url = fallback;
                    }
                }
            }
        }
        self.ingest_and_request_avatars(&mut data.related);
        self.watch_loading = false;
        self.selected_audio_idx = 0;
        self.selected_caption_idx = None;
        self.used_ytdlp = used_ytdlp;

        // Play Shorts directly at the resolved quality (avoid jarring 2x-3x reload stutters).
        let is_hls_master = data.hls_url.as_deref() == Some(selected_video_url.as_str());
        let initial_video_url = selected_video_url;
        let initial_audio_url = selected_audio_url;
        let initial_format_idx = selected_format_idx;
        self.selected_format_idx = initial_format_idx;
        self.abr_initial_format_idx = initial_format_idx;
        self.abr_target_height = initial_format_idx
            .and_then(|i| data.video_formats.get(i))
            .map(|f| f.height)
            .or_else(|| {
                (!is_shorts && !data.is_live).then_some(self.preferred_quality.max_height())
            });
        self.abr_downgraded = false;
        self.abr_upgrade_attempts = 0;
        self.abr_tried_formats.clear();
        self.buffering_since = None;
        self.last_abr_switch_at = None;
        self.stable_playback_since = None;
        self.last_abr_progress_pos = 0.0;

        let start_at = if data.is_live || is_shorts {
            None
        } else {
            let valid_resume = |s: f64| -> bool {
                s > 2.0 && (data.duration_secs <= 0.0 || s < data.duration_secs * 0.94)
            };
            let res = self
                .pending_resume
                .take()
                .filter(|(id, _, _)| id == &data.id)
                .and_then(|(_, secs, pct)| {
                    if valid_resume(secs) {
                        Some(secs)
                    } else if let Some(p) =
                        pct.filter(|&p| p > 0.02 && p < 0.94 && data.duration_secs > 10.0)
                    {
                        Some(f64::from(p) * data.duration_secs)
                    } else {
                        None
                    }
                })
                .or_else(|| data.resume_secs.filter(|&s| valid_resume(s)))
                .or_else(|| {
                    self.resume_positions
                        .get(&data.id)
                        .copied()
                        .filter(|&s| valid_resume(s))
                });
            if res.is_none() {
                if let Some(&saved) = self.resume_positions.get(&data.id) {
                    if data.duration_secs > 10.0 && saved >= data.duration_secs * 0.94 {
                        self.resume_positions.remove(&data.id);
                    }
                }
            }
            res
        };

        self.playing_hls_master = is_hls_master;
        let target_fmt = if is_hls_master {
            initial_format_idx.and_then(|i| data.video_formats.get(i))
        } else {
            None
        };
        self.pending_hls_target_height = target_fmt.map(|f| f.height);
        let suppress_resume_toast = self.start_paused;
        if let Some(mpv) = &self.mpv {
            mpv.set_cache_pause_wait(if is_shorts { 0.3 } else { 1.5 });
            mpv.set_hls_bitrate(target_fmt.map(|f| f.bitrate));
            let mpv_start = if data.is_live {
                None
            } else {
                Some(start_at.unwrap_or(0.0))
            };
            mpv.load_stream(
                &initial_video_url,
                initial_audio_url.as_deref(),
                mpv_start,
            );
            mpv.set_speed(self.speed);
            mpv.set_loop_file(is_shorts);
            if self.start_paused || (is_shorts && (self.page != Page::Shorts || self.shorts_grid_view)) {
                mpv.set_paused(true);
            }
            self.start_paused = false;
        }
        self.active_stream_urls = (initial_video_url, initial_audio_url);
        self.watch_started_at = Some(Instant::now());
        if !suppress_resume_toast {
            if let Some(s) = start_at {
                self.toast(format!("Resumed at {}", format_duration(s)));
            }
        }
        let hist_item = VideoItem {
            id: data.id.clone(),
            title: data.title.clone(),
            channel: data.channel.clone(),
            channel_id: data.channel_id.clone(),
            channel_avatar_url: data.channel_avatar_url.clone(),
            views_text: data.views_text.clone(),
            published_text: data.published_text.clone(),
            duration_text: if is_shorts {
                "Short".to_owned()
            } else {
                format_duration(data.duration_secs)
            },
            thumbnail_url: format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", data.id),
            is_live: data.is_live,
            is_new: false,
            is_members_only: data.is_members_only,
            watched_percent: start_at
                .and_then(|s| (data.duration_secs > 10.0).then_some((s / data.duration_secs) as f32)),
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
        if !is_shorts {
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
        }
        self.related_base = data.related.clone();
        self.up_next_active_chip = usize::MAX;
        self.up_next_loading = false;
        let auto_chip_req = if !is_shorts && !data.is_live && self.up_next_mode != UpNextFilterMode::All
        {
            let matching_chip = data.related_chips.iter().find(|c| {
                UpNextFilterMode::classify_chip(&c.title, &data.channel) == Some(self.up_next_mode)
            });
            let chip_title = matching_chip
                .map(|c| c.title.clone())
                .unwrap_or_else(|| match self.up_next_mode {
                    UpNextFilterMode::All => "All".to_owned(),
                    UpNextFilterMode::FromChannel => format!("From {}", data.channel),
                    UpNextFilterMode::Related => "Related".to_owned(),
                    UpNextFilterMode::ForYou => "For you".to_owned(),
                });
            let cont = matching_chip.and_then(|c| c.continuation.clone());
            Some(Request::LoadRelatedChip {
                video_id: data.id.clone(),
                mode: self.up_next_mode,
                chip_title,
                continuation: cont,
                channel_name: data.channel.clone(),
                video_title: data.title.clone(),
                base_related: self.related_base.clone(),
                home_fallback: self.home_items.clone(),
            })
        } else {
            None
        };
        self.watch = Some(*data);
        if let Some(req) = auto_chip_req {
            self.up_next_loading = true;
            let _ = self.backend.tx.send(req);
        }
        if is_shorts {
            self.trigger_shorts_prefetch();
        }
        self.maybe_check_dropboard(true);
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
        self.playing_hls_master = false;
        self.pending_hls_target_height = None;
        self.stream_recovery_attempts = 0;
        self.stream_recovering = None;
        self.buffering_since = None;
        self.last_abr_switch_at = None;
        self.stable_playback_since = None;
        self.last_abr_progress_pos = 0.0;
        self.abr_initial_format_idx = None;
        self.abr_target_height = None;
        self.abr_downgraded = false;
        self.abr_upgrade_attempts = 0;
        self.abr_tried_formats.clear();
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
        self.related_base.clear();
        self.up_next_active_chip = usize::MAX;
        self.up_next_loading = false;
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
            dislikes_text: None,
            is_liked: false,
            is_disliked: false,
            is_subscribed: false,
            notifications: ChannelNotificationParams::default(),
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
            related_chips: Vec::new(),
            is_live: false,
            is_members_only: false,
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
        self.start_paused = false;
        self.watch_started_at = None;
        self.active_stream_urls = (String::new(), None);
        self.playing_hls_master = false;
        self.pending_hls_target_height = None;
        self.stream_recovery_attempts = 0;
        self.stream_recovering = None;
        self.buffering_since = None;
        self.last_abr_switch_at = None;
        self.stable_playback_since = None;
        self.last_abr_progress_pos = 0.0;
        self.abr_initial_format_idx = None;
        self.abr_target_height = None;
        self.abr_downgraded = false;
        self.abr_upgrade_attempts = 0;
        self.abr_tried_formats.clear();
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
        self.related_base.clear();
        self.up_next_active_chip = usize::MAX;
        self.up_next_loading = false;
        if !(self.right_tab == RightTab::Queue && !self.queue.is_empty()) {
            self.right_tab = RightTab::UpNext;
        }
        let _ = self.backend.tx.send(Request::Watch {
            video_id: video_id.to_owned(),
            quality: self.preferred_quality,
            sync_history: self.sync_history,
        });
        if self.dearrow_enabled || self.ryd_enabled {
            let _ = self.backend.tx.send(Request::LoadCommunityMeta {
                video_id: video_id.to_owned(),
                dearrow: self.dearrow_enabled,
                ryd: self.ryd_enabled,
            });
        }
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
            if q.contains("/shorts/") {
                let item = VideoItem {
                    id: vid.clone(),
                    title: format!("Short {vid}"),
                    channel: String::new(),
                    channel_id: None,
                    channel_avatar_url: None,
                    views_text: String::new(),
                    published_text: String::new(),
                    duration_text: "Short".to_owned(),
                    thumbnail_url: format!("https://i.ytimg.com/vi/{vid}/mqdefault.jpg"),
                    is_live: false,
                    watched_percent: None,
                    resume_secs: None,
                    playlist_browse_id: None,
                    playlist_set_video_id: None,
                    is_new: false,
                    is_members_only: false,
                };
                self.open_short(item, Vec::new());
            } else {
                self.open_video_id(&vid);
            }
            return;
        }
        self.search_history
            .retain(|h| !h.eq_ignore_ascii_case(&q));
        self.search_history.insert(0, q.clone());
        self.search_history.truncate(15);

        self.active_query = q.clone();
        self.page = Page::Search;
        self.search_filter = SearchFilter::default();
        self.search_filters_open = false;
        self.search_channels.clear();
        self.search_items.clear();
        self.search_continuation = None;
        self.search_loading = true;
        self.last_error = None;
        let _ = self.backend.tx.send(Request::Search {
            query: q,
            params: innertube::encode_search_params(self.search_filter),
            continuation: None,
        });
    }

    pub(super) fn trigger_filtered_search(&mut self) {
        if self.active_query.trim().is_empty() {
            return;
        }
        self.search_channels.clear();
        self.search_items.clear();
        self.search_continuation = None;
        self.search_loading = true;
        self.last_error = None;
        let _ = self.backend.tx.send(Request::Search {
            query: self.active_query.clone(),
            params: innertube::encode_search_params(self.search_filter),
            continuation: None,
        });
    }

    pub(super) fn switch_quality(&mut self, idx: usize) {
        self.switch_quality_inner(idx, true, true);
    }

    fn switch_quality_inner(&mut self, idx: usize, notify_toast: bool, is_manual: bool) {
        let Some(w) = &self.watch else { return };
        let Some(fmt) = w.video_formats.get(idx) else { return };
        let label = fmt.label();
        let target_h = fmt.height;
        let target_fps = fmt.fps;
        let target_bps = fmt.bitrate;

        if is_manual {
            self.abr_initial_format_idx = Some(idx);
            self.abr_target_height = Some(target_h);
            self.abr_downgraded = false;
            self.abr_upgrade_attempts = 0;
            self.abr_tried_formats.clear();
            self.buffering_since = None;
            self.stable_playback_since = None;
        }

        if self.playing_hls_master {
            if let Some(mpv) = &self.mpv {
                let tracks = mpv.video_tracks();
                let picked = tracks
                    .iter()
                    .filter(|t| t.height == target_h)
                    .min_by_key(|t| (t.fps.round() as i32 - target_fps as i32).abs())
                    .or_else(|| {
                        tracks
                            .iter()
                            .filter(|t| t.height > 0 && t.height <= target_h)
                            .max_by_key(|t| t.height)
                    });
                if let Some(tr) = picked {
                    mpv.set_vid(Some(tr.id));
                    self.pending_hls_target_height = None;
                    self.selected_format_idx = Some(idx);
                    if notify_toast {
                        self.toast(format!("Quality: {label}"));
                    }
                    return;
                }
                if let Some(hls) = w.hls_url.as_deref() {
                    let pos = self.playback.time_pos.max(0.0);
                    mpv.set_hls_bitrate((target_bps > 0).then_some(target_bps));
                    mpv.load_stream(hls, None, (!w.is_live).then_some(pos));
                    self.pending_hls_target_height = Some(target_h);
                    self.selected_format_idx = Some(idx);
                    if notify_toast {
                        self.toast(format!("Quality: {label}"));
                    }
                    return;
                }
            }
        }

        let video_url = self.backend.proxy_url(&fmt.url, fmt.content_length);
        let audio = if fmt.has_audio {
            None
        } else {
            self.selected_audio_stream(w)
        };
        let pos = self.playback.time_pos.max(0.0);
        if let Some(mpv) = &self.mpv {
            mpv.set_hls_bitrate(None);
            mpv.load_stream(
                &video_url,
                audio.as_deref(),
                (!w.is_live).then_some(pos),
            );
        }
        self.playing_hls_master = false;
        self.pending_hls_target_height = None;
        self.active_stream_urls = (video_url, audio);
        self.selected_format_idx = Some(idx);
        if notify_toast {
            self.toast(format!("Quality: {label}"));
        }
    }

    pub(super) fn switch_quality_auto(&mut self) {
        self.switch_quality_auto_inner(true, true);
    }

    fn switch_quality_auto_inner(&mut self, notify_toast: bool, is_manual: bool) {
        let Some(w) = &self.watch else { return };
        let Some(hls) = w.hls_url.as_deref() else { return };
        if is_manual {
            self.abr_initial_format_idx = None;
            self.abr_target_height = None;
            self.abr_downgraded = false;
            self.abr_upgrade_attempts = 0;
            self.abr_tried_formats.clear();
            self.buffering_since = None;
            self.stable_playback_since = None;
        }
        if let Some(mpv) = &self.mpv {
            mpv.set_hls_bitrate(None);
            if self.playing_hls_master {
                mpv.set_vid(None);
            } else {
                let pos = self.playback.time_pos.max(0.0);
                mpv.load_stream(hls, None, (!w.is_live).then_some(pos));
                self.playing_hls_master = true;
                self.active_stream_urls = (hls.to_owned(), None);
            }
        }
        self.pending_hls_target_height = None;
        self.selected_format_idx = None;
        if notify_toast {
            self.toast("Quality: Auto");
        }
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
                let pos = self.playback.time_pos.max(0.0);
                if let Some(mpv) = &self.mpv {
                    mpv.load_stream(&video_url, Some(&audio), (!w.is_live).then_some(pos));
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
        patch(&mut self.shorts_items);
        patch(&mut self.shorts_queue);
        if let Some(w) = &mut self.watch {
            if w.channel_avatar_url.is_none() {
                if let Some(cid) = &w.channel_id {
                    if let Some(av) = map.get(cid) {
                        w.channel_avatar_url = Some(av.clone());
                    }
                }
            }
            patch(&mut w.related);
        }
        if let Some(bg) = &mut self.bg_player_tab {
            if let Some(w) = &mut bg.watch {
                if w.channel_avatar_url.is_none() {
                    if let Some(cid) = &w.channel_id {
                        if let Some(av) = map.get(cid) {
                            w.channel_avatar_url = Some(av.clone());
                        }
                    }
                }
                patch(&mut w.related);
            }
        }
        if let Some(w) = &mut self.shorts_slot.watch {
            if w.channel_avatar_url.is_none() {
                if let Some(cid) = &w.channel_id {
                    if let Some(av) = map.get(cid) {
                        w.channel_avatar_url = Some(av.clone());
                    }
                }
            }
            patch(&mut w.related);
        }
        for n in &mut self.notifications {
            if n.channel_avatar_url.is_none() {
                if let Some(cid) = &n.channel_id {
                    if let Some(av) = map.get(cid) {
                        n.channel_avatar_url = Some(av.clone());
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
                } else if it.duration_text == "Short" || self.page == Page::Shorts {
                    let list = match self.page {
                        Page::Shorts => self.shorts_items.clone(),
                        Page::Home => self.prioritized_home_shorts(),
                        Page::Subscriptions => self.subs_items.clone(),
                        Page::Trending => self.trending_items.clone(),
                        Page::Channel => self.channel_items.clone(),
                        Page::Search => self.search_items.clone(),
                        _ => Vec::new(),
                    };
                    self.open_short(it, list);
                } else {
                    self.open_video(it);
                }
            }
            GridAction::OpenInSecondTab(it) => {
                self.open_video_in_second_tab(ctx, it);
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
            Page::Shorts => NavEntry::Shorts,
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
                        append: false,
                    });
                }
            }
            NavEntry::Shorts => {
                self.open_shorts_page();
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
                if self.active_query == query
                    && (!self.search_items.is_empty() || !self.search_channels.is_empty())
                {
                    self.page = Page::Search;
                } else if !query.is_empty() {
                    self.search_input = query.clone();
                    self.active_query = query.clone();
                    self.page = Page::Search;
                    self.search_channels.clear();
                    self.search_items.clear();
                    self.search_continuation = None;
                    self.search_loading = true;
                    let _ = self.backend.tx.send(Request::Search {
                        query,
                        params: innertube::encode_search_params(self.search_filter),
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
        } else if self.page == Page::Watch || self.page == Page::Shorts {
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
                Response::ShortsLoaded { mut items } => {
                    self.shorts_loading = false;
                    self.ingest_resume_metadata(&mut items);
                    self.ingest_and_request_avatars(&mut items);
                    self.shorts_items = items.clone();
                    if self.page == Page::Shorts && !self.shorts_grid_view {
                        if self.shorts_slot.watch.is_none()
                            && self.shorts_slot.active_watch_id.is_none()
                        {
                            if let Some(first) = items.first().cloned() {
                                self.open_short(first, items);
                            }
                        } else {
                            for it in items {
                                if !self.shorts_queue.iter().any(|q| q.id == it.id) {
                                    self.shorts_queue.push(it);
                                }
                            }
                            self.trigger_shorts_prefetch();
                        }
                    }
                }
                Response::ShortsQueueLoaded {
                    seed_video_id,
                    mut items,
                } => {
                    if self.shorts_slot.matches_video(&seed_video_id) {
                        self.ingest_and_request_avatars(&mut items);
                        for it in items {
                            if !self.shorts_queue.iter().any(|x| x.id == it.id) {
                                self.shorts_queue.push(it);
                            }
                        }
                        self.trigger_shorts_prefetch();
                    }
                }
                Response::ChannelLoaded {
                    channel_id,
                    info,
                    mut items,
                    chips,
                    continuation,
                    append,
                } => {
                    if self.channel_id.as_deref() == Some(channel_id.as_str()) {
                        self.channel_loading = false;
                        self.ingest_resume_metadata(&mut items);
                        if !info.name.is_empty() && info.name != "Channel" {
                            self.channel_name = info.name.clone();
                        }
                        if !info.tabs.is_empty()
                            || info.avatar_url.is_some()
                            || !info.subscribers_text.is_empty()
                            || self.channel_info.is_none()
                        {
                            self.channel_info = Some(info);
                        }
                        if !chips.is_empty() {
                            self.channel_chips = chips;
                        }
                        let ch_avatar = self
                            .channel_info
                            .as_ref()
                            .and_then(|i| i.avatar_url.clone());
                        for it in &mut items {
                            if it.channel.is_empty() && !self.channel_name.is_empty() {
                                it.channel = self.channel_name.clone();
                            }
                            if it.channel_avatar_url.is_none() {
                                it.channel_avatar_url = ch_avatar.clone();
                            }
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
                    params,
                    channels,
                    mut items,
                    continuation,
                    append,
                } => {
                    if query == self.active_query
                        && params == innertube::encode_search_params(self.search_filter)
                    {
                        self.search_loading = false;
                        self.ingest_resume_metadata(&mut items);
                        self.ingest_and_request_avatars(&mut items);
                        for ch in &channels {
                            if let Some(url) = &ch.avatar_url {
                                self.channel_avatars
                                    .entry(ch.id.clone())
                                    .or_insert_with(|| url.clone());
                            }
                        }
                        if append {
                            for ch in channels {
                                if !self.search_channels.iter().any(|x| x.id == ch.id) {
                                    self.search_channels.push(ch);
                                }
                            }
                            for it in items {
                                if !self.search_items.iter().any(|x| x.id == it.id) {
                                    self.search_items.push(it);
                                }
                            }
                        } else {
                            self.search_channels = channels;
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
                Response::HoverPreviewReady {
                    video_id,
                    video_url,
                    audio_url,
                    duration_secs,
                } => {
                    if self.hover_preview_quality != HoverPreviewQuality::Off
                        && self.hover_preview_loading_id.as_deref() == Some(video_id.as_str())
                    {
                        if self.hover_preview_mpv.is_none() && self.mpv_error.is_none() {
                            if let Some(gpa) = self.get_proc_address.clone() {
                                match MpvPlayer::new_preview(
                                    &self.backend.data_dir,
                                    gpa,
                                    ctx.clone(),
                                ) {
                                    Ok(player) => {
                                        player.set_hwdec(self.hwdec_mode);
                                        player.set_volume(self.volume);
                                        player.set_mute(self.hover_preview_muted);
                                        self.hover_preview_mpv = Some(player);
                                    }
                                    Err(e) => {
                                        log::warn!("hover preview mpv init failed: {e:#}");
                                        self.hover_preview_loading_id = None;
                                    }
                                }
                            } else {
                                self.hover_preview_loading_id = None;
                            }
                        }
                        if let Some(mpv) = &self.hover_preview_mpv {
                            mpv.set_hwdec(self.hwdec_mode);
                            mpv.set_volume(self.volume);
                            mpv.set_mute(self.hover_preview_muted);
                            mpv.load_stream(&video_url, audio_url.as_deref(), Some(0.0));
                            self.hover_preview_active_id = Some(video_id);
                            self.hover_preview_loading_id = None;
                            self.hover_preview_duration = duration_secs;
                            self.hover_preview_playback = PlaybackState::default();
                        }
                    }
                }
                Response::HoverPreviewFailed { video_id } => {
                    if self.hover_preview_loading_id.as_deref() == Some(video_id.as_str()) {
                        self.hover_preview_loading_id = None;
                    }
                }
                Response::WatchReady {
                    data,
                    selected_video_url,
                    selected_audio_url,
                    selected_format_idx,
                    used_ytdlp,
                } => {
                    if self.shorts_slot.matches_video(&data.id) {
                        self.shorts_slot.ensure_mpv(
                            &self.backend.data_dir,
                            &self.get_proc_address,
                            ctx,
                            self.volume,
                            self.hwdec_mode,
                            self.sub_font_size,
                            self.sub_bg_opacity,
                        );
                    }
                    let vid = data.id.clone();
                    self.with_matching_slot(&vid, |app, is_shorts| {
                        app.apply_watch_ready(
                            data,
                            selected_video_url,
                            selected_audio_url,
                            selected_format_idx,
                            used_ytdlp,
                            is_shorts,
                        );
                    });
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
                    self.with_matching_slot(&video_id, |app, _is_shorts| {
                        app.stream_recovering = None;
                        if let Some(w) = app.watch.as_mut() {
                            w.video_formats = video_formats;
                            w.audio_url = audio_url;
                            w.audio_content_length = audio_content_length;
                            w.audio_tracks = audio_tracks;
                            w.hls_url = hls_url;
                            app.selected_format_idx = selected_format_idx;
                            app.selected_audio_idx = selected_audio_idx;
                            app.used_ytdlp = used_ytdlp;
                            app.last_error = None;
                            let start_at = (!w.is_live).then_some(resume_secs.max(0.0));
                            if let Some(mpv) = &app.mpv {
                                mpv.load_stream(
                                    &selected_video_url,
                                    selected_audio_url.as_deref(),
                                    start_at,
                                );
                                mpv.set_speed(app.speed);
                            }
                            app.active_stream_urls = (selected_video_url, selected_audio_url);
                            app.watch_started_at = Some(Instant::now());
                            app.buffering_since = None;
                            app.stable_playback_since = None;
                            app.toast("Refreshed expired stream");
                        }
                    });
                }
                Response::StreamRecoverFailed { video_id, message } => {
                    self.with_matching_slot(&video_id, |app, _is_shorts| {
                        app.stream_recovering = None;
                        app.last_error = Some(message);
                    });
                }
                Response::CommentsLoaded {
                    video_id,
                    comments,
                    next_token,
                    sort_options,
                    create_params,
                    append,
                } => {
                    self.with_matching_slot(&video_id, |app, _is_shorts| {
                        app.comments_loading = false;
                        if !sort_options.is_empty() {
                            app.comment_sort_options = sort_options;
                        }
                        if create_params.is_some() {
                            app.comment_create_params = create_params;
                        }
                        if append {
                            app.comments.extend(comments);
                        } else {
                            app.comments = comments;
                        }
                        app.comments_next_token = next_token;
                    });
                }
                Response::CommentRepliesLoaded {
                    video_id,
                    comment_id,
                    replies,
                    next_token,
                } => {
                    self.with_matching_slot(&video_id, |app, _is_shorts| {
                        if let Some(c) = app.comments.iter_mut().find(|c| c.id == comment_id) {
                            c.replies.extend(replies);
                            c.replies_token = next_token;
                            c.replies_loading = false;
                        }
                    });
                }
                Response::CommentPosted { video_id } => {
                    self.with_matching_slot(&video_id, |app, _is_shorts| {
                        app.toast("Comment posted");
                    });
                }
                Response::CommentReplyPosted {
                    video_id,
                    comment_id,
                    text,
                } => {
                    self.with_matching_slot(&video_id, |app, _is_shorts| {
                        let author = app
                            .account_name
                            .clone()
                            .unwrap_or_else(|| "You".to_owned());
                        let avatar = app.account_avatar.clone();
                        if let Some(c) = app.comments.iter_mut().find(|c| c.id == comment_id) {
                            c.replies.push(CommentItem {
                                id: format!("local-reply-{}", c.replies.len()),
                                author,
                                avatar_url: avatar,
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
                        app.toast("Reply posted");
                    });
                }
                Response::CommentActionUpdated {
                    video_id,
                    comment_id,
                    liked,
                } => {
                    self.with_matching_slot(&video_id, |app, _is_shorts| {
                        for c in &mut app.comments {
                            if c.id == comment_id {
                                c.is_liked = liked;
                            }
                            for r in &mut c.replies {
                                if r.id == comment_id {
                                    r.is_liked = liked;
                                }
                            }
                        }
                    });
                }
                Response::LiveChatLoaded {
                    video_id,
                    messages,
                    next_token,
                    send_params,
                    emotes,
                    is_replay,
                } => {
                    self.with_matching_slot(&video_id, |app, _is_shorts| {
                        if send_params.is_some() {
                            app.live_chat_send_params = send_params;
                        }
                        app.live_chat_is_replay |= is_replay;
                        for em in emotes {
                            if !app
                                .live_chat_emotes
                                .iter()
                                .any(|x| x.emoji_id == em.emoji_id)
                            {
                                app.live_chat_emotes.push(em);
                            }
                        }
                        for m in messages {
                            if !app.live_chat.iter().any(|x| x.id == m.id) {
                                app.live_chat.push(m);
                            }
                        }
                        if app.live_chat.len() > 250 {
                            let drop_n = app.live_chat.len() - 250;
                            app.live_chat.drain(..drop_n);
                        }
                        app.live_chat_token = next_token;
                        app.live_chat_next_poll = Some(Instant::now() + Duration::from_secs(3));
                    });
                }
                Response::LiveChatSent { video_id } => {
                    self.with_matching_slot(&video_id, |app, _is_shorts| {
                        app.live_chat_next_poll = Some(Instant::now());
                    });
                }
                Response::HistoryRemoved => {
                    self.toast("Removed from Watch History");
                }
                Response::LikeUpdated { video_id, liked } => {
                    self.with_matching_slot(&video_id, |app, _is_shorts| {
                        if let Some(w) = &mut app.watch {
                            w.is_liked = liked;
                            if liked {
                                w.is_disliked = false;
                            }
                        }
                    });
                    self.toast(if liked {
                        "Added to Liked videos"
                    } else {
                        "Removed like"
                    });
                }
                Response::DislikeUpdated { video_id, disliked } => {
                    self.with_matching_slot(&video_id, |app, _is_shorts| {
                        if let Some(w) = &mut app.watch {
                            w.is_disliked = disliked;
                            if disliked {
                                w.is_liked = false;
                            }
                        }
                    });
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
                    for w in [
                        self.watch.as_mut(),
                        self.bg_player_tab.as_mut().and_then(|b| b.watch.as_mut()),
                        self.shorts_slot.watch.as_mut(),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        if w.channel_id.as_deref() == Some(channel_id.as_str()) {
                            w.is_subscribed = subscribed;
                        }
                    }
                    if let Some(ci) = &mut self.channel_info {
                        if ci.id == channel_id {
                            ci.is_subscribed = subscribed;
                        }
                    }
                    for ch in &mut self.search_channels {
                        if ch.id == channel_id {
                            ch.is_subscribed = subscribed;
                        }
                    }
                    self.toast(if subscribed {
                        "Subscribed"
                    } else {
                        "Unsubscribed"
                    });
                }
                Response::NotificationsCountLoaded(count) => {
                    self.unread_notifications = count;
                }
                Response::NotificationsLoaded {
                    mut items,
                    unseen_count,
                    seen_params,
                } => {
                    self.notifications_loading = false;
                    self.notifications_seen_params = seen_params.clone();
                    self.notifications_loaded_at = Some(Instant::now());
                    if seen_params.is_none() {
                        for n in &mut items {
                            if let Some(vid) = &n.video_id {
                                if self.seen_fallback_notif_ids.contains(vid) {
                                    n.read = true;
                                }
                            }
                        }
                        if self.notifications_open {
                            for n in &items {
                                if let Some(vid) = &n.video_id {
                                    self.seen_fallback_notif_ids.insert(vid.clone());
                                }
                            }
                            self.unread_notifications = 0;
                        } else {
                            self.unread_notifications =
                                items.iter().filter(|n| !n.read).count() as u32;
                        }
                    } else {
                        self.unread_notifications = unseen_count;
                        if self.notifications_open && unseen_count > 0 {
                            if let Some(p) = seen_params {
                                let _ = self
                                    .backend
                                    .tx
                                    .send(Request::MarkNotificationsSeen { params: p });
                            }
                        }
                    }
                    self.notifications = items;
                }
                Response::ChannelNotificationLevelUpdated { channel_id, level } => {
                    for w in [
                        self.watch.as_mut(),
                        self.bg_player_tab.as_mut().and_then(|b| b.watch.as_mut()),
                        self.shorts_slot.watch.as_mut(),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        if w.channel_id.as_deref() == Some(channel_id.as_str()) {
                            w.notifications.level = level;
                        }
                    }
                    if let Some(ci) = &mut self.channel_info {
                        if ci.id == channel_id {
                            ci.notifications.level = level;
                        }
                    }
                    self.toast(format!("Notifications: {}", level.label()));
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
                    self.with_matching_slot(&video_id, |app, _is_shorts| {
                        app.sponsor_segments = segments;
                    });
                }
                Response::DeArrowLoaded {
                    video_id,
                    title,
                    thumbnail_url,
                } => {
                    if let Some(t) = &title {
                        for w in [
                            self.watch.as_mut(),
                            self.bg_player_tab.as_mut().and_then(|b| b.watch.as_mut()),
                            self.shorts_slot.watch.as_mut(),
                        ]
                        .into_iter()
                        .flatten()
                        {
                            if w.id == video_id {
                                w.title = t.clone();
                            }
                        }
                        for list in [
                            &mut self.history,
                            &mut self.saved,
                            &mut self.queue,
                            &mut self.home_items,
                            &mut self.subs_items,
                            &mut self.shorts_items,
                            &mut self.shorts_queue,
                        ] {
                            for it in list.iter_mut().filter(|x| x.id == video_id) {
                                it.title = t.clone();
                            }
                        }
                    }
                    if let Some(thumb) = &thumbnail_url {
                        for list in [
                            &mut self.history,
                            &mut self.saved,
                            &mut self.queue,
                            &mut self.home_items,
                            &mut self.subs_items,
                            &mut self.shorts_items,
                            &mut self.shorts_queue,
                        ] {
                            for it in list.iter_mut().filter(|x| x.id == video_id) {
                                it.thumbnail_url = thumb.clone();
                            }
                        }
                    }
                }
                Response::DislikesLoaded {
                    video_id,
                    dislikes_text,
                } => {
                    self.with_matching_slot(&video_id, |app, _is_shorts| {
                        if let Some(w) = &mut app.watch {
                            w.dislikes_text = Some(dislikes_text);
                        }
                    });
                }
                Response::SignedIn { name, avatar } => {
                    self.signed_in = true;
                    self.signing_in = false;
                    self.account_name = name;
                    self.account_avatar = avatar;
                    self.home_is_fallback = false;
                    let _ = self.backend.tx.send(Request::LoadNotifications);
                    self.toast("Signed in to YouTube");
                }
                Response::SignedOut => {
                    self.reset_signed_out_state();
                    self.toast("Signed out");
                }
                Response::SessionRevoked => {
                    self.reset_signed_out_state();
                    self.toast("YouTube session expired — please sign in again");
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
                                        player.set_subtitle_style(
                                            self.sub_font_size,
                                            self.sub_bg_opacity,
                                        );
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
                Response::AmbientArtReady {
                    key,
                    width,
                    height,
                    rgba,
                    tiled_width,
                    tiled_height,
                    tiled_rgba,
                } => {
                    if self.ambient_requested_key.as_deref() == Some(key.as_str()) {
                        let soft_img = egui::ColorImage::from_rgba_unmultiplied(
                            [width as usize, height as usize],
                            &rgba,
                        );
                        let soft = ctx.load_texture(
                            format!("ambient-soft-{key}"),
                            soft_img,
                            egui::TextureOptions::LINEAR,
                        );
                        let tiled_img = egui::ColorImage::from_rgba_unmultiplied(
                            [tiled_width as usize, tiled_height as usize],
                            &tiled_rgba,
                        );
                        let tiled_opts = egui::TextureOptions {
                            magnification: egui::TextureFilter::Linear,
                            minification: egui::TextureFilter::Linear,
                            mipmap_mode: None,
                            wrap_mode: egui::TextureWrapMode::Repeat,
                        };
                        let tiled = ctx.load_texture(
                            format!("ambient-tiled-{key}"),
                            tiled_img,
                            tiled_opts,
                        );
                        if let Some((_, old)) = self.ambient_tex_soft.take() {
                            self.ambient_prev_soft = Some(old);
                        }
                        self.ambient_tex_soft = Some((key.clone(), soft));
                        self.ambient_tex_tiled = Some((key, tiled));
                        self.ambient_changed_at = Some(Instant::now());
                    }
                }
                Response::DropBoardStatus(online) => {
                    self.dropboard_online = online;
                }
                Response::ScreenshotDone {
                    sent_to_dropboard,
                    error,
                } => {
                    if sent_to_dropboard {
                        self.dropboard_online = true;
                        self.toast("Screenshot sent to DropBoard");
                    } else if let Some(err) = error {
                        self.toast(format!("Screenshot failed: {err}"));
                    } else {
                        self.dropboard_online = false;
                        self.toast("Screenshot copied to clipboard");
                    }
                }
                Response::RelatedChipLoaded {
                    video_id,
                    mode,
                    chip_title,
                    mut items,
                } => {
                    let home_avatars = &mut items;
                    let _ = home_avatars;
                    self.with_matching_slot(&video_id, |app, _is_shorts| {
                        app.up_next_loading = false;
                        app.ingest_and_request_avatars(&mut items);
                        if let Some(w) = &mut app.watch {
                            if !items.is_empty() || mode == UpNextFilterMode::All {
                                w.related = items;
                            }
                            for c in &mut w.related_chips {
                                c.selected = c.title.eq_ignore_ascii_case(&chip_title);
                            }
                        }
                    });
                }
                Response::Error(msg) => {
                    self.home_loading = false;
                    self.subs_loading = false;
                    self.playlist_loading = false;
                    self.remote_history_loading = false;
                    self.trending_loading = false;
                    self.shorts_loading = false;
                    self.channel_loading = false;
                    self.search_loading = false;
                    self.watch_loading = false;
                    if let Some(bg) = &mut self.bg_player_tab {
                        bg.watch_loading = false;
                    }
                    self.shorts_slot.watch_loading = false;
                    self.comments_loading = false;
                    self.notifications_loading = false;
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
        if let Some(slot) = self.rebinding_shortcut {
            let captured = ctx.input(|i| {
                if i.key_pressed(Key::Escape) {
                    return Some(None);
                }
                for ev in &i.events {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = ev
                    {
                        return Some(Some(KeyBinding {
                            key: KeyBinding::key_name(*key).to_owned(),
                            ctrl: modifiers.ctrl,
                            alt: modifiers.alt,
                            shift: modifiers.shift,
                        }));
                    }
                }
                None
            });
            if let Some(opt_bind) = captured {
                self.rebinding_shortcut = None;
                if let Some(bind) = opt_bind {
                    let label = bind.display();
                    self.shortcuts.set(slot, bind);
                    self.toast(format!("{}: {label}", slot.label()));
                }
            }
            return;
        }
        let mut hud_msg: Option<(HudIcon, &'static str)> = None;
        let mut wake_controls = false;
        let mut take_screenshot = false;
        let mut step_short_delta: isize = 0;
        let mut reload_home = false;
        let mut exit_pip = false;
        let mut set_fullscreen: Option<bool> = None;
        ctx.input(|i| {
            if i.key_pressed(Key::Escape) {
                if self.notifications_open {
                    self.notifications_open = false;
                } else if self.player_menu != PlayerMenu::None {
                    self.player_menu = PlayerMenu::None;
                } else if self.win_pip_mode {
                    exit_pip = true;
                } else if self.fullscreen {
                    self.fullscreen = false;
                    set_fullscreen = Some(false);
                }
            }
            if self.page == Page::Home && i.key_pressed(Key::F5) {
                reload_home = true;
            }
            if self.page == Page::Shorts && !self.shorts_grid_view {
                if i.key_pressed(Key::ArrowUp) {
                    step_short_delta = -1;
                }
                if i.key_pressed(Key::ArrowDown) {
                    step_short_delta = 1;
                }
                if self.shortcuts.play_pause.matches(i) || self.shortcuts.play_pause_alt.matches(i)
                {
                    if let Some(m) = &self.shorts_slot.mpv {
                        m.toggle_pause();
                        hud_msg = Some(if self.shorts_slot.playback.paused {
                            (HudIcon::Play, "Playing")
                        } else {
                            (HudIcon::Pause, "Paused")
                        });
                        wake_controls = true;
                    }
                }
                if self.shortcuts.mute.matches(i) {
                    self.shorts_slot.muted = !self.shorts_slot.muted;
                    if let Some(m) = &self.shorts_slot.mpv {
                        m.set_mute(self.shorts_slot.muted);
                    }
                    hud_msg = Some(if self.shorts_slot.muted {
                        (HudIcon::VolumeMuted, "Muted")
                    } else {
                        (HudIcon::VolumeOn, "Unmuted")
                    });
                    wake_controls = true;
                }
                if self.shortcuts.seek_back.matches(i) {
                    if let Some(m) = &self.shorts_slot.mpv {
                        m.seek_rel(-5.0);
                        hud_msg = Some((HudIcon::SeekBack, "-5s"));
                        wake_controls = true;
                    }
                }
                if self.shortcuts.seek_forward.matches(i) {
                    if let Some(m) = &self.shorts_slot.mpv {
                        m.seek_rel(5.0);
                        hud_msg = Some((HudIcon::SeekForward, "+5s"));
                        wake_controls = true;
                    }
                }
                if self.shortcuts.screenshot.matches(i) {
                    take_screenshot = true;
                }
                return;
            }
            if self.page != Page::Watch {
                return;
            }
            if self.shortcuts.screenshot.matches(i) {
                take_screenshot = true;
            }
            if self.shortcuts.stats_for_nerds.matches(i) {
                self.show_stats_for_nerds = !self.show_stats_for_nerds;
            }
            if self.shortcuts.play_pause.matches(i) || self.shortcuts.play_pause_alt.matches(i) {
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
            if self.shortcuts.seek_back.matches(i) {
                if let Some(m) = &self.mpv {
                    m.seek_rel(-5.0);
                    hud_msg = Some((HudIcon::SeekBack, "-5s"));
                    wake_controls = true;
                }
            }
            if self.shortcuts.seek_forward.matches(i) {
                if let Some(m) = &self.mpv {
                    m.seek_rel(5.0);
                    hud_msg = Some((HudIcon::SeekForward, "+5s"));
                    wake_controls = true;
                }
            }
            if self.shortcuts.seek_back_big.matches(i) {
                if let Some(m) = &self.mpv {
                    m.seek_rel(-10.0);
                    hud_msg = Some((HudIcon::SeekBack, "-10s"));
                    wake_controls = true;
                }
            }
            if self.shortcuts.seek_forward_big.matches(i) {
                if let Some(m) = &self.mpv {
                    m.seek_rel(10.0);
                    hud_msg = Some((HudIcon::SeekForward, "+10s"));
                    wake_controls = true;
                }
            }
            if self.shortcuts.mute.matches(i) {
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
            if self.shortcuts.theater.matches(i) && !self.fullscreen {
                self.theater_mode = !self.theater_mode;
            }
            if self.shortcuts.fullscreen.matches(i) {
                self.fullscreen = !self.fullscreen;
                set_fullscreen = Some(self.fullscreen);
                wake_controls = true;
            }
            if self.shortcuts.toggle_captions.matches(i) {
                if let Some(w) = &self.watch {
                    if !w.captions.is_empty() {
                        let next = if self.selected_caption_idx.is_some() {
                            None
                        } else {
                            Some(0)
                        };
                        self.selected_caption_idx = next;
                        let url = next.and_then(|idx| w.captions.get(idx)).map(|c| c.vtt_url.as_str());
                        if let Some(m) = &self.mpv {
                            m.set_subtitle(url);
                        }
                        wake_controls = true;
                    }
                }
            }
            if self.shortcuts.volume_up.matches(i) {
                self.volume = (self.volume + 5.0).min(100.0);
                if let Some(m) = &self.mpv {
                    m.set_volume(self.volume);
                }
                wake_controls = true;
            }
            if self.shortcuts.volume_down.matches(i) {
                self.volume = (self.volume - 5.0).max(0.0);
                if let Some(m) = &self.mpv {
                    m.set_volume(self.volume);
                }
                wake_controls = true;
            }
        });
        if exit_pip {
            self.exit_win_pip(ctx);
        }
        if let Some(fs) = set_fullscreen {
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(fs));
        }
        if reload_home {
            self.reload_home_feed();
        }
        if step_short_delta != 0 {
            self.step_short(step_short_delta);
        }
        if take_screenshot {
            self.capture_current_page_screenshot();
        }
        if let Some((ico, msg)) = hud_msg {
            self.trigger_hud(ico, msg);
        }
        if wake_controls {
            self.last_pointer_move = Instant::now();
        }
    }

    pub(super) fn maybe_check_dropboard(&mut self, force: bool) {
        let due = force
            || self
                .dropboard_last_check
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(5));
        if due {
            self.dropboard_last_check = Some(Instant::now());
            let _ = self.backend.tx.send(Request::CheckDropBoard);
        }
    }

    pub(super) fn capture_current_page_screenshot(&mut self) {
        if self.page == Page::Shorts && !self.shorts_grid_view {
            let mut slot = Box::new(PlayerSlot::new(1.0));
            std::mem::swap(&mut self.shorts_slot, &mut slot);
            self.swap_active_with(&mut slot);
            self.capture_video_screenshot();
            self.swap_active_with(&mut slot);
            std::mem::swap(&mut self.shorts_slot, &mut slot);
        } else {
            self.capture_video_screenshot();
        }
    }

    pub(super) fn capture_video_screenshot(&mut self) {
        let Some(mpv) = &self.mpv else {
            self.toast("No active video player");
            return;
        };
        let title = match &self.watch {
            Some(w) if self.playback.time_pos > 0.5 => {
                format!("{} @ {}", w.title, format_duration(self.playback.time_pos))
            }
            Some(w) => w.title.clone(),
            None => "YouTube Screenshot".to_owned(),
        };
        match mpv.capture_frame_png(&self.backend.data_dir) {
            Ok(png_bytes) => {
                let _ = self.backend.tx.send(Request::SendScreenshot { png_bytes, title });
            }
            Err(e) => {
                self.toast(format!("Screenshot failed: {e:#}"));
            }
        }
    }

    pub(super) fn select_up_next_chip(
        &mut self,
        idx: usize,
        mode: UpNextFilterMode,
        chip_title: String,
        continuation: Option<String>,
    ) {
        self.up_next_active_chip = idx;
        self.up_next_mode = mode;
        let Some(w) = &mut self.watch else {
            return;
        };
        if mode == UpNextFilterMode::All && continuation.is_none() && !self.related_base.is_empty() {
            w.related = self.related_base.clone();
            self.up_next_loading = false;
            return;
        }
        let req = Request::LoadRelatedChip {
            video_id: w.id.clone(),
            mode,
            chip_title,
            continuation,
            channel_name: w.channel.clone(),
            video_title: w.title.clone(),
            base_related: self.related_base.clone(),
            home_fallback: self.home_items.clone(),
        };
        self.up_next_loading = true;
        let _ = self.backend.tx.send(req);
    }

    pub(super) fn set_always_on_top(&mut self, ctx: &egui::Context, on: bool) {
        self.always_on_top = on;
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(if on {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        }));
        self.toast(if on {
            "Keep on Top: On"
        } else {
            "Keep on Top: Off"
        });
    }

    pub(super) fn toggle_win_pip(&mut self, ctx: &egui::Context) {
        if self.win_pip_mode {
            self.exit_win_pip(ctx);
        } else {
            self.enter_win_pip(ctx);
        }
    }

    pub(super) fn enter_win_pip(&mut self, ctx: &egui::Context) {
        if self.win_pip_mode {
            return;
        }
        if self.watch.is_none() {
            if self
                .bg_player_tab
                .as_ref()
                .is_some_and(|b| b.watch.is_some())
            {
                self.switch_player_tab();
            } else {
                self.toast("Play a video first to pop out Windows PiP");
                return;
            }
        }
        let (outer_rect, maximized, monitor_size) = ctx.input(|i| {
            let vp = i.viewport();
            (vp.outer_rect, vp.maximized.unwrap_or(false), vp.monitor_size)
        });
        self.win_pip_saved_rect = outer_rect;
        self.win_pip_saved_maximized = maximized;
        self.win_pip_saved_aot = self.always_on_top;
        self.win_pip_saved_page = self.page;

        if self.fullscreen {
            self.fullscreen = false;
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
        }
        self.win_pip_mode = true;
        self.always_on_top = true;
        self.page = Page::Watch;
        self.watch_scrolled_into_pip = false;
        self.player_menu = PlayerMenu::None;
        self.notifications_open = false;

        let pip_size = egui::vec2(448.0, 252.0);
        let target_pos = if let Some(ms) = monitor_size {
            egui::pos2((ms.x - pip_size.x - 24.0).max(16.0), (ms.y - pip_size.y - 64.0).max(16.0))
        } else if let Some(r) = outer_rect {
            egui::pos2((r.right() - pip_size.x - 16.0).max(16.0), (r.bottom() - pip_size.y - 16.0).max(16.0))
        } else {
            egui::pos2(80.0, 80.0)
        };
        if maximized {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
        }
        ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(pip_size));
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(target_pos));
    }

    pub(super) fn exit_win_pip(&mut self, ctx: &egui::Context) {
        if !self.win_pip_mode {
            return;
        }
        self.win_pip_mode = false;
        self.always_on_top = self.win_pip_saved_aot;
        self.player_menu = PlayerMenu::None;
        ctx.send_viewport_cmd(ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(if self.always_on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        }));
        if let Some(r) = self.win_pip_saved_rect.take() {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(r.size()));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(r.min));
        } else {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(1280.0, 800.0)));
        }
        if self.win_pip_saved_maximized {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
        }
    }

    pub(super) fn sleep_timer_remaining_label(&self) -> Option<String> {
        match self.sleep_timer {
            SleepTimer::Off => None,
            SleepTimer::EndOfVideo => Some("End of video".to_owned()),
            SleepTimer::Until(deadline) => {
                let rem = deadline
                    .saturating_duration_since(Instant::now())
                    .as_secs_f64();
                Some(format_duration(rem.max(1.0)))
            }
        }
    }

    fn pause_all_for_sleep(&mut self, msg: &str) {
        if let Some(mpv) = &self.mpv {
            mpv.set_paused(true);
        }
        if let Some(bg) = &self.bg_player_tab {
            if let Some(mpv) = &bg.mpv {
                mpv.set_paused(true);
            }
        }
        if let Some(mpv) = &self.shorts_slot.mpv {
            mpv.set_paused(true);
        }
        self.sleep_timer = SleepTimer::Off;
        self.toast(msg);
        self.trigger_hud(HudIcon::Pause, "Sleep timer");
    }

    pub(super) fn hide_to_tray(&mut self, ctx: &egui::Context) {
        self.hidden_to_tray = true;
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
    }

    pub(super) fn restore_from_tray(&mut self, ctx: &egui::Context) {
        self.hidden_to_tray = false;
        self.was_minimized = false;
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        if let Some(ws) = &self.win_shell {
            ws.restore_main_window();
        }
    }

    pub(super) fn play_next_video(&mut self) {
        if !self.queue.is_empty() {
            let next = self.queue.remove(0);
            self.persist_queue();
            self.open_video(next);
        } else if let Some(next) = self.watch.as_ref().and_then(|w| w.related.first()).cloned() {
            self.open_video(next);
        }
    }

    pub(super) fn play_previous_video(&mut self) {
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

    pub(super) fn toggle_play_pause(&mut self) {
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

    fn handle_win_shell(&mut self, ctx: &egui::Context) {
        let mut events = Vec::new();
        if let Some(ws) = &self.win_shell {
            while let Some(ev) = ws.try_recv() {
                events.push(ev);
            }
        }
        for ev in events {
            match ev {
                ShellEvent::ShowWindow => {
                    if self.hidden_to_tray {
                        self.restore_from_tray(ctx);
                    } else if self.close_to_tray || self.minimize_to_tray {
                        self.hide_to_tray(ctx);
                    } else {
                        self.restore_from_tray(ctx);
                    }
                }
                ShellEvent::TogglePlayPause => self.toggle_play_pause(),
                ShellEvent::Prev => self.play_previous_video(),
                ShellEvent::Next => self.play_next_video(),
                ShellEvent::ToggleAlwaysOnTop => self.set_always_on_top(ctx, !self.always_on_top),
                ShellEvent::Quit => {
                    self.quitting_from_tray = true;
                    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }

        // Intercept window Close / Minimize when tray options are enabled
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && self.close_to_tray && !self.quitting_from_tray {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.hide_to_tray(ctx);
        }
        let minimized = ctx.input(|i| i.viewport().minimized) == Some(true);
        if minimized && !self.was_minimized && self.minimize_to_tray && !self.hidden_to_tray {
            self.hide_to_tray(ctx);
        }
        self.was_minimized = minimized;

        if let Some(ws) = &mut self.win_shell {
            let title = if self.page == Page::Watch {
                self.watch.as_ref().map(|w| w.title.as_str())
            } else {
                None
            };
            ws.sync_state(
                self.playback.paused,
                !self.hidden_to_tray,
                self.always_on_top,
                title,
            );
            ws.sync_window_material(self.window_material, self.win_pip_mode);
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
                    MediaControlEvent::Toggle => self.toggle_play_pause(),
                    MediaControlEvent::Next => self.play_next_video(),
                    MediaControlEvent::Previous => self.play_previous_video(),
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

    fn poll_active_stream_health(&mut self) {
        if self.watch.is_none() || self.mpv.is_none() {
            return;
        }
        let was_eof = self.playback.eof;
        if let Some(mpv) = &self.mpv {
            self.playback = mpv.poll_state();
            if self.playing_hls_master {
                if let Some(target_h) = self.pending_hls_target_height {
                    let tracks = mpv.video_tracks();
                    if !tracks.is_empty() {
                        let picked = tracks
                            .iter()
                            .filter(|t| t.height > 0 && t.height <= target_h)
                            .max_by_key(|t| (t.height, t.fps.round() as i32))
                            .or_else(|| {
                                tracks
                                    .iter()
                                    .filter(|t| t.height > 0)
                                    .min_by_key(|t| t.height)
                            });
                        if let Some(tr) = picked {
                            mpv.set_vid(Some(tr.id));
                        }
                        self.pending_hls_target_height = None;
                    }
                }
            }
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
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(6));
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
            && (proxy_failed || start_stalled || mid_eof_drop)
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
            return;
        }

        self.poll_adaptive_quality(is_remote_watch, is_live);
    }

    fn measured_network_bps(&self) -> u64 {
        let proxy_bps = self
            .backend
            .estimated_bandwidth_bps()
            .filter(|(_, age)| *age <= Duration::from_secs(20))
            .map(|(bps, _)| bps)
            .unwrap_or(0);
        proxy_bps.max(self.playback.cache_speed_bps)
    }

    fn poll_adaptive_quality(&mut self, is_remote_watch: bool, is_live: bool) {
        const PROLONGED_BUFFER_WAIT: Duration = Duration::from_millis(5500);
        const ABR_SWITCH_COOLDOWN: Duration = Duration::from_secs(6);
        const UPGRADE_STABLE_WAIT: Duration = Duration::from_secs(10);
        const UPGRADE_COOLDOWN: Duration = Duration::from_secs(12);

        if self.watch_loading
            || !is_remote_watch
            || is_live
            || self.stream_recovering.is_some()
            || self.last_error.is_some()
            || self.seeking_pos.is_some()
            || self.playback.paused
            || self.playback.idle
            || self.playback.eof
        {
            self.buffering_since = None;
            if self.playback.paused || self.seeking_pos.is_some() {
                self.stable_playback_since = None;
            }
            return;
        }

        let is_stalled_buffering = self.playback.buffering
            || (self.watch.is_some() && self.playback.video_h == 0);

        if is_stalled_buffering {
            self.stable_playback_since = None;
            if (self.playback.time_pos - self.last_abr_progress_pos).abs() > 0.25 {
                self.last_abr_progress_pos = self.playback.time_pos;
                self.buffering_since = Some(Instant::now());
                return;
            }
            let stalled_for = self
                .buffering_since
                .get_or_insert_with(Instant::now)
                .elapsed();
            let cooldown_ok = self
                .last_abr_switch_at
                .is_none_or(|t| t.elapsed() >= ABR_SWITCH_COOLDOWN);
            if stalled_for >= PROLONGED_BUFFER_WAIT && cooldown_ok {
                self.try_abr_fallback();
            }
            return;
        }

        self.buffering_since = None;
        self.last_abr_progress_pos = self.playback.time_pos;
        let stable_for = self
            .stable_playback_since
            .get_or_insert_with(Instant::now)
            .elapsed();

        if self.abr_downgraded
            && self.abr_upgrade_attempts < 2
            && stable_for >= UPGRADE_STABLE_WAIT
            && self
                .last_abr_switch_at
                .is_none_or(|t| t.elapsed() >= UPGRADE_COOLDOWN)
        {
            self.try_abr_upgrade();
        }
    }

    fn try_abr_fallback(&mut self) {
        let Some(w) = self.watch.as_ref() else {
            return;
        };
        if let Some(cur_idx) = self.selected_format_idx {
            self.abr_tried_formats.insert(cur_idx);
            if self.abr_initial_format_idx.is_none() {
                self.abr_initial_format_idx = Some(cur_idx);
            }
            if self.abr_target_height.is_none() {
                if let Some(cur_fmt) = w.video_formats.get(cur_idx) {
                    self.abr_target_height = Some(cur_fmt.height);
                }
            }
        } else if self.abr_target_height.is_none() {
            self.abr_target_height = Some(self.preferred_quality.max_height());
        }

        let action = pick_abr_fallback_action(
            &w.video_formats,
            self.selected_format_idx,
            &self.abr_tried_formats,
            w.hls_url.is_some(),
            self.playing_hls_master,
        );
        let Some(action) = action else {
            return;
        };

        self.buffering_since = None;
        self.stable_playback_since = None;
        self.last_abr_switch_at = Some(Instant::now());

        match action {
            AbrFallbackAction::SwitchFormat {
                idx,
                same_height_codec_swap,
            } => {
                let (label, next_h) = {
                    let f = &w.video_formats[idx];
                    (f.label(), f.height)
                };
                if !same_height_codec_swap
                    || self.abr_target_height.is_some_and(|th| next_h < th)
                {
                    self.abr_downgraded = true;
                }
                self.switch_quality_inner(idx, false, false);
                let msg = if same_height_codec_swap {
                    format!("Buffering >5s — switched to {label}")
                } else {
                    format!("Buffering >5s — Auto lowered to {label}")
                };
                self.toast(msg);
            }
            AbrFallbackAction::SwitchHlsAuto => {
                self.abr_downgraded = true;
                self.switch_quality_auto_inner(false, false);
                self.toast("Buffering >5s — switched to Auto");
            }
        }
    }

    fn try_abr_upgrade(&mut self) {
        let Some(target_h) = self.abr_target_height else {
            return;
        };
        let Some(w) = self.watch.as_ref() else {
            return;
        };
        let cur_h = self
            .selected_format_idx
            .and_then(|i| w.video_formats.get(i))
            .map(|f| f.height)
            .unwrap_or(0);
        if self.selected_format_idx.is_some() && cur_h >= target_h {
            self.abr_downgraded = false;
            return;
        }

        let allow_retried = self
            .last_abr_switch_at
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(24));
        let next_idx = pick_abr_recovery_idx(
            &w.video_formats,
            target_h,
            self.abr_initial_format_idx,
            &self.abr_tried_formats,
            allow_retried,
        )
        .or_else(|| {
            pick_abr_recovery_idx(
                &w.video_formats,
                target_h,
                self.abr_initial_format_idx,
                &self.abr_tried_formats,
                true,
            )
            .filter(|_| self.measured_network_bps() >= default_bitrate_for_height(target_h) * 4)
        });
        let Some(idx) = next_idx else {
            return;
        };
        let target_fmt = &w.video_formats[idx];
        if target_fmt.height <= cur_h && self.selected_format_idx.is_some() {
            return;
        }

        let measured_bps = self.measured_network_bps();
        if !should_upgrade_to_target(target_fmt, measured_bps, self.playback.cache_duration) {
            return;
        }

        let label = target_fmt.label();
        self.abr_upgrade_attempts += 1;
        self.abr_downgraded = false;
        self.abr_tried_formats.clear();
        self.buffering_since = None;
        self.stable_playback_since = None;
        self.last_abr_switch_at = Some(Instant::now());
        self.switch_quality_inner(idx, false, false);
        if measured_bps >= 1_000_000 {
            let mbps = measured_bps as f64 / 1_000_000.0;
            self.toast(format!("Fast network ({mbps:.0} Mbps) — restored {label}"));
        } else {
            self.toast(format!("Network recovered — restored {label}"));
        }
    }

    fn update_resume_from_active(&mut self) {
        if self.watch_loading || self.playback.duration <= 15.0 || self.playback.time_pos <= 2.0 {
            return;
        }
        let Some(w) = self.watch.as_ref().map(|w| w.id.clone()) else {
            return;
        };
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

    fn poll_playback_and_sponsorblock(&mut self, ctx: &egui::Context) {
        if let SleepTimer::Until(deadline) = self.sleep_timer {
            if Instant::now() >= deadline {
                self.pause_all_for_sleep("Sleep timer ended — playback paused");
            } else {
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }
        if self.page != Page::Shorts || self.shorts_grid_view {
            if self.shorts_slot.watch.is_some() && !self.shorts_slot.playback.paused {
                if let Some(mpv) = &self.shorts_slot.mpv {
                    mpv.set_paused(true);
                }
            }
        }
        if self.shorts_slot.watch.is_some() && self.shorts_slot.mpv.is_some() {
            let mut slot = Box::new(PlayerSlot::new(1.0));
            std::mem::swap(&mut self.shorts_slot, &mut slot);
            self.swap_active_with(&mut slot);
            self.poll_active_stream_health();
            let playing_short =
                self.page == Page::Shorts && !self.shorts_grid_view && !self.playback.paused;
            self.swap_active_with(&mut slot);
            std::mem::swap(&mut self.shorts_slot, &mut slot);
            if playing_short {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        }

        if let Some(mut bg) = self.bg_player_tab.take() {
            if bg.watch.is_some() && bg.mpv.is_some() {
                self.swap_active_with(&mut bg);
                self.poll_active_stream_health();
                self.update_resume_from_active();
                let bg_playing = !self.playback.paused;
                self.swap_active_with(&mut bg);
                if bg_playing {
                    ctx.request_repaint_after(Duration::from_millis(400));
                }
            }
            self.bg_player_tab = Some(bg);
        }

        if self.watch.is_none() || self.mpv.is_none() {
            if let Some(om) = &mut self.os_media {
                om.sync_playback(false, true, 0.0);
            }
            return;
        }
        let was_eof = self.playback.eof;
        self.poll_active_stream_health();
        if !self.watch_loading {
            self.backend
                .cursor
                .update(self.playback.time_pos, self.playback.paused);
            self.update_resume_from_active();
            if self.last_watch_saved_at.elapsed() >= Duration::from_secs(4) {
                self.flush_last_watched_now();
            }
        }

        if !self.playback.paused && self.page == Page::Watch {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        if self.page == Page::Watch || (self.page == Page::Shorts && !self.shorts_grid_view) {
            self.maybe_check_dropboard(false);
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
            if let Some(seg) = self.sponsor_segments.iter().find(|s| {
                self.sponsorblock_categories.contains(&s.category)
                    && s.fits(dur)
                    && pos >= s.start
                    && pos < s.end - 0.5
            }) {
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
        let sleep_eof_triggered = if !is_live
            && self.sleep_timer == SleepTimer::EndOfVideo
            && self.playback.eof
            && !was_eof
            && self.playback.duration > 1.0
            && self
                .watch_started_at
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(2))
        {
            self.pause_all_for_sleep("Sleep timer: paused at end of video");
            true
        } else {
            false
        };
        if !sleep_eof_triggered
            && !is_live
            && self.autoplay_next
            && self.playback.eof
            && !was_eof
            && self.playback.duration > 1.0
            && self
                .watch_started_at
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(2))
        {
            let next_opt = if !self.queue.is_empty() {
                let next = self.queue.remove(0);
                self.persist_queue();
                Some(next)
            } else {
                self.watch.as_ref().and_then(|w| w.related.first()).cloned()
            };
            if let Some(next) = next_opt {
                let stay_in_pip = self.page != Page::Watch || self.watch_scrolled_into_pip;
                let saved_page = self.page;
                let saved_pip = self.watch_scrolled_into_pip;
                self.open_video(next);
                if stay_in_pip {
                    self.page = saved_page;
                    self.watch_scrolled_into_pip = saved_pip;
                    self.scroll_watch_to_top = false;
                }
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
    fn sync_and_paint_ambient_backdrop(&mut self, ctx: &egui::Context, screen: egui::Rect) {
        let wants_ambient = self.ambient_mode != AmbientMode::Off && !self.fullscreen;
        if wants_ambient {
            let target_id = self
                .watch
                .as_ref()
                .map(|w| w.id.as_str())
                .filter(|id| !id.starts_with("local:"))
                .or_else(|| {
                    self.bg_player_tab
                        .as_ref()
                        .and_then(|b| b.watch.as_ref())
                        .map(|w| w.id.as_str())
                        .filter(|id| !id.starts_with("local:"))
                })
                .or_else(|| {
                    self.home_items
                        .iter()
                        .find(|it| !it.id.is_empty() && !it.id.starts_with("local:"))
                        .map(|it| it.id.as_str())
                })
                .map(str::to_owned);
            if let Some(vid) = target_id {
                if self.ambient_requested_key.as_deref() != Some(vid.as_str()) {
                    self.ambient_requested_key = Some(vid.clone());
                    let _ = self.backend.tx.send(Request::LoadAmbientArt {
                        key: vid.clone(),
                        url: format!("https://i.ytimg.com/vi/{vid}/hqdefault.jpg"),
                    });
                }
            }
        }

        let acrylic = cfg!(windows)
            && self.window_material != WindowMaterial::None
            && !self.fullscreen
            && crate::win_shell::windows_backdrop_supported();
        let ambient = wants_ambient && self.ambient_tex_soft.is_some();
        theme::set_backdrop_flags(acrylic, ambient);

        if !acrylic && !ambient {
            return;
        }

        let painter = ctx.layer_painter(egui::LayerId::background());
        if acrylic {
            // Subtle dark wash so Windows 11 Acrylic/Mica stays readable with crisp contrast
            painter.rect_filled(screen, 0.0, Color32::from_rgba_unmultiplied(12, 12, 16, 86));
        } else {
            painter.rect_filled(screen, 0.0, theme::BG);
        }

        if ambient {
            let strength = self.ambient_opacity.clamp(0.05, 0.70);
            let fade = self
                .ambient_changed_at
                .map_or(1.0, |at| (at.elapsed().as_secs_f32() / 0.7).clamp(0.0, 1.0));
            if fade < 1.0 {
                ctx.request_repaint_after(Duration::from_millis(33));
            } else {
                self.ambient_prev_soft = None;
            }
            let full_uv = egui::Rect::from_min_max(egui::pos2(0.06, 0.06), egui::pos2(0.94, 0.94));
            match self.ambient_mode {
                AmbientMode::SoftBlur => {
                    if let Some(prev) = &self.ambient_prev_soft {
                        if fade < 1.0 {
                            painter.image(
                                prev.id(),
                                screen,
                                full_uv,
                                Color32::WHITE.gamma_multiply(strength * (1.0 - fade)),
                            );
                        }
                    }
                    if let Some((_, tex)) = &self.ambient_tex_soft {
                        painter.image(
                            tex.id(),
                            screen,
                            full_uv,
                            Color32::WHITE.gamma_multiply(strength * fade),
                        );
                    }
                }
                AmbientMode::TiledBlur => {
                    if let Some((_, tex)) = &self.ambient_tex_tiled {
                        let tile_px = 560.0_f32;
                        let u_span = (screen.width() / tile_px).max(1.0);
                        let v_span = (screen.height() / tile_px).max(1.0);
                        let tiled_uv = egui::Rect::from_min_max(
                            egui::pos2(0.0, 0.0),
                            egui::pos2(u_span, v_span),
                        );
                        painter.image(
                            tex.id(),
                            screen,
                            tiled_uv,
                            Color32::WHITE.gamma_multiply(strength * 0.92 * fade),
                        );
                    }
                }
                AmbientMode::Off => {}
            }
            let mut scrim = egui::Mesh::default();
            let top_col = if acrylic {
                Color32::from_black_alpha(20)
            } else {
                Color32::from_black_alpha(36)
            };
            let bot_col = if acrylic {
                Color32::from_black_alpha(72)
            } else {
                Color32::from_black_alpha(128)
            };
            scrim.colored_vertex(screen.left_top(), top_col);
            scrim.colored_vertex(screen.right_top(), top_col);
            scrim.colored_vertex(screen.left_bottom(), bot_col);
            scrim.colored_vertex(screen.right_bottom(), bot_col);
            scrim.add_triangle(0, 1, 2);
            scrim.add_triangle(1, 3, 2);
            painter.add(egui::Shape::mesh(scrim));
        }
    }
}

impl eframe::App for YtApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.win_pip_mode {
            return [0.0, 0.0, 0.0, 1.0];
        }
        if cfg!(windows)
            && self.window_material != WindowMaterial::None
            && !self.fullscreen
            && crate::win_shell::windows_backdrop_supported()
        {
            return [0.0, 0.0, 0.0, 0.0];
        }
        [15.0 / 255.0, 15.0 / 255.0, 15.0 / 255.0, 1.0]
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.persist_queue();
        storage.set_string("volume", format!("{:.1}", self.volume));
        storage.set_string(
            "sponsorblock",
            if self.sponsorblock_enabled { "1" } else { "0" }.into(),
        );
        if let Ok(sbc) = serde_json::to_string(&self.sponsorblock_categories) {
            storage.set_string("sb_categories", sbc);
        }
        storage.set_string(
            "dearrow",
            if self.dearrow_enabled { "1" } else { "0" }.into(),
        );
        storage.set_string(
            "ryd",
            if self.ryd_enabled { "1" } else { "0" }.into(),
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
            "theater_mode",
            if self.theater_mode { "1" } else { "0" }.into(),
        );
        storage.set_string("up_next_mode", self.up_next_mode.key().into());
        storage.set_string(
            "always_on_top",
            if self.always_on_top { "1" } else { "0" }.into(),
        );
        storage.set_string(
            "close_to_tray",
            if self.close_to_tray { "1" } else { "0" }.into(),
        );
        storage.set_string(
            "minimize_to_tray",
            if self.minimize_to_tray { "1" } else { "0" }.into(),
        );
        storage.set_string(
            "ambient_glow",
            if self.ambient_glow { "1" } else { "0" }.into(),
        );
        let pt = match self.po_token_mode {
            PoTokenMode::Auto => "auto",
            PoTokenMode::On => "on",
            PoTokenMode::Off => "off",
        };
        storage.set_string("po_token_mode", pt.into());
        storage.set_string("window_material", self.window_material.key().into());
        storage.set_string("ambient_mode", self.ambient_mode.key().into());
        storage.set_string("ambient_opacity", format!("{:.2}", self.ambient_opacity));
        storage.set_string("sub_font_size", self.sub_font_size.to_string());
        storage.set_string("sub_bg_opacity", format!("{:.2}", self.sub_bg_opacity));
        if let Ok(sc) = serde_json::to_string(&self.shortcuts) {
            storage.set_string("shortcuts", sc);
        }
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
        storage.set_string(
            "hover_preview_quality",
            self.hover_preview_quality.key().into(),
        );
        storage.set_string(
            "hover_preview_muted",
            if self.hover_preview_muted { "1" } else { "0" }.into(),
        );
        if let Ok(h) = serde_json::to_string(&self.history) {
            storage.set_string("history", h);
        }
        if let Ok(s) = serde_json::to_string(&self.saved) {
            storage.set_string("saved", s);
        }
        if let Ok(r) = serde_json::to_string(&self.resume_positions) {
            storage.set_string("resume_positions", r);
        }
        if let Ok(sh) = serde_json::to_string(&self.search_history) {
            storage.set_string("search_history", sh);
        }
    }

    fn on_exit(&mut self, gl: Option<&glow::Context>) {
        self.flush_last_watched_now();
        self.persist_queue();
        self.backend.cursor.stop();
        if let Some(mpv) = &self.mpv {
            mpv.destroy_gl(gl);
        }
        if let Some(bg) = &self.bg_player_tab {
            if let Some(mpv) = &bg.mpv {
                mpv.destroy_gl(gl);
            }
        }
        if let Some(mpv) = &self.shorts_slot.mpv {
            mpv.destroy_gl(gl);
        }
        if let Some(mpv) = &self.hover_preview_mpv {
            mpv.destroy_gl(gl);
        }
        if let Ok(mut q) = self.retired_mpv.lock() {
            for mpv in q.drain(..) {
                mpv.destroy_gl(gl);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.schedule_retired_mpv_gl_cleanup(&ctx);
        self.drain_backend(&ctx);
        self.handle_shortcuts(&ctx);
        self.handle_os_media();
        self.handle_win_shell(&ctx);
        self.poll_playback_and_sponsorblock(&ctx);

        if self.win_pip_mode {
            theme::set_backdrop_flags(false, false);
            if self.watch.is_none() {
                self.exit_win_pip(&ctx);
            } else {
                egui::CentralPanel::default()
                    .frame(
                        Frame::central_panel(ui.style())
                            .fill(Color32::BLACK)
                            .inner_margin(Margin::ZERO),
                    )
                    .show(ui, |ui| {
                        self.draw_win_pip_view(ui, &ctx);
                    });
                self.draw_toast_snackbar(&ctx);
                return;
            }
        }

        let screen = ui.max_rect();
        self.sync_and_paint_ambient_backdrop(&ctx, screen);

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
                    .fill(theme::panel_bg())
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

                if !self.fullscreen
                    && !matches!(self.page, Page::Watch | Page::Shorts)
                    && (self.watch.is_some() || self.bg_player_tab.is_some())
                {
                    self.draw_player_tab_bar(ui);
                }

                self.begin_hover_preview_frame(&ctx);
                match self.page {
                    Page::Home => self.draw_home_page(ui),
                    Page::Shorts => self.draw_shorts_page(ui),
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
                    Page::Search => self.draw_search_page(ui, &ctx),
                    Page::History => {
                        if self.signed_in && self.sync_history {
                            let items = self.remote_history.clone();
                            let loading = self.remote_history_loading;
                            let cont = self.remote_history_continuation.clone();
                            if let Some(act) = cards::draw_history_feed(
                                ui,
                                "Watch History (Synced with YouTube)",
                                &items,
                                &mut self.history_filter,
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
                            if let Some(act) = cards::draw_history_feed(
                                ui,
                                "Watch History (Local)",
                                &items,
                                &mut self.history_filter,
                                false,
                                false,
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

        self.end_hover_preview_frame(&ctx);

        let has_video_player = self.watch.is_some()
            || self.bg_player_tab.as_ref().is_some_and(|b| b.watch.is_some());
        if !self.fullscreen
            && has_video_player
            && self.page != Page::Shorts
            && (self.page != Page::Watch || self.watch_scrolled_into_pip)
        {
            self.draw_mini_player(&ctx);
        }
        self.draw_playlist_picker_modal(&ctx);
        self.draw_toast_snackbar(&ctx);
        self.record_nav_transition();
    }
}

impl YtApp {
    pub(super) fn stop_hover_preview(&mut self) {
        if self.hover_preview_loading_id.take().is_some() {
            let _ = self.backend.tx.send(Request::CancelHoverPreview);
        }
        self.hover_preview_candidate = None;
        self.hover_preview_active_id = None;
        self.hover_preview_duration = 0.0;
        self.hover_preview_last_hover_at = None;
        self.hover_preview_playback = PlaybackState::default();
        if let Some(mpv) = &self.hover_preview_mpv {
            mpv.stop();
            if self.hover_preview_stopped_at.is_none() {
                self.hover_preview_stopped_at = Some(Instant::now());
            }
        }
    }

    fn begin_hover_preview_frame(&mut self, ctx: &egui::Context) {
        let allowed = self.hover_preview_quality != HoverPreviewQuality::Off
            && !matches!(self.page, Page::Watch | Page::Shorts)
            && !self.win_pip_mode
            && !self.hidden_to_tray
            && !self.fullscreen;
        if !allowed {
            self.stop_hover_preview();
            if self.hover_preview_mpv.is_some() {
                let retired = self.hover_preview_mpv.take();
                self.retire_mpv(retired);
                self.hover_preview_stopped_at = None;
            }
            ctx.data_mut(|d| {
                d.insert_temp(cards::hover_input_id(), cards::HoverPreviewInput::default());
                d.insert_temp(
                    cards::hover_output_id(),
                    cards::HoverPreviewOutput::default(),
                );
            });
            return;
        }
        if self.hover_preview_active_id.is_none()
            && self.hover_preview_loading_id.is_none()
            && self.hover_preview_candidate.is_none()
            && self
                .hover_preview_stopped_at
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(20))
        {
            if let Some(m) = self.hover_preview_mpv.take() {
                self.retire_mpv(Some(m));
            }
            self.hover_preview_stopped_at = None;
        }
        if let (Some(mpv), Some(_)) = (&self.hover_preview_mpv, &self.hover_preview_active_id) {
            self.hover_preview_stopped_at = None;
            self.hover_preview_playback = mpv.poll_state();
            if !self.hover_preview_playback.paused {
                ctx.request_repaint_after(Duration::from_millis(33));
            }
        }
        let input = cards::HoverPreviewInput {
            enabled: true,
            active_id: self.hover_preview_active_id.clone(),
            loading_id: self.hover_preview_loading_id.clone(),
            muted: self.hover_preview_muted,
            time_pos: self.hover_preview_playback.time_pos,
            duration: self
                .hover_preview_duration
                .max(self.hover_preview_playback.duration),
            has_frame: self.hover_preview_active_id.is_some()
                && self.hover_preview_playback.video_h > 0,
            render_handle: self.hover_preview_mpv.as_ref().map(|m| m.render_handle()),
        };
        ctx.data_mut(|d| {
            d.insert_temp(cards::hover_input_id(), input);
            d.insert_temp(
                cards::hover_output_id(),
                cards::HoverPreviewOutput::default(),
            );
        });
    }

    fn end_hover_preview_frame(&mut self, ctx: &egui::Context) {
        if self.hover_preview_quality == HoverPreviewQuality::Off
            || matches!(self.page, Page::Watch | Page::Shorts)
            || self.win_pip_mode
            || self.hidden_to_tray
            || self.fullscreen
        {
            return;
        }
        let out = ctx
            .data_mut(|d| d.get_temp::<cards::HoverPreviewOutput>(cards::hover_output_id()))
            .unwrap_or_default();

        if out.toggle_mute {
            self.hover_preview_muted = !self.hover_preview_muted;
            if let Some(mpv) = &self.hover_preview_mpv {
                mpv.set_mute(self.hover_preview_muted);
                mpv.set_volume(self.volume);
            }
        }

        let popup_open = egui::Popup::is_any_open(ctx);
        match out.hovered_id {
            Some(vid) if !popup_open => {
                self.hover_preview_last_hover_at = Some(Instant::now());
                if self.hover_preview_active_id.as_deref() == Some(vid.as_str())
                    || self.hover_preview_loading_id.as_deref() == Some(vid.as_str())
                {
                    return;
                }
                match &self.hover_preview_candidate {
                    Some((cand, started)) if cand == &vid => {
                        if started.elapsed() >= Duration::from_millis(500) {
                            let target = vid;
                            self.hover_preview_candidate = None;
                            self.hover_preview_active_id = None;
                            self.hover_preview_duration = 0.0;
                            self.hover_preview_playback = PlaybackState::default();
                            if let Some(mpv) = &self.hover_preview_mpv {
                                mpv.stop();
                            }
                            self.hover_preview_loading_id = Some(target.clone());
                            let _ = self.backend.tx.send(Request::ResolveHoverPreview {
                                video_id: target,
                                max_height: self.hover_preview_quality.max_height(),
                            });
                        } else {
                            ctx.request_repaint_after(Duration::from_millis(80));
                        }
                    }
                    _ => {
                        if self.hover_preview_active_id.is_some()
                            || self.hover_preview_loading_id.is_some()
                        {
                            self.stop_hover_preview();
                            self.hover_preview_last_hover_at = Some(Instant::now());
                        }
                        self.hover_preview_candidate = Some((vid, Instant::now()));
                        ctx.request_repaint_after(Duration::from_millis(520));
                    }
                }
            }
            _ => {
                self.hover_preview_candidate = None;
                let has_active_or_loading = self.hover_preview_active_id.is_some()
                    || self.hover_preview_loading_id.is_some();
                if has_active_or_loading {
                    let expired = popup_open
                        || self
                            .hover_preview_last_hover_at
                            .is_none_or(|t| t.elapsed() >= Duration::from_millis(160));
                    if expired {
                        self.stop_hover_preview();
                    } else {
                        ctx.request_repaint_after(Duration::from_millis(60));
                    }
                }
            }
        }
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

pub(super) fn parse_sleep_timer_input(raw: &str) -> Option<Duration> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    if let Some((lhs, rhs)) = s.split_once(':') {
        let h: u64 = lhs.trim().parse().ok()?;
        let m: u64 = rhs.trim().parse().ok()?;
        let total = h.saturating_mul(3600).saturating_add(m.saturating_mul(60));
        return (total >= 10).then(|| Duration::from_secs(total.min(24 * 3600)));
    }
    if let Ok(mins) = s.parse::<f64>() {
        if !mins.is_finite() || mins <= 0.0 {
            return None;
        }
        let secs = (mins * 60.0).round() as u64;
        return (secs >= 10).then(|| Duration::from_secs(secs.min(24 * 3600)));
    }

    let mut total_secs: f64 = 0.0;
    let mut matched = false;
    let mut num_buf = String::new();
    let mut unit_buf = String::new();
    let flush = |n_str: &mut String, u_str: &mut String, total: &mut f64, ok: &mut bool| {
        if n_str.is_empty() {
            u_str.clear();
            return;
        }
        if let Ok(v) = n_str.parse::<f64>() {
            if v.is_finite() && v > 0.0 {
                let u = u_str.trim();
                let mult = if u.is_empty()
                    || u.starts_with("m")
                    || u.starts_with("menit")
                    || u.starts_with("min")
                {
                    60.0
                } else if u.starts_with("h") || u.starts_with("j") {
                    3600.0
                } else if u.starts_with("s") || u.starts_with("d") {
                    1.0
                } else {
                    0.0
                };
                if mult > 0.0 {
                    *total += v * mult;
                    *ok = true;
                }
            }
        }
        n_str.clear();
        u_str.clear();
    };

    for ch in s.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            if !unit_buf.is_empty() {
                flush(&mut num_buf, &mut unit_buf, &mut total_secs, &mut matched);
            }
            num_buf.push(ch);
        } else if ch.is_ascii_alphabetic() {
            unit_buf.push(ch);
        } else if !num_buf.is_empty() && !unit_buf.is_empty() {
            flush(&mut num_buf, &mut unit_buf, &mut total_secs, &mut matched);
        }
    }
    flush(&mut num_buf, &mut unit_buf, &mut total_secs, &mut matched);
    if !matched {
        return None;
    }
    let secs = total_secs.round() as u64;
    (secs >= 10).then(|| Duration::from_secs(secs.min(24 * 3600)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AbrFallbackAction {
    SwitchFormat {
        idx: usize,
        same_height_codec_swap: bool,
    },
    SwitchHlsAuto,
}

fn default_bitrate_for_height(height: u32) -> u64 {
    match height {
        2160.. => 18_000_000,
        1440.. => 9_000_000,
        1080.. => 4_500_000,
        720.. => 2_500_000,
        480.. => 1_200_000,
        _ => 700_000,
    }
}

fn pick_abr_fallback_action(
    formats: &[crate::model::VideoFormat],
    current_idx: Option<usize>,
    tried: &HashSet<usize>,
    has_hls: bool,
    playing_hls_master: bool,
) -> Option<AbrFallbackAction> {
    if let Some(cur_idx) = current_idx {
        if let Some(cur) = formats.get(cur_idx) {
            let cur_codec = cur.codec_family();
            if let Some((alt_idx, _)) = formats
                .iter()
                .enumerate()
                .filter(|(i, f)| {
                    *i != cur_idx
                        && !tried.contains(i)
                        && f.height == cur.height
                        && f.codec_family() != cur_codec
                        && f.has_audio == cur.has_audio
                })
                .min_by_key(|(_, f)| {
                    let fps_diff = (f.fps as i32 - cur.fps as i32).abs();
                    let codec_prio = match f.codec_family() {
                        "VP9" => 0,
                        "H.264" => 1,
                        "AV1" => 2,
                        _ => 3,
                    };
                    (fps_diff, codec_prio)
                })
            {
                return Some(AbrFallbackAction::SwitchFormat {
                    idx: alt_idx,
                    same_height_codec_swap: true,
                });
            }

            if let Some((low_idx, _)) = formats
                .iter()
                .enumerate()
                .filter(|(i, f)| *i != cur_idx && !tried.contains(i) && f.height < cur.height)
                .max_by_key(|(_, f)| {
                    let codec_prio = match f.codec_family() {
                        "H.264" => 3,
                        "VP9" => 2,
                        "AV1" => 1,
                        _ => 0,
                    };
                    (f.height, !f.has_audio, codec_prio, f.fps)
                })
            {
                return Some(AbrFallbackAction::SwitchFormat {
                    idx: low_idx,
                    same_height_codec_swap: false,
                });
            }
        }
    } else if has_hls && playing_hls_master {
        if let Some((low_idx, _)) = formats
            .iter()
            .enumerate()
            .filter(|(i, f)| !tried.contains(i) && f.height <= 480)
            .max_by_key(|(_, f)| (f.height, !f.has_audio))
            .or_else(|| {
                formats
                    .iter()
                    .enumerate()
                    .filter(|(i, f)| !tried.contains(i) && f.height <= 720)
                    .max_by_key(|(_, f)| (f.height, !f.has_audio))
            })
        {
            return Some(AbrFallbackAction::SwitchFormat {
                idx: low_idx,
                same_height_codec_swap: false,
            });
        }
    }

    if has_hls && !playing_hls_master {
        return Some(AbrFallbackAction::SwitchHlsAuto);
    }
    None
}

fn pick_abr_recovery_idx(
    formats: &[crate::model::VideoFormat],
    target_height: u32,
    initial_idx: Option<usize>,
    tried: &HashSet<usize>,
    allow_retried: bool,
) -> Option<usize> {
    if let Some(init_i) = initial_idx {
        if formats.get(init_i).is_some_and(|f| f.height == target_height)
            && (allow_retried || !tried.contains(&init_i))
        {
            return Some(init_i);
        }
    }
    formats
        .iter()
        .enumerate()
        .filter(|(i, f)| f.height <= target_height && (allow_retried || !tried.contains(i)))
        .max_by_key(|(_, f)| (f.height, !f.has_audio, f.fps))
        .map(|(i, _)| i)
}

fn should_upgrade_to_target(
    target_fmt: &crate::model::VideoFormat,
    measured_bps: u64,
    cache_duration: f64,
) -> bool {
    let req_bps = target_fmt
        .bitrate
        .max(default_bitrate_for_height(target_fmt.height));
    if measured_bps > 0 {
        measured_bps >= req_bps.saturating_mul(2) && cache_duration >= 5.0
    } else {
        cache_duration >= 18.0
    }
}

#[cfg(test)]
mod ui_tests {
    use super::{
        AbrFallbackAction, parse_sleep_timer_input, pick_abr_fallback_action,
        pick_abr_recovery_idx, should_upgrade_to_target, shuffle_vec,
    };
    use crate::model::VideoFormat;
    use std::collections::HashSet;
    use std::time::Duration;

    #[test]
    fn shuffle_preserves_all_elements() {
        let mut v = vec![1, 2, 3, 4, 5, 6, 7, 8];
        shuffle_vec(&mut v, 12345);
        v.sort();
        assert_eq!(v, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn parses_sleep_timer_presets_and_manual_inputs() {
        assert_eq!(parse_sleep_timer_input("30"), Some(Duration::from_secs(1800)));
        assert_eq!(parse_sleep_timer_input("120"), Some(Duration::from_secs(7200)));
        assert_eq!(parse_sleep_timer_input("2h"), Some(Duration::from_secs(7200)));
        assert_eq!(parse_sleep_timer_input("2 jam"), Some(Duration::from_secs(7200)));
        assert_eq!(parse_sleep_timer_input("1:30"), Some(Duration::from_secs(5400)));
        assert_eq!(parse_sleep_timer_input("1h 30m"), Some(Duration::from_secs(5400)));
        assert_eq!(parse_sleep_timer_input("45 menit"), Some(Duration::from_secs(2700)));
        assert_eq!(parse_sleep_timer_input("0"), None);
        assert_eq!(parse_sleep_timer_input("abc"), None);
    }

    #[test]
    fn abr_fallback_swaps_codec_at_same_res_then_drops_resolution() {
        let fmt = |height: u32, mime: &str, bitrate: u64| VideoFormat {
            itag: 0,
            url: format!("u{height}"),
            mime: mime.to_owned(),
            width: height * 16 / 9,
            height,
            fps: 30,
            bitrate,
            content_length: 0,
            has_audio: false,
        };
        let formats = vec![
            fmt(1080, "video/mp4; codecs=\"avc1.640028\"", 4_000_000),
            fmt(1080, "video/webm; codecs=\"vp9\"", 3_500_000),
            fmt(720, "video/mp4; codecs=\"avc1.4d401f\"", 2_000_000),
            fmt(480, "video/mp4; codecs=\"avc1.4d401e\"", 1_000_000),
        ];

        let mut tried = HashSet::from([0usize]);
        assert_eq!(
            pick_abr_fallback_action(&formats, Some(0), &tried, false, false),
            Some(AbrFallbackAction::SwitchFormat {
                idx: 1,
                same_height_codec_swap: true
            })
        );

        tried.insert(1);
        assert_eq!(
            pick_abr_fallback_action(&formats, Some(1), &tried, false, false),
            Some(AbrFallbackAction::SwitchFormat {
                idx: 2,
                same_height_codec_swap: false
            })
        );

        tried.insert(2);
        assert_eq!(
            pick_abr_fallback_action(&formats, Some(2), &tried, false, false),
            Some(AbrFallbackAction::SwitchFormat {
                idx: 3,
                same_height_codec_swap: false
            })
        );

        // 300 Mbps restores 1080p from 480p; 3 Mbps stays at 480p.
        assert!(should_upgrade_to_target(&formats[0], 300_000_000, 6.0));
        assert!(!should_upgrade_to_target(&formats[0], 3_000_000, 6.0));
        assert_eq!(
            pick_abr_recovery_idx(&formats, 1080, Some(0), &tried, true),
            Some(0)
        );
    }
}
