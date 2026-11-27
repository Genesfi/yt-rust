#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auth;
mod audio_viz;
mod backend;
mod images;
mod innertube;
mod jsc;
mod login;
mod model;
mod mpv;
mod os_media;
mod potoken;
mod proxy;
mod sponsorblock;
mod theme;
mod ui;
mod win_shell;
mod ytdlp;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

fn data_dir() -> PathBuf {
    if let Some(proj) = directories::ProjectDirs::from("com", "yt-rust", "yt-rust") {
        let dir = proj.data_local_dir().to_path_buf();
        let _ = std::fs::create_dir_all(&dir);
        return dir;
    }
    PathBuf::from(".")
}

fn install_crash_diagnostics(data_dir: &std::path::Path) {
    let crash_path = data_dir.join("crash.log");
    let panic_path = crash_path.clone();
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!("PANIC: {info}\n");
        let _ = std::fs::write(&panic_path, &msg);
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let _ = std::fs::write(dir.join("yt-rust-crash.log"), &msg);
            }
        }
    }));

    #[cfg(windows)]
    windows_seh::install(crash_path);
}

#[cfg(windows)]
mod windows_seh {
    use std::ffi::c_void;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    static CRASH_PATHS: OnceLock<Vec<PathBuf>> = OnceLock::new();

    #[repr(C)]
    struct ExceptionRecord {
        exception_code: u32,
        exception_flags: u32,
        exception_record: *mut ExceptionRecord,
        exception_address: *mut c_void,
        number_parameters: u32,
        exception_information: [usize; 15],
    }

    #[repr(C)]
    struct ExceptionPointers {
        exception_record: *mut ExceptionRecord,
        context_record: *mut c_void,
    }

    unsafe extern "system" {
        fn SetUnhandledExceptionFilter(
            filter: Option<unsafe extern "system" fn(*mut ExceptionPointers) -> i32>,
        ) -> *mut c_void;
        fn GetModuleHandleExA(flags: u32, name: *const c_void, module: *mut *mut c_void) -> i32;
        fn GetModuleFileNameA(module: *mut c_void, filename: *mut u8, size: u32) -> u32;
    }

    unsafe extern "system" fn handler(info: *mut ExceptionPointers) -> i32 {
        unsafe {
            if !info.is_null() && !(*info).exception_record.is_null() {
                let rec = &*(*info).exception_record;
                let addr = rec.exception_address;
                let mut hmod: *mut c_void = std::ptr::null_mut();
                let mut mod_name = String::from("<unknown>");
                if GetModuleHandleExA(0x06, addr, &mut hmod) != 0 && !hmod.is_null() {
                    let mut buf = [0u8; 512];
                    let n = GetModuleFileNameA(hmod, buf.as_mut_ptr(), buf.len() as u32) as usize;
                    if n > 0 {
                        mod_name = String::from_utf8_lossy(&buf[..n]).into_owned();
                    }
                }
                let offset = (addr as usize).saturating_sub(hmod as usize);
                let report = format!(
                    "SEH_EXCEPTION code=0x{:08X} addr={:?} module={} offset=0x{:X}\n",
                    rec.exception_code, addr, mod_name, offset
                );
                if let Some(paths) = CRASH_PATHS.get() {
                    for p in paths {
                        let _ = std::fs::write(p, &report);
                    }
                }
            }
        }
        0
    }

    pub fn install(data_crash: PathBuf) {
        let mut paths = vec![data_crash];
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                paths.push(dir.join("yt-rust-crash.log"));
            }
        }
        let _ = CRASH_PATHS.set(paths);
        unsafe {
            SetUnhandledExceptionFilter(Some(handler));
        }
    }
}

fn main() -> eframe::Result {
    let mut args = std::env::args_os().skip(1);
    if let (Some(flag), Some(dir)) = (args.next(), args.next()) {
        if flag == login::FLAG {
            login::child_main(dir.into());
            return Ok(());
        }
        if flag == potoken::FLAG {
            potoken::child_main(dir.into());
            return Ok(());
        }
    }

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let dir = data_dir();
    install_crash_diagnostics(&dir);
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1320.0, 840.0])
        .with_min_inner_size([840.0, 540.0])
        .with_title("YouTube — yt-rust")
        .with_icon(Arc::new(theme::generate_app_icon()));
    if cfg!(windows) {
        viewport = viewport.with_transparent(true);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        persistence_path: Some(dir.join("app.ron")),
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        ..Default::default()
    };

    eframe::run_native(
        "yt-rust",
        options,
        Box::new(move |cc| {
            static CTX: OnceLock<egui::Context> = OnceLock::new();
            let _ = CTX.set(cc.egui_ctx.clone());
            let wake = Arc::new(|| {
                if let Some(c) = CTX.get() {
                    c.request_repaint();
                }
            });

            let backend = backend::Backend::spawn(dir, wake);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            cc.egui_ctx
                .add_bytes_loader(Arc::new(images::HttpImages::new(backend.clone())));
            cc.egui_ctx
                .add_texture_loader(Arc::new(images::Textures::default()));

            Ok(Box::new(ui::YtApp::new(cc, backend)))
        }),
    )
}
