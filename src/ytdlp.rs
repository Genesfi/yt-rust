use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DownloadProgress {
    pub percent: f32,
    pub speed: String,
    pub total_size: String,
    pub status: String,
}

pub fn parse_progress_line(line: &str) -> Option<DownloadProgress> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("DLPROG:") {
        let mut parts = rest.splitn(3, '|');
        let pct_str = parts.next().unwrap_or("").trim().trim_end_matches('%');
        let raw_speed = parts.next().unwrap_or("").trim();
        let raw_size = parts.next().unwrap_or("").trim();
        let percent = pct_str
            .parse::<f32>()
            .map(|p| (p / 100.0).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let speed = if raw_speed.is_empty() || raw_speed == "N/A" || raw_speed == "Unknown" {
            String::new()
        } else {
            raw_speed.to_owned()
        };
        let total_size = if raw_size.is_empty() || raw_size == "N/A" || raw_size == "Unknown" {
            String::new()
        } else {
            raw_size.to_owned()
        };
        return Some(DownloadProgress {
            percent,
            speed,
            total_size,
            status: "Downloading...".to_owned(),
        });
    }
    if trimmed.starts_with("[Merger]")
        || trimmed.starts_with("[ExtractAudio]")
        || trimmed.starts_with("[Fixup")
    {
        return Some(DownloadProgress {
            percent: 1.0,
            speed: String::new(),
            total_size: String::new(),
            status: "Merging & finalizing...".to_owned(),
        });
    }
    None
}

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::model::{AudioTrack, VideoFormat};

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

#[cfg(windows)]
const QJS: &str = "qjs.exe";
#[cfg(not(windows))]
const QJS: &str = "qjs";
#[cfg(all(windows, target_arch = "x86"))]
const QJS_ASSET: &str = "qjs-windows-x86.exe";
#[cfg(all(windows, not(target_arch = "x86")))]
const QJS_ASSET: &str = "qjs-windows-x86_64.exe";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const QJS_ASSET: &str = "qjs-darwin-arm64";
#[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
const QJS_ASSET: &str = "qjs-darwin-x86_64";
#[cfg(all(unix, not(target_os = "macos"), target_arch = "aarch64"))]
const QJS_ASSET: &str = "qjs-linux-aarch64";
#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "aarch64")))]
const QJS_ASSET: &str = "qjs-linux-x86_64";
const QJS_REPO: &str = "quickjs-ng/quickjs";
const QJS_MAX_AGE: Duration = Duration::from_secs(30 * 24 * 3600);

pub struct YtdlpStreams {
    pub video_url: String,
    pub video_content_length: u64,
    pub audio_url: Option<String>,
    pub audio_content_length: u64,
    pub hls_url: Option<String>,
    pub video_formats: Vec<VideoFormat>,
    pub audio_tracks: Vec<AudioTrack>,
    pub user_agent: Option<String>,
    pub title: Option<String>,
    pub channel: Option<String>,
    pub channel_id: Option<String>,
    pub description: String,
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
    ensure_github_tool(http, data_dir, BIN, REPO, ASSET, MAX_AGE).await
}

async fn resolve_js_runtime(http: &reqwest::Client, data_dir: &Path) -> Option<String> {
    let exe = |name: &str| {
        if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_owned()
        }
    };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for name in ["node", "deno"] {
                let p = dir.join(exe(name));
                if p.is_file() {
                    return Some(format!("{name}:{}", p.display()));
                }
            }
        }
    }
    #[cfg(windows)]
    {
        let mut cands = vec![PathBuf::from(r"C:\Program Files\nodejs\node.exe")];
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            cands.push(PathBuf::from(local).join("Programs").join("nodejs").join("node.exe"));
        }
        for c in cands {
            if c.is_file() {
                return Some(format!("node:{}", c.display()));
            }
        }
    }
    match ensure_github_tool(http, data_dir, QJS, QJS_REPO, QJS_ASSET, QJS_MAX_AGE).await {
        Ok(p) => Some(format!("quickjs:{}", p.display())),
        Err(e) => {
            log::warn!("No Node/Deno found and QuickJS download failed: {e:#}");
            None
        }
    }
}

