use std::path::{Path, PathBuf};

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::window::WindowBuilder;
use wry::{PageLoadEvent, WebContext, WebViewBuilder};

pub const FLAG: &str = "--login";
const START_URL: &str = "https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fwww.youtube.com%2F";
const HOME_URL: &str = "https://www.youtube.com/";

pub enum Outcome {
    Cookie(String),
    Cancelled,
    Failed(String),
}

pub fn spawn(profile_dir: PathBuf, done: impl FnOnce(Outcome) + Send + 'static) {
    std::thread::Builder::new()
        .name("yt-rust-login".into())
        .spawn(move || {
            let res = std::env::current_exe().and_then(|exe| {
                std::process::Command::new(exe)
                    .arg(FLAG)
                    .arg(&profile_dir)
                    .stdin(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .output()
            });
            done(match res {
                Err(e) => Outcome::Failed(format!("Could not start login window: {e}")),
                Ok(out) => {
                    let text = String::from_utf8_lossy(&out.stdout);
                    let line = text.lines().last().unwrap_or_default();
                    if let Some(c) = line.strip_prefix("cookie ") {
                        Outcome::Cookie(c.to_owned())
                    } else if let Some(e) = line.strip_prefix("failed ") {
                        Outcome::Failed(e.to_owned())
                    } else if line == "cancelled" {
                        Outcome::Cancelled
                    } else {
                        Outcome::Failed(format!("Sign-in window closed ({})", out.status))
                    }
                }
            });
        })
        .expect("spawn login thread");
}

pub fn child_main(profile_dir: PathBuf) {
    match run(profile_dir) {
        Ok(Outcome::Cookie(c)) => println!("cookie {c}"),
        Ok(Outcome::Cancelled) => println!("cancelled"),
        Ok(Outcome::Failed(e)) => println!("failed {e}"),
        Err(e) => println!("failed {e:#}"),
    }
}

pub fn forget(profile_dir: &Path) {
    for _ in 0..10 {
        match std::fs::remove_dir_all(profile_dir) {
            Ok(()) => return,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(400)),
        }
    }
}

fn run(profile_dir: PathBuf) -> anyhow::Result<Outcome> {
    let mut event_loop = EventLoopBuilder::<()>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title("Sign in to YouTube — yt-rust")
        .with_inner_size(tao::dpi::LogicalSize::new(480.0, 720.0))
        .build(&event_loop)?;

    let proxy = event_loop.create_proxy();
    let mut context = WebContext::new(Some(profile_dir));
    let builder = WebViewBuilder::new_with_web_context(&mut context)
        .with_url(START_URL)
        .with_on_page_load_handler(move |ev, url| {
            if matches!(ev, PageLoadEvent::Finished) && url.starts_with(HOME_URL) {
                let _ = proxy.send_event(());
            }
        });

    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(window.default_vbox().expect("tao vbox"))?
    };
    #[cfg(not(target_os = "linux"))]
    let webview = builder.build(&window)?;

    let mut outcome = Outcome::Cancelled;
    event_loop.run_return(|event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(()) => match webview.cookies_for_url(HOME_URL) {
                Ok(cookies) => {
                    if cookies.iter().any(|c| c.name() == "SAPISID") {
                        let joined = cookies
                            .iter()
                            .map(|c| format!("{}={}", c.name(), c.value()))
                            .collect::<Vec<_>>()
                            .join("; ");
                        outcome = Outcome::Cookie(joined);
                        *flow = ControlFlow::Exit;
                    }
                }
                Err(e) => {
                    outcome = Outcome::Failed(format!("Could not read cookies: {e}"));
                    *flow = ControlFlow::Exit;
                }
            },
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *flow = ControlFlow::Exit,
            _ => {}
        }
    });
    drop(webview);
    Ok(outcome)
}
