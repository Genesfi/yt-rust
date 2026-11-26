use sha2::{Digest, Sha256};

use crate::model::{SponsorCategory, SponsorSegment};

const API: &str = "https://sponsor.ajay.app/api/skipSegments";
const CATEGORIES: &str = r#"["sponsor","selfpromo","interaction","intro","outro"]"#;

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
