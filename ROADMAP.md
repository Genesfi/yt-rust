# yt-rust Roadmap

Project path: `F:\Native Win\yt-rust`  
Repository: `https://github.com/Genesfi/yt-rust`  
Reference sibling project (for Windows/mpv/UI patterns): `F:\Native Win\ytm-rs`

---

## Current Status (v0.1.0 — Main Branch)

All core playback, authentication, sync, and browse features are implemented and verified (`8/8` unit tests passing, release binary compiles cleanly):

- **Playback Engine (`src/mpv.rs`, `src/proxy.rs`, `src/ytdlp.rs`)**
  - Dynamic `libmpv-2.dll` loader (`libloading`) rendering into `eframe` `Glow` OpenGL FBO (`FLIP_Y = 1`, intersected `gl.scissor`).
  - `hwdec=auto-copy` with `hwdec-codecs=h264,hevc` + `vd-lavc-dr=no` + `opengl-pbo=no` (software multi-thread decode for VP9/AV1, avoiding Windows driver/codec-switch crashes).
  - Localhost 4-worker parallel `&range=` HTTP prefetch proxy (`StreamProxy`) eliminating single-stream 1080p throttling, using a stored `tokio::runtime::Handle` safe from UI threads.
  - Multi-quality / codec selector (`H.264`, `VP9`, `AV1`), multi-audio track selector (*Original* / auto-dubbed), playback speed (`0.5x–2.0x`), `.vtt` closed captions, chapters, storyboard thumbnail scrub preview, `yt-dlp` fallback, and MP4/M4A download to `Downloads/yt-rust`.
  - Crash diagnostics (`crash.log`, `yt-rust-crash.log`, `mpv.log`).

- **YouTube Account, Sync & Navigation (`src/innertube.rs`, `src/auth.rs`, `src/login.rs`, `src/backend.rs`)**
  - Google login via WebView2 (`tao` + `wry`), session storage in Windows Credential Manager (`keyring`), `SAPISIDHASH` request signing.
  - Personalized Home + topic chips (arrow buttons, wheel scroll, left-click drag panning).
  - Two-way Watch History (`FEhistory` + `videostatsPlaybackUrl` / `videostatsWatchtimeUrl` + `player_sts.txt`).
  - Resume progress (`startPercent` / `percentDurationWatched` / `startTimeSeconds` + local `resume_positions` persistence) with red thumbnail progress bars and auto-resume on play.
  - Subscriptions feed (`FEsubscriptions`) & sidebar Subscriptions list (`/youtubei/v1/guide`).
  - Liked Videos (`VLLL`), Watch Later (`VLWL`), and user Playlists (`FEplaylist_aggregations`).
  - Save to Playlist modal (`playlist/get_add_to_playlist`, `browse/edit_playlist`, `playlist/create`) with playlist thumbnails, membership checkmarks, and search filter.
  - Channel pages with banner, avatar, `@handle`, subscriber/video counts, subscribe toggle, and interactive tabs (`Videos`, `Shorts`, `Live`, `Playlists`).
  - Search + live autocomplete (`suggestqueries-clients6.youtube.com`), comments pagination, live chat polling, Like/Dislike, SponsorBlock (`src/sponsorblock.rs`), Play Queue, and draggable PiP mini-player with interactive seekbar.

---

## Next Roadmap (Ready for New Chat)

### Phase 1 — Code Cleanup & Windows Polish
- [x] Clean up remaining `dead_code` compiler warnings in `src/backend.rs`, `src/model.rs`, and `src/theme.rs`.
- [x] Embed an application window/taskbar icon (`theme::generate_app_icon()` via viewport `IconData`; `.exe` PE resource `.ico` optional).
- [x] Add Settings options for: custom download directory + "Open Downloads folder" button (`FOLDER_OPEN` icon), and optional `hwdec` mode toggle (`Auto-copy` vs `Software CPU`).
- [x] Modularize `src/ui.rs` into focused submodules (`src/ui/{mod.rs, topbar.rs, sidebar.rs, watch.rs, pages.rs, cards.rs}`).

### Phase 2 — OS Media Integration & Player UX
- [x] **Windows SMTC & Media Keys** (`Play/Pause`, `Next`, `Previous`, `Seek`) showing current video title, channel, and thumbnail in Windows media flyout.
- [x] **Hover/Fullscreen Auto-Hide Overlay** on the video surface so fullscreen feels clean like native YouTube.
- [x] **Stats for Nerds** modal/overlay (`Ctrl+I` or control-bar toggle): active codec, resolution, fps, bitrate, proxy cache speed/duration, `hwdec-current`, dropped frames.
- [x] Round top corners cleanly on PiP video rendering (`draw_mini_player`) + optional snap-to-corner behavior.

### Phase 3 — Community & Discovery Parity
- [x] **Interactive Comments & Live Chat Input**: post comments (`comment/create_comment`), expand comment replies (`commentRepliesRenderer`), sort Top/Newest, and send live chat messages (`live_chat/send_message`).
- [ ] **Channel Home & Search Within Channel**, plus vertical **Shorts Player** mode.
- [x] **Remove from Watch History**, **Save Queue as Playlist**, & Queue drag-and-drop reordering.
- [x] **Subscriptions Filtering** (`All` / `Unwatched` / `Live now`).

### Phase 4 — Offline Downloads Manager & CI/Release
- [x] **Downloads Page** in sidebar tracking live `yt-dlp` download progress (`%`, speed, size), listing downloaded MP4/M4A files, and supporting offline playback directly in `libmpv`.
- [x] **GitHub Actions CI** workflow running `cargo test` on push/PR and publishing Windows release builds to GitHub Releases.

---

## Notes for Starting a New Chat

- Build/test inside workspace with offline cargo flags if network is restricted in sandbox:
  ```powershell
  cargo test --offline --manifest-path "F:\Native Win\yt-rust\Cargo.toml"
  cargo build --release --offline --manifest-path "F:\Native Win\yt-rust\Cargo.toml"
  ```
- First suggested prompt for new chat:
  > *"Baca `ROADMAP.md` di repo `yt-rust`, lalu kita lanjut ke Phase 1 (pecah `src/ui.rs` jadi modul rapi + bersihkan warning + tambah icon app) atau Phase 2 (Windows SMTC & Stats for Nerds)."*
