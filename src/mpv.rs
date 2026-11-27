use std::ffi::{CStr, CString, c_char, c_double, c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::glow::{self, HasContext};
use libloading::{Library, Symbol};

pub const AMB_GRID_W: usize = 16;
pub const AMB_GRID_H: usize = 10;

#[derive(Clone, Debug)]
pub struct LiveAmbientSnapshot {
    pub grid: [[u8; 3]; AMB_GRID_W * AMB_GRID_H],
    pub quadrants: [[u8; 3]; 4],
}

pub type GlProcAddrFn = Arc<dyn Fn(&CStr) -> *const c_void + Send + Sync>;

const MPV_FORMAT_FLAG: c_int = 3;
const MPV_FORMAT_DOUBLE: c_int = 5;

const MPV_RENDER_PARAM_INVALID: c_int = 0;
const MPV_RENDER_PARAM_API_TYPE: c_int = 1;
const MPV_RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
const MPV_RENDER_PARAM_OPENGL_FBO: c_int = 3;
const MPV_RENDER_PARAM_FLIP_Y: c_int = 4;

const MPV_RENDER_UPDATE_FRAME: u64 = 1;

#[repr(C)]
pub struct MpvHandle {
    _private: [u8; 0],
}

#[repr(C)]
pub struct MpvRenderContext {
    _private: [u8; 0],
}

#[repr(C)]
struct MpvRenderParam {
    type_: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct MpvOpenglInitParams {
    get_proc_address:
        unsafe extern "C" fn(ctx: *mut c_void, name: *const c_char) -> *mut c_void,
    get_proc_address_ctx: *mut c_void,
}

#[repr(C)]
struct MpvOpenglFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

struct MpvVtable {
    _lib: Library,
    mpv_create: unsafe extern "C" fn() -> *mut MpvHandle,
    mpv_initialize: unsafe extern "C" fn(*mut MpvHandle) -> c_int,
    mpv_terminate_destroy: unsafe extern "C" fn(*mut MpvHandle),
    mpv_wait_event: unsafe extern "C" fn(*mut MpvHandle, c_double) -> *mut MpvEvent,
    mpv_set_option_string:
        unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int,
    mpv_command: unsafe extern "C" fn(*mut MpvHandle, *const *const c_char) -> c_int,
    mpv_set_property:
        unsafe extern "C" fn(*mut MpvHandle, *const c_char, c_int, *mut c_void) -> c_int,
    mpv_set_property_string:
        unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int,
    mpv_get_property:
        unsafe extern "C" fn(*mut MpvHandle, *const c_char, c_int, *mut c_void) -> c_int,
    mpv_get_property_string: unsafe extern "C" fn(*mut MpvHandle, *const c_char) -> *mut c_char,
    mpv_free: unsafe extern "C" fn(*mut c_void),
    mpv_render_context_create: unsafe extern "C" fn(
        *mut *mut MpvRenderContext,
        *mut MpvHandle,
        *mut MpvRenderParam,
    ) -> c_int,
    mpv_render_context_set_update_callback: unsafe extern "C" fn(
        *mut MpvRenderContext,
        Option<unsafe extern "C" fn(*mut c_void)>,
        *mut c_void,
    ),
    mpv_render_context_update: unsafe extern "C" fn(*mut MpvRenderContext) -> u64,
    mpv_render_context_render:
        unsafe extern "C" fn(*mut MpvRenderContext, *mut MpvRenderParam) -> c_int,
    mpv_render_context_free: unsafe extern "C" fn(*mut MpvRenderContext),
}

// SAFETY: libmpv API is thread-safe for handle commands/properties; render_context is used only on the GL thread.
unsafe impl Send for MpvVtable {}
unsafe impl Sync for MpvVtable {}

impl MpvVtable {
    fn load(path: &Path) -> anyhow::Result<Self> {
        unsafe {
            let lib = Library::new(path)?;
            macro_rules! sym {
                ($name:ident, $ty:ty) => {{
                    let s: Symbol<$ty> = lib.get(concat!(stringify!($name), "\0").as_bytes())?;
                    *s
                }};
            }
            Ok(Self {
                mpv_create: sym!(mpv_create, unsafe extern "C" fn() -> *mut MpvHandle),
                mpv_initialize: sym!(mpv_initialize, unsafe extern "C" fn(*mut MpvHandle) -> c_int),
                mpv_terminate_destroy: sym!(
                    mpv_terminate_destroy,
                    unsafe extern "C" fn(*mut MpvHandle)
                ),
                mpv_wait_event: sym!(
                    mpv_wait_event,
                    unsafe extern "C" fn(*mut MpvHandle, c_double) -> *mut MpvEvent
                ),
                mpv_set_option_string: sym!(
                    mpv_set_option_string,
                    unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int
                ),
                mpv_command: sym!(
                    mpv_command,
                    unsafe extern "C" fn(*mut MpvHandle, *const *const c_char) -> c_int
                ),
                mpv_set_property: sym!(
                    mpv_set_property,
                    unsafe extern "C" fn(*mut MpvHandle, *const c_char, c_int, *mut c_void) -> c_int
                ),
                mpv_set_property_string: sym!(
                    mpv_set_property_string,
                    unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int
                ),
                mpv_get_property: sym!(
                    mpv_get_property,
                    unsafe extern "C" fn(*mut MpvHandle, *const c_char, c_int, *mut c_void) -> c_int
                ),
                mpv_get_property_string: sym!(
                    mpv_get_property_string,
                    unsafe extern "C" fn(*mut MpvHandle, *const c_char) -> *mut c_char
                ),
                mpv_free: sym!(mpv_free, unsafe extern "C" fn(*mut c_void)),
                mpv_render_context_create: sym!(
                    mpv_render_context_create,
                    unsafe extern "C" fn(
                        *mut *mut MpvRenderContext,
                        *mut MpvHandle,
                        *mut MpvRenderParam,
                    ) -> c_int
                ),
                mpv_render_context_set_update_callback: sym!(
                    mpv_render_context_set_update_callback,
                    unsafe extern "C" fn(
                        *mut MpvRenderContext,
                        Option<unsafe extern "C" fn(*mut c_void)>,
                        *mut c_void,
                    )
                ),
                mpv_render_context_update: sym!(
                    mpv_render_context_update,
                    unsafe extern "C" fn(*mut MpvRenderContext) -> u64
                ),
                mpv_render_context_render: sym!(
                    mpv_render_context_render,
                    unsafe extern "C" fn(*mut MpvRenderContext, *mut MpvRenderParam) -> c_int
                ),
                mpv_render_context_free: sym!(
                    mpv_render_context_free,
                    unsafe extern "C" fn(*mut MpvRenderContext)
                ),
                _lib: lib,
            })
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MpvVideoTrack {
    pub id: i64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub selected: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PlaybackState {
    pub time_pos: f64,
    pub duration: f64,
    pub paused: bool,
    pub buffering: bool,
    pub eof: bool,
    pub idle: bool,
    pub video_w: i64,
    pub video_h: i64,
    pub fps: f64,
    pub dropped_frames: i64,
    pub hwdec: String,
    pub video_codec: String,
    pub audio_codec: String,
    pub current_ao: String,
    pub cache_duration: f64,
    pub cache_speed_bps: u64,
}

struct GlSurface {
    fbo: glow::Framebuffer,
    tex: glow::Texture,
    w: i32,
    h: i32,
}

struct RenderContextHolder {
    vt: Arc<MpvVtable>,
    render_ctx: *mut MpvRenderContext,
    surface: Option<GlSurface>,
    amb_surface: Option<GlSurface>,
    amb_enabled: Arc<AtomicBool>,
    amb_last_sample: Option<Instant>,
    amb_smooth: Box<[[f32; 3]; AMB_GRID_W * AMB_GRID_H]>,
    amb_snap: Arc<Mutex<Option<LiveAmbientSnapshot>>>,
    _proc_box: *mut GlProcAddrFn,
    _wake_box: *mut egui::Context,
}

// SAFETY: RenderContextHolder is only touched from the UI/GL thread via PaintCallback & on_exit.
unsafe impl Send for RenderContextHolder {}
unsafe impl Sync for RenderContextHolder {}

#[derive(Clone)]
pub struct MpvRenderHandle(Arc<Mutex<Option<RenderContextHolder>>>);

impl MpvRenderHandle {
    pub fn paint_callback(&self, rect: egui::Rect) -> egui::PaintCallback {
        self.paint_callback_rounded(rect, 0.0, 0.0)
    }

    pub fn paint_callback_rounded(
        &self,
        rect: egui::Rect,
        top_radius: f32,
        bottom_radius: f32,
    ) -> egui::PaintCallback {
        let holder = self.0.clone();
        egui::PaintCallback {
            rect,
            callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |info, painter| {
                let Ok(mut guard) = holder.lock() else { return };
                let Some(ctx) = guard.as_mut() else { return };
                render_into_egui(ctx, info, painter, top_radius, bottom_radius);
            })),
        }
    }
}

pub struct MpvPlayer {
    vt: Arc<MpvVtable>,
    handle: *mut MpvHandle,
    render: Arc<Mutex<Option<RenderContextHolder>>>,
    amb_enabled: Arc<AtomicBool>,
    amb_snap: Arc<Mutex<Option<LiveAmbientSnapshot>>>,
    pub dll_path: PathBuf,
}

// SAFETY: handle calls are thread-safe in libmpv; render is behind a Mutex and used on the GL thread.
unsafe impl Send for MpvPlayer {}
unsafe impl Sync for MpvPlayer {}

unsafe extern "C" fn gl_get_proc_address(
    ctx: *mut c_void,
    name: *const c_char,
) -> *mut c_void {
    if ctx.is_null() || name.is_null() {
        return std::ptr::null_mut();
    }
    let fun = unsafe { &*(ctx as *const GlProcAddrFn) };
    let cname = unsafe { CStr::from_ptr(name) };
    fun(cname) as *mut c_void
}

unsafe extern "C" fn on_mpv_render_update(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    let egui_ctx = unsafe { &*(ctx as *const egui::Context) };
    egui_ctx.request_repaint();
}

pub fn find_libmpv(data_dir: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    let names = ["libmpv-2.dll", "mpv-2.dll", "mpv-1.dll"];
    #[cfg(target_os = "macos")]
    let names = ["libmpv.2.dylib", "libmpv.dylib"];
    #[cfg(all(unix, not(target_os = "macos")))]
    let names = ["libmpv.so.2", "libmpv.so"];

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for n in names {
                let p = dir.join(n);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    for n in names {
        let p = data_dir.join(n);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for n in names {
                let p = dir.join(n);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

impl MpvPlayer {
    pub fn new(
        data_dir: &Path,
        get_proc_address: GlProcAddrFn,
        egui_ctx: egui::Context,
    ) -> anyhow::Result<Self> {
        Self::new_inner(data_dir, get_proc_address, egui_ctx, false)
    }

    pub fn new_preview(
        data_dir: &Path,
        get_proc_address: GlProcAddrFn,
        egui_ctx: egui::Context,
    ) -> anyhow::Result<Self> {
        Self::new_inner(data_dir, get_proc_address, egui_ctx, true)
    }

    fn new_inner(
        data_dir: &Path,
        get_proc_address: GlProcAddrFn,
        egui_ctx: egui::Context,
        is_preview: bool,
    ) -> anyhow::Result<Self> {
        let dll_path = find_libmpv(data_dir)
            .ok_or_else(|| anyhow::anyhow!("libmpv-2.dll not found"))?;
        let vt = Arc::new(MpvVtable::load(&dll_path)?);

        unsafe {
            let handle = (vt.mpv_create)();
            anyhow::ensure!(!handle.is_null(), "mpv_create returned null");

            let set_opt = |k: &str, v: &str| {
                if let (Ok(ck), Ok(cv)) = (CString::new(k), CString::new(v)) {
                    (vt.mpv_set_option_string)(handle, ck.as_ptr(), cv.as_ptr());
                }
            };
            set_opt("vo", "libmpv");
            #[cfg(windows)]
            set_opt("ao", "wasapi,null");
            set_opt("audio-fallback-to-null", "yes");
            set_opt("secondary-sub-pos", "10");
            // Use auto-copy hardware decode for H.264/HEVC and multi-thread CPU decode for VP9/AV1.
            set_opt("hwdec", "auto-copy");
            set_opt("hwdec-codecs", "h264,hevc");
            // Disable FFmpeg direct-rendering and OpenGL PBO upload which can crash libmpv/drivers on codec or format changes.
            set_opt("vd-lavc-dr", "no");
            set_opt("opengl-pbo", "no");
            set_opt("vd-lavc-threads", "0");
            set_opt("ytdl", "no");
            set_opt("keep-open", "yes");
            set_opt("screenshot-format", "png");
            set_opt("cache", "yes");
            if is_preview {
                set_opt("demuxer-max-bytes", "20MiB");
                set_opt("demuxer-max-back-bytes", "4MiB");
                set_opt("demuxer-readahead-secs", "15");
                set_opt("cache-pause-wait", "0.5");
                set_opt("loop-file", "inf");
                set_opt("mute", "yes");
            } else {
                set_opt("demuxer-max-bytes", "72MiB");
                set_opt("demuxer-max-back-bytes", "24MiB");
                set_opt("demuxer-readahead-secs", "60");
                set_opt("cache-pause-wait", "1.5");
            }
            set_opt("network-timeout", "15");
            set_opt(
                "stream-lavf-o",
                "reconnect=1,reconnect_streamed=1,reconnect_delay_max=5",
            );
            set_opt("user-agent", crate::innertube::WEB_UA);
            set_opt("referrer", "https://www.youtube.com/");
            set_opt("terminal", "no");
            if !is_preview {
                if let Some(log_str) = data_dir.join("mpv.log").to_str() {
                    set_opt("log-file", log_str);
                }
                set_opt("msg-level", "all=info");
            } else {
                set_opt("msg-level", "all=warn");
            }

            let rc = (vt.mpv_initialize)(handle);
            if rc < 0 {
                (vt.mpv_terminate_destroy)(handle);
                anyhow::bail!("mpv_initialize failed with code {rc}");
            }

            let proc_box = Box::into_raw(Box::new(get_proc_address));
            let wake_box = Box::into_raw(Box::new(egui_ctx));

            let api_type = CString::new("opengl").unwrap();
            let mut init_params = MpvOpenglInitParams {
                get_proc_address: gl_get_proc_address,
                get_proc_address_ctx: proc_box as *mut c_void,
            };
            let mut params = [
                MpvRenderParam {
                    type_: MPV_RENDER_PARAM_API_TYPE,
                    data: api_type.as_ptr() as *mut c_void,
                },
                MpvRenderParam {
                    type_: MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                    data: (&mut init_params as *mut MpvOpenglInitParams) as *mut c_void,
                },
                MpvRenderParam {
                    type_: MPV_RENDER_PARAM_INVALID,
                    data: std::ptr::null_mut(),
                },
            ];

            let mut render_ctx: *mut MpvRenderContext = std::ptr::null_mut();
            let rrc = (vt.mpv_render_context_create)(
                &mut render_ctx,
                handle,
                params.as_mut_ptr(),
            );
            if rrc < 0 || render_ctx.is_null() {
                drop(Box::from_raw(proc_box));
                drop(Box::from_raw(wake_box));
                (vt.mpv_terminate_destroy)(handle);
                anyhow::bail!("mpv_render_context_create failed with code {rrc}");
            }

            (vt.mpv_render_context_set_update_callback)(
                render_ctx,
                Some(on_mpv_render_update),
                wake_box as *mut c_void,
            );

            let amb_enabled = Arc::new(AtomicBool::new(false));
            let amb_snap = Arc::new(Mutex::new(None));
            Ok(Self {
                vt: vt.clone(),
                handle,
                render: Arc::new(Mutex::new(Some(RenderContextHolder {
                    vt,
                    render_ctx,
                    surface: None,
                    amb_surface: None,
                    amb_enabled: amb_enabled.clone(),
                    amb_last_sample: None,
                    amb_smooth: Box::new([[0.0; 3]; AMB_GRID_W * AMB_GRID_H]),
                    amb_snap: amb_snap.clone(),
                    _proc_box: proc_box,
                    _wake_box: wake_box,
                }))),
                amb_enabled,
                amb_snap,
                dll_path,
            })
        }
    }

    pub fn load_stream(&self, video_url: &str, audio_url: Option<&str>, start_at: Option<f64>) {
        // Tear down the active decoder cleanly before switching codec/format (e.g. H.264 -> VP9)
        self.command(&["stop"]);
        // Clear previous external audio and subtitles before loading
        self.set_property_str("audio-files", "");
        self.set_property_str("sub-files", "");
        self.set_property_str("sub-visibility", "no");
        self.set_property_str("vid", "auto");
        if let Some(start) = start_at {
            self.set_property_str("start", &format!("{:.2}", start.max(0.0)));
        } else {
            self.set_property_str("start", "none");
        }
        if let Some(audio) = audio_url {
            self.set_property_str("audio-files", audio);
        }
        self.command(&["loadfile", video_url, "replace"]);
        self.set_paused(false);
    }

    pub fn set_hls_bitrate(&self, max_bps: Option<u64>) {
        match max_bps {
            Some(bps) if bps > 0 => self.set_property_str("hls-bitrate", &bps.to_string()),
            _ => self.set_property_str("hls-bitrate", "max"),
        }
    }

    pub fn set_live_stream_tuning(&self, is_live: bool) {
        if is_live {
            self.set_property_str("demuxer-readahead-secs", "10");
            self.set_property_str("cache-pause-wait", "0.35");
            self.set_property_str("demuxer-lavf-o", "live_start_index=-3");
        } else {
            self.set_property_str("demuxer-readahead-secs", "60");
            self.set_property_str("cache-pause-wait", "1.5");
            self.set_property_str("demuxer-lavf-o", "");
        }
    }

    pub fn set_vid(&self, id: Option<i64>) {
        match id {
            Some(vid) => self.set_property_str("vid", &vid.to_string()),
            None => self.set_property_str("vid", "auto"),
        }
    }

    pub fn set_video_enabled(&self, enabled: bool) {
        self.set_property_str("vid", if enabled { "auto" } else { "no" });
    }

    pub fn reload_audio_device(&self, device: &str) {
        let dev = if device.trim().is_empty() {
            "auto"
        } else {
            device.trim()
        };
        self.set_property_str("audio-device", dev);
        self.command(&["ao-reload"]);
    }

    pub fn audio_devices(&self) -> Vec<(String, String)> {
        let Some(raw) = self.get_string("audio-device-list") else {
            return Vec::new();
        };
        let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&raw) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for item in arr {
            let Some(name) = item["name"].as_str() else {
                continue;
            };
            let desc = item["description"].as_str().unwrap_or(name).to_owned();
            if !name.is_empty() {
                out.push((name.to_owned(), desc));
            }
        }
        out
    }

    pub fn set_audio_filters(&self, loudnorm: bool, eq: crate::model::EqPreset) {
        self.set_property_str("af", &build_mpv_af_chain(loudnorm, eq));
    }

    pub fn video_tracks(&self) -> Vec<MpvVideoTrack> {
        let count = self.get_double("track-list/count").unwrap_or(0.0) as usize;
        let mut out = Vec::new();
        for i in 0..count.min(64) {
            let Some(ty) = self.get_string(&format!("track-list/{i}/type")) else {
                continue;
            };
            if ty != "video" {
                continue;
            }
            let Some(id) = self.get_double(&format!("track-list/{i}/id")) else {
                continue;
            };
            let width = self
                .get_double(&format!("track-list/{i}/demux-w"))
                .unwrap_or(0.0) as u32;
            let height = self
                .get_double(&format!("track-list/{i}/demux-h"))
                .unwrap_or(0.0) as u32;
            let fps = self
                .get_double(&format!("track-list/{i}/demux-fps"))
                .unwrap_or(0.0);
            let selected = self
                .get_flag(&format!("track-list/{i}/selected"))
                .unwrap_or(false);
            out.push(MpvVideoTrack {
                id: id as i64,
                width,
                height,
                fps,
                selected,
            });
        }
        out
    }

    pub fn set_dual_subtitles(&self, primary_url: Option<&str>, secondary_url: Option<&str>) {
        let sub_ids = self.sub_track_ids();
        for id in sub_ids.into_iter().rev() {
            self.command(&["sub-remove", &id.to_string()]);
        }
        self.set_property_str("sub-files", "");
        match primary_url {
            Some(url) => {
                self.set_property_str("sub-visibility", "yes");
                self.command(&["sub-add", url, "select"]);
            }
            None => {
                self.set_property_str("sub-visibility", "no");
                self.set_property_str("sid", "no");
            }
        }
        match secondary_url {
            Some(url) => {
                let primary_id = self.get_double("sid").map(|v| v as i64);
                self.command(&["sub-add", url, "auto"]);
                let sec_id = self
                    .sub_track_ids()
                    .into_iter()
                    .rev()
                    .find(|id| Some(*id) != primary_id);
                if let Some(id) = sec_id {
                    self.set_property_str("secondary-sid", &id.to_string());
                    self.set_property_str("secondary-sub-visibility", "yes");
                }
            }
            None => {
                self.set_property_str("secondary-sid", "no");
                self.set_property_str("secondary-sub-visibility", "no");
            }
        }
    }

    fn sub_track_ids(&self) -> Vec<i64> {
        let count = self.get_double("track-list/count").unwrap_or(0.0) as usize;
        let mut ids = Vec::new();
        for i in 0..count.min(64) {
            if self.get_string(&format!("track-list/{i}/type")).as_deref() == Some("sub") {
                if let Some(id) = self.get_double(&format!("track-list/{i}/id")) {
                    ids.push(id as i64);
                }
            }
        }
        ids
    }

    pub fn set_subtitle_style(&self, font_size: u32, bg_opacity: f32) {
        self.set_dual_subtitle_style(
            font_size,
            bg_opacity,
            crate::model::SubtitleColorPreset::White,
            font_size.saturating_sub(6).max(20),
            bg_opacity,
            crate::model::SubtitleColorPreset::Yellow,
        );
    }

    pub fn set_dual_subtitle_style(
        &self,
        font_size: u32,
        bg_opacity: f32,
        color: crate::model::SubtitleColorPreset,
        sec_font_size: u32,
        sec_bg_opacity: f32,
        sec_color: crate::model::SubtitleColorPreset,
    ) {
        let size = font_size.clamp(18, 96);
        self.set_property_str("sub-font-size", &size.to_string());
        self.set_property_str("sub-color", color.mpv_hex());
        self.set_property_str("sub-back-color", &format_mpv_sub_back_color(bg_opacity));
        self.set_property_str(
            "sub-border-size",
            if bg_opacity > 0.05 { "1.5" } else { "2.8" },
        );

        let sec_size = sec_font_size.clamp(18, 96);
        self.set_property_str("secondary-sub-pos", "10");
        self.set_property_str("secondary-sub-font-size", &sec_size.to_string());
        self.set_property_str("secondary-sub-color", sec_color.mpv_hex());
        self.set_property_str(
            "secondary-sub-back-color",
            &format_mpv_sub_back_color(sec_bg_opacity),
        );
        self.set_property_str(
            "secondary-sub-border-size",
            if sec_bg_opacity > 0.05 { "1.5" } else { "2.8" },
        );
    }

    pub fn frame_step(&self) {
        self.command(&["frame-step"]);
    }

    pub fn frame_back_step(&self) {
        self.command(&["frame-back-step"]);
    }

    pub fn set_ab_loop(&self, a: Option<f64>, b: Option<f64>) {
        let a_str = a.map_or_else(|| "no".to_owned(), |v| format!("{:.3}", v.max(0.0)));
        let b_str = b.map_or_else(|| "no".to_owned(), |v| format!("{:.3}", v.max(0.0)));
        self.set_property_str("ab-loop-a", &a_str);
        self.set_property_str("ab-loop-b", &b_str);
    }

    pub fn stop(&self) {
        self.command(&["stop"]);
    }

    pub fn set_paused(&self, paused: bool) {
        let mut flag: c_int = if paused { 1 } else { 0 };
        self.set_property("pause", MPV_FORMAT_FLAG, (&mut flag as *mut c_int) as *mut c_void);
    }

    pub fn toggle_pause(&self) {
        self.command(&["cycle", "pause"]);
    }

    pub fn seek_abs(&self, secs: f64) {
        let s = format!("{:.3}", secs.max(0.0));
        self.command(&["seek", &s, "absolute"]);
    }

    pub fn seek_rel(&self, delta: f64) {
        let s = format!("{delta:+.3}");
        self.command(&["seek", &s, "relative"]);
    }

    pub fn set_volume(&self, volume_0_100: f64) {
        let mut val: c_double = volume_0_100.clamp(0.0, 150.0);
        self.set_property(
            "volume",
            MPV_FORMAT_DOUBLE,
            (&mut val as *mut c_double) as *mut c_void,
        );
    }

    pub fn set_mute(&self, muted: bool) {
        let mut flag: c_int = if muted { 1 } else { 0 };
        self.set_property("mute", MPV_FORMAT_FLAG, (&mut flag as *mut c_int) as *mut c_void);
    }

    pub fn set_speed(&self, speed: f64) {
        let mut val: c_double = speed.clamp(0.25, 4.0);
        self.set_property(
            "speed",
            MPV_FORMAT_DOUBLE,
            (&mut val as *mut c_double) as *mut c_void,
        );
    }

    pub fn set_loop_file(&self, looped: bool) {
        self.set_property_str("loop-file", if looped { "inf" } else { "no" });
    }

    pub fn set_hwdec(&self, mode: crate::model::HwdecMode) {
        self.set_property_str("hwdec", mode.mpv_value());
    }

    pub fn set_cache_pause_wait(&self, secs: f64) {
        self.set_property_str("cache-pause-wait", &format!("{secs:.2}"));
    }

    /// Capture the current clean video frame as PNG bytes via libmpv `screenshot-to-file`.
    pub fn capture_frame_png(&self, temp_dir: &Path) -> anyhow::Result<Vec<u8>> {
        std::fs::create_dir_all(temp_dir)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let tmp_path = temp_dir.join(format!("yt_rust_shot_{stamp}.png"));
        let path_str = tmp_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid screenshot temp path"))?;
        self.set_property_str("screenshot-format", "png");
        self.command(&["screenshot-to-file", path_str, "video"]);

        let mut bytes = Vec::new();
        for _ in 0..20 {
            if let Ok(b) = std::fs::read(&tmp_path) {
                if !b.is_empty() {
                    bytes = b;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = std::fs::remove_file(&tmp_path);
        anyhow::ensure!(!bytes.is_empty(), "No video frame available to capture");
        Ok(bytes)
    }

    pub fn poll_state(&self) -> PlaybackState {
        // Drain libmpv's event queue so internal buffers never stall
        unsafe {
            for _ in 0..32 {
                let ev = (self.vt.mpv_wait_event)(self.handle, 0.0);
                if ev.is_null() || (*ev).event_id == 0 {
                    break;
                }
            }
        }
        let mut st = PlaybackState::default();
        st.time_pos = self.get_double("time-pos").unwrap_or(0.0);
        st.duration = self.get_double("duration").unwrap_or(0.0);
        st.paused = self.get_flag("pause").unwrap_or(false);
        st.buffering = self.get_flag("paused-for-cache").unwrap_or(false);
        st.eof = self.get_flag("eof-reached").unwrap_or(false);
        st.idle = self.get_flag("idle-active").unwrap_or(false);
        st.video_w = self.get_double("video-params/w").unwrap_or(0.0) as i64;
        st.video_h = self.get_double("video-params/h").unwrap_or(0.0) as i64;
        st.fps = self
            .get_double("estimated-vf-fps")
            .or_else(|| self.get_double("container-fps"))
            .unwrap_or(0.0);
        st.dropped_frames = self.get_double("frame-drop-count").unwrap_or(0.0) as i64;
        st.hwdec = self.get_string("hwdec-current").unwrap_or_default();
        st.video_codec = self.get_string("video-codec").unwrap_or_default();
        st.audio_codec = self.get_string("audio-codec-name").unwrap_or_default();
        st.current_ao = self.get_string("current-ao").unwrap_or_default();
        st.cache_duration = self
            .get_double("demuxer-cache-duration")
            .filter(|d| d.is_finite() && *d > 0.0)
            .or_else(|| {
                self.get_double("demuxer-cache-time")
                    .filter(|t| t.is_finite())
                    .map(|t| (t - st.time_pos).max(0.0))
            })
            .unwrap_or(0.0);
        st.cache_speed_bps = self
            .get_double("cache-speed")
            .filter(|s| s.is_finite() && *s > 0.0)
            .map(|bytes_per_sec| (bytes_per_sec * 8.0) as u64)
            .unwrap_or(0);
        st
    }

    pub fn render_handle(&self) -> MpvRenderHandle {
        MpvRenderHandle(self.render.clone())
    }

    pub fn paint_callback(&self, rect: egui::Rect) -> egui::PaintCallback {
        self.render_handle().paint_callback(rect)
    }

    pub fn paint_callback_rounded(
        &self,
        rect: egui::Rect,
        top_radius: f32,
        bottom_radius: f32,
    ) -> egui::PaintCallback {
        self.render_handle()
            .paint_callback_rounded(rect, top_radius, bottom_radius)
    }

    pub fn set_ambient_capture(&self, enabled: bool) {
        let prev = self.amb_enabled.swap(enabled, Ordering::Relaxed);
        if !enabled && prev {
            if let Ok(mut g) = self.amb_snap.lock() {
                *g = None;
            }
        }
    }

    pub fn live_ambient_snapshot(&self) -> Option<LiveAmbientSnapshot> {
        self.amb_snap.lock().ok().and_then(|g| g.clone())
    }

    pub fn destroy_gl(&self, gl: Option<&glow::Context>) {
        if let Ok(mut guard) = self.render.lock() {
            if let Some(mut ctx) = guard.take() {
                unsafe {
                    if let (Some(gl), Some(surf)) = (gl, ctx.surface.take()) {
                        gl.delete_framebuffer(surf.fbo);
                        gl.delete_texture(surf.tex);
                    }
                    if let (Some(gl), Some(amb)) = (gl, ctx.amb_surface.take()) {
                        gl.delete_framebuffer(amb.fbo);
                        gl.delete_texture(amb.tex);
                    }
                    (ctx.vt.mpv_render_context_set_update_callback)(
                        ctx.render_ctx,
                        None,
                        std::ptr::null_mut(),
                    );
                    (ctx.vt.mpv_render_context_free)(ctx.render_ctx);
                    drop(Box::from_raw(ctx._proc_box));
                    drop(Box::from_raw(ctx._wake_box));
                }
            }
        }
    }

    fn command(&self, args: &[&str]) {
        let cstrings: Vec<CString> = args
            .iter()
            .filter_map(|s| CString::new(*s).ok())
            .collect();
        if cstrings.len() != args.len() {
            return;
        }
        let mut ptrs: Vec<*const c_char> = cstrings.iter().map(|s| s.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        unsafe {
            (self.vt.mpv_command)(self.handle, ptrs.as_ptr());
        }
    }

    fn set_property_str(&self, name: &str, value: &str) {
        if let (Ok(cn), Ok(cv)) = (CString::new(name), CString::new(value)) {
            unsafe {
                (self.vt.mpv_set_property_string)(self.handle, cn.as_ptr(), cv.as_ptr());
            }
        }
    }

    fn set_property(&self, name: &str, format: c_int, data: *mut c_void) {
        if let Ok(cn) = CString::new(name) {
            unsafe {
                (self.vt.mpv_set_property)(self.handle, cn.as_ptr(), format, data);
            }
        }
    }

    fn get_double(&self, name: &str) -> Option<f64> {
        let cn = CString::new(name).ok()?;
        let mut out: c_double = 0.0;
        let rc = unsafe {
            (self.vt.mpv_get_property)(
                self.handle,
                cn.as_ptr(),
                MPV_FORMAT_DOUBLE,
                (&mut out as *mut c_double) as *mut c_void,
            )
        };
        (rc >= 0).then_some(out)
    }

    fn get_flag(&self, name: &str) -> Option<bool> {
        let cn = CString::new(name).ok()?;
        let mut out: c_int = 0;
        let rc = unsafe {
            (self.vt.mpv_get_property)(
                self.handle,
                cn.as_ptr(),
                MPV_FORMAT_FLAG,
                (&mut out as *mut c_int) as *mut c_void,
            )
        };
        (rc >= 0).then_some(out != 0)
    }

    fn get_string(&self, name: &str) -> Option<String> {
        let cn = CString::new(name).ok()?;
        let ptr = unsafe { (self.vt.mpv_get_property_string)(self.handle, cn.as_ptr()) };
        if ptr.is_null() {
            return None;
        }
        let s = unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned();
        unsafe { (self.vt.mpv_free)(ptr as *mut c_void) };
        Some(s)
    }
}

impl Drop for MpvPlayer {
    fn drop(&mut self) {
        self.destroy_gl(None);
        unsafe {
            (self.vt.mpv_terminate_destroy)(self.handle);
        }
    }
}

fn render_into_egui(
    ctx: &mut RenderContextHolder,
    info: egui::PaintCallbackInfo,
    painter: &eframe::egui_glow::Painter,
    top_radius: f32,
    bottom_radius: f32,
) {
    let gl = painter.gl();
    let vp = info.viewport_in_pixels();
    let w = vp.width_px.max(1);
    let h = vp.height_px.max(1);

    unsafe {
        // Save currently bound draw/read FBOs (egui may draw to default 0 or an offscreen FBO)
        let prev_draw_fbo = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING) as u32;
        let prev_read_fbo = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING) as u32;

        // Ensure offscreen FBO + texture match viewport pixel size
        let needs_alloc = match &ctx.surface {
            Some(s) => s.w != w || s.h != h,
            None => true,
        };
        if needs_alloc {
            if let Some(old) = ctx.surface.take() {
                gl.delete_framebuffer(old.fbo);
                gl.delete_texture(old.tex);
            }
            if let (Ok(fbo), Ok(tex)) = (gl.create_framebuffer(), gl.create_texture()) {
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    w,
                    h,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MIN_FILTER,
                    glow::LINEAR as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MAG_FILTER,
                    glow::LINEAR as i32,
                );
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(tex),
                    0,
                );
                gl.bind_texture(glow::TEXTURE_2D, None);
                ctx.surface = Some(GlSurface { fbo, tex, w, h });
            }
        }

        let Some(surf) = &ctx.surface else {
            let draw_target = std::num::NonZeroU32::new(prev_draw_fbo).map(glow::NativeFramebuffer);
            let read_target = std::num::NonZeroU32::new(prev_read_fbo).map(glow::NativeFramebuffer);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, draw_target);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, read_target);
            return;
        };

        let flags = (ctx.vt.mpv_render_context_update)(ctx.render_ctx);
        let has_new_frame = (flags & MPV_RENDER_UPDATE_FRAME) != 0;

        if has_new_frame || needs_alloc {
            // Disable egui's scissor test while mpv renders into the offscreen FBO
            gl.disable(glow::SCISSOR_TEST);

            let raw_fbo: u32 = surf.fbo.0.into();
            let mut fbo_param = MpvOpenglFbo {
                fbo: raw_fbo as c_int,
                w,
                h,
                internal_format: glow::RGBA8 as c_int,
            };
            let mut flip_y: c_int = 1;
            let mut params = [
                MpvRenderParam {
                    type_: MPV_RENDER_PARAM_OPENGL_FBO,
                    data: (&mut fbo_param as *mut MpvOpenglFbo) as *mut c_void,
                },
                MpvRenderParam {
                    type_: MPV_RENDER_PARAM_FLIP_Y,
                    data: (&mut flip_y as *mut c_int) as *mut c_void,
                },
                MpvRenderParam {
                    type_: MPV_RENDER_PARAM_INVALID,
                    data: std::ptr::null_mut(),
                },
            ];
            (ctx.vt.mpv_render_context_render)(ctx.render_ctx, params.as_mut_ptr());
        }

        // Hardware-downsample the rendered video frame into a tiny 16x10 FBO for Video Ambilight.
        if ctx.amb_enabled.load(Ordering::Relaxed)
            && ctx
                .amb_last_sample
                .is_none_or(|t| t.elapsed() >= Duration::from_millis(30))
        {
            if ctx.amb_surface.is_none() {
                if let (Ok(fbo), Ok(tex)) = (gl.create_framebuffer(), gl.create_texture()) {
                    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        glow::RGBA8 as i32,
                        AMB_GRID_W as i32,
                        AMB_GRID_H as i32,
                        0,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(None),
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MIN_FILTER,
                        glow::LINEAR as i32,
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MAG_FILTER,
                        glow::LINEAR as i32,
                    );
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                    gl.framebuffer_texture_2d(
                        glow::FRAMEBUFFER,
                        glow::COLOR_ATTACHMENT0,
                        glow::TEXTURE_2D,
                        Some(tex),
                        0,
                    );
                    gl.bind_texture(glow::TEXTURE_2D, None);
                    ctx.amb_surface = Some(GlSurface {
                        fbo,
                        tex,
                        w: AMB_GRID_W as i32,
                        h: AMB_GRID_H as i32,
                    });
                }
            }
            if let Some(amb) = &ctx.amb_surface {
                gl.disable(glow::SCISSOR_TEST);
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(surf.fbo));
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(amb.fbo));
                gl.blit_framebuffer(
                    0,
                    0,
                    w,
                    h,
                    0,
                    0,
                    AMB_GRID_W as i32,
                    AMB_GRID_H as i32,
                    glow::COLOR_BUFFER_BIT,
                    glow::LINEAR,
                );
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(amb.fbo));
                let mut raw = [0u8; AMB_GRID_W * AMB_GRID_H * 4];
                gl.read_pixels(
                    0,
                    0,
                    AMB_GRID_W as i32,
                    AMB_GRID_H as i32,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut raw)),
                );
                let first = ctx.amb_last_sample.is_none();
                let mix = if first { 1.0 } else { 0.24 };
                let snap = update_smooth_amb_grid(&mut ctx.amb_smooth, &raw, mix);
                if let Ok(mut g) = ctx.amb_snap.lock() {
                    *g = Some(snap);
                }
                ctx.amb_last_sample = Some(Instant::now());
            }
        }

        // Restore egui's exact clip scissor (mpv_render_context_render alters glScissor!)
        // and blit the rendered offscreen texture into the widget's viewport rect.
        let clip_min_x = (info.pixels_per_point * info.clip_rect.min.x).round() as i32;
        let clip_min_y = (info.pixels_per_point * info.clip_rect.min.y).round() as i32;
        let clip_max_x = (info.pixels_per_point * info.clip_rect.max.x).round() as i32;
        let clip_max_y = (info.pixels_per_point * info.clip_rect.max.y).round() as i32;
        let scr_w = info.screen_size_px[0] as i32;
        let scr_h = info.screen_size_px[1] as i32;
        let clip_x = clip_min_x.clamp(0, scr_w);
        let clip_y = (scr_h - clip_max_y).clamp(0, scr_h);
        let clip_w = (clip_max_x.clamp(0, scr_w) - clip_x).max(0);
        let clip_h = ((scr_h - clip_min_y).clamp(0, scr_h) - clip_y).max(0);

        gl.enable(glow::SCISSOR_TEST);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(surf.fbo));
        let draw_target = std::num::NonZeroU32::new(prev_draw_fbo).map(glow::NativeFramebuffer);
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, draw_target);

        let max_r = (vp.width_px.min(vp.height_px) / 2).max(0);
        let top_r = ((top_radius.max(0.0) * info.pixels_per_point).round() as i32).clamp(0, max_r);
        let bot_r =
            ((bottom_radius.max(0.0) * info.pixels_per_point).round() as i32).clamp(0, max_r);

        let blit_with_scissor = |sx: i32, sy: i32, sw: i32, sh: i32| {
            let x0 = sx.max(clip_x);
            let y0 = sy.max(clip_y);
            let x1 = (sx + sw).min(clip_x + clip_w);
            let y1 = (sy + sh).min(clip_y + clip_h);
            if x1 > x0 && y1 > y0 {
                gl.scissor(x0, y0, x1 - x0, y1 - y0);
                gl.blit_framebuffer(
                    0,
                    0,
                    w,
                    h,
                    vp.left_px,
                    vp.from_bottom_px,
                    vp.left_px + vp.width_px,
                    vp.from_bottom_px + vp.height_px,
                    glow::COLOR_BUFFER_BIT,
                    glow::LINEAR,
                );
            }
        };

        if top_r == 0 && bot_r == 0 {
            blit_with_scissor(clip_x, clip_y, clip_w, clip_h);
        } else {
            let mid_y = vp.from_bottom_px + bot_r;
            let mid_h = (vp.height_px - top_r - bot_r).max(0);
            if mid_h > 0 {
                blit_with_scissor(vp.left_px, mid_y, vp.width_px, mid_h);
            }
            for dy in 0..bot_r {
                let d = bot_r as f32 - (dy as f32 + 0.5);
                let span = ((bot_r as f32).powi(2) - d * d).max(0.0).sqrt();
                let inset = ((bot_r as f32) - span).ceil() as i32;
                let row_w = (vp.width_px - inset * 2).max(0);
                if row_w > 0 {
                    blit_with_scissor(vp.left_px + inset, vp.from_bottom_px + dy, row_w, 1);
                }
            }
            for dy in 0..top_r {
                let d = top_r as f32 - (dy as f32 + 0.5);
                let span = ((top_r as f32).powi(2) - d * d).max(0.0).sqrt();
                let inset = ((top_r as f32) - span).ceil() as i32;
                let row_w = (vp.width_px - inset * 2).max(0);
                if row_w > 0 {
                    let y = vp.from_bottom_px + vp.height_px - 1 - dy;
                    blit_with_scissor(vp.left_px + inset, y, row_w, 1);
                }
            }
            gl.scissor(clip_x, clip_y, clip_w, clip_h);
        }
        let read_target = std::num::NonZeroU32::new(prev_read_fbo).map(glow::NativeFramebuffer);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, read_target);
    }
}

