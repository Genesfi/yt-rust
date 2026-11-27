# yt-rust Roadmap

Project path: `F:\Native Win\yt-rust`  
Repository: `https://github.com/Genesfi/yt-rust`  
Reference sibling project (for Windows/mpv/UI patterns): `F:\Native Win\ytm-rs`

---

## Current Status (v0.2.x — Main Branch)

Phases 1–7 are complete (`42/42` unit tests passing via `cargo test --locked`, Windows release binary + CI verified):

- **Playback Engine (`src/mpv.rs`, `src/proxy.rs`, `src/ytdlp.rs`, `src/potoken.rs`, `src/jsc.rs`)**
  - Dynamic `libmpv-2.dll` loader (`libloading`) with one-click in-app downloader (`shinchiro/mpv-winbuild-cmake`), rendering into `eframe` `Glow` OpenGL FBOs with GL-thread cleanup & paused-frame render skipping.
  - **Dual-Tab Concurrent Playback (`Tab 1` / `Tab 2`)** + crash-safe `last_watch.json` 2-tab session resume and Home **Continue watching** cards.
  - `hwdec=auto-copy` (or Software CPU toggle), native WEB `jsc` cipher solver (`src/jsc.rs`), on-demand hidden WebView2 BotGuard **PO Token** minter (`src/potoken.rs`), authenticated InnerTube player fallback, automatic `HTTP 403` URL refresh, and `yt-dlp` fallback (`node` / QuickJS).
  - Localhost multi-worker `&range=` HTTP prefetch proxy (`StreamProxy`) with latest-seek cursor prioritization.
  - Codec/resolution selector (`H.264`, `VP9`, `AV1` + live HLS `vid` switching), multi-audio tracks, playback speed (`0.5x–2.0x`), `.vtt` closed captions with styling, chapters, storyboard seekbar scrub preview, hover-auto-hiding overlay controls, Stats for Nerds (`Ctrl+I`), DVR Live edge bar (`● Live` + `-MM:SS`), **Sleep Timer** (presets + manual input), and live-progress `yt-dlp` Downloader + offline playback.

- **Shorts, PiP & Desktop Integration (`src/ui/`, `src/win_shell.rs`, `src/os_media.rs`, `src/theme.rs`)**
  - **YouTube Shorts**: 9:16 horizontal shelves on Home & Search results, plus a rounded center-locked vertical **Shorts Reel** (stays centered with Sidebar/Comments open, overlaid avatar/channel/subscribe/title, right-edge hover arrows, aligned action rail, smooth cubic ease-out slide animation + portrait thumbnail buffering preview, wheel debounce, single-pass quality selection, Grid View, Like/Dislike, Comments, Share, Save).
  - **In-App Mini-Player & Borderless Windows PiP**: Corner-snapping PiP with on-hover overlay controls and expandable **Up next** Queue drawer (auto-lift + snap-back), plus a borderless always-on-top Windows floating video window.
  - **Windows 11 Shell & Appearance**: Live-switchable **Solid Dark**, **Acrylic**, **Mica**, and **Mica Alt** (`DwmSetWindowAttribute`), **Ambient Background Mode** (**Soft Blur** / **Tiled Repeat** + cinema glow), searchable categorized Settings, Windows SMTC & hardware media keys, System Tray, Taskbar `ITaskbarList3` preview buttons, customizable keyboard shortcuts, and mouse `XButton1` / `XButton2` navigation.

- **YouTube Account, Discovery & Community (`src/innertube.rs`, `src/auth.rs`, `src/login.rs`, `src/sponsorblock.rs`)**
  - Google WebView2 sign-in with Windows Credential Manager (`keyring`), cookie rotation, and `SAPISIDHASH` request signing.
  - Personalized Home + topic chips, Subscriptions (`All` / `Unwatched` / `Live`), Trending, Two-way Watch History (`FEhistory` + `videostats` + Remove from History), Liked Videos (`VLLL`), Watch Later (`VLWL`), Playlists (`FEplaylist_aggregations`, `Play All` / `Shuffle`, Save to Playlist, Save Queue as Playlist, track remove/reorder).
  - **Channel Pages**: Banner, avatar, handle, Subscribe/Bell level menu (`All` / `Personalized` / `None`), sticky tab bar (`Home` / `Videos` / `Shorts` / `Live` / `Podcasts` / `Playlists`), `Latest` / `Popular` / `Oldest` sort chips, in-channel search, and `Members only` badges.
  - **Search & Autocomplete**: Filter chips (`All`, `Videos`, `Channels`, `Playlists`, `Live`), 4-column **Filters** drawer (Upload date, Type, Duration, Sort by), dedicated Channel & Playlist cards, separated Search **Shorts** 9:16 shelf, and left-aligned search history + live autocomplete suggestions.
  - **Community & Integrations**: Top/Newest comments (like, post, reply, expand replies, pinned & creator heart badges, clickable `MM:SS` timestamps), Live Chat & archived VOD Live Chat Replay (Super Chats, membership banners, inline custom emote picker, send message), Topbar Notifications popover, **SponsorBlock** auto-skip, **DeArrow**, and **Return YouTube Dislike**.

---

## Completed Roadmap (v0.1.0 – v0.2.x)

### Phase 1 — Code Cleanup & Windows Polish
- [x] Clean up initial `dead_code` warnings & modularize `src/ui/` (`mod.rs`, `topbar.rs`, `sidebar.rs`, `cards.rs`, `pages.rs`, `watch.rs`).
- [x] Application window/taskbar `.ico` resource, custom download folder picker, and `hwdec` toggle (`Auto-copy` vs `Software CPU`).

### Phase 2 — OS Media Integration & Player UX
- [x] Windows SMTC & hardware media keys, hover/fullscreen auto-hide overlay, **Stats for Nerds** (`Ctrl+I`), and rounded corner-snapping mini-player.

