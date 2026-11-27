//! Real-time audio spectrum & waveform analyzer backed by Windows WASAPI loopback capture
//! and a pure-Rust radix-2 FFT with adaptive dB scaling.

use std::f32::consts::TAU;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

pub const NUM_BANDS: usize = 64;
const FFT_SIZE: usize = 2048;

#[derive(Clone)]
pub struct AudioSpectrumHandle {
    shared: Arc<SharedSpectrum>,
}

struct SharedSpectrum {
    bands: Mutex<[f32; NUM_BANDS]>,
    wave: Mutex<[f32; NUM_BANDS]>,
    active: AtomicBool,
    reopen: AtomicBool,
    stop: AtomicBool,
    wake: Condvar,
    wake_mu: Mutex<()>,
}

impl AudioSpectrumHandle {
    pub fn start() -> Self {
        let shared = Arc::new(SharedSpectrum {
            bands: Mutex::new([0.0; NUM_BANDS]),
            wave: Mutex::new([0.0; NUM_BANDS]),
            active: AtomicBool::new(false),
            reopen: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            wake: Condvar::new(),
            wake_mu: Mutex::new(()),
        });
        let bg = shared.clone();
        #[cfg(windows)]
        {
            let _ = std::thread::Builder::new()
                .name("audio-spectrum-loopback".into())
                .spawn(move || imp::run_loopback(bg));
        }
        #[cfg(not(windows))]
        {
            let _ = bg;
        }
        Self { shared }
    }

    pub fn set_active(&self, active: bool) {
        let prev = self.shared.active.swap(active, Ordering::SeqCst);
        if active && !prev {
            self.shared.wake.notify_all();
        }
    }

    pub fn notify_device_changed(&self) {
        self.shared.reopen.store(true, Ordering::SeqCst);
        self.shared.wake.notify_all();
    }

    /// Snapshot current smoothed frequency bands and wave envelope (`0.0..=1.0`).
    pub fn snapshot(&self, decay_only: bool) -> ([f32; NUM_BANDS], [f32; NUM_BANDS]) {
        let mut bands = [0.0_f32; NUM_BANDS];
        let mut wave = [0.0_f32; NUM_BANDS];
        if let Ok(mut g) = self.shared.bands.lock() {
            if decay_only {
                for b in g.iter_mut() {
                    *b *= 0.85;
                    if *b < 0.004 {
                        *b = 0.0;
                    }
                }
            }
            bands = *g;
        }
        if let Ok(mut w) = self.shared.wave.lock() {
            if decay_only {
                for v in w.iter_mut() {
                    *v *= 0.85;
                    if *v < 0.004 {
                        *v = 0.0;
                    }
                }
            }
            wave = *w;
        }
        (bands, wave)
    }
}

impl Drop for AudioSpectrumHandle {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.wake.notify_all();
    }
}

/// In-place radix-2 Cooley-Tukey FFT on `re` / `im` slices (len must be power of two).
pub fn fft_in_place(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two() && im.len() == n);

    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    let mut len = 2usize;
    while len <= n {
        let half = len >> 1;
        let angle_step = -TAU / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..half {
                let ang = k as f32 * angle_step;
                let (sin_a, cos_a) = ang.sin_cos();
                let u_re = re[start + k];
                let u_im = im[start + k];
                let v_re = re[start + k + half];
                let v_im = im[start + k + half];
                let t_re = v_re * cos_a - v_im * sin_a;
                let t_im = v_re * sin_a + v_im * cos_a;
                re[start + k] = u_re + t_re;
                im[start + k] = u_im + t_im;
                re[start + k + half] = u_re - t_re;
                im[start + k + half] = u_im - t_im;
            }
        }
        len <<= 1;
    }
}

