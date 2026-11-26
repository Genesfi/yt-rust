# Contributing to yt-rust

Thank you for your interest in contributing to **yt-rust**! Contributions of all kinds — bug fixes, InnerTube parser improvements, UI polish, documentation, and roadmap features — are very welcome.

## Finding Something to Work On

- Check [ROADMAP.md](ROADMAP.md) for planned features in **Phase 5–7** (each phase is broken into self-contained tasks).
- Look through open Issues and Pull Requests on GitHub to avoid duplicated effort.
- If you want to propose a large change or new architectural direction, feel free to open an Issue first to discuss it.

## Development Setup

### Prerequisites
- [Rust toolchain](https://rustup.rs/) (edition 2024, `stable`)
- **Windows**: WebView2 Runtime (pre-installed on Windows 10/11, used for Google sign-in)
- **`libmpv-2.dll`**: Either let the app download it from **Settings -> Download libmpv-2.dll automatically**, or place `libmpv-2.dll` next to your built binary / in `%LOCALAPPDATA%\yt-rust\yt-rust\`.
- **Optional**: `ffmpeg` + `node` (or let the app auto-fetch `yt-dlp` / QuickJS `qjs`) for fallback streams and MP4/M4A merging.

### Building & Testing

```powershell
# Run all unit tests
cargo test --locked

# Build debug / release binary
cargo build
cargo build --release
```

If you are working in an offline environment once crates are cached:

```powershell
cargo test --offline
cargo build --release --offline
```

## Project Architecture & Conventions

- `src/main.rs` — `eframe` OpenGL (`Glow`) bootstrap, custom image/texture loaders (`src/images.rs`), crash/panic logging.
- `src/ui/` — Modular UI built with `egui`:
  - `mod.rs` — `YtApp` state, shortcuts, backend event drain, OS media sync.
  - `topbar.rs` / `sidebar.rs` — Chrome navigation, autocomplete search bar, collapsible sidebar.
  - `pages.rs` / `cards.rs` — Home, Subscriptions, Trending, Channel, Playlists, History, Downloads, Settings, and reusable cards/modals.
  - `watch.rs` — OpenGL `libmpv` video surface, YouTube-style overlay controls, Live DVR seekbar, Stats for Nerds, PiP mini-player, Shorts view, comments/replies, queue, and Live Chat.
- `src/innertube.rs` — Direct YouTube InnerTube web/player requests and JSON parsers.
- `src/backend.rs` — Background Tokio runtime actor, download progress streaming, watchtime history reporting.
- `src/mpv.rs` — Dynamic `libmpv-2.dll` loader (`libloading`) and FBO render context.
- `src/proxy.rs` — Localhost 4-worker range-prefetch proxy (`StreamProxy`) with `User-Agent` matching and `307` fallback.
- `src/os_media.rs` — Windows SMTC & hardware media key integration via `souvlaki`.

### Code & UI Guidelines
1. **Zero Unicode emojis in app UI** — Always use bundled `lucide.ttf` glyphs (`theme::icon::*`) or custom vector painters in `src/theme.rs`, centered via `theme::draw_icon_centered`.
2. **Keep UI non-blocking** — All HTTP calls, `yt-dlp` invocations, and disk I/O belong in `src/backend.rs` (or background Tokio tasks) sending `Response` variants back and calling `wake()`.
3. **Avoid duplicated logic** — Reuse helpers in `src/theme.rs`, `src/ui/cards.rs`, and `src/innertube.rs` rather than copy-pasting UI widgets or JSON walkers.
4. **Tests & warnings** — Keep `cargo test --locked` passing and avoid introducing new compiler warnings (`cargo check` / `cargo clippy`).

## Pull Request Checklist

1. Create a focused branch (`feat/...`, `fix/...`, `docs/...`) from `main`.
2. Add unit tests for new parsers, range logic, or pure state helpers (see `innertube::tests`, `proxy::tests`, `ytdlp::tests`, `os_media::imp::tests`).
3. Verify `cargo test --locked` passes cleanly.
4. For UI changes, attach a brief before/after screenshot in the PR description.
