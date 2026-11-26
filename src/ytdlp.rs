use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use serde_json::Value;
use sha2::{Digest, Sha256};

#[cfg(windows)]
const BIN: &str = "yt-dlp.exe";
#[cfg(not(windows))]
const BIN: &str = "yt-dlp";

#[cfg(all(windows, not(target_arch = "aarch64")))]
const ASSET: &str = "yt-dlp.exe";
#[cfg(all(windows, target_arch = "aarch64"))]
const ASSET: &str = "yt-dlp_arm64.exe";
#[cfg(target_os = "macos")]
const ASSET: &str = "yt-dlp_macos";
#[cfg(all(unix, not(target_os = "macos")))]
const ASSET: &str = "yt-dlp_linux";

const REPO: &str = "yt-dlp/yt-dlp";
const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);

pub struct YtdlpStreams {
    pub video_url: String,
    pub audio_url: Option<String>,
    pub title: Option<String>,
    pub channel: Option<String>,
    pub duration_secs: f64,
}

pub fn remove_leftover_cookies(data_dir: &Path) {
    let Ok(files) = std::fs::read_dir(data_dir) else {
        return;
    };
    for f in files.flatten() {
        let name = f.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("cookies-") && name.ends_with(".txt") {
            let _ = std::fs::remove_file(f.path());
        }
    }
}

struct TempCookieJar {
    path: PathBuf,
}

impl TempCookieJar {
    fn create(data_dir: &Path, cookie: &str) -> anyhow::Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = data_dir.join(format!("cookies-{nonce}.txt"));
        let mut txt = String::from("# Netscape HTTP Cookie File\n");
        for kv in cookie.split(';') {
            if let Some((k, v)) = kv.trim().split_once('=') {
                txt.push_str(&format!(
                    ".youtube.com\tTRUE\t/\tTRUE\t2147483647\t{k}\t{v}\n"
                ));
            }
        }
        std::fs::write(&path, txt)?;
        Ok(Self { path })
    }
}