/// Compute `NUM_BANDS` logarithmically spaced magnitudes in `0.0..=1.0` using adaptive dB
/// scaling (`agc_peak_db` tracks live signal peaks so podcasts and music both fill cleanly).
pub fn compute_spectrum_bands(
    samples: &[f32; FFT_SIZE],
    sample_rate: u32,
    agc_peak_db: &mut f32,
) -> ([f32; NUM_BANDS], [f32; NUM_BANDS]) {
    let mut re = [0.0_f32; FFT_SIZE];
    let mut im = [0.0_f32; FFT_SIZE];
    for i in 0..FFT_SIZE {
        let hann = 0.5 * (1.0 - (TAU * i as f32 / (FFT_SIZE - 1) as f32).cos());
        re[i] = samples[i] * hann;
    }
    fft_in_place(&mut re, &mut im);

    let sr = (sample_rate.max(8000)) as f32;
    let nyquist = sr * 0.5;
    let f_min = 42.0_f32;
    let f_max = nyquist.min(15_000.0);
    let log_ratio = (f_max / f_min).ln();
    let max_bin = FFT_SIZE / 2 - 1;
    let norm_div = FFT_SIZE as f32 * 0.25;

    let mut raw_db = [-120.0_f32; NUM_BANDS];
    let mut frame_max_db = -90.0_f32;
    for (b, db_slot) in raw_db.iter_mut().enumerate() {
        let frac0 = b as f32 / NUM_BANDS as f32;
        let frac1 = (b + 1) as f32 / NUM_BANDS as f32;
        let hz0 = f_min * (log_ratio * frac0).exp();
        let hz1 = f_min * (log_ratio * frac1).exp();
        let bin0 = ((hz0 * FFT_SIZE as f32 / sr).floor() as usize).clamp(1, max_bin);
        let bin1 = ((hz1 * FFT_SIZE as f32 / sr).ceil() as usize).clamp(bin0, max_bin);

        let mut peak = 0.0_f32;
        for k in bin0..=bin1 {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt() / norm_div;
            if mag > peak {
                peak = mag;
            }
        }
        // Subtle treble tilt so voice & hi-hats stay crisp next to bass
        let tilt_db = 5.5 * (b as f32 / (NUM_BANDS - 1) as f32);
        let db = 20.0 * (peak + 1e-6).log10() + tilt_db;
        *db_slot = db;
        if db > frame_max_db {
            frame_max_db = db;
        }
    }

    if frame_max_db > *agc_peak_db {
        *agc_peak_db += (frame_max_db - *agc_peak_db) * 0.35;
    } else {
        *agc_peak_db = (*agc_peak_db - 0.14).max(-46.0);
    }
    let ceil_db = (*agc_peak_db).clamp(-44.0, -2.0);
    let floor_db = (ceil_db - 46.0).max(-82.0);
    let span = (ceil_db - floor_db).max(16.0);

    let mut out = [0.0_f32; NUM_BANDS];
    for (b, &db) in raw_db.iter().enumerate() {
        let norm = ((db - floor_db) / span).clamp(0.0, 1.0);
        out[b] = norm.powf(1.25);
    }

    // Time-domain RMS envelope across `NUM_BANDS` chunks normalized against same AGC level
    let mut wave = [0.0_f32; NUM_BANDS];
    let chunk = (FFT_SIZE / NUM_BANDS).max(1);
    for (i, wslot) in wave.iter_mut().enumerate() {
        let s = &samples[i * chunk..((i + 1) * chunk).min(FFT_SIZE)];
        let rms = (s.iter().map(|&v| v * v).sum::<f32>() / s.len() as f32).sqrt();
        let wdb = 20.0 * (rms + 1e-6).log10() + 8.0;
        let norm = ((wdb - floor_db) / span).clamp(0.0, 1.0);
        *wslot = norm.powf(1.05);
    }
    (out, wave)
}