/// Auto-downloads `libmpv-2.dll` (on Windows x64) from shinchiro/mpv-winbuild-cmake into `data_dir`.
pub async fn download_libmpv_windows(
    http: &reqwest::Client,
    data_dir: &Path,
) -> anyhow::Result<PathBuf> {
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = (http, data_dir);
        anyhow::bail!("Auto-download is only implemented for Windows x86_64");
    }
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        let release: serde_json::Value = http
            .get("https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/latest")
            .header("User-Agent", "yt-rust")
            .timeout(Duration::from_secs(20))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let asset = release["assets"]
            .as_array()
            .and_then(|arr| {
                arr.iter().find(|a| {
                    a["name"].as_str().is_some_and(|n| {
                        n.starts_with("mpv-dev-x86_64-")
                            && !n.contains("v3")
                            && n.ends_with(".7z")
                    })
                })
            })
            .ok_or_else(|| anyhow::anyhow!("Could not find mpv-dev-x86_64 .7z asset in latest release"))?;

        let url = asset["browser_download_url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing download URL"))?
            .to_owned();

        let bytes = http
            .get(url)
            .timeout(Duration::from_secs(180))
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        std::fs::create_dir_all(data_dir)?;
        let out_dll = data_dir.join("libmpv-2.dll");
        let tmp_dll = data_dir.join("libmpv-2.dll.part");

        let dest_clone = tmp_dll.clone();
        let found = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
            let mut cursor = std::io::Cursor::new(&bytes[..]);
            let mut extracted = false;
            sevenz_rust::decompress_with_extract_fn(&mut cursor, ".", |entry, reader, _| {
                let name = entry.name().replace('\\', "/");
                if !entry.is_directory() && (name.ends_with("/libmpv-2.dll") || name == "libmpv-2.dll") {
                    let mut out = std::fs::File::create(&dest_clone)?;
                    std::io::copy(reader, &mut out)?;
                    extracted = true;
                    return Ok(false);
                }
                Ok(true)
            })?;
            Ok(extracted)
        })
        .await??;

        anyhow::ensure!(found, "libmpv-2.dll was not found inside the downloaded archive");
        std::fs::rename(&tmp_dll, &out_dll)?;
        Ok(out_dll)
    }
}