async fn ensure_github_tool(
    http: &reqwest::Client,
    data_dir: &Path,
    bin_name: &str,
    repo: &str,
    asset: &str,
    max_age: Duration,
) -> anyhow::Result<PathBuf> {
    let bin = data_dir.join(bin_name);
    let fresh = std::fs::metadata(&bin)
        .and_then(|m| m.modified())
        .is_ok_and(|t| SystemTime::now().duration_since(t).unwrap_or_default() < max_age);
    if fresh {
        return Ok(bin);
    }
    match fetch_release_asset(http, &bin, repo, asset).await {
        Ok(()) => Ok(bin),
        Err(e) if bin.is_file() => {
            log::warn!("{bin_name} update failed, keeping existing binary: {e:#}");
            Ok(bin)
        }
        Err(e) => Err(e),
    }
}

async fn fetch_release_asset(
    http: &reqwest::Client,
    dest: &Path,
    repo: &str,
    asset_name: &str,
) -> anyhow::Result<()> {
    let release: Value = http
        .get(format!("https://api.github.com/repos/{repo}/releases/latest"))
        .header("User-Agent", "yt-rust")
        .timeout(Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let asset = release["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"] == asset_name))
        .ok_or_else(|| anyhow::anyhow!("No {asset_name} found in {repo} release"))?;
    let url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing download URL for {asset_name}"))?;
    let expected_sha = asset["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .map(str::to_ascii_lowercase);

    log::info!("Downloading {asset_name}...");
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
        anyhow::ensure!(got == want, "{asset_name} SHA-256 mismatch: {got} != {want}");
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

const DEFAULT_STREAM_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36";

async fn probe_stream_ok(http: &reqwest::Client, url: &str, ua: Option<&str>) -> bool {
    if url.contains(".m3u8") || url.contains("/manifest/hls") {
        return true;
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    let probe_url = if url.contains("range=") {
        url.to_owned()
    } else {
        format!("{url}{sep}range=0-1023")
    };
    let ua = ua.filter(|s| !s.trim().is_empty()).unwrap_or(DEFAULT_STREAM_UA);
    match http
        .get(&probe_url)
        .header("User-Agent", ua)
        .timeout(Duration::from_secs(6))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => true,
    }
}

/// Fallback stream resolution using yt-dlp when InnerTube direct URLs fail.
pub async fn resolve_video(
    http: &reqwest::Client,
    data_dir: &Path,
    cookie: Option<&str>,
    video_id: &str,
    max_height: u32,
) -> anyhow::Result<YtdlpStreams> {
    let (bin_res, runtime) = tokio::join!(
        ensure_ytdlp(http, data_dir),
        resolve_js_runtime(http, data_dir),
    );
    let bin = bin_res?;
    let data_dir_owned = data_dir.to_owned();
    let cookie_owned = cookie.filter(|c| !c.trim().is_empty()).map(str::to_owned);
    let vid = video_id.to_owned();
    let rt_owned = runtime.clone();
    let bin_first = bin.clone();
    let streams = tokio::task::spawn_blocking(move || {
        run_ytdlp(
            &bin_first,
            rt_owned.as_deref(),
            &data_dir_owned,
            cookie_owned.as_deref(),
            &vid,
            max_height,
            false,
        )
    })
    .await??;

    // YouTube c=WEB URLs resolved with cookies can return HTTP 403 on GVS without a browser PO-token.
    // Probe the first 1 KB; if 403, re-resolve using web_safari/tv/android_vr HLS/non-PO-token clients.
    if !probe_stream_ok(http, &streams.video_url, streams.user_agent.as_deref()).await {
        log::warn!(
            "yt-dlp primary stream for {video_id} returned 403 on preflight; falling back to web_safari/tv clients"
        );
        let data_dir_retry = data_dir.to_owned();
        let cookie_retry = cookie.filter(|c| !c.trim().is_empty()).map(str::to_owned);
        let vid_retry = video_id.to_owned();
        if let Ok(Ok(alt)) = tokio::task::spawn_blocking(move || {
            run_ytdlp(
                &bin,
                runtime.as_deref(),
                &data_dir_retry,
                cookie_retry.as_deref(),
                &vid_retry,
                max_height,
                true,
            )
        })
        .await
        {
            return Ok(alt);
        }
    }
    Ok(streams)
}

fn apply_js_runtime(cmd: &mut Command, runtime: Option<&str>) {
    cmd.args(["--js-runtimes", "node"]);
    if let Some(rt) = runtime.filter(|r| !r.starts_with("node") || r.contains(':')) {
        cmd.args(["--js-runtimes", rt]);
    }
}

fn invoke_ytdlp_json(
    bin: &Path,
    runtime: Option<&str>,
    jar_path: Option<&Path>,
    video_id: &str,
    selector: &str,
    extra_args: &[&str],
) -> anyhow::Result<std::process::Output> {
    let mut cmd = Command::new(bin);
    cmd.args(["--no-warnings", "--no-playlist", "--no-cache-dir"]);
    apply_js_runtime(&mut cmd, runtime);
    cmd.args(extra_args);
    cmd.args(["-f", selector, "-j"]);
    if let Some(jar) = jar_path {
        cmd.arg("--cookies").arg(jar);
    }
    cmd.arg(format!("https://www.youtube.com/watch?v={video_id}"));

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }

    Ok(cmd.output()?)
}

