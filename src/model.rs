use serde::{Deserialize, Serialize};

/// A filter chip on Home / Explore (e.g. "All", "Gaming", "Music", "Live").
#[derive(Clone, Debug, Default)]
pub struct FeedChip {
    pub title: String,
    pub params: Option<String>,
    pub continuation: Option<String>,
    pub selected: bool,
}

/// A video card shown in Home feed, Subscriptions, Trending, Search, History, Channel, or Related list.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VideoItem {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub channel_id: Option<String>,
    #[serde(default)]
    pub channel_avatar_url: Option<String>,
    pub views_text: String,
    pub published_text: String,
    pub duration_text: String,
    pub thumbnail_url: String,
    pub is_live: bool,
    /// 0.0..=1.0 playback progress bar shown at bottom of thumbnail.
    #[serde(default)]
    pub watched_percent: Option<f32>,
    /// Server-side or local resume position in seconds.
    #[serde(default)]
    pub resume_secs: Option<f64>,
    /// Set when a card represents a playlist (e.g. inside a channel's Playlists tab).
    #[serde(default)]
    pub playlist_browse_id: Option<String>,
    /// Playlist item instance ID (`playlistVideoRenderer.setVideoId`) for remove/reorder actions.
    #[serde(default)]
    pub playlist_set_video_id: Option<String>,
    /// True if YouTube marks this upload as "New" or it was recently published.
    #[serde(default)]
    pub is_new: bool,
    /// True if this video/Short is Members-only.
    #[serde(default)]
    pub is_members_only: bool,
}

impl VideoItem {
    /// Returns true if this item represents a vertical YouTube Short.
    pub fn is_short(&self) -> bool {
        self.duration_text.eq_ignore_ascii_case("Short")
    }
}

/// Notification preference level for a subscribed channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NotificationLevel {
    All,
    #[default]
    Personalized,
    None,
}

impl NotificationLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Personalized => "Personalized",
            Self::None => "None",
        }
    }
}

/// Extracted InnerTube notification menu params & current state for a channel.
#[derive(Clone, Debug, Default)]
pub struct ChannelNotificationParams {
    pub level: NotificationLevel,
    pub all_params: Option<String>,
    pub personalized_params: Option<String>,
    pub none_params: Option<String>,
}

impl ChannelNotificationParams {
    pub fn params_for(&self, level: NotificationLevel) -> Option<&str> {
        match level {
            NotificationLevel::All => self.all_params.as_deref(),
            NotificationLevel::Personalized => self.personalized_params.as_deref(),
            NotificationLevel::None => self.none_params.as_deref(),
        }
    }

    pub fn has_controls(&self) -> bool {
        self.all_params.is_some()
            || self.personalized_params.is_some()
            || self.none_params.is_some()
    }
}

/// Entry from InnerTube `notification/get_notification_menu`.
#[derive(Clone, Debug, Default)]
pub struct NotificationMenuItem {
    pub message: String,
    pub sent_time_text: String,
    pub section: Option<String>,
    pub channel_avatar_url: Option<String>,
    pub video_thumbnail_url: Option<String>,
    pub video_id: Option<String>,
    pub channel_id: Option<String>,
    pub read: bool,
    pub is_live: bool,
}

