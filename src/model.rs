use serde::{Deserialize, Serialize};

/// A filter chip on Home / Explore (e.g. "All", "Gaming", "Music", "Live").
#[derive(Clone, Debug)]
pub struct FeedChip {
    pub title: String,
    pub params: Option<String>,
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
    pub fn label(&self) -> String {
        let codec = if self.mime.contains("av01") {
            "AV1"
        } else if self.mime.contains("vp9") || self.mime.contains("vp09") {
            "VP9"
        } else if self.mime.contains("avc1") {
            "H.264"
        } else {
            "Video"
        };
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
    pub author: String,
    pub avatar_url: Option<String>,
    pub published: String,
    pub likes: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct LiveChatMessage {
    pub id: String,
    pub author: String,
    pub text: String,
    pub is_owner: bool,
    pub is_mod: bool,
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
    pub is_liked: bool,
    pub is_disliked: bool,
    pub is_subscribed: bool,
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
    pub is_live: bool,
    pub resume_secs: Option<f64>,
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

#[derive(Clone, Copy, Debug)]
pub struct SponsorSegment {
    pub start: f64,
    pub end: f64,
    pub video_duration: f64,
    pub category: SponsorCategory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SponsorCategory {
    Sponsor,
    SelfPromo,
    Interaction,
    Intro,
    Outro,
    Other,
}

impl SponsorSegment {
    pub fn fits(&self, duration: f64) -> bool {
        self.video_duration <= 0.0 || duration <= 0.0 || (self.video_duration - duration).abs() < 4.0
    }
}