impl Drop for TempCookieJar {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn find_existing(data_dir: &Path) -> Option<PathBuf> {
    let local = data_dir.join(BIN);
    if local.is_file() {
        return Some(local);
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join(BIN);
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

async fn ensure_ytdlp(http: &reqwest::Client, data_dir: &Path) -> anyhow::Result<PathBuf> {
    if let Some(existing) = find_existing(data_dir) {
        let fresh = std::fs::metadata(&existing)
            .and_then(|m| m.modified())
            .is_ok_and(|t| SystemTime::now().duration_since(t).unwrap_or_default() < MAX_AGE);
        if fresh || existing != data_dir.join(BIN) {
            return Ok(existing);
        }
    }
    let bin = data_dir.join(BIN);
    match fetch_latest(http, &bin).await {
        Ok(()) => Ok(bin),
        Err(e) if bin.is_file() => {
            log::warn!("yt-dlp update failed, keeping existing binary: {e:#}");
            Ok(bin)
        }
        Err(e) => Err(e),
    }
}

async fn fetch_latest(http: &reqwest::Client, dest: &Path) -> anyhow::Result<()> {
    let release: Value = http
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("User-Agent", "yt-rust")
        .timeout(Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let asset = release["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"] == ASSET))
        .ok_or_else(|| anyhow::anyhow!("No {ASSET} found in yt-dlp release"))?;
    let url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing download URL for {ASSET}"))?;
    let expected_sha = asset["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .map(str::to_ascii_lowercase);

    log::info!("Downloading {ASSET}...");
    let bytes = http
        .get(url)
        .timeout(Duration::from_secs(120))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    if let Some(want) = expected_sha {
        let got = format!("{:x}", Sha256::digest(&bytes));
        anyhow::ensure!(got == want, "yt-dlp SHA-256 mismatch: {got} != {want}");
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

/// Fallback stream resolution using yt-dlp when InnerTube direct URLs fail.
pub async fn resolve_video(
    http: &reqwest::Client,
    data_dir: &Path,
    cookie: Option<&str>,
    video_id: &str,
    max_height: u32,
) -> anyhow::Result<YtdlpStreams> {
    let bin = ensure_ytdlp(http, data_dir).await?;
    let data_dir = data_dir.to_owned();
    let cookie = cookie.filter(|c| !c.trim().is_empty()).map(str::to_owned);
    let vid = video_id.to_owned();
    tokio::task::spawn_blocking(move || {
        run_ytdlp(&bin, &data_dir, cookie.as_deref(), &vid, max_height)
    })
    .await?
}

fn run_ytdlp(
    bin: &Path,
    data_dir: &Path,
    cookie: Option<&str>,
    video_id: &str,
    max_height: u32,
) -> anyhow::Result<YtdlpStreams> {
    let _jar = cookie
        .map(|c| TempCookieJar::create(data_dir, c))
        .transpose()?;
    let selector = format!(
        "bestvideo[height<={max_height}]+bestaudio/best[height<={max_height}]/best"
    );
    let mut cmd = Command::new(bin);
    cmd.args([
        "--no-warnings",
        "--no-playlist",
        "--no-cache-dir",
        "-f",
        &selector,
        "-j",
    ]);
    if let Some(ref jar) = _jar {
        cmd.arg("--cookies").arg(&jar.path);
    }
    cmd.arg(format!("https://www.youtube.com/watch?v={video_id}"));

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }

    let out = cmd.output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let msg = err
            .lines()
            .rfind(|l| l.contains("ERROR"))
            .unwrap_or("yt-dlp failed");
        if msg.contains("Sign in to confirm") && cookie.is_none() {
            anyhow::bail!("YouTube requested sign-in verification for this video — please Sign in first.");
        }
        anyhow::bail!("{}", msg.trim());
    }

    let v: Value = serde_json::from_slice(&out.stdout)?;
    let title = v["title"].as_str().map(str::to_owned);
    let channel = v["uploader"]
        .as_str()
        .or_else(|| v["channel"].as_str())
        .map(str::to_owned);
    let duration_secs = v["duration"].as_f64().unwrap_or(0.0);

    if let Some(req_formats) = v["requested_formats"].as_array() {
        let video_url = req_formats
            .iter()
            .find(|f| f["vcodec"].as_str().is_some_and(|c| c != "none"))
            .and_then(|f| f["url"].as_str())
            .ok_or_else(|| anyhow::anyhow!("yt-dlp returned no video URL"))?
            .to_owned();
        let audio_url = req_formats
            .iter()
            .find(|f| f["acodec"].as_str().is_some_and(|c| c != "none"))
            .and_then(|f| f["url"].as_str())
            .map(str::to_owned);
        return Ok(YtdlpStreams {
            video_url,
            audio_url,
            title,
            channel,
            duration_secs,
        });
    }

    let video_url = v["url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("yt-dlp returned no URL"))?
        .to_owned();
    Ok(YtdlpStreams {
        video_url,
        audio_url: None,
        title,
        channel,
        duration_secs,
    })
}

pub fn default_downloads_dir(data_dir: &Path) -> PathBuf {
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    match home {
        Some(h) => PathBuf::from(h).join("Downloads").join("yt-rust"),
        None => data_dir.join("downloads"),
    }
}

/// Downloads a video (MP4) or audio (M4A/Opus) into the configured download folder (or Downloads/yt-rust) via yt-dlp.
pub async fn download_video(
    http: &reqwest::Client,
    data_dir: &Path,
    custom_dir: Option<&Path>,
    cookie: Option<&str>,
    video_id: &str,
    audio_only: bool,
    max_height: u32,
) -> anyhow::Result<PathBuf> {
    let bin = ensure_ytdlp(http, data_dir).await?;
    let data_dir_owned = data_dir.to_path_buf();
    let cookie = cookie.filter(|c| !c.trim().is_empty()).map(str::to_owned);
    let out_dir = custom_dir
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_downloads_dir(data_dir));
    let vid = video_id.to_owned();
    tokio::task::spawn_blocking(move || -> anyhow::Result<PathBuf> {
        std::fs::create_dir_all(&out_dir)?;
        let _jar = cookie
            .as_deref()
            .map(|c| TempCookieJar::create(&data_dir_owned, c))
            .transpose()?;
        let out_dir_str = out_dir.to_string_lossy().into_owned();
        let mut cmd = Command::new(&bin);
        let fmt = format!(
            "bestvideo[height<={max_height}]+bestaudio/best[height<={max_height}]/best"
        );
        cmd.args([
            "--no-warnings",
            "--no-playlist",
            "--restrict-filenames",
            "-P",
            &out_dir_str,
            "-o",
            "%(title).100B [%(id)s].%(ext)s",
            "--print",
            "after_move:filepath",
        ]);
        if let Some(ref jar) = _jar {
            cmd.arg("--cookies").arg(&jar.path);
        }
        if audio_only {
            cmd.args(["-x", "--audio-format", "m4a", "-f", "bestaudio/best"]);
        } else {
            cmd.args(["-f", &fmt, "--merge-output-format", "mp4"]);
        }
        cmd.arg(format!("https://www.youtube.com/watch?v={vid}"));

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }

        let out = cmd.output()?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let msg = err
                .lines()
                .rfind(|l| l.contains("ERROR"))
                .unwrap_or("yt-dlp download failed");
            anyhow::bail!("{}", msg.trim());
        }
        let printed = String::from_utf8_lossy(&out.stdout);
        let saved = printed
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .map(|l| PathBuf::from(l.trim()))
            .unwrap_or(out_dir);
        Ok(saved)
    })
    .await?
}