/// A YouTube channel card returned in global search results (`channelRenderer`).
#[derive(Clone, Debug, Default)]
pub struct SearchChannelResult {
    pub id: String,
    pub title: String,
    pub handle: String,
    pub subscribers_text: String,
    pub video_count_text: String,
    pub description_snippet: String,
    pub avatar_url: Option<String>,
    pub is_subscribed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SearchTypeFilter {
    #[default]
    All,
    Videos,
    Channels,
    Playlists,
    Live,
}

impl SearchTypeFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Videos => "Videos",
            Self::Channels => "Channels",
            Self::Playlists => "Playlists",
            Self::Live => "Live",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SearchDateFilter {
    #[default]
    Anytime,
    LastHour,
    Today,
    ThisWeek,
    ThisMonth,
    ThisYear,
}

impl SearchDateFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::Anytime => "Anytime",
            Self::LastHour => "Last hour",
            Self::Today => "Today",
            Self::ThisWeek => "This week",
            Self::ThisMonth => "This month",
            Self::ThisYear => "This year",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SearchDurationFilter {
    #[default]
    Any,
    Short,
    Medium,
    Long,
}

impl SearchDurationFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::Any => "Any",
            Self::Short => "Under 4 min",
            Self::Medium => "4–20 min",
            Self::Long => "Over 20 min",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SearchSortBy {
    #[default]
    Relevance,
    UploadDate,
    ViewCount,
    Rating,
}

impl SearchSortBy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Relevance => "Relevance",
            Self::UploadDate => "Upload date",
            Self::ViewCount => "View count",
            Self::Rating => "Rating",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SearchFilter {
    pub kind: SearchTypeFilter,
    pub date: SearchDateFilter,
    pub duration: SearchDurationFilter,
    pub sort: SearchSortBy,
}

impl SearchFilter {
    pub fn is_default(self) -> bool {
        self == Self::default()
    }

    pub fn active_advanced_count(self) -> usize {
        usize::from(self.date != SearchDateFilter::Anytime)
            + usize::from(self.duration != SearchDurationFilter::Any)
            + usize::from(self.sort != SearchSortBy::Relevance)
    }
}

/// Channel header info & available tabs returned on `Page::Channel`.
#[derive(Clone, Debug, Default)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub handle: String,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub subscribers_text: String,
    pub is_subscribed: bool,
    pub notifications: ChannelNotificationParams,
    pub tabs: Vec<ChannelTab>,
}

#[derive(Clone, Debug)]
pub struct ChannelTab {
    pub title: String,
    pub params: Option<String>,
    pub selected: bool,
}

/// Subscribed channel entry from InnerTube `/youtubei/v1/guide`.
#[derive(Clone, Debug)]
pub struct SidebarChannel {
    pub name: String,
    pub browse_id: String,
    pub avatar_url: Option<String>,
    pub is_live: bool,
}

/// User playlist entry (Watch Later, Liked Videos, custom playlists).
#[derive(Clone, Debug)]
pub struct PlaylistSummary {
    pub title: String,
    pub browse_id: String,
    pub subtitle: String,
    pub thumbnail_url: Option<String>,
}

/// Selectable playlist entry from `playlist/get_add_to_playlist`.
#[derive(Clone, Debug)]
pub struct PlaylistOption {
    pub id: String,
    pub title: String,
    pub is_added: bool,
    pub thumbnail_url: Option<String>,
}

/// Selectable audio track from adaptiveFormats (Original / Auto-dubbed).
#[derive(Clone, Debug)]
pub struct AudioTrack {
    pub id: String,
    pub label: String,
    pub is_default: bool,
    pub url: String,
    pub content_length: u64,
}

/// A selectable video stream format from InnerTube `adaptiveFormats` or `formats`.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct VideoFormat {
    pub itag: u64,
    pub url: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate: u64,
    pub content_length: u64,
    pub has_audio: bool,
}

impl VideoFormat {
    pub fn codec_family(&self) -> &'static str {
        if self.mime.contains("av01") {
            "AV1"
        } else if self.mime.contains("vp9") || self.mime.contains("vp09") {
            "VP9"
        } else if self.mime.contains("avc1") {
            "H.264"
        } else {
            "Video"
        }
    }

    pub fn label(&self) -> String {
        let codec = self.codec_family();
        if self.fps > 30 {
            format!("{}p{} ({codec})", self.height, self.fps)
        } else {
            format!("{}p ({codec})", self.height)
        }
    }
}

#[derive(Clone, Debug)]
pub struct CaptionTrack {
    pub name: String,
    pub language_code: String,
    pub vtt_url: String,
    pub is_auto: bool,
}