pub fn smooth_bands(prev: &mut [f32; NUM_BANDS], target: &[f32; NUM_BANDS]) {
    for (p, &t) in prev.iter_mut().zip(target.iter()) {
        if t > *p {
            *p += (t - *p) * 0.65;
        } else {
            *p = (*p * 0.80 + t * 0.20).max(t);
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use super::{FFT_SIZE, NUM_BANDS, SharedSpectrum, compute_spectrum_bands, smooth_bands};

    type Hresult = i32;

    const COINIT_MULTITHREADED: u32 = 0x0;
    const CLSCTX_ALL: u32 = 0x17;
    const E_RENDER: u32 = 0;
    const E_CONSOLE: u32 = 0;
    const E_MULTIMEDIA: u32 = 1;
    const AUDCLNT_SHAREMODE_SHARED: u32 = 0;
    const AUDCLNT_STREAMFLAGS_LOOPBACK: u32 = 0x0002_0000;
    const AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM: u32 = 0x8000_0000;
    const AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY: u32 = 0x0800_0000;
    const AUDCLNT_BUFFERFLAGS_SILENT: u32 = 0x2;
    const WAVE_FORMAT_PCM: u16 = 1;
    const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
    const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    const IID_IUNKNOWN: Guid = Guid {
        data1: 0x0000_0000,
        data2: 0x0000,
        data3: 0x0000,
        data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
    };
    const IID_IAGILE_OBJECT: Guid = Guid {
        data1: 0x94EA_2B94,
        data2: 0xE9CC,
        data3: 0x49E0,
        data4: [0xC0, 0xFF, 0xEE, 0x64, 0xCA, 0x8F, 0x5B, 0x90],
    };
    const IID_IACTIVATE_AUDIO_INTERFACE_COMPLETION_HANDLER: Guid = Guid {
        data1: 0x41D9_49AB,
        data2: 0x9862,
        data3: 0x444A,
        data4: [0x80, 0xF6, 0xC2, 0x61, 0x33, 0x4D, 0xA5, 0xEB],
    };
    const CLSID_MMDEVICE_ENUMERATOR: Guid = Guid {
        data1: 0xBCDE_0395,
        data2: 0xE52F,
        data3: 0x467C,
        data4: [0x8E, 0x3D, 0xC4, 0x57, 0x92, 0x91, 0x69, 0x2E],
    };
    const IID_IMMDEVICE_ENUMERATOR: Guid = Guid {
        data1: 0xA956_64D2,
        data2: 0x9614,
        data3: 0x4F35,
        data4: [0xA7, 0x46, 0xDE, 0x8D, 0xB6, 0x36, 0x17, 0xE6],
    };
    const IID_IAUDIOCLIENT: Guid = Guid {
        data1: 0x1CB9_AD4C,
        data2: 0xDBFA,
        data3: 0x4C32,
        data4: [0xB1, 0x78, 0xC2, 0xF5, 0x68, 0xA7, 0x03, 0xB2],
    };
    const IID_IAUDIOCAPTURECLIENT: Guid = Guid {
        data1: 0xC8AD_BD64,
        data2: 0xE71E,
        data3: 0x48A0,
        data4: [0xA4, 0xDE, 0x18, 0x5C, 0x39, 0x5C, 0xD3, 0x17],
    };

    #[repr(C, packed)]
    #[derive(Clone, Copy)]
    struct WaveFormatEx {
        w_format_tag: u16,
        n_channels: u16,
        n_samples_per_sec: u32,
        n_avg_bytes_per_sec: u32,
        n_block_align: u16,
        w_bits_per_sample: u16,
        cb_size: u16,
    }

    #[repr(C, packed)]
    #[derive(Clone, Copy)]
    struct WaveFormatExtensible {
        format: WaveFormatEx,
        samples_union: u16,
        dw_channel_mask: u32,
        sub_format: Guid,
    }

    #[repr(C)]
    struct IMMDeviceEnumeratorVtbl {
        query_interface: usize,
        add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        enum_audio_endpoints: usize,
        get_default_audio_endpoint:
            unsafe extern "system" fn(*mut c_void, u32, u32, *mut *mut c_void) -> Hresult,
    }

    #[repr(C)]
    struct IMMDeviceVtbl {
        query_interface: usize,
        add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        activate: unsafe extern "system" fn(
            *mut c_void,
            *const Guid,
            u32,
            *const c_void,
            *mut *mut c_void,
        ) -> Hresult,
    }

    #[repr(C)]
    struct IAudioClientVtbl {
        query_interface: usize,
        add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        initialize: unsafe extern "system" fn(
            *mut c_void,
            u32,
            u32,
            i64,
            i64,
            *const WaveFormatEx,
            *const Guid,
        ) -> Hresult,
        get_buffer_size: usize,
        get_stream_latency: usize,
        get_current_padding: usize,
        is_format_supported: unsafe extern "system" fn(
            *mut c_void,
            u32,
            *const WaveFormatEx,
            *mut *mut WaveFormatEx,
        ) -> Hresult,
        get_mix_format: unsafe extern "system" fn(*mut c_void, *mut *mut WaveFormatEx) -> Hresult,
        get_device_period: usize,
        start: unsafe extern "system" fn(*mut c_void) -> Hresult,
        stop: unsafe extern "system" fn(*mut c_void) -> Hresult,
        reset: usize,
        set_event_handle: usize,
        get_service:
            unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> Hresult,
    }

    #[repr(C)]
    struct IAudioCaptureClientVtbl {
        query_interface: usize,
        add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        get_buffer: unsafe extern "system" fn(
            *mut c_void,
            *mut *mut u8,
            *mut u32,
            *mut u32,
            *mut u64,
            *mut u64,
        ) -> Hresult,
        release_buffer: unsafe extern "system" fn(*mut c_void, u32) -> Hresult,
        get_next_packet_size: unsafe extern "system" fn(*mut c_void, *mut u32) -> Hresult,
    }

    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(pv_reserved: *mut c_void, dw_co_init: u32) -> Hresult;
        fn CoUninitialize();
        fn CoCreateInstance(
            rclsid: *const Guid,
            p_unk_outer: *mut c_void,
            dw_cls_context: u32,
            riid: *const Guid,
            ppv: *mut *mut c_void,
        ) -> Hresult;
        fn CoTaskMemFree(pv: *mut c_void);
    }

    #[link(name = "mmdevapi")]
    unsafe extern "system" {
        fn ActivateAudioInterfaceAsync(
            device_interface_path: *const u16,
            riid: *const Guid,
            activation_params: *const PropVariantBlob,
            completion_handler: *mut c_void,
            activation_operation: *mut *mut c_void,
        ) -> Hresult;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcessId() -> u32;
        fn CreateEventW(
            lp_event_attributes: *mut c_void,
            b_manual_reset: i32,
            b_initial_state: i32,
            lp_name: *const u16,
        ) -> *mut c_void;
        fn SetEvent(h_event: *mut c_void) -> i32;
        fn WaitForSingleObject(h_handle: *mut c_void, dw_milliseconds: u32) -> u32;
        fn CloseHandle(h_object: *mut c_void) -> i32;
    }

    #[repr(C)]
    struct PropVariantBlob {
        vt: u16,
        reserved1: u16,
        reserved2: u16,
        reserved3: u16,
        cb_size: u32,
        _pad: u32,
        p_blob_data: *mut u8,
    }

    #[repr(C)]
    struct AudioClientProcessLoopbackParams {
        target_process_id: u32,
        process_loopback_mode: u32,
    }

    #[repr(C)]
    struct AudioClientActivationParams {
        activation_type: u32,
        process_loopback_params: AudioClientProcessLoopbackParams,
    }

    #[repr(C)]
    struct IActivateAudioInterfaceAsyncOperationVtbl {
        query_interface: usize,
        add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        get_activate_result:
            unsafe extern "system" fn(*mut c_void, *mut Hresult, *mut *mut c_void) -> Hresult,
    }

    #[repr(C)]
    struct CompletionHandlerVtbl {
        query_interface:
            unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> Hresult,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        activate_completed: unsafe extern "system" fn(*mut c_void, *mut c_void) -> Hresult,
    }

    #[repr(C)]
    struct CompletionHandler {
        vtbl: *const CompletionHandlerVtbl,
        ref_count: std::sync::atomic::AtomicU32,
        event: *mut c_void,
    }

    const COMPLETION_VTBL: CompletionHandlerVtbl = CompletionHandlerVtbl {
        query_interface: ch_query_interface,
        add_ref: ch_add_ref,
        release: ch_release,
        activate_completed: ch_activate_completed,
    };

    fn guid_eq(a: &Guid, b: &Guid) -> bool {
        a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
    }

    unsafe extern "system" fn ch_query_interface(
        this: *mut c_void,
        riid: *const Guid,
        ppv: *mut *mut c_void,
    ) -> Hresult {
        if ppv.is_null() || riid.is_null() {
            return 0x8000_4003_u32 as i32;
        }
        let id = unsafe { &*riid };
        if guid_eq(id, &IID_IUNKNOWN)
            || guid_eq(id, &IID_IACTIVATE_AUDIO_INTERFACE_COMPLETION_HANDLER)
            || guid_eq(id, &IID_IAGILE_OBJECT)
        {
            unsafe { *ppv = this };
            let _ = unsafe { ch_add_ref(this) };
            0
        } else {
            unsafe { *ppv = std::ptr::null_mut() };
            0x8000_4002_u32 as i32
        }
    }

    unsafe extern "system" fn ch_add_ref(this: *mut c_void) -> u32 {
        let obj = unsafe { &*(this as *const CompletionHandler) };
        obj.ref_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    unsafe extern "system" fn ch_release(this: *mut c_void) -> u32 {
        let obj = unsafe { &*(this as *const CompletionHandler) };
        let prev = obj.ref_count.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            drop(unsafe { Box::from_raw(this as *mut CompletionHandler) });
        }
        prev.saturating_sub(1)
    }

    unsafe extern "system" fn ch_activate_completed(
        this: *mut c_void,
        _op: *mut c_void,
    ) -> Hresult {
        let obj = unsafe { &*(this as *const CompletionHandler) };
        if !obj.event.is_null() {
            unsafe { SetEvent(obj.event) };
        }
        0
    }

    unsafe fn release_com(ptr: *mut c_void) {
        if !ptr.is_null() {
            let vtbl = unsafe { &*(*(ptr as *const *const IAudioCaptureClientVtbl)) };
            unsafe { (vtbl.release)(ptr) };
        }
    }

    struct CaptureSession {
        audio_client: *mut c_void,
        capture_client: *mut c_void,
        channels: usize,
        sample_rate: u32,
        bits_per_sample: u16,
        is_float: bool,
    }

    impl Drop for CaptureSession {
        fn drop(&mut self) {
            unsafe {
                if !self.audio_client.is_null() {
                    let vtbl = &*(*(self.audio_client as *const *const IAudioClientVtbl));
                    let _ = (vtbl.stop)(self.audio_client);
                }
                release_com(self.capture_client);
                release_com(self.audio_client);
            }
        }
    }

    unsafe fn inspect_wave_format(pwfx: *const WaveFormatEx) -> (usize, u32, u16, bool) {
        let wfx = unsafe { *pwfx };
        let is_float = if wfx.w_format_tag == WAVE_FORMAT_IEEE_FLOAT {
            true
        } else if wfx.w_format_tag == WAVE_FORMAT_EXTENSIBLE && wfx.cb_size >= 22 {
            let ext = unsafe { &*(pwfx as *const WaveFormatExtensible) };
            ext.sub_format.data1 == WAVE_FORMAT_IEEE_FLOAT as u32
        } else {
            wfx.w_format_tag != WAVE_FORMAT_PCM && wfx.w_bits_per_sample == 32
        };
        (
            (wfx.n_channels as usize).max(1),
            wfx.n_samples_per_sec.max(8000),
            wfx.w_bits_per_sample,
            is_float,
        )
    }

    unsafe fn try_init_on_device(
        device: *mut c_void,
        flags: u32,
        dur_hns: i64,
        pwfx: *const WaveFormatEx,
    ) -> Result<CaptureSession, Hresult> {
        let mut audio_client: *mut c_void = std::ptr::null_mut();
        let dvtbl = unsafe { &*(*(device as *const *const IMMDeviceVtbl)) };
        let hr_act = unsafe {
            (dvtbl.activate)(
                device,
                &IID_IAUDIOCLIENT,
                CLSCTX_ALL,
                std::ptr::null(),
                &mut audio_client,
            )
        };
        if hr_act < 0 || audio_client.is_null() {
            return Err(hr_act);
        }
        let avtbl = unsafe { &*(*(audio_client as *const *const IAudioClientVtbl)) };
        let hr_init = unsafe {
            (avtbl.initialize)(
                audio_client,
                AUDCLNT_SHAREMODE_SHARED,
                flags,
                dur_hns,
                0,
                pwfx,
                std::ptr::null(),
            )
        };
        if hr_init < 0 {
            unsafe { release_com(audio_client) };
            return Err(hr_init);
        }
        let mut capture_client: *mut c_void = std::ptr::null_mut();
        let hr_srv = unsafe {
            (avtbl.get_service)(audio_client, &IID_IAUDIOCAPTURECLIENT, &mut capture_client)
        };
        if hr_srv < 0 || capture_client.is_null() {
            unsafe { release_com(audio_client) };
            return Err(hr_srv);
        }
        let hr_start = unsafe { (avtbl.start)(audio_client) };
        if hr_start < 0 {
            unsafe {
                release_com(capture_client);
                release_com(audio_client);
            }
            return Err(hr_start);
        }
        let (channels, sample_rate, bits_per_sample, is_float) =
            unsafe { inspect_wave_format(pwfx) };
        Ok(CaptureSession {
            audio_client,
            capture_client,
            channels,
            sample_rate,
            bits_per_sample,
            is_float,
        })
    }

    fn open_process_loopback() -> Result<CaptureSession, Hresult> {
        unsafe {
            let ev = CreateEventW(std::ptr::null_mut(), 0, 0, std::ptr::null());
            if ev.is_null() {
                return Err(0x8007_000E_u32 as i32);
            }
            let handler = Box::into_raw(Box::new(CompletionHandler {
                vtbl: &COMPLETION_VTBL,
                ref_count: std::sync::atomic::AtomicU32::new(1),
                event: ev,
            }));
            let mut params = AudioClientActivationParams {
                activation_type: 1, // AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK
                process_loopback_params: AudioClientProcessLoopbackParams {
                    target_process_id: GetCurrentProcessId(),
                    process_loopback_mode: 0, // PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE
                },
            };
            let pv = PropVariantBlob {
                vt: 65, // VT_BLOB
                reserved1: 0,
                reserved2: 0,
                reserved3: 0,
                cb_size: std::mem::size_of::<AudioClientActivationParams>() as u32,
                _pad: 0,
                p_blob_data: (&mut params as *mut AudioClientActivationParams) as *mut u8,
            };
            let path: Vec<u16> = "VAD\\Process_Loopback\0".encode_utf16().collect();
            let mut async_op: *mut c_void = std::ptr::null_mut();
            let hr_async = ActivateAudioInterfaceAsync(
                path.as_ptr(),
                &IID_IAUDIOCLIENT,
                &pv,
                handler as *mut c_void,
                &mut async_op,
            );
            if hr_async < 0 || async_op.is_null() {
                ch_release(handler as *mut c_void);
                CloseHandle(ev);
                return Err(hr_async);
            }
            WaitForSingleObject(ev, 3000);

            let op_vtbl = &*(*(async_op as *const *const IActivateAudioInterfaceAsyncOperationVtbl));
            let mut hr_act: Hresult = 0;
            let mut audio_client: *mut c_void = std::ptr::null_mut();
            let hr_res = (op_vtbl.get_activate_result)(async_op, &mut hr_act, &mut audio_client);
            (op_vtbl.release)(async_op);
            ch_release(handler as *mut c_void);
            CloseHandle(ev);

            if hr_res < 0 {
                return Err(hr_res);
            }
            if hr_act < 0 || audio_client.is_null() {
                return Err(hr_act);
            }

            let stereo_f32 = WaveFormatEx {
                w_format_tag: WAVE_FORMAT_IEEE_FLOAT,
                n_channels: 2,
                n_samples_per_sec: 48_000,
                n_avg_bytes_per_sec: 48_000 * 8,
                n_block_align: 8,
                w_bits_per_sample: 32,
                cb_size: 0,
            };
            let stereo_pcm16 = WaveFormatEx {
                w_format_tag: WAVE_FORMAT_PCM,
                n_channels: 2,
                n_samples_per_sec: 44_100,
                n_avg_bytes_per_sec: 44_100 * 4,
                n_block_align: 4,
                w_bits_per_sample: 16,
                cb_size: 0,
            };
            let avtbl = &*(*(audio_client as *const *const IAudioClientVtbl));
            let mut chosen = stereo_f32;
            let mut hr_init = (avtbl.initialize)(
                audio_client,
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                0,
                0,
                &chosen,
                std::ptr::null(),
            );
            if hr_init < 0 {
                chosen = stereo_pcm16;
                hr_init = (avtbl.initialize)(
                    audio_client,
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_LOOPBACK,
                    0,
                    0,
                    &chosen,
                    std::ptr::null(),
                );
            }
            if hr_init < 0 {
                release_com(audio_client);
                return Err(hr_init);
            }
            let mut capture_client: *mut c_void = std::ptr::null_mut();
            let hr_srv = (avtbl.get_service)(
                audio_client,
                &IID_IAUDIOCAPTURECLIENT,
                &mut capture_client,
            );
            if hr_srv < 0 || capture_client.is_null() {
                release_com(audio_client);
                return Err(hr_srv);
            }
            let hr_start = (avtbl.start)(audio_client);
            if hr_start < 0 {
                release_com(capture_client);
                release_com(audio_client);
                return Err(hr_start);
            }
            let (channels, sample_rate, bits_per_sample, is_float) =
                inspect_wave_format(&chosen);
            Ok(CaptureSession {
                audio_client,
                capture_client,
                channels,
                sample_rate,
                bits_per_sample,
                is_float,
            })
        }
    }

    fn open_loopback(logged_err: &mut bool) -> Option<CaptureSession> {
        let proc_err = match open_process_loopback() {
            Ok(sess) => {
                *logged_err = false;
                return Some(sess);
            }
            Err(hr) => hr,
        };
        unsafe {
            let mut enumerator: *mut c_void = std::ptr::null_mut();
            if CoCreateInstance(
                &CLSID_MMDEVICE_ENUMERATOR,
                std::ptr::null_mut(),
                CLSCTX_ALL,
                &IID_IMMDEVICE_ENUMERATOR,
                &mut enumerator,
            ) < 0
                || enumerator.is_null()
            {
                return None;
            }
            let evtbl = &*(*(enumerator as *const *const IMMDeviceEnumeratorVtbl));

            let mut last_hr: Hresult = 0;
            let mut last_meta = (0u16, 0u16, 0u32, 0u16);

            for role in [E_CONSOLE, E_MULTIMEDIA] {
                let mut device: *mut c_void = std::ptr::null_mut();
                let hr_dev =
                    (evtbl.get_default_audio_endpoint)(enumerator, E_RENDER, role, &mut device);
                if hr_dev < 0 || device.is_null() {
                    last_hr = hr_dev;
                    continue;
                }

                // Read native mix format from an initial IAudioClient
                let mut probe_client: *mut c_void = std::ptr::null_mut();
                let dvtbl = &*(*(device as *const *const IMMDeviceVtbl));
                let mut pwfx: *mut WaveFormatEx = std::ptr::null_mut();
                if (dvtbl.activate)(
                    device,
                    &IID_IAUDIOCLIENT,
                    CLSCTX_ALL,
                    std::ptr::null(),
                    &mut probe_client,
                ) >= 0
                    && !probe_client.is_null()
                {
                    let pvtbl = &*(*(probe_client as *const *const IAudioClientVtbl));
                    let _ = (pvtbl.get_mix_format)(probe_client, &mut pwfx);
                }

                let mix_copy = if !pwfx.is_null() { Some(*pwfx) } else { None };
                let mix_sr = mix_copy.map_or(48_000, |w| w.n_samples_per_sec.max(8000));
                if let Some(w) = mix_copy {
                    last_meta = (
                        w.w_format_tag,
                        w.n_channels,
                        w.n_samples_per_sec,
                        w.w_bits_per_sample,
                    );
                }

                let stereo_f32 = WaveFormatEx {
                    w_format_tag: WAVE_FORMAT_IEEE_FLOAT,
                    n_channels: 2,
                    n_samples_per_sec: mix_sr,
                    n_avg_bytes_per_sec: mix_sr * 8,
                    n_block_align: 8,
                    w_bits_per_sample: 32,
                    cb_size: 0,
                };
                let stereo_pcm16 = WaveFormatEx {
                    w_format_tag: WAVE_FORMAT_PCM,
                    n_channels: 2,
                    n_samples_per_sec: mix_sr,
                    n_avg_bytes_per_sec: mix_sr * 4,
                    n_block_align: 4,
                    w_bits_per_sample: 16,
                    cb_size: 0,
                };

                let auto_flags = AUDCLNT_STREAMFLAGS_LOOPBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;

                // Try exact mix format first, then IsFormatSupported closest match, then Stereo fallbacks
                let mut candidates: Vec<(u32, i64, *const WaveFormatEx)> = Vec::new();
                if !pwfx.is_null() {
                    candidates.push((AUDCLNT_STREAMFLAGS_LOOPBACK, 0, pwfx));
                    candidates.push((AUDCLNT_STREAMFLAGS_LOOPBACK, 400_000, pwfx));
                }

                let mut closest: *mut WaveFormatEx = std::ptr::null_mut();
                if !probe_client.is_null() {
                    let pvtbl = &*(*(probe_client as *const *const IAudioClientVtbl));
                    let _ = (pvtbl.is_format_supported)(
                        probe_client,
                        AUDCLNT_SHAREMODE_SHARED,
                        &stereo_f32,
                        &mut closest,
                    );
                    if !closest.is_null() {
                        candidates.push((AUDCLNT_STREAMFLAGS_LOOPBACK, 0, closest));
                    }
                    release_com(probe_client);
                }

                candidates.push((auto_flags, 0, &stereo_f32));
                candidates.push((AUDCLNT_STREAMFLAGS_LOOPBACK, 0, &stereo_f32));
                candidates.push((auto_flags, 0, &stereo_pcm16));
                candidates.push((AUDCLNT_STREAMFLAGS_LOOPBACK, 0, &stereo_pcm16));

                let mut found: Option<CaptureSession> = None;
                for (flags, dur, fmt) in candidates {
                    match try_init_on_device(device, flags, dur, fmt) {
                        Ok(sess) => {
                            found = Some(sess);
                            break;
                        }
                        Err(hr) => last_hr = hr,
                    }
                }

                if !closest.is_null() {
                    CoTaskMemFree(closest as *mut c_void);
                }
                if !pwfx.is_null() {
                    CoTaskMemFree(pwfx as *mut c_void);
                }
                (dvtbl.release)(device);

                if let Some(sess) = found {
                    (evtbl.release)(enumerator);
                    *logged_err = false;
                    return Some(sess);
                }
            }

            (evtbl.release)(enumerator);
            if !*logged_err {
                *logged_err = true;
                log::warn!(
                    "WASAPI loopback unavailable: proc_hr=0x{:08x}, dev_hr=0x{:08x} mix(tag=0x{:x}, ch={}, hz={}, bits={})",
                    proc_err as u32,
                    last_hr as u32,
                    last_meta.0,
                    last_meta.1,
                    last_meta.2,
                    last_meta.3
                );
            }
            None
        }
    }

    pub(super) fn run_loopback(shared: Arc<SharedSpectrum>) {
        let co_hr = unsafe { CoInitializeEx(std::ptr::null_mut(), COINIT_MULTITHREADED) };
        let mut session: Option<CaptureSession> = None;
        let mut ring = [0.0_f32; FFT_SIZE];
        let mut ring_pos = 0usize;
        let mut smooth = [0.0_f32; NUM_BANDS];
        let mut smooth_wave = [0.0_f32; NUM_BANDS];
        let mut agc_peak_db = -30.0_f32;
        let mut logged_err = false;

        while !shared.stop.load(Ordering::Relaxed) {
            if !shared.active.load(Ordering::Relaxed) {
                if session.take().is_some() {
                    smooth = [0.0; NUM_BANDS];
                    smooth_wave = [0.0; NUM_BANDS];
                    agc_peak_db = -30.0;
                    if let Ok(mut g) = shared.bands.lock() {
                        *g = smooth;
                    }
                    if let Ok(mut w) = shared.wave.lock() {
                        *w = smooth_wave;
                    }
                }
                logged_err = false;
                if let Ok(mu) = shared.wake_mu.lock() {
                    let _ = shared.wake.wait_timeout(mu, Duration::from_millis(200));
                }
                continue;
            }

            if shared.reopen.swap(false, Ordering::SeqCst) || session.is_none() {
                session = open_loopback(&mut logged_err);
                if session.is_none() {
                    std::thread::sleep(Duration::from_millis(1200));
                    continue;
                }
            }

            let mut got_audio = false;
            let mut failed = false;
            if let Some(sess) = session.as_ref() {
                unsafe {
                    let cvtbl =
                        &*(*(sess.capture_client as *const *const IAudioCaptureClientVtbl));
                    loop {
                        let mut packet_frames = 0u32;
                        let hr_pkt =
                            (cvtbl.get_next_packet_size)(sess.capture_client, &mut packet_frames);
                        if hr_pkt < 0 {
                            failed = true;
                            break;
                        }
                        if packet_frames == 0 {
                            break;
                        }
                        let mut data_ptr: *mut u8 = std::ptr::null_mut();
                        let mut num_frames = 0u32;
                        let mut flags = 0u32;
                        let hr_buf = (cvtbl.get_buffer)(
                            sess.capture_client,
                            &mut data_ptr,
                            &mut num_frames,
                            &mut flags,
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                        );
                        if hr_buf < 0 {
                            failed = true;
                            break;
                        }
                        got_audio = true;
                        if (flags & AUDCLNT_BUFFERFLAGS_SILENT) != 0 || data_ptr.is_null() {
                            for _ in 0..num_frames {
                                ring[ring_pos] = 0.0;
                                ring_pos = (ring_pos + 1) % FFT_SIZE;
                            }
                        } else if sess.is_float && sess.bits_per_sample == 32 {
                            let total = num_frames as usize * sess.channels;
                            let floats = std::slice::from_raw_parts(data_ptr as *const f32, total);
                            for frame in floats.chunks_exact(sess.channels) {
                                let sum: f32 = frame.iter().copied().sum();
                                ring[ring_pos] = sum / sess.channels as f32;
                                ring_pos = (ring_pos + 1) % FFT_SIZE;
                            }
                        } else if !sess.is_float && sess.bits_per_sample == 32 {
                            let total = num_frames as usize * sess.channels;
                            let ints = std::slice::from_raw_parts(data_ptr as *const i32, total);
                            for frame in ints.chunks_exact(sess.channels) {
                                let sum: f32 =
                                    frame.iter().map(|&s| s as f32 / i32::MAX as f32).sum();
                                ring[ring_pos] = sum / sess.channels as f32;
                                ring_pos = (ring_pos + 1) % FFT_SIZE;
                            }
                        } else if sess.bits_per_sample == 16 {
                            let total = num_frames as usize * sess.channels;
                            let ints = std::slice::from_raw_parts(data_ptr as *const i16, total);
                            for frame in ints.chunks_exact(sess.channels) {
                                let sum: f32 = frame.iter().map(|&s| s as f32 / 32768.0).sum();
                                ring[ring_pos] = sum / sess.channels as f32;
                                ring_pos = (ring_pos + 1) % FFT_SIZE;
                            }
                        }
                        let _ = (cvtbl.release_buffer)(sess.capture_client, num_frames);
                    }
                }
            }

            if failed {
                session = None;
                std::thread::sleep(Duration::from_millis(120));
                continue;
            }

            if got_audio {
                let sr = session.as_ref().map_or(48_000, |s| s.sample_rate);
                let mut ordered = [0.0_f32; FFT_SIZE];
                for i in 0..FFT_SIZE {
                    ordered[i] = ring[(ring_pos + i) % FFT_SIZE];
                }
                let (target_b, target_w) = compute_spectrum_bands(&ordered, sr, &mut agc_peak_db);
                smooth_bands(&mut smooth, &target_b);
                smooth_bands(&mut smooth_wave, &target_w);
            } else {
                for b in &mut smooth {
                    *b *= 0.88;
                }
                for w in &mut smooth_wave {
                    *w *= 0.88;
                }
            }

            if let Ok(mut g) = shared.bands.lock() {
                *g = smooth;
            }
            if let Ok(mut w) = shared.wave.lock() {
                *w = smooth_wave;
            }
            std::thread::sleep(Duration::from_millis(16));
        }

        drop(session);
        if co_hr >= 0 {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_detects_dominant_frequency_band() {
        let sr = 48_000_u32;
        let mut buf = [0.0_f32; FFT_SIZE];
        // Even modest speech/podcast amplitude (-24 dBFS) should produce strong visible bands
        for (i, slot) in buf.iter_mut().enumerate() {
            *slot = 0.06 * (TAU * 1200.0 * i as f32 / sr as f32).sin();
        }
        let mut agc = -30.0_f32;
        let (bands, wave) = compute_spectrum_bands(&buf, sr, &mut agc);
        let peak_idx = bands
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(idx, _)| idx)
            .unwrap();
        assert!(bands[peak_idx] > 0.65);
        assert!(peak_idx > 20 && peak_idx < 48, "unexpected peak band {peak_idx}");
        assert!(bands[0] < bands[peak_idx] * 0.25);
        assert!(wave.iter().any(|&w| w > 0.4));
    }
}