### Phase 3 — Community & Discovery Parity
- [x] Interactive comments & replies, Live Chat compose/emotes, Channel Home & Search Within Channel, vertical Shorts Reel, Remove from Watch History, Save Queue as Playlist, Queue drag-reorder, and Subscriptions filtering.

### Phase 4 — Offline Downloads Manager & CI/Release
- [x] Offline **Downloads Page** (`yt-dlp` progress tracking & local playback) and GitHub Actions CI + Windows Release workflow.

### Phase 5 — Playback Resilience & Deeper Community Features
- [x] Fixed archived VOD `is_live` detection, HLS/livestream quality switching, authenticated player fallback & `HTTP 403` refresh, Playlist `Play All` / `Shuffle` & track editing, Live Chat Replay + Super Chats + Emote picker, and Comment likes/pinned/heart badges.

### Phase 6 — Desktop Experience, Notifications & Settings
- [x] Topbar Notifications menu + bell levels, System Tray + Taskbar `ITaskbarList3` buttons, Borderless Windows Floating PiP, Sleep Timer, DWM Acrylic/Mica + Ambient Background Mode, Searchable Settings, SponsorBlock, DeArrow, and Return YouTube Dislike.

### Phase 7 — Dual Tabs, Solver, Search Filters & Shorts UX
- [x] **Dual-Tab Player & Session Resume**: Concurrent `Tab 1` / `Tab 2` playback, `last_watch.json` persistence, and Home **Continue watching** cards.
- [x] **Native Stream Resilience**: Native WEB `jsc` solver (`src/jsc.rs`) and on-demand hidden WebView2 BotGuard **PO Token** minter (`src/potoken.rs`).
- [x] **Hover Video Preview & Timestamps**: Muted on-hover grid card previews and clickable `MM:SS` / `H:MM:SS` timestamps across descriptions, comments, and replies.
- [x] **Redesigned Queue & Mini-Player**: Auto-lifting PiP Queue drawer with snap-back on close and hover transport overlay.
- [x] **Search Filters, Channels & Shorts Shelf**: `All` / `Videos` / `Channels` / `Playlists` / `Live` tabs, 4-column **Filters** drawer, Channel/Playlist cards, separated Search Shorts 9:16 shelf, and left-aligned search history/suggestions.
- [x] **Polished Vertical Shorts Reel**: Center-locked rounded 9:16 viewport, right-edge hover arrows, overlaid channel/subscribe/title header, single-pass quality selection, smooth slide transition animation, and thumbnail buffering preview.

---

## Next Roadmap (v0.3.0+ — Planned Feature Ideas)

### Phase 8 — History Controls, Playlist Tools & Radio/Mixes
- [x] **Watch History Search, Clear & Pause Controls**: In-page Watch History search filter (`FEhistory`), "Clear Watch History" confirmation action, and "Pause Watch History" toggle synced with Google account preferences.
- [x] **Playlist Privacy & Metadata Editing**: Visibility selector (`Private` / `Unlisted` / `Public`) when creating or editing playlists, inline title/description editing, and search/filter inside large playlists.
- [x] **YouTube Mixes / Radio (`RD...`) Queues**: Detect and render YouTube Mix cards (`compactRadioRenderer` / `lockupViewModel`) with auto-refilling endless Up Next mixes.
- [x] **Channel Community Posts**: Parse and render Channel **Posts** (`Community` tab — text posts, polls with vote results, and attached images/video previews).

### Phase 9 — Advanced Player, Transcript & Audio/Visual Tools
- [x] **Interactive Video Transcript Panel**: Fetch YouTube timed transcript (`get_transcript`), provide a searchable auto-scrolling drawer synced with current playback time, click-to-seek lines, and one-click `.txt` / `.srt` export.
- [x] **Dual Subtitles & Auto-Translate**: Display simultaneous primary + secondary subtitles (including YouTube `tlang=` auto-translated `.vtt` tracks) with independent font scale and color contrast options.
- [x] **Audio-Only Mode, Volume Normalization & Equalizer**: Toggle active tab into low-bandwidth audio-only stream (`vid=no`), enable `libmpv` `loudnorm` loudness equalization, and add multi-band EQ presets (`Flat`, `Voice Clarity`, `Bass Boost`, `Music`).
- [x] **Theater Mode, Frame Step, A-B Loop & Frame Snapshot**: Full-width Theater watch toggle (`T`), frame-by-frame stepping (`,` / `.` while paused), seekbar **A-B loop** markers for replaying sections, and lossless frame screenshot capture.

### Phase 10 — Batch Downloads, Packaging & Cross-Platform
- [ ] **Download Queue & Batch Playlist Downloads**: Multi-item `yt-dlp` download queue with pause/cancel/retry controls, full-playlist batch downloads, and `--embed-subs` / `--embed-thumbnail` toggles.
- [ ] **Self-Updater & Windows Installer**: GitHub Releases version checker inside Settings with one-click portable update, plus optional Inno Setup / MSIX Windows installer generation in CI.
- [ ] **Cross-Platform CI & Parity (Linux / macOS)**: Expand `.github/workflows/ci.yml` to verify builds, `souvlaki` MPRIS / Now Playing, keyring backends, and `libmpv` discovery across `ubuntu-latest` and `macos-latest`.

---

## Notes for Starting a New Chat

- Run tests & release builds from the workspace root:
  ```powershell
  cargo test --locked
  cargo build --release
  ```
- First suggested prompt for a new chat:
  > *"Baca `ROADMAP.md` di repo `yt-rust`, lalu kita kerjakan Phase 8 (Watch History search/clear/pause, Playlist visibility & edit, YouTube Mixes, atau Channel Community Posts)."*