#[derive(Clone, Debug)]
pub struct Chapter {
    pub start_secs: f64,
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct CommentItem {
    pub id: String,
    pub author: String,
    pub avatar_url: Option<String>,
    pub published: String,
    pub likes: String,
    pub text: String,
    pub reply_count_text: String,
    pub replies_token: Option<String>,
    pub reply_params: Option<String>,
    pub replies: Vec<CommentItem>,
    pub replies_loading: bool,
    pub is_liked: bool,
    pub like_action_params: Option<String>,
    pub unlike_action_params: Option<String>,
    pub is_pinned: bool,
    pub has_creator_heart: bool,
}

#[derive(Clone, Debug)]
pub enum LiveChatRun {
    Text(String),
    Emoji { alt: String, url: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveChatKind {
    Normal,
    SuperChat { amount: String, bg_color: u32 },
    Membership { header: String },
}

#[derive(Clone, Debug)]
pub struct LiveChatEmote {
    pub shortcut: String,
    pub emoji_id: String,
    pub url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LiveChatMessage {
    pub id: String,
    pub author: String,
    pub avatar_url: Option<String>,
    pub text: String,
    pub runs: Vec<LiveChatRun>,
    pub is_owner: bool,
    pub is_mod: bool,
    pub kind: LiveChatKind,
    pub offset_ms: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct StoryboardSpec {
    pub base_url: String,
    pub level: usize,
    pub width: u32,
    pub height: u32,
    pub count: usize,
    pub cols: usize,
    pub rows: usize,
    pub interval_ms: u64,
    pub name_template: String,
    pub sigh: String,
}

impl StoryboardSpec {
    /// Returns `(sheet_image_url, [u0, v0, u1, v1])` for `secs` into the video.
    pub fn frame_at(&self, secs: f64, duration_secs: f64) -> Option<(String, [f32; 4])> {
        if self.count == 0 || self.cols == 0 || self.rows == 0 {
            return None;
        }
        let frame_idx = if self.interval_ms > 0 {
            ((secs.max(0.0) * 1000.0) / self.interval_ms as f64).floor() as usize
        } else if duration_secs > 0.0 {
            ((secs.max(0.0) / duration_secs) * self.count as f64).floor() as usize
        } else {
            0
        }
        .min(self.count - 1);

        let per_sheet = self.cols * self.rows;
        let sheet = frame_idx / per_sheet;
        let in_sheet = frame_idx % per_sheet;
        let col = in_sheet % self.cols;
        let row = in_sheet / self.cols;

        let sheet_frames = (self.count - sheet * per_sheet).min(per_sheet);
        let sheet_rows = sheet_frames.div_ceil(self.cols).max(1);

        let name = self.name_template.replace("$M", &sheet.to_string());
        let mut url = self
            .base_url
            .replace("$L", &self.level.to_string())
            .replace("$N", &name);
        if !self.sigh.is_empty() {
            if url.contains('?') {
                url.push_str(&format!("&sigh={}", self.sigh));
            } else {
                url.push_str(&format!("?sigh={}", self.sigh));
            }
        }

        let u0 = col as f32 / self.cols as f32;
        let u1 = (col + 1) as f32 / self.cols as f32;
        let v0 = row as f32 / sheet_rows as f32;
        let v1 = (row + 1) as f32 / sheet_rows as f32;
        Some((url, [u0, v0, u1, v1]))
    }
}

/// Full watch page data for a video.
#[derive(Clone, Debug)]
pub struct WatchData {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub channel_id: Option<String>,
    pub channel_avatar_url: Option<String>,
    pub subscribers_text: String,
    pub views_text: String,
    pub published_text: String,
    pub likes_text: String,
    pub dislikes_text: Option<String>,
    pub is_liked: bool,
    pub is_disliked: bool,
    pub is_subscribed: bool,
    pub notifications: ChannelNotificationParams,
    pub description: String,
    pub duration_secs: f64,
    pub video_formats: Vec<VideoFormat>,
    pub audio_url: Option<String>,
    pub audio_content_length: u64,
    pub audio_tracks: Vec<AudioTrack>,
    pub hls_url: Option<String>,
    pub captions: Vec<CaptionTrack>,
    pub chapters: Vec<Chapter>,
    pub storyboard: Option<StoryboardSpec>,
    pub comments_token: Option<String>,
    pub live_chat_token: Option<String>,
    pub related: Vec<VideoItem>,
    pub related_chips: Vec<FeedChip>,
    pub is_live: bool,
    pub is_members_only: bool,
    pub resume_secs: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UpNextFilterMode {
    #[default]
    All,
    FromChannel,
    Related,
    ForYou,
}

impl UpNextFilterMode {
    pub fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::FromChannel => "from_channel",
            Self::Related => "related",
            Self::ForYou => "for_you",
        }
    }

    pub fn from_key(s: &str) -> Self {
        match s {
            "from_channel" => Self::FromChannel,
            "related" => Self::Related,
            "for_you" => Self::ForYou,
            _ => Self::All,
        }
    }

    pub fn classify_chip(title: &str, channel_name: &str) -> Option<Self> {
        let low = title.trim().to_ascii_lowercase();
        if low == "all" || low == "semua" {
            return Some(Self::All);
        }
        if low == "related" || low == "terkait" {
            return Some(Self::Related);
        }
        if low == "for you" || low == "untuk anda" {
            return Some(Self::ForYou);
        }
        let ch = channel_name.trim().to_ascii_lowercase();
        if low.starts_with("from ")
            || low.starts_with("dari ")
            || (!ch.is_empty() && low == ch)
        {
            return Some(Self::FromChannel);
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HwdecMode {
    AutoCopy,
    Software,
}

impl HwdecMode {
    pub fn mpv_value(self) -> &'static str {
        match self {
            Self::AutoCopy => "auto-copy",
            Self::Software => "no",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::AutoCopy => "Auto-copy (GPU H.264/HEVC + CPU VP9/AV1)",
            Self::Software => "Software CPU (Maximum stability)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HoverPreviewQuality {
    Off,
    P360,
    P480,
    P720,
}

impl HoverPreviewQuality {
    pub fn max_height(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::P360 => 360,
            Self::P480 => 480,
            Self::P720 => 720,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::P360 => "360",
            Self::P480 => "480",
            Self::P720 => "720",
        }
    }

    pub fn from_key(s: &str) -> Self {
        match s {
            "off" => Self::Off,
            "360" => Self::P360,
            "720" => Self::P720,
            _ => Self::P480,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::P360 => "360p (Fastest)",
            Self::P480 => "480p",
            Self::P720 => "720p (HD)",
        }
    }

    pub const ALL: [Self; 4] = [Self::Off, Self::P360, Self::P480, Self::P720];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreferredQuality {
    Best4K,
    P1440,
    P1080,
    P720,
    P480,
}

impl PreferredQuality {
    pub fn max_height(self) -> u32 {
        match self {
            Self::Best4K => 2160,
            Self::P1440 => 1440,
            Self::P1080 => 1080,
            Self::P720 => 720,
            Self::P480 => 480,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Best4K => "Auto (up to 4K)",
            Self::P1440 => "1440p",
            Self::P1080 => "1080p",
            Self::P720 => "720p",
            Self::P480 => "480p",
        }
    }

    pub const ALL: [Self; 5] = [
        Self::Best4K,
        Self::P1440,
        Self::P1080,
        Self::P720,
        Self::P480,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PoTokenMode {
    #[default]
    Auto,
    On,
    Off,
}

impl PoTokenMode {
    pub const ALL: [Self; 3] = [Self::Auto, Self::On, Self::Off];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto (When blocked)",
            Self::On => "Always On",
            Self::Off => "Off",
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Auto => 1,
            Self::On => 2,
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Off,
            2 => Self::On,
            _ => Self::Auto,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WindowMaterial {
    #[default]
    None,
    Acrylic,
    Mica,
    MicaAlt,
}

impl WindowMaterial {
    pub const ALL: [Self; 4] = [Self::None, Self::Acrylic, Self::Mica, Self::MicaAlt];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Solid Dark",
            Self::Acrylic => "Acrylic",
            Self::Mica => "Mica",
            Self::MicaAlt => "Mica Alt",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Acrylic => "acrylic",
            Self::Mica => "mica",
            Self::MicaAlt => "mica_alt",
        }
    }

    pub fn from_key(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|m| m.key() == s.trim())
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AmbientMode {
    #[default]
    SoftBlur,
    TiledBlur,
    Off,
}

impl AmbientMode {
    pub const ALL: [Self; 3] = [Self::SoftBlur, Self::TiledBlur, Self::Off];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::SoftBlur => "Soft Blur",
            Self::TiledBlur => "Tiled Repeat",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::SoftBlur => "soft",
            Self::TiledBlur => "tiled",
        }
    }

    pub fn from_key(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|m| m.key() == s.trim())
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SponsorSegment {
    pub start: f64,
    pub end: f64,
    pub video_duration: f64,
    pub category: SponsorCategory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SponsorCategory {
    Sponsor,
    SelfPromo,
    Interaction,
    Intro,
    Outro,
    Other,
}

impl SponsorCategory {
    pub const ALL: &'static [Self] = &[
        Self::Sponsor,
        Self::SelfPromo,
        Self::Interaction,
        Self::Intro,
        Self::Outro,
        Self::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Sponsor => "Sponsor",
            Self::SelfPromo => "Unpaid / Self-Promo",
            Self::Interaction => "Interaction Reminder",
            Self::Intro => "Intro / Intermission",
            Self::Outro => "Outro / Endcards",
            Self::Other => "Other / Highlight",
        }
    }
}

/// Customizable keyboard shortcut binding (e.g. `"Space"`, `"Ctrl+I"`, `"Left"`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub shift: bool,
}

impl KeyBinding {
    pub fn simple(key: &str) -> Self {
        Self {
            key: key.to_owned(),
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    pub fn ctrl(key: &str) -> Self {
        Self {
            key: key.to_owned(),
            ctrl: true,
            alt: false,
            shift: false,
        }
    }

    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        parts.push(self.key.as_str());
        parts.join("+")
    }

    pub fn parse(spec: &str) -> Option<Self> {
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut key_part = None;
        for tok in spec.split('+').map(str::trim).filter(|t| !t.is_empty()) {
            match tok.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                _ => key_part = Some(tok),
            }
        }
        let k = key_part?;
        let canonical = match k.to_ascii_lowercase().as_str() {
            "space" | " " => "Space".to_owned(),
            "left" | "arrowleft" => "Left".to_owned(),
            "right" | "arrowright" => "Right".to_owned(),
            "up" | "arrowup" => "Up".to_owned(),
            "down" | "arrowdown" => "Down".to_owned(),
            "enter" | "return" => "Enter".to_owned(),
            other if other.len() == 1 => other.to_ascii_uppercase(),
            other => {
                let mut ch = other.chars();
                let first = ch.next()?.to_ascii_uppercase();
                format!("{first}{}", ch.as_str())
            }
        };
        Some(Self {
            key: canonical,
            ctrl,
            alt,
            shift,
        })
    }

    pub fn egui_key(&self) -> Option<egui::Key> {
        match self.key.as_str() {
            "Space" => Some(egui::Key::Space),
            "Left" => Some(egui::Key::ArrowLeft),
            "Right" => Some(egui::Key::ArrowRight),
            "Up" => Some(egui::Key::ArrowUp),
            "Down" => Some(egui::Key::ArrowDown),
            "Enter" => Some(egui::Key::Enter),
            s => egui::Key::from_name(s),
        }
    }

    pub fn key_name(key: egui::Key) -> &'static str {
        match key {
            egui::Key::Space => "Space",
            egui::Key::ArrowLeft => "Left",
            egui::Key::ArrowRight => "Right",
            egui::Key::ArrowUp => "Up",
            egui::Key::ArrowDown => "Down",
            _ => key.name(),
        }
    }

    pub fn matches(&self, input: &egui::InputState) -> bool {
        let Some(k) = self.egui_key() else {
            return false;
        };
        input.key_pressed(k)
            && input.modifiers.ctrl == self.ctrl
            && input.modifiers.alt == self.alt
            && input.modifiers.shift == self.shift
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutConfig {
    pub play_pause: KeyBinding,
    pub play_pause_alt: KeyBinding,
    pub seek_back: KeyBinding,
    pub seek_forward: KeyBinding,
    pub seek_back_big: KeyBinding,
    pub seek_forward_big: KeyBinding,
    pub mute: KeyBinding,
    pub theater: KeyBinding,
    pub fullscreen: KeyBinding,
    pub volume_up: KeyBinding,
    pub volume_down: KeyBinding,
    pub stats_for_nerds: KeyBinding,
    pub toggle_captions: KeyBinding,
    pub screenshot: KeyBinding,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            play_pause: KeyBinding::simple("Space"),
            play_pause_alt: KeyBinding::simple("K"),
            seek_back: KeyBinding::simple("Left"),
            seek_forward: KeyBinding::simple("Right"),
            seek_back_big: KeyBinding::simple("J"),
            seek_forward_big: KeyBinding::simple("L"),
            mute: KeyBinding::simple("M"),
            theater: KeyBinding::simple("T"),
            fullscreen: KeyBinding::simple("F"),
            volume_up: KeyBinding::simple("Up"),
            volume_down: KeyBinding::simple("Down"),
            stats_for_nerds: KeyBinding::ctrl("I"),
            toggle_captions: KeyBinding::simple("C"),
            screenshot: KeyBinding::simple("S"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutSlot {
    PlayPause,
    PlayPauseAlt,
    SeekBack,
    SeekForward,
    SeekBackBig,
    SeekForwardBig,
    Mute,
    Theater,
    Fullscreen,
    VolumeUp,
    VolumeDown,
    StatsForNerds,
    ToggleCaptions,
    Screenshot,
}

impl ShortcutSlot {
    pub const ALL: &'static [Self] = &[
        Self::PlayPause,
        Self::PlayPauseAlt,
        Self::SeekBack,
        Self::SeekForward,
        Self::SeekBackBig,
        Self::SeekForwardBig,
        Self::Mute,
        Self::Theater,
        Self::Fullscreen,
        Self::VolumeUp,
        Self::VolumeDown,
        Self::StatsForNerds,
        Self::ToggleCaptions,
        Self::Screenshot,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::PlayPause => "Play / Pause",
            Self::PlayPauseAlt => "Play / Pause (Alt)",
            Self::SeekBack => "Seek Back (-5s)",
            Self::SeekForward => "Seek Forward (+5s)",
            Self::SeekBackBig => "Seek Back (-10s)",
            Self::SeekForwardBig => "Seek Forward (+10s)",
            Self::Mute => "Mute / Unmute",
            Self::Theater => "Theater Mode",
            Self::Fullscreen => "Fullscreen",
            Self::VolumeUp => "Volume Up (+5%)",
            Self::VolumeDown => "Volume Down (-5%)",
            Self::StatsForNerds => "Stats for Nerds",
            Self::ToggleCaptions => "Subtitles / Captions",
            Self::Screenshot => "Screenshot (DropBoard / Clipboard)",
        }
    }
}

impl ShortcutConfig {
    pub fn get(&self, slot: ShortcutSlot) -> &KeyBinding {
        match slot {
            ShortcutSlot::PlayPause => &self.play_pause,
            ShortcutSlot::PlayPauseAlt => &self.play_pause_alt,
            ShortcutSlot::SeekBack => &self.seek_back,
            ShortcutSlot::SeekForward => &self.seek_forward,
            ShortcutSlot::SeekBackBig => &self.seek_back_big,
            ShortcutSlot::SeekForwardBig => &self.seek_forward_big,
            ShortcutSlot::Mute => &self.mute,
            ShortcutSlot::Theater => &self.theater,
            ShortcutSlot::Fullscreen => &self.fullscreen,
            ShortcutSlot::VolumeUp => &self.volume_up,
            ShortcutSlot::VolumeDown => &self.volume_down,
            ShortcutSlot::StatsForNerds => &self.stats_for_nerds,
            ShortcutSlot::ToggleCaptions => &self.toggle_captions,
            ShortcutSlot::Screenshot => &self.screenshot,
        }
    }

    pub fn set(&mut self, slot: ShortcutSlot, binding: KeyBinding) {
        match slot {
            ShortcutSlot::PlayPause => self.play_pause = binding,
            ShortcutSlot::PlayPauseAlt => self.play_pause_alt = binding,
            ShortcutSlot::SeekBack => self.seek_back = binding,
            ShortcutSlot::SeekForward => self.seek_forward = binding,
            ShortcutSlot::SeekBackBig => self.seek_back_big = binding,
            ShortcutSlot::SeekForwardBig => self.seek_forward_big = binding,
            ShortcutSlot::Mute => self.mute = binding,
            ShortcutSlot::Theater => self.theater = binding,
            ShortcutSlot::Fullscreen => self.fullscreen = binding,
            ShortcutSlot::VolumeUp => self.volume_up = binding,
            ShortcutSlot::VolumeDown => self.volume_down = binding,
            ShortcutSlot::StatsForNerds => self.stats_for_nerds = binding,
            ShortcutSlot::ToggleCaptions => self.toggle_captions = binding,
            ShortcutSlot::Screenshot => self.screenshot = binding,
        }
    }
}

impl SponsorSegment {
    pub fn fits(&self, duration: f64) -> bool {
        self.video_duration <= 0.0 || duration <= 0.0 || (self.video_duration - duration).abs() < 4.0
    }
}

/// Persisted snapshot of active watch tab(s) for crash-safe / restart resume.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LastWatched {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub thumbnail_url: String,
    pub position_secs: f64,
    pub duration_secs: f64,
    #[serde(default)]
    pub second_tab: Option<Box<LastWatched>>,
    #[serde(default)]
    pub active_tab: u8,
}

#[cfg(test)]
mod tests {
    use super::{KeyBinding, ShortcutConfig, ShortcutSlot, UpNextFilterMode};

    #[test]
    fn classifies_up_next_filter_chips() {
        assert_eq!(
            UpNextFilterMode::classify_chip("All", "Dr. Tirta"),
            Some(UpNextFilterMode::All)
        );
        assert_eq!(
            UpNextFilterMode::classify_chip("Semua", "Dr. Tirta"),
            Some(UpNextFilterMode::All)
        );
        assert_eq!(
            UpNextFilterMode::classify_chip("From Dr. Tirta", "Dr. Tirta"),
            Some(UpNextFilterMode::FromChannel)
        );
        assert_eq!(
            UpNextFilterMode::classify_chip("Dari Ado", "Ado"),
            Some(UpNextFilterMode::FromChannel)
        );
        assert_eq!(
            UpNextFilterMode::classify_chip("Terkait", "Ado"),
            Some(UpNextFilterMode::Related)
        );
        assert_eq!(
            UpNextFilterMode::classify_chip("Untuk Anda", "Ado"),
            Some(UpNextFilterMode::ForYou)
        );
        assert_eq!(UpNextFilterMode::classify_chip("Anime", "Ado"), None);
    }

    #[test]
    fn parses_and_formats_key_bindings() {
        let b = KeyBinding::parse("ctrl + shift + i").unwrap();
        assert!(b.ctrl);
        assert!(b.shift);
        assert!(!b.alt);
        assert_eq!(b.key, "I");
        assert_eq!(b.display(), "Ctrl+Shift+I");
        assert_eq!(b.egui_key(), Some(egui::Key::I));

        let sp = KeyBinding::parse("space").unwrap();
        assert_eq!(sp.display(), "Space");
        assert_eq!(sp.egui_key(), Some(egui::Key::Space));

        let mut cfg = ShortcutConfig::default();
        assert_eq!(cfg.get(ShortcutSlot::StatsForNerds).display(), "Ctrl+I");
        cfg.set(ShortcutSlot::Mute, KeyBinding::simple("N"));
        let raw = serde_json::to_string(&cfg).unwrap();
        let back: ShortcutConfig = serde_json::from_str(&raw).unwrap();
        assert_eq!(back, cfg);
    }
}