pub fn format_mpv_sub_back_color(opacity: f32) -> String {
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{alpha:02X}000000")
}

/// Flips bottom-up OpenGL RGBA pixels to top-down `AMB_GRID_W x AMB_GRID_H`, gently boosts
/// saturation, temporally blends with `smooth`, and computes 4 quadrant tint colors.
pub fn update_smooth_amb_grid(
    smooth: &mut [[f32; 3]; AMB_GRID_W * AMB_GRID_H],
    rgba_bottom_up: &[u8; AMB_GRID_W * AMB_GRID_H * 4],
    mix: f32,
) -> LiveAmbientSnapshot {
    let mix = mix.clamp(0.0, 1.0);
    let mut grid = [[0u8; 3]; AMB_GRID_W * AMB_GRID_H];
    let mut q_sums = [[0.0_f32; 3]; 4];
    let mut q_counts = [0.0_f32; 4];

    for y in 0..AMB_GRID_H {
        let src_y = AMB_GRID_H - 1 - y;
        for x in 0..AMB_GRID_W {
            let src_i = (src_y * AMB_GRID_W + x) * 4;
            let dst_i = y * AMB_GRID_W + x;

            let r = rgba_bottom_up[src_i] as f32;
            let g = rgba_bottom_up[src_i + 1] as f32;
            let b = rgba_bottom_up[src_i + 2] as f32;
            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let sat = 1.24_f32;
            let br = (luma + (r - luma) * sat).clamp(8.0, 255.0);
            let bg = (luma + (g - luma) * sat).clamp(8.0, 255.0);
            let bb = (luma + (b - luma) * sat).clamp(8.0, 255.0);

            smooth[dst_i][0] += (br - smooth[dst_i][0]) * mix;
            smooth[dst_i][1] += (bg - smooth[dst_i][1]) * mix;
            smooth[dst_i][2] += (bb - smooth[dst_i][2]) * mix;

            grid[dst_i] = [
                smooth[dst_i][0].round().clamp(0.0, 255.0) as u8,
                smooth[dst_i][1].round().clamp(0.0, 255.0) as u8,
                smooth[dst_i][2].round().clamp(0.0, 255.0) as u8,
            ];

            let qi = (if y >= AMB_GRID_H / 2 { 2 } else { 0 })
                + if x >= AMB_GRID_W / 2 { 1 } else { 0 };
            q_sums[qi][0] += smooth[dst_i][0];
            q_sums[qi][1] += smooth[dst_i][1];
            q_sums[qi][2] += smooth[dst_i][2];
            q_counts[qi] += 1.0;
        }
    }

    let mut quadrants = [[0u8; 3]; 4];
    for i in 0..4 {
        let c = q_counts[i].max(1.0);
        quadrants[i] = [
            (q_sums[i][0] / c).round().clamp(0.0, 255.0) as u8,
            (q_sums[i][1] / c).round().clamp(0.0, 255.0) as u8,
            (q_sums[i][2] / c).round().clamp(0.0, 255.0) as u8,
        ];
    }

    LiveAmbientSnapshot { grid, quadrants }
}

