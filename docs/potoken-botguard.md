# Rencana & Spesifikasi Teknis: BotGuard PO Token On-Demand (`Auto / On / Off`) untuk `yt-rust`

Dokumen ini adalah blueprint siap pakai untuk menambahkan **BotGuard PO Token minter** di `yt-rust` dengan mode **`Auto` / `On` / `Off`** agar playback dan `yt-dlp` tetap lancar saat jaringan terkena bot check YouTube (`LOGIN_REQUIRED: Sign in to confirm you're not a bot`) atau `403` / `Requested format is not available`.

---

## 1. Kenapa Pendekatan `yt-rust` Berbeda dengan `ytm-rs`

- Di **`ytm-rs`**, sudah ada cipher/n-sig solver internal (`jsc.rs` via `rquickjs`) + client `WEB_REMIX` (`webmusic.rs`), sehingga PO Token dipakai langsung di Rust.
- Di **`yt-rust`**, jalur cepat Rust (`src/innertube.rs`) mengandalkan client tanpa-login **`VISIONOS`** dan **`ANDROID_VR`**, sedangkan dekripsi cipher/n-sig dan client `web` diserahkan ke **`yt-dlp.exe`** (`src/ytdlp.rs`).
- Jadi untuk `yt-rust`, cara paling bersih & hemat maintenance adalah:
  1. Cetak PO Token lewat hidden WebView2 (`src/potoken.rs`) menggunakan `tao` + `wry` (yang **sudah ada** di `Cargo.toml`).
  2. Teruskan token hasil cetakan ke **`yt-dlp`** lewat flag resmi:
     `--extractor-args "youtube:po_token=web.player+<player_pot>,web.gvs+<stream_pot>"`
     (sehingga `yt-dlp` bisa memakai client `web` untuk 1080p/VP9/AV1 tanpa terblokir PO Token atau `Requested format is not available`).

Referensi implementasi Rust yang sudah teruji jalan di Windows:
- `F:\Native Win\ytm-rs\src\potoken.rs` (berhasil mencetak `player_pot` & `stream_pot` asli dengan overhead ~83 MB WebView2 saat aktif dan 0 MB saat mati).

---

## 2. Mode Pengaturan: `Auto` / `On` / `Off`

Tambahkan enum setting `PoTokenMode` (disimpan di config/prefs UI Settings):

1. **`Auto` (Default / Direkomendasikan)**
   - **Saat `VISIONOS` / `ANDROID_VR` berhasil**: Child process `--potoken` **tidak pernah dinyalakan** (`0 MB` RAM). Jika sebelumnya hidup karena sempat kena flag lalu `innertube::watch` berhasil lagi, panggil `potoken.close_if_running()`.
   - **Saat `innertube::watch` gagal (`LOGIN_REQUIRED` / `Sign in to confirm`) atau `yt-dlp` gagal dengan `Requested format is not available` / auth error**:
     - Nyalakan hidden WebView2 on-demand, cetak `(player_pot, stream_pot)` dari `visitor_data` + `video_id`.
     - Jalankan ulang / teruskan ke `yt-dlp` dengan `youtube:po_token=...`.
   - **Auto-exit**: Mati otomatis setelah idle 5 menit atau saat `innertube::watch` sukses kembali.

2. **`On` (Always On)**
   - Setiap kali `yt-dlp` dipanggil (atau langsung sejak fallback aktif), langsung sertakan PO Token tanpa menunggu gagal terlebih dahulu. Berguna jika IP user memang sedang terus-terusan di-flag YouTube.

3. **`Off` (Manual Mati)**
   - Jangan pernah menjalankan child process `--potoken`. Tutup segera jika sedang berjalan. `yt-rust` hanya memakai `VISIONOS` / `ANDROID_VR` / `yt-dlp` standar (`web_safari,tv,android_vr`).

---

## 3. Arsitektur Child Process (`src/potoken.rs`)

Karena `eframe` sudah memegang event loop `winit` utama, WebView2 (`tao` + `wry`) harus berjalan di child process terpisah (pola yang sama persis dengan `src/login.rs` `--login`):

### A. Entry Point (`src/main.rs`)
- Tambahkan `mod potoken;`
- Di awal `main()` sebelum inisialisasi `eframe`/logging, cek argumen `--potoken <profile_dir>`: panggil `potoken::child_main(dir.into())` dan langsung `return Ok(())`.
- Pastikan **tidak ada log yang dicetak ke `stdout`** di mode `--potoken`, karena `stdout` dipakai sebagai jalur JSON RPC ke proses induk (`stdin` untuk request, `stdout` untuk response).

### B. Hidden WebView2 & Protocol BotGuard (`tao` + `wry`)
1. Buat hidden window: `WindowBuilder::new().with_visible(false)` + `WebViewBuilder` mengarah ke **`https://www.youtube.com/robots.txt`** (`~792` bytes, tanpa CSP `'unsafe-eval'`, memberi origin asli `https://www.youtube.com` yang dibutuhkan BotGuard).
   - Catatan `User-Agent`: gunakan desktop Chrome UA (`Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.3`).
   - Saat spawn child di Windows, pakai `.creation_flags(0x0800_0000)` (`CREATE_NO_WINDOW`) supaya jendela console tidak berkedip.
