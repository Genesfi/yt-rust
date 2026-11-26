# yt-rust Roadmap

Project path: `F:\Native Win\yt-rust`  
Repository: `https://github.com/Genesfi/yt-rust`  
Reference sibling project (for Windows/mpv/UI patterns): `F:\Native Win\ytm-rs`

---

## Current Status (v0.1.0 — Main Branch)

Phases 1–4 are complete (`11/11` unit tests passing, Windows release binary + CI verified):

- **Playback Engine (`src/mpv.rs`, `src/proxy.rs`, `src/ytdlp.rs`)**
  - Dynamic `libmpv-2.dll` loader (`libloading`) rendering into `eframe` `Glow` OpenGL FBO (`FLIP_Y = 1`, intersected `gl.scissor`).
  - `hwdec=auto-copy` (or CPU toggle in Settings) with `hwdec-codecs=h264,hevc` + `vd-lavc-dr=no` + `opengl-pbo=no`.
  - Localhost 4-worker parallel `&range=` HTTP prefetch proxy (`StreamProxy`) with per-session `User-Agent` matching, alternate-UA retry on `HTTP 403`, and automatic `307` redirect fallback if chunk 0 fails.
  - Multi-quality / codec selector (`H.264`, `VP9`, `AV1`), multi-audio track selector (*Original* / auto-dubbed), playback speed (`0.5x–2.0x`), `.vtt` closed captions, chapters, storyboard thumbnail scrub preview, auto-hiding overlay controls, Stats for Nerds (`Ctrl+I`), YouTube-style Live edge bar (`● Live` + `-MM:SS` DVR rewind), vertical Shorts Player mode, `yt-dlp` fallback (`node` / QuickJS `qjs`), and live-progress MP4/M4A downloader.
  - Windows SMTC & hardware media keys integration (`src/os_media.rs` via `souvlaki`) and crash diagnostics (`crash.log`, `yt-rust-crash.log`, `mpv.log`).

- **YouTube Account, Sync & Navigation (`src/innertube.rs`, `src/auth.rs`, `src/login.rs`, `src/backend.rs`)**
  - Google login via WebView2 (`tao` + `wry`), session storage in Windows Credential Manager (`keyring`), `SAPISIDHASH` request signing.
  - Personalized Home + topic chips (arrow buttons, wheel scroll, left-click drag panning).
  - Two-way Watch History (`FEhistory` + `videostatsPlaybackUrl` / `videostatsWatchtimeUrl` + `player_sts.txt`) and Remove from Watch History (`feedback`).
  - Resume progress (`startPercent` / `percentDurationWatched` / `startTimeSeconds` + local `resume_positions` persistence) with red thumbnail progress bars and auto-resume on play.
  - Subscriptions feed (`FEsubscriptions`) with `All` / `Unwatched` / `Live now` filtering & sidebar Subscriptions list (`/youtubei/v1/guide`).
  - Liked Videos (`VLLL`), Watch Later (`VLWL`), and user Playlists (`FEplaylist_aggregations`) with unbounded sidebar thumbnail backfill.
  - Save to Playlist modal (`playlist/get_add_to_playlist`, `browse/edit_playlist`, `playlist/create`) and **Save Queue as Playlist**.
  - Channel pages with banner, avatar, `@handle`, subscriber/video counts, subscribe toggle, `Home` / `Videos` / `Shorts` / `Live` / `Playlists` tabs, and **Search Within Channel**.
  - Search + live autocomplete, interactive comments (Top/Newest sort, post comment, reply to comment, expand replies), Live Chat (polling, send message, avatars & custom emotes), Like/Dislike, SponsorBlock, drag-to-reorder persistent Play Queue, Downloads Manager page, mouse Back/Forward, and rounded corner-snapping PiP mini-player.

---