pub fn build_mpv_af_chain(loudnorm: bool, eq: crate::model::EqPreset) -> String {
    use crate::model::EqPreset;
    let mut parts: Vec<&'static str> = Vec::new();
    match eq {
        EqPreset::Flat => {}
        EqPreset::VoiceClarity => {
            parts.push("highpass=f=95");
            parts.push("equalizer=f=250:t=q:w=1.0:g=-2");
            parts.push("equalizer=f=2200:t=q:w=1.1:g=4.5");
            parts.push("equalizer=f=4500:t=q:w=1.2:g=3");
        }
        EqPreset::BassBoost => {
            parts.push("equalizer=f=60:t=q:w=1.0:g=5.5");
            parts.push("equalizer=f=130:t=q:w=1.1:g=4");
            parts.push("equalizer=f=250:t=q:w=1.2:g=1.5");
        }
        EqPreset::Music => {
            parts.push("equalizer=f=65:t=q:w=1.1:g=3.5");
            parts.push("equalizer=f=250:t=q:w=1.1:g=1");
            parts.push("equalizer=f=1000:t=q:w=1.0:g=-1");
            parts.push("equalizer=f=4000:t=q:w=1.2:g=2.5");
            parts.push("equalizer=f=10000:t=q:w=1.1:g=3.5");
        }
    }
    if loudnorm {
        parts.push("loudnorm=I=-16:TP=-1.5:LRA=11");
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("lavfi=[{}]", parts.join(","))
    }
}

