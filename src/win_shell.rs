//! Native Windows desktop shell integration:
//! - System Tray icon (`Shell_NotifyIconW`) with dynamic playback context menu
//! - Windows Taskbar thumbnail preview toolbar (`ITaskbarList3`: Prev / Play-Pause / Next)
//! - Main window restore helper when waking from tray

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellEvent {
    ShowWindow,
    TogglePlayPause,
    Prev,
    Next,
    ToggleAlwaysOnTop,
    Quit,
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, OnceLock};

    use crossbeam_channel::{Receiver, Sender, unbounded};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::ShellEvent;

    type Hwnd = *mut c_void;
    type Hicon = *mut c_void;
    type Hmenu = *mut c_void;
    type Hbitmap = *mut c_void;
    type Hmodule = *mut c_void;
    type Hresult = i32;

    const WM_NULL: u32 = 0x0000;
    const WM_DESTROY: u32 = 0x0002;
    const WM_CLOSE: u32 = 0x0010;
    const WM_GETICON: u32 = 0x007F;
    const WM_COMMAND: u32 = 0x0111;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_LBUTTONDBLCLK: u32 = 0x0203;
    const WM_RBUTTONUP: u32 = 0x0205;
    const WM_CONTEXTMENU: u32 = 0x007B;
    const WM_APP_TRAY: u32 = 0x8000 + 42;
    const WM_APP_UPDATE_TIP: u32 = 0x8000 + 43;

    const GWLP_WNDPROC: i32 = -4;
    const HWND_MESSAGE: Hwnd = -3isize as Hwnd;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;
    const IDI_APPLICATION: *const u16 = 32512 as *const u16;
    const SW_RESTORE: i32 = 9;

    const NIM_ADD: u32 = 0x0000_0000;
    const NIM_MODIFY: u32 = 0x0000_0001;
    const NIM_DELETE: u32 = 0x0000_0002;
    const NIF_MESSAGE: u32 = 0x0000_0001;
    const NIF_ICON: u32 = 0x0000_0002;
    const NIF_TIP: u32 = 0x0000_0004;

    const MF_STRING: u32 = 0x0000_0000;
    const MF_CHECKED: u32 = 0x0000_0008;
    const MF_SEPARATOR: u32 = 0x0000_0800;
    const TPM_RIGHTALIGN: u32 = 0x0008;
    const TPM_BOTTOMALIGN: u32 = 0x0020;
    const TPM_RETURNCMD: u32 = 0x0100;
    const TPM_NONOTIFY: u32 = 0x0080;

    const MENU_ID_SHOW: usize = 1001;
    const MENU_ID_PLAY: usize = 1002;
    const MENU_ID_PREV: usize = 1003;
    const MENU_ID_NEXT: usize = 1004;
    const MENU_ID_TOP: usize = 1005;
    const MENU_ID_QUIT: usize = 1006;

    const THB_ID_PREV: u32 = 201;
    const THB_ID_PLAY: u32 = 202;
    const THB_ID_NEXT: u32 = 203;
    const THBN_CLICKED: u16 = 0x1800;
    const THB_ICON: i32 = 0x0000_0002;
    const THB_TOOLTIP: i32 = 0x0000_0004;
    const THB_FLAGS: i32 = 0x0000_0008;
    const THBF_ENABLED: i32 = 0x0000_0000;

    const COINIT_APARTMENTTHREADED: u32 = 0x2;
    const CLSCTX_INPROC_SERVER: u32 = 0x1;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    const CLSID_TASKBAR_LIST: Guid = Guid {
        data1: 0x56FD_F344,
        data2: 0xFD6D,
        data3: 0x11D0,
        data4: [0x95, 0x8A, 0x00, 0x60, 0x97, 0xC9, 0xA0, 0x90],
    };
    const IID_ITASKBAR_LIST3: Guid = Guid {
        data1: 0xEA1A_FB91,
        data2: 0x9E28,
        data3: 0x4B86,
        data4: [0x90, 0xE9, 0x9E, 0x9F, 0x8A, 0x5E, 0xEF, 0xAF],
    };

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct Msg {
        hwnd: Hwnd,
        message: u32,
        w_param: usize,
        l_param: isize,
        time: u32,
        pt: Point,
        l_private: u32,
    }

    #[repr(C)]
    struct WndClassW {
        style: u32,
        lpfn_wnd_proc: unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize,
        cb_cls_extra: i32,
        cb_wnd_extra: i32,
        h_instance: Hmodule,
        h_icon: Hicon,
        h_cursor: *mut c_void,
        hbr_background: *mut c_void,
        lpsz_menu_name: *const u16,
        lpsz_class_name: *const u16,
    }

    #[repr(C)]
    union NotifyIconTimeout {
        timeout: u32,
        version: u32,
    }

    #[repr(C)]
    struct NotifyIconDataW {
        cb_size: u32,
        hwnd: Hwnd,
        id: u32,
        flags: u32,
        callback_message: u32,
        icon: Hicon,
        tip: [u16; 128],
        state: u32,
        state_mask: u32,
        info: [u16; 256],
        u: NotifyIconTimeout,
        info_title: [u16; 64],
        info_flags: u32,
        guid_item: Guid,
        balloon_icon: Hicon,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ThumbButton {
        mask: i32,
        id: u32,
        bitmap: u32,
        icon: Hicon,
        tip: [u16; 260],
        flags: i32,
    }

    #[repr(C)]
    struct IconInfo {
        f_icon: i32,
        x_hotspot: u32,
        y_hotspot: u32,
        hbm_mask: Hbitmap,
        hbm_color: Hbitmap,
    }

    #[repr(C)]
    struct ITaskbarList3Vtbl {
        // IUnknown (0..2)
        query_interface: usize,
        add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        // ITaskbarList (3..7)
        hr_init: unsafe extern "system" fn(*mut c_void) -> Hresult,
        add_tab: usize,
        delete_tab: usize,
        activate_tab: usize,
        set_active_alt: usize,
        // ITaskbarList2 (8)
        mark_fullscreen_window: usize,
        // ITaskbarList3 (9..20)
        set_progress_value: usize,
        set_progress_state: usize,
        register_tab: usize,
        unregister_tab: usize,
        set_tab_order: usize,
        set_tab_active: usize,
        thumb_bar_add_buttons:
            unsafe extern "system" fn(*mut c_void, Hwnd, u32, *const ThumbButton) -> Hresult,
        thumb_bar_update_buttons:
            unsafe extern "system" fn(*mut c_void, Hwnd, u32, *const ThumbButton) -> Hresult,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterClassW(wc: *const WndClassW) -> u16;
        fn CreateWindowExW(
            ex_style: u32,
            class_name: *const u16,
            window_name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: Hwnd,
            menu: Hmenu,
            instance: Hmodule,
            param: *mut c_void,
        ) -> Hwnd;
        fn DefWindowProcW(hwnd: Hwnd, msg: u32, wp: usize, lp: isize) -> isize;
        fn PostMessageW(hwnd: Hwnd, msg: u32, wp: usize, lp: isize) -> i32;
        fn SendMessageW(hwnd: Hwnd, msg: u32, wp: usize, lp: isize) -> isize;
        fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min: u32, max: u32) -> i32;
        fn TranslateMessage(msg: *const Msg) -> i32;
        fn DispatchMessageW(msg: *const Msg) -> isize;
        fn PostQuitMessage(exit_code: i32);
        fn DestroyWindow(hwnd: Hwnd) -> i32;
        fn LoadIconW(instance: Hmodule, name: *const u16) -> Hicon;
        fn CreatePopupMenu() -> Hmenu;
        fn AppendMenuW(menu: Hmenu, flags: u32, id: usize, text: *const u16) -> i32;
        fn TrackPopupMenu(
            menu: Hmenu,
            flags: u32,
            x: i32,
            y: i32,
            reserved: i32,
            hwnd: Hwnd,
            rect: *const c_void,
        ) -> i32;
        fn DestroyMenu(menu: Hmenu) -> i32;
        fn GetCursorPos(pt: *mut Point) -> i32;
        fn SetForegroundWindow(hwnd: Hwnd) -> i32;
        fn ShowWindow(hwnd: Hwnd, cmd: i32) -> i32;
        fn CreateIconIndirect(info: *const IconInfo) -> Hicon;
        fn DestroyIcon(icon: Hicon) -> i32;
        fn RegisterWindowMessageW(name: *const u16) -> u32;
        fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, new_long: isize) -> isize;
        fn CallWindowProcW(
            prev: isize,
            hwnd: Hwnd,
            msg: u32,
            wp: usize,
            lp: isize,
        ) -> isize;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(name: *const u16) -> Hmodule;
    }

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn Shell_NotifyIconW(message: u32, data: *const NotifyIconDataW) -> i32;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateBitmap(
            w: i32,
            h: i32,
            planes: u32,
            bpp: u32,
            bits: *const c_void,
        ) -> Hbitmap;
        fn DeleteObject(obj: *mut c_void) -> i32;
    }

    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(reserved: *mut c_void, coinit: u32) -> Hresult;
        fn CoCreateInstance(
            rclsid: *const Guid,
            outer: *mut c_void,
            clsctx: u32,
            riid: *const Guid,
            ppv: *mut *mut c_void,
        ) -> Hresult;
    }

    fn wide_z(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn copy_wide_fixed<const N: usize>(s: &str, dst: &mut [u16; N]) {
        dst.fill(0);
        for (slot, ch) in dst.iter_mut().take(N.saturating_sub(1)).zip(s.encode_utf16()) {
            *slot = ch;
        }
    }

    #[derive(Clone, Copy)]
    enum GlyphKind {
        Prev,
        Play,
        Pause,
        Next,
    }

    fn create_glyph_icon(kind: GlyphKind) -> Hicon {
        let mut pixels = [0u32; 16 * 16];
        let white = 0xFFFF_FFFFu32;
        let mut plot = |x: i32, y: i32| {
            if (0..16).contains(&x) && (0..16).contains(&y) {
                pixels[(y as usize) * 16 + (x as usize)] = white;
            }
        };
        match kind {
            GlyphKind::Play => {
                for y in 2i32..=13 {
                    let dist = (y - 7).abs().min((y - 8).abs());
                    let max_x = 12 - dist;
                    for x in 4..=max_x {
                        plot(x, y);
                    }
                }
            }
            GlyphKind::Pause => {
                for y in 2i32..=13 {
                    for x in 3..=5 {
                        plot(x, y);
                    }
                    for x in 10..=12 {
                        plot(x, y);
                    }
                }
            }
            GlyphKind::Next => {
                for y in 3i32..=12 {
                    let dist = (y - 7).abs().min((y - 8).abs());
                    let max_x = 9 - dist;
                    for x in 2..=max_x {
                        plot(x, y);
                    }
                    for x in 11..=13 {
                        plot(x, y);
                    }
                }
            }
            GlyphKind::Prev => {
                for y in 3i32..=12 {
                    let dist = (y - 7).abs().min((y - 8).abs());
                    let min_x = 6 + dist;
                    for x in 2..=4 {
                        plot(x, y);
                    }
                    for x in min_x..=13 {
                        plot(x, y);
                    }
                }
            }
        }
        let mask = [0u8; 32];
        unsafe {
            let hbm_color = CreateBitmap(16, 16, 1, 32, pixels.as_ptr() as *const c_void);
            let hbm_mask = CreateBitmap(16, 16, 1, 1, mask.as_ptr() as *const c_void);
            let info = IconInfo {
                f_icon: 1,
                x_hotspot: 0,
                y_hotspot: 0,
                hbm_mask,
                hbm_color,
            };
            let hicon = CreateIconIndirect(&info);
            if !hbm_color.is_null() {
                DeleteObject(hbm_color);
            }
            if !hbm_mask.is_null() {
                DeleteObject(hbm_mask);
            }
            hicon
        }
    }

    struct SharedTrayState {
        tx: Sender<ShellEvent>,
        wake: Arc<dyn Fn() + Send + Sync>,
        is_paused: AtomicBool,
        is_visible: AtomicBool,
        always_on_top: AtomicBool,
        tip: std::sync::Mutex<String>,
        icon: usize,
    }

    static TRAY_STATE: OnceLock<Arc<SharedTrayState>> = OnceLock::new();
    static HOOK_STATE: OnceLock<HookState> = OnceLock::new();

    struct HookState {
        old_proc: isize,
        taskbar_created_msg: u32,
        buttons_need_init: AtomicBool,
        tx: Sender<ShellEvent>,
        wake: Arc<dyn Fn() + Send + Sync>,
    }

    unsafe extern "system" fn main_wnd_hook(
        hwnd: Hwnd,
        msg: u32,
        wp: usize,
        lp: isize,
    ) -> isize {
        if let Some(st) = HOOK_STATE.get() {
            if msg == st.taskbar_created_msg && st.taskbar_created_msg != 0 {
                st.buttons_need_init.store(true, Ordering::Relaxed);
                (st.wake)();
            } else if msg == WM_COMMAND {
                let code = ((wp >> 16) & 0xFFFF) as u16;
                let id = (wp & 0xFFFF) as u32;
                if code == THBN_CLICKED {
                    let ev = match id {
                        THB_ID_PREV => Some(ShellEvent::Prev),
                        THB_ID_PLAY => Some(ShellEvent::TogglePlayPause),
                        THB_ID_NEXT => Some(ShellEvent::Next),
                        _ => None,
                    };
                    if let Some(e) = ev {
                        let _ = st.tx.send(e);
                        (st.wake)();
                        return 0;
                    }
                }
            }
            return unsafe { CallWindowProcW(st.old_proc, hwnd, msg, wp, lp) };
        }
        unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
    }

    unsafe extern "system" fn tray_wnd_proc(
        hwnd: Hwnd,
        msg: u32,
        wp: usize,
        lp: isize,
    ) -> isize {
        match msg {
            WM_APP_TRAY => {
                let event = (lp & 0xFFFF) as u32;
                match event {
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                        if let Some(st) = TRAY_STATE.get() {
                            let _ = st.tx.send(ShellEvent::ShowWindow);
                            (st.wake)();
                        }
                    }
                    WM_RBUTTONUP | WM_CONTEXTMENU => unsafe {
                        if let Some(st) = TRAY_STATE.get() {
                            show_tray_context_menu(hwnd, st);
                        }
                    },
                    _ => {}
                }
                0
            }
            WM_APP_UPDATE_TIP => {
                if let Some(st) = TRAY_STATE.get() {
                    let tip_text = st.tip.lock().map(|g| g.clone()).unwrap_or_default();
                    let mut nid = make_nid(hwnd, st.icon as Hicon);
                    nid.flags = NIF_TIP;
                    copy_wide_fixed(&tip_text, &mut nid.tip);
                    unsafe {
                        Shell_NotifyIconW(NIM_MODIFY, &nid);
                    }
                }
                0
            }
            WM_CLOSE => unsafe {
                if let Some(st) = TRAY_STATE.get() {
                    let nid = make_nid(hwnd, st.icon as Hicon);
                    Shell_NotifyIconW(NIM_DELETE, &nid);
                }
                DestroyWindow(hwnd);
                0
            },
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
        }
    }

    fn make_nid(hwnd: Hwnd, icon: Hicon) -> NotifyIconDataW {
        NotifyIconDataW {
            cb_size: std::mem::size_of::<NotifyIconDataW>() as u32,
            hwnd,
            id: 1,
            flags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            callback_message: WM_APP_TRAY,
            icon,
            tip: [0; 128],
            state: 0,
            state_mask: 0,
            info: [0; 256],
            u: NotifyIconTimeout { version: 4 },
            info_title: [0; 64],
            info_flags: 0,
            guid_item: Guid {
                data1: 0,
                data2: 0,
                data3: 0,
                data4: [0; 8],
            },
            balloon_icon: std::ptr::null_mut(),
        }
    }

    unsafe fn show_tray_context_menu(hwnd: Hwnd, st: &SharedTrayState) {
        unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() {
                return;
            }
            let show_label = if st.is_visible.load(Ordering::Relaxed) {
                wide_z("Hide to tray")
            } else {
                wide_z("Show YouTube")
            };
            let play_label = if st.is_paused.load(Ordering::Relaxed) {
                wide_z("Play")
            } else {
                wide_z("Pause")
            };
            let prev_label = wide_z("Previous");
            let next_label = wide_z("Next");
            let top_label = wide_z("Keep on Top");
            let quit_label = wide_z("Quit");

            AppendMenuW(menu, MF_STRING, MENU_ID_SHOW, show_label.as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, MENU_ID_PLAY, play_label.as_ptr());
            AppendMenuW(menu, MF_STRING, MENU_ID_PREV, prev_label.as_ptr());
            AppendMenuW(menu, MF_STRING, MENU_ID_NEXT, next_label.as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            let top_flags = if st.always_on_top.load(Ordering::Relaxed) {
                MF_STRING | MF_CHECKED
            } else {
                MF_STRING
            };
            AppendMenuW(menu, top_flags, MENU_ID_TOP, top_label.as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, MENU_ID_QUIT, quit_label.as_ptr());

            let mut pt = Point::default();
            GetCursorPos(&mut pt);
            SetForegroundWindow(hwnd);
            let cmd = TrackPopupMenu(
                menu,
                TPM_RIGHTALIGN | TPM_BOTTOMALIGN | TPM_RETURNCMD | TPM_NONOTIFY,
                pt.x,
                pt.y,
                0,
                hwnd,
                std::ptr::null(),
            );
            PostMessageW(hwnd, WM_NULL, 0, 0);
            DestroyMenu(menu);

            let ev = match cmd as usize {
                MENU_ID_SHOW => Some(ShellEvent::ShowWindow),
                MENU_ID_PLAY => Some(ShellEvent::TogglePlayPause),
                MENU_ID_PREV => Some(ShellEvent::Prev),
                MENU_ID_NEXT => Some(ShellEvent::Next),
                MENU_ID_TOP => Some(ShellEvent::ToggleAlwaysOnTop),
                MENU_ID_QUIT => Some(ShellEvent::Quit),
                _ => None,
            };
            if let Some(e) = ev {
                let _ = st.tx.send(e);
                (st.wake)();
            }
        }
    }

    pub struct WinShell {
        app_hwnd: Hwnd,
        tray_hwnd: Arc<std::sync::atomic::AtomicUsize>,
        shared: Arc<SharedTrayState>,
        rx: Receiver<ShellEvent>,
        taskbar: *mut c_void,
        buttons_added: bool,
        last_paused: Option<bool>,
        last_tip: String,
        icon_prev: Hicon,
        icon_play: Hicon,
        icon_pause: Hicon,
        icon_next: Hicon,
    }

    impl WinShell {
        pub fn new(
            cc: &eframe::CreationContext<'_>,
            wake: Arc<dyn Fn() + Send + Sync>,
        ) -> Option<Self> {
            let app_hwnd = cc.window_handle().ok().and_then(|h| match h.as_raw() {
                RawWindowHandle::Win32(w) => Some(w.hwnd.get() as Hwnd),
                _ => None,
            })?;

            let mut hicon = unsafe { SendMessageW(app_hwnd, WM_GETICON, ICON_SMALL, 0) as Hicon };
            if hicon.is_null() {
                hicon = unsafe { SendMessageW(app_hwnd, WM_GETICON, ICON_BIG, 0) as Hicon };
            }
            if hicon.is_null() {
                hicon = unsafe { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) };
            }

            let (tx, rx) = unbounded();
            let shared = Arc::new(SharedTrayState {
                tx: tx.clone(),
                wake: wake.clone(),
                is_paused: AtomicBool::new(true),
                is_visible: AtomicBool::new(true),
                always_on_top: AtomicBool::new(false),
                tip: std::sync::Mutex::new("YouTube — yt-rust".to_owned()),
                icon: hicon as usize,
            });
            let _ = TRAY_STATE.set(shared.clone());

            let tray_hwnd = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let tray_hwnd_bg = tray_hwnd.clone();
            let icon_usize = hicon as usize;
            let _ = std::thread::Builder::new()
                .name("yt-rust-tray".into())
                .spawn(move || unsafe {
                    let hinst = GetModuleHandleW(std::ptr::null());
                    let cls = wide_z("yt_rust_tray_wnd");
                    let wc = WndClassW {
                        style: 0,
                        lpfn_wnd_proc: tray_wnd_proc,
                        cb_cls_extra: 0,
                        cb_wnd_extra: 0,
                        h_instance: hinst,
                        h_icon: std::ptr::null_mut(),
                        h_cursor: std::ptr::null_mut(),
                        hbr_background: std::ptr::null_mut(),
                        lpsz_menu_name: std::ptr::null(),
                        lpsz_class_name: cls.as_ptr(),
                    };
                    RegisterClassW(&wc);
                    let hwnd = CreateWindowExW(
                        0,
                        cls.as_ptr(),
                        cls.as_ptr(),
                        0,
                        0,
                        0,
                        0,
                        0,
                        HWND_MESSAGE,
                        std::ptr::null_mut(),
                        hinst,
                        std::ptr::null_mut(),
                    );
                    if hwnd.is_null() {
                        return;
                    }
                    tray_hwnd_bg.store(hwnd as usize, Ordering::Release);
                    let mut nid = make_nid(hwnd, icon_usize as Hicon);
                    copy_wide_fixed("YouTube — yt-rust", &mut nid.tip);
                    Shell_NotifyIconW(NIM_ADD, &nid);

                    let mut msg: Msg = std::mem::zeroed();
                    while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                });

            // Taskbar button hook + COM ITaskbarList3 setup on UI thread
            let msg_name = wide_z("TaskbarButtonCreated");
            let tb_msg = unsafe { RegisterWindowMessageW(msg_name.as_ptr()) };
            if HOOK_STATE.get().is_none() {
                let old_proc = unsafe {
                    SetWindowLongPtrW(app_hwnd, GWLP_WNDPROC, main_wnd_hook as *const () as isize)
                };
                if old_proc != 0 {
                    let _ = HOOK_STATE.set(HookState {
                        old_proc,
                        taskbar_created_msg: tb_msg,
                        buttons_need_init: AtomicBool::new(true),
                        tx,
                        wake,
                    });
                }
            }

            let taskbar = unsafe {
                let _ = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
                let mut ptr: *mut c_void = std::ptr::null_mut();
                let hr = CoCreateInstance(
                    &CLSID_TASKBAR_LIST,
                    std::ptr::null_mut(),
                    CLSCTX_INPROC_SERVER,
                    &IID_ITASKBAR_LIST3,
                    &mut ptr,
                );
                if hr >= 0 && !ptr.is_null() {
                    let vtbl = &*(*(ptr as *const *const ITaskbarList3Vtbl));
                    if (vtbl.hr_init)(ptr) >= 0 {
                        ptr
                    } else {
                        (vtbl.release)(ptr);
                        std::ptr::null_mut()
                    }
                } else {
                    std::ptr::null_mut()
                }
            };

            let mut shell = Self {
                app_hwnd,
                tray_hwnd,
                shared,
                rx,
                taskbar,
                buttons_added: false,
                last_paused: None,
                last_tip: "YouTube — yt-rust".to_owned(),
                icon_prev: create_glyph_icon(GlyphKind::Prev),
                icon_play: create_glyph_icon(GlyphKind::Play),
                icon_pause: create_glyph_icon(GlyphKind::Pause),
                icon_next: create_glyph_icon(GlyphKind::Next),
            };
            shell.ensure_taskbar_buttons(true);
            Some(shell)
        }

        fn build_buttons(&self, paused: bool) -> [ThumbButton; 3] {
            let mut b0 = ThumbButton {
                mask: THB_ICON | THB_TOOLTIP | THB_FLAGS,
                id: THB_ID_PREV,
                bitmap: 0,
                icon: self.icon_prev,
                tip: [0; 260],
                flags: THBF_ENABLED,
            };
            copy_wide_fixed("Previous", &mut b0.tip);

            let mut b1 = ThumbButton {
                mask: THB_ICON | THB_TOOLTIP | THB_FLAGS,
                id: THB_ID_PLAY,
                bitmap: 0,
                icon: if paused {
                    self.icon_play
                } else {
                    self.icon_pause
                },
                tip: [0; 260],
                flags: THBF_ENABLED,
            };
            copy_wide_fixed(if paused { "Play" } else { "Pause" }, &mut b1.tip);

            let mut b2 = ThumbButton {
                mask: THB_ICON | THB_TOOLTIP | THB_FLAGS,
                id: THB_ID_NEXT,
                bitmap: 0,
                icon: self.icon_next,
                tip: [0; 260],
                flags: THBF_ENABLED,
            };
            copy_wide_fixed("Next", &mut b2.tip);
            [b0, b1, b2]
        }

        fn ensure_taskbar_buttons(&mut self, paused: bool) {
            if self.taskbar.is_null() {
                return;
            }
            if let Some(st) = HOOK_STATE.get() {
                if st.buttons_need_init.swap(false, Ordering::Relaxed) {
                    let buttons = self.build_buttons(paused);
                    unsafe {
                        let vtbl = &*(*(self.taskbar as *const *const ITaskbarList3Vtbl));
                        if (vtbl.thumb_bar_add_buttons)(
                            self.taskbar,
                            self.app_hwnd,
                            3,
                            buttons.as_ptr(),
                        ) >= 0
                        {
                            self.buttons_added = true;
                            self.last_paused = Some(paused);
                        }
                    }
                }
            }
        }

        pub fn try_recv(&self) -> Option<ShellEvent> {
            self.rx.try_recv().ok()
        }

        pub fn sync_state(
            &mut self,
            paused: bool,
            visible: bool,
            always_on_top: bool,
            now_playing_title: Option<&str>,
        ) {
            self.shared.is_paused.store(paused, Ordering::Relaxed);
            self.shared.is_visible.store(visible, Ordering::Relaxed);
            self.shared
                .always_on_top
                .store(always_on_top, Ordering::Relaxed);

            self.ensure_taskbar_buttons(paused);
            if self.buttons_added && !self.taskbar.is_null() && self.last_paused != Some(paused) {
                let buttons = self.build_buttons(paused);
                unsafe {
                    let vtbl = &*(*(self.taskbar as *const *const ITaskbarList3Vtbl));
                    let _ = (vtbl.thumb_bar_update_buttons)(
                        self.taskbar,
                        self.app_hwnd,
                        1,
                        &buttons[1],
                    );
                }
                self.last_paused = Some(paused);
            }

            let target_tip = match now_playing_title {
                Some(t) if !t.is_empty() => format!("{t} — yt-rust"),
                _ => "YouTube — yt-rust".to_owned(),
            };
            if target_tip != self.last_tip {
                self.last_tip = target_tip.clone();
                if let Ok(mut g) = self.shared.tip.lock() {
                    *g = target_tip;
                }
                let hwnd_val = self.tray_hwnd.load(Ordering::Acquire);
                if hwnd_val != 0 {
                    unsafe {
                        PostMessageW(hwnd_val as Hwnd, WM_APP_UPDATE_TIP, 0, 0);
                    }
                }
            }
        }

        pub fn restore_main_window(&self) {
            unsafe {
                ShowWindow(self.app_hwnd, SW_RESTORE);
                SetForegroundWindow(self.app_hwnd);
            }
        }
    }

    impl Drop for WinShell {
        fn drop(&mut self) {
            let hwnd_val = self.tray_hwnd.load(Ordering::Acquire);
            if hwnd_val != 0 {
                unsafe {
                    PostMessageW(hwnd_val as Hwnd, WM_CLOSE, 0, 0);
                }
            }
            unsafe {
                if !self.taskbar.is_null() {
                    let vtbl = &*(*(self.taskbar as *const *const ITaskbarList3Vtbl));
                    (vtbl.release)(self.taskbar);
                }
                for ic in [self.icon_prev, self.icon_play, self.icon_pause, self.icon_next] {
                    if !ic.is_null() {
                        DestroyIcon(ic);
                    }
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::sync::Arc;

    use super::ShellEvent;

    pub struct WinShell;

    impl WinShell {
        pub fn new(
            _cc: &eframe::CreationContext<'_>,
            _wake: Arc<dyn Fn() + Send + Sync>,
        ) -> Option<Self> {
            None
        }

        pub fn try_recv(&self) -> Option<ShellEvent> {
            None
        }

        pub fn sync_state(
            &mut self,
            _paused: bool,
            _visible: bool,
            _always_on_top: bool,
            _now_playing_title: Option<&str>,
        ) {
        }

        pub fn restore_main_window(&self) {}
    }
}

pub use imp::WinShell;