## Completed Roadmap (v0.1.0)

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
- [x] **Interactive Comments & Live Chat Input**: post comments (`comment/create_comment`), reply to comments (`comment/create_comment_reply`), expand comment replies (`commentRepliesRenderer`), sort Top/Newest, and send live chat messages (`live_chat/send_message`) with avatars & custom emotes.
- [x] **Channel Home & Search Within Channel**, plus vertical **Shorts Player** mode.
- [x] **Remove from Watch History**, **Save Queue as Playlist**, & Queue drag-and-drop reordering.
- [x] **Subscriptions Filtering** (`All` / `Unwatched` / `Live now`).

### Phase 4 — Offline Downloads Manager & CI/Release
- [x] **Downloads Page** in sidebar tracking live `yt-dlp` download progress (`%`, speed, size), listing downloaded MP4/M4A files, and supporting offline playback directly in `libmpv`.
- [x] **GitHub Actions CI** workflow running `cargo test` on push/PR and publishing Windows release builds to GitHub Releases.

---

## Next Roadmap (v0.2.0 Ideas — Ready for Contributors & New Chat)

### Phase 5 — Playback Resilience & Deeper Community Features
- [ ] **Authenticated Player Client Fallback & Stream Recovery**: when unauthenticated `VISIONOS` `/player` hits `LOGIN_REQUIRED`, try authenticated Web/TV InnerTube `/player` before spawning `yt-dlp`, and auto-refresh expired `googlevideo` URLs if `HTTP 403` occurs mid-playback.
- [ ] **Playlist Playback & Management**: "Play All" / "Shuffle Play" buttons on Playlist & Channel pages (loading full playlist into Queue), plus removing or reordering tracks inside custom YouTube playlists.
- [ ] **Rich Live Chat & Emote Picker**: render Super Chats (`liveChatPaidMessageRenderer`), membership banners, and an inline custom channel emote picker in the Live Chat compose bar.
- [ ] **Comment Actions & Badges**: thumb-up (`comment/perform_comment_action`) on comments/replies, plus pinned comment & creator heart badges.

### Phase 6 — Desktop Experience, Notifications & Settings
- [ ] **YouTube Notifications Menu**: topbar bell popup (`notification/get_notification_menu`) with unread count badge and per-channel notification level toggle (`All` / `Personalized` / `None`).
- [ ] **System Tray & Window Modes**: optional minimize/close to system tray, "Keep on Top" for mini-player/compact windows, and Windows taskbar thumbnail preview buttons (`Prev` / `Play-Pause` / `Next`).
- [ ] **Subtitle & Player Customization**: caption font size/background opacity controls, ambient cinema glow behind the video surface, and custom keyboard shortcuts in Settings.
- [ ] **Community Integrations (Optional)**: opt-in SponsorBlock category toggles, DeArrow title/thumbnail cleanup, and Return YouTube Dislike count display.

### Phase 7 — Packaging, Search Filters & Cross-Platform
- [ ] **Search Filters & History Controls**: Search type/upload-date/duration filter chips (`browse` / `search` params), "Clear Watch History" / pause watch history toggle, and Playlist visibility selector (`Private` / `Unlisted` / `Public`).
- [ ] **Self-Updater & Windows Installer**: GitHub Releases update checker with one-click portable update, plus optional Inno Setup / MSIX installer in CI.
- [ ] **Cross-Platform CI & Parity (Linux / macOS)**: expand `.github/workflows/ci.yml` to verify builds, `souvlaki` MPRIS/Now Playing, keyring backends, and `libmpv` discovery across `ubuntu-latest` and `macos-latest`.

---

## Notes for Starting a New Chat

- Build/test inside workspace with offline cargo flags if network is restricted in sandbox:
  ```powershell
  cargo test --offline --manifest-path "F:\Native Win\yt-rust\Cargo.toml"
  cargo build --release --offline --manifest-path "F:\Native Win\yt-rust\Cargo.toml"
  ```
- First suggested prompt for new chat:
  > *"Baca `ROADMAP.md` di repo `yt-rust`, lalu kita lanjut ke Phase 5 (Authenticated player fallback, Playlist Play All, Rich Live Chat & Emote Picker, Comment Like/Badges)."*