#[cfg(test)]
mod tests {
    use crate::model::EqPreset;

    use super::{
        AMB_GRID_H, AMB_GRID_W, build_mpv_af_chain, format_mpv_sub_back_color,
        update_smooth_amb_grid,
    };

    #[test]
    fn formats_mpv_sub_back_color_hex() {
        assert_eq!(format_mpv_sub_back_color(0.0), "#00000000");
        assert_eq!(format_mpv_sub_back_color(1.0), "#FF000000");
        assert_eq!(format_mpv_sub_back_color(0.5), "#80000000");
        assert_eq!(format_mpv_sub_back_color(1.5), "#FF000000");
    }

    #[test]
    fn builds_audio_filter_chains() {
        assert_eq!(build_mpv_af_chain(false, EqPreset::Flat), "");
        assert_eq!(
            build_mpv_af_chain(true, EqPreset::Flat),
            "lavfi=[loudnorm=I=-16:TP=-1.5:LRA=11]"
        );
        let voice = build_mpv_af_chain(true, EqPreset::VoiceClarity);
        assert!(voice.starts_with("lavfi=[highpass=f=95,"));
        assert!(voice.ends_with(",loudnorm=I=-16:TP=-1.5:LRA=11]"));
        assert!(build_mpv_af_chain(false, EqPreset::BassBoost).contains("equalizer=f=60"));
        assert!(build_mpv_af_chain(false, EqPreset::Music).contains("equalizer=f=10000"));
    }

