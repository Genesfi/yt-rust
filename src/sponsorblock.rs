use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::model::{SponsorCategory, SponsorSegment};

const API: &str = "https://sponsor.ajay.app/api/skipSegments";
const CATEGORIES: &str =
    r#"["sponsor","selfpromo","interaction","intro","outro","music_offtopic"]"#;
const DEARROW_API: &str = "https://sponsor.ajay.app/api/branding";
const DEARROW_THUMB_API: &str = "https://dearrow-thumb.ajay.app/api/v1/getThumbnail";
const RYD_API: &str = "https://returnyoutubedislikeapi.com/votes";

#[derive(Clone, Debug, Default)]
pub struct DeArrowBranding {
    pub title: Option<String>,
    pub thumbnail_url: Option<String>,
}

#[derive(serde::Deserialize)]
struct VideoEntry {
    #[serde(rename = "videoID")]
    video_id: String,
    segments: Vec<RawSegment>,
}

#[derive(serde::Deserialize)]
struct RawSegment {
    segment: [f64; 2],
    #[serde(rename = "actionType")]
    action: String,
    #[serde(default)]
    category: String,
    #[serde(rename = "videoDuration", default)]
    duration: f64,
}

pub async fn fetch_segments(http: &reqwest::Client, video_id: &str) -> anyhow::Result<Vec<SponsorSegment>> {
    let hash = Sha256::digest(video_id.as_bytes());
    let url = url::Url::parse_with_params(
        &format!("{API}/{:02x}{:02x}", hash[0], hash[1]),
        [("categories", CATEGORIES), ("actionType", "skip")],
    )?;
    let resp = http
        .get(url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    let videos: Vec<VideoEntry> = resp.error_for_status()?.json().await?;
    let mut out: Vec<SponsorSegment> = videos
        .into_iter()
        .filter(|v| v.video_id == video_id)
        .flat_map(|v| v.segments)
        .filter(|s| s.action == "skip" && s.segment[1] > s.segment[0] + 0.5)
        .map(|s| SponsorSegment {
            start: s.segment[0],
            end: s.segment[1],
            video_duration: s.duration,
            category: match s.category.as_str() {
                "sponsor" => SponsorCategory::Sponsor,
                "selfpromo" => SponsorCategory::SelfPromo,
                "interaction" => SponsorCategory::Interaction,
                "intro" => SponsorCategory::Intro,
                "outro" => SponsorCategory::Outro,
                _ => SponsorCategory::Other,
            },
        })
        .collect();
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    Ok(out)
}

pub fn parse_dearrow_branding(video_id: &str, v: &Value) -> Option<DeArrowBranding> {
    let title = v["titles"].as_array().and_then(|arr| {
        arr.iter()
            .filter(|t| !t["original"].as_bool().unwrap_or(false))
            .max_by_key(|t| {
                (
                    t["locked"].as_bool().unwrap_or(false),
                    t["votes"].as_i64().unwrap_or(0),
                )
            })
            .and_then(|t| t["title"].as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    });
    let thumbnail_url = v["thumbnails"].as_array().and_then(|arr| {
        let best = arr
            .iter()
            .filter(|t| !t["original"].as_bool().unwrap_or(false))
            .max_by_key(|t| {
                (
                    t["locked"].as_bool().unwrap_or(false),
                    t["votes"].as_i64().unwrap_or(0),
                )
            })?;
        let ts = best["timestamp"].as_f64()?;
        Some(format!("{DEARROW_THUMB_API}?videoID={video_id}&time={ts:.2}"))
    });
    if title.is_none() && thumbnail_url.is_none() {
        None
    } else {
        Some(DeArrowBranding {
            title,
            thumbnail_url,
        })
    }
}

pub async fn fetch_dearrow(http: &reqwest::Client, video_id: &str) -> Option<DeArrowBranding> {
    let url = url::Url::parse_with_params(DEARROW_API, [("videoID", video_id)]).ok()?;
    let resp = http
        .get(url)
        .timeout(std::time::Duration::from_secs(4))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().await.ok()?;
    parse_dearrow_branding(video_id, &v)
}

pub fn format_compact_count(n: u64) -> String {
    if n >= 1_000_000 {
        let m = n as f64 / 1_000_000.0;
        if m >= 10.0 {
            format!("{}M", m.round() as u64)
        } else {
            format!("{m:.1}M").replace(".0M", "M")
        }
    } else if n >= 1_000 {
        let k = n as f64 / 1_000.0;
        if k >= 10.0 {
            format!("{}K", k.round() as u64)
        } else {
            format!("{k:.1}K").replace(".0K", "K")
        }
    } else {
        n.to_string()
    }
}

pub fn parse_ryd_dislikes(v: &Value) -> Option<String> {
    let n = v["dislikes"].as_u64()?;
    Some(format_compact_count(n))
}

pub async fn fetch_dislikes_count(http: &reqwest::Client, video_id: &str) -> Option<String> {
    let url = url::Url::parse_with_params(RYD_API, [("videoId", video_id)]).ok()?;
    let resp = http
        .get(url)
        .timeout(std::time::Duration::from_secs(4))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().await.ok()?;
    parse_ryd_dislikes(&v)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{format_compact_count, parse_dearrow_branding, parse_ryd_dislikes};

    #[test]
    fn formats_compact_counts_and_parses_ryd() {
        assert_eq!(format_compact_count(420), "420");
        assert_eq!(format_compact_count(1450), "1.4K");
        assert_eq!(format_compact_count(24_000), "24K");
        assert_eq!(format_compact_count(1_200_000), "1.2M");
        assert_eq!(
            parse_ryd_dislikes(&json!({ "dislikes": 12450 })).as_deref(),
            Some("12K")
        );
    }

    #[test]
    fn parses_dearrow_branding_response() {
        let v = json!({
            "titles": [
                { "title": "OMG YOU WON'T BELIEVE THIS", "original": true, "votes": 0 },
                { "title": "How Rust Async Works", "original": false, "votes": 5, "locked": false }
            ],
            "thumbnails": [
                { "timestamp": 14.5, "original": false, "votes": 3 }
            ]
        });
        let b = parse_dearrow_branding("vid123", &v).unwrap();
        assert_eq!(b.title.as_deref(), Some("How Rust Async Works"));
        assert_eq!(
            b.thumbnail_url.as_deref(),
            Some("https://dearrow-thumb.ajay.app/api/v1/getThumbnail?videoID=vid123&time=14.50")
        );
    }
}