fn run_ytdlp(
    bin: &Path,
    runtime: Option<&str>,
    data_dir: &Path,
    cookie: Option<&str>,
    video_id: &str,
    max_height: u32,
    prefer_alt_clients: bool,
) -> anyhow::Result<YtdlpStreams> {
    let _jar = cookie
        .map(|c| TempCookieJar::create(data_dir, c))
        .transpose()?;
    let jar_path = _jar.as_ref().map(|j| j.path.as_path());
    let selector = format!(
        "bestvideo[height<={max_height}]+bestaudio/best[height<={max_height}]/best"
    );
    let hls_sel = format!(
        "bestvideo[height<={max_height}][protocol^=m3u8]+bestaudio/best[height<={max_height}][protocol^=m3u8]/bestvideo[height<={max_height}]+bestaudio/best[height<={max_height}]/best"
    );
    let mut out = if prefer_alt_clients {
        invoke_ytdlp_json(
            bin,
            runtime,
            jar_path,
            video_id,
            &hls_sel,
            &["--extractor-args", "youtube:player_client=web_safari,tv,android_vr"],
        )?
    } else {
        invoke_ytdlp_json(bin, runtime, jar_path, video_id, &selector, &[])?
    };
    if !out.status.success() && !prefer_alt_clients {
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("Requested format is not available") {
            out = invoke_ytdlp_json(
                bin,
                runtime,
                jar_path,
                video_id,
                &hls_sel,
                &["--extractor-args", "youtube:player_client=web_safari,tv,android_vr"],
            )?;
        }
    }
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
    parse_ytdlp_streams(&v)
}

fn codec_mime(vcodec: &str) -> String {
    let lc = vcodec.to_ascii_lowercase();
    if lc.contains("av01") || lc.contains("av1") {
        "video/mp4; codecs=\"av01.0.08M.08\"".to_owned()
    } else if lc.contains("vp9") || lc.contains("vp09") {
        "video/webm; codecs=\"vp9\"".to_owned()
    } else if lc.contains("avc1") || lc.contains("h264") {
        "video/mp4; codecs=\"avc1.640028\"".to_owned()
    } else {
        format!("video/mp4; codecs=\"{vcodec}\"")
    }
}

fn ytdlp_codec_rank(mime: &str) -> u8 {
    if mime.contains("avc1") {
        3
    } else if mime.contains("vp9") || mime.contains("vp09") {
        2
    } else if mime.contains("av01") {
        1
    } else {
        0
    }
}