    #[test]
    fn flips_and_smooths_live_ambient_grid() {
        let mut raw = [0u8; AMB_GRID_W * AMB_GRID_H * 4];
        // Bottom GL row (y = 0 in GL -> last row after flip) painted red, top GL row painted blue
        for x in 0..AMB_GRID_W {
            let bot_idx = x * 4;
            raw[bot_idx] = 220;
            raw[bot_idx + 1] = 20;
            raw[bot_idx + 2] = 20;
            raw[bot_idx + 3] = 255;

            let top_idx = ((AMB_GRID_H - 1) * AMB_GRID_W + x) * 4;
            raw[top_idx] = 20;
            raw[top_idx + 1] = 40;
            raw[top_idx + 2] = 230;
            raw[top_idx + 3] = 255;
        }
        let mut smooth = [[0.0_f32; 3]; AMB_GRID_W * AMB_GRID_H];
        let snap = update_smooth_amb_grid(&mut smooth, &raw, 1.0);
        // After flip: row 0 is blue, last row is red
        assert!(snap.grid[0][2] > snap.grid[0][0]);
        let last_row_start = (AMB_GRID_H - 1) * AMB_GRID_W;
        assert!(snap.grid[last_row_start][0] > snap.grid[last_row_start][2]);
        assert!(snap.quadrants[0][2] > snap.quadrants[2][2]);
    }
}