2. **Penting**: Runtime Tokio di dalam child process **wajib `new_multi_thread().worker_threads(1)`**, bukan `new_current_thread()`, karena main thread diblokir oleh `event_loop.run_return(...)`.
3. **Alur 2 Tahap BotGuard**:
   - **Tahap Init (sekali per TTL `integrityToken`, kurangi margin 10 menit)**:
     1. `POST https://www.youtube.com/api/jnn/v1/Create`
        - Header: `Accept: application/json`, `Content-Type: application/json+protobuf`, `x-goog-api-key` (public BotGuard client key, lihat `GOOGLE_API_KEY` di `src/potoken.rs`), `x-user-agent: grpc-web-javascript/0.1`
        - Body: `["O43z0dpjhgX20SCx4KAo"]`
     2. Parse/descramble response (base64url decode, jika indeks `[1]` string: tiap byte `(b + 97) as u8` ➔ JSON array challenge).
     3. Di WebView JS (`evaluate_script`): jalankan `new Function(interpreterJavascript)()` ➔ `bgVm.a(program, ...)` ➔ `asyncSnapshotFunction` ➔ kirim `botguardResponse` via `window.ipc.postMessage(...)`.
     4. `POST https://www.youtube.com/api/jnn/v1/GenerateIT`
        - Body: `["O43z0dpjhgX20SCx4KAo", "<botguardResponse>"]`
        - Hasil: `[integrityTokenB64, ttlSeconds]`.
     5. Di WebView JS: panggil `webPoSignalOutput[0](integrityTokenU8)` untuk membuat callback `poTokenMinter`.
   - **Tahap Mint per Request (<50 ms setelah init)**:
     1. Cache `player_pot = obtainPoToken(visitor_data)` per `visitor_data` selama `minter` belum expired.
     2. Cetak `stream_pot = obtainPoToken(video_id)`.
     3. Konversi hasil `Uint8Array` ke URL-safe Base64.

### C. Ketergantungan (`Cargo.toml`)
- `tao = "0.37.1"` dan `wry = "0.57.0"` sudah ada di `yt-rust/Cargo.toml`.
- `base64` belum ada di `yt-rust/Cargo.toml` (bisa tambahkan `base64 = "0.22"` atau tulis helper encode/decode URL-safe Base64 sederhana ~25 baris di `src/potoken.rs`).

---

## 4. Titik Integrasi di `yt-rust`

1. **`src/backend.rs` (`resolve_watch`)**:
   - Simpan `potoken: Arc<PoTokenClient>` dan setting `po_token_mode: Arc<AtomicU8>` (Off / Auto / On) di state `Backend`.
   - Jika `innertube::watch` berhasil: panggil `potoken.close_if_running()` (saat mode `Auto`).
   - Jika fallback ke `ytdlp::resolve_video`:
     - Saat mode `On`: minta `potoken.mint(&visitor_data, video_id)` terlebih dulu sebelum memanggil `ytdlp`.
     - Saat mode `Auto`: jalankan `ytdlp` seperti biasa (atau langsung cetak PO Token jika `innertube::watch` gagal dengan `LOGIN_REQUIRED` / `Sign in to confirm`), lalu jika gagal format/bot-check, cetak PO Token dan retry `ytdlp` dengan PO Token.
   - Catatan `StreamProxy` (`src/proxy.rs`): pastikan URL dari `ytdlp` dengan `&pot=` tetap bisa diputar lancar oleh `libmpv` (baik langsung maupun lewat `proxy.register`).

2. **`src/ytdlp.rs` (`invoke_ytdlp_json` & `run_ytdlp`)**:
   - Tambahkan parameter opsional `po_token: Option<&PoTokens>` pada `resolve_video` / `run_ytdlp`.
   - Jika `Some(pot)`, tambahkan argumen ke `yt-dlp`:
     ```rust
     let arg = format!(
         "youtube:player_client=web,web_safari,tv;po_token=web.player+{},web.gvs+{}",
         pot.player, pot.stream
     );
     cmd.args(["--extractor-args", &arg]);
     ```
   - Catatan: jika `jar_path` (`--cookies`) ada, cek apakah `web` / `web_safari` menerima `po_token` bersamaan dengan cookies — jika akun login dipakai, `yt-dlp` juga mendukung scope `web.player+` & `web.gvs+` dengan cookies sesi.

3. **`src/ui/` (Settings)**:
   - Tambahkan pemilih `BotGuard PO Token`: `Auto (When blocked)` / `Always On` / `Off` di halaman Settings (simpan ke persistence config `yt-rust`).
   - Jika user mengubah ke `Off`, langsung panggil `backend.close_potoken()`.