fn parse_ytdlp_streams(v: &Value) -> anyhow::Result<YtdlpStreams> {
    let title = v["title"].as_str().map(str::to_owned);
    let channel = v["uploader"]
        .as_str()
        .or_else(|| v["channel"].as_str())
        .map(str::to_owned);
    let channel_id = v["channel_id"]
        .as_str()
        .filter(|s| s.starts_with("UC"))
        .map(str::to_owned);
    let description = v["description"].as_str().unwrap_or_default().to_owned();
    let duration_secs = v["duration"].as_f64().unwrap_or(0.0);

    let mut video_formats = Vec::new();
    let mut audio_cands: Vec<(bool, u64, AudioTrack)> = Vec::new();
    let mut best_hls_url: Option<String> = v["manifest_url"].as_str().map(str::to_owned);

    if let Some(formats) = v["formats"].as_array() {
        for f in formats {
            let protocol = f["protocol"].as_str().unwrap_or_default();
            let Some(url) = f["url"].as_str() else {
                continue;
            };
            if protocol.contains("m3u8") {
                if f["vcodec"].as_str().is_some_and(|c| c != "none") {
                    best_hls_url = Some(url.to_owned());
                }
                continue;
            }
            if protocol != "https" && protocol != "http" {
                continue;
            }
            let vcodec = f["vcodec"].as_str().unwrap_or("none");
            let acodec = f["acodec"].as_str().unwrap_or("none");
            let content_length = f["filesize"]
                .as_u64()
                .or_else(|| f["filesize_approx"].as_u64())
                .unwrap_or(0);

            if vcodec != "none" {
                let height = f["height"].as_u64().unwrap_or(0) as u32;
                if height == 0 {
                    continue;
                }
                let width = f["width"].as_u64().unwrap_or(0) as u32;
                let fps = f["fps"].as_f64().map_or(30, |x| x.round() as u32).max(1);
                let bitrate = f["tbr"]
                    .as_f64()
                    .or_else(|| f["vbr"].as_f64())
                    .map_or(0, |b| (b * 1000.0) as u64);
                let itag = f["format_id"]
                    .as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                video_formats.push(VideoFormat {
                    itag,
                    url: url.to_owned(),
                    mime: codec_mime(vcodec),
                    width,
                    height,
                    fps,
                    bitrate,
                    content_length,
                    has_audio: acodec != "none",
                });
            } else if acodec != "none" {
                let abr = f["abr"]
                    .as_f64()
                    .or_else(|| f["tbr"].as_f64())
                    .map_or(0, |b| (b * 1000.0) as u64);
                let lang = f["language"].as_str().unwrap_or_default();
                let note = f["format_note"].as_str().unwrap_or_default();
                let is_default = note.to_ascii_lowercase().contains("original")
                    || f["language_preference"].as_i64().unwrap_or(0) >= 0;
                let label = if !lang.is_empty() && !note.is_empty() {
                    format!("{lang} ({note})")
                } else if !note.is_empty() {
                    note.to_owned()
                } else if !lang.is_empty() {
                    lang.to_owned()
                } else {
                    format!("Audio ({acodec})")
                };
                audio_cands.push((
                    is_default,
                    abr,
                    AudioTrack {
                        id: f["format_id"].as_str().unwrap_or_default().to_owned(),
                        label,
                        is_default,
                        url: url.to_owned(),
                        content_length,
                    },
                ));
            }
        }
    }

    video_formats.sort_by(|a, b| {
        b.height
            .cmp(&a.height)
            .then_with(|| b.fps.cmp(&a.fps))
            .then_with(|| ytdlp_codec_rank(&b.mime).cmp(&ytdlp_codec_rank(&a.mime)))
            .then_with(|| b.bitrate.cmp(&a.bitrate))
    });
    let mut deduped_v = Vec::new();
    for f in video_formats {
        let same = deduped_v.iter().any(|d: &VideoFormat| {
            d.height == f.height
                && d.fps == f.fps
                && ytdlp_codec_rank(&d.mime) == ytdlp_codec_rank(&f.mime)
                && d.has_audio == f.has_audio
        });
        if !same {
            deduped_v.push(f);
        }
    }

    audio_cands.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let mut audio_tracks: Vec<AudioTrack> = Vec::new();
    for (_, _, tr) in audio_cands {
        if !audio_tracks.iter().any(|x| x.label == tr.label) {
            audio_tracks.push(tr);
        }
    }

    let (fallback_video_url, fallback_vlen, fallback_audio_url, fallback_alen) =
        if let Some(req_formats) = v["requested_formats"].as_array() {
            let vf = req_formats
                .iter()
                .find(|f| f["vcodec"].as_str().is_some_and(|c| c != "none"));
            let af = req_formats
                .iter()
                .find(|f| f["acodec"].as_str().is_some_and(|c| c != "none"));
            (
                vf.and_then(|f| f["url"].as_str()).map(str::to_owned),
                vf.and_then(|f| f["filesize"].as_u64().or_else(|| f["filesize_approx"].as_u64()))
                    .unwrap_or(0),
                af.and_then(|f| f["url"].as_str()).map(str::to_owned),
                af.and_then(|f| f["filesize"].as_u64().or_else(|| f["filesize_approx"].as_u64()))
                    .unwrap_or(0),
            )
        } else {
            (
                v["url"].as_str().map(str::to_owned),
                v["filesize"]
                    .as_u64()
                    .or_else(|| v["filesize_approx"].as_u64())
                    .unwrap_or(0),
                None,
                0,
            )
        };

    let audio_url = audio_tracks
        .first()
        .map(|t| t.url.clone())
        .or(fallback_audio_url);
    let audio_content_length = audio_tracks
        .first()
        .map(|t| t.content_length)
        .filter(|&n| n > 0)
        .unwrap_or(fallback_alen);
    let video_url = fallback_video_url
        .or_else(|| deduped_v.first().map(|f| f.url.clone()))
        .or_else(|| best_hls_url.clone())
        .ok_or_else(|| anyhow::anyhow!("yt-dlp returned no video URL"))?;
    let user_agent = v["requested_formats"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|f| f["http_headers"]["User-Agent"].as_str())
        .or_else(|| v["http_headers"]["User-Agent"].as_str())
        .or_else(|| {
            v["formats"].as_array().and_then(|a| {
                a.iter()
                    .find_map(|f| f["http_headers"]["User-Agent"].as_str())
            })
        })
        .map(str::to_owned);

    Ok(YtdlpStreams {
        video_url,
        video_content_length: fallback_vlen,
        audio_url,
        audio_content_length,
        hls_url: best_hls_url,
        video_formats: deduped_v,
        audio_tracks,
        user_agent,
        title,
        channel,
        channel_id,
        description,
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
    on_progress: impl Fn(DownloadProgress) + Send + 'static,
) -> anyhow::Result<PathBuf> {
    let (bin_res, runtime) = tokio::join!(
        ensure_ytdlp(http, data_dir),
        resolve_js_runtime(http, data_dir),
    );
    let bin = bin_res?;
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
            "--newline",
            "--progress",
            "--progress-template",
            "download:DLPROG:%(progress._percent_str)s|%(progress._speed_str)s|%(progress._total_bytes_estimate_str,progress._total_bytes_str)s",
            "-P",
            &out_dir_str,
            "-o",
            "%(title).100B [%(id)s].%(ext)s",
            "--print",
            "after_move:filepath",
        ]);
        apply_js_runtime(&mut cmd, runtime.as_deref());
        if let Some(ref jar) = _jar {
            cmd.arg("--cookies").arg(&jar.path);
        }
        if audio_only {
            cmd.args(["-x", "--audio-format", "m4a", "-f", "bestaudio/best"]);
        } else {
            cmd.args(["-f", &fmt, "--merge-output-format", "mp4"]);
        }
        cmd.arg(format!("https://www.youtube.com/watch?v={vid}"));
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }

        let mut child = cmd.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("missing yt-dlp stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("missing yt-dlp stderr"))?;
        let err_handle = std::thread::spawn(move || {
            let mut errs = String::new();
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if !errs.is_empty() {
                    errs.push('\n');
                }
                errs.push_str(&line);
            }
            errs
        });

        let mut last_path: Option<PathBuf> = None;
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(prog) = parse_progress_line(&line) {
                on_progress(prog);
            } else {
                let t = line.trim();
                if !t.is_empty() && !t.starts_with('[') {
                    last_path = Some(PathBuf::from(t));
                }
            }
        }
        let status = child.wait()?;
        let err_text = err_handle.join().unwrap_or_default();
        if !status.success() {
            let msg = err_text
                .lines()
                .rfind(|l| l.contains("ERROR"))
                .unwrap_or("yt-dlp download failed");
            anyhow::bail!("{}", msg.trim());
        }
        Ok(last_path.unwrap_or(out_dir))
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::parse_progress_line;

    #[test]
    fn parses_ytdlp_progress_template_and_postprocess() {
        let p = parse_progress_line("DLPROG: 42.5%| 3.12MiB/s| 120.40MiB").unwrap();
        assert!((p.percent - 0.425).abs() < 0.001);
        assert_eq!(p.speed, "3.12MiB/s");
        assert_eq!(p.total_size, "120.40MiB");

        let m = parse_progress_line("[Merger] Merging formats into \"test.mp4\"").unwrap();
        assert_eq!(m.percent, 1.0);
        assert_eq!(m.status, "Merging & finalizing...");
    }
}
