use crate::lang::LangCtx;
use crate::overlay::OverlayView;

pub const STYLES: &[&str] = &["hud", "classic"];

/// Everything the native overlay needs to render without referencing external state.
#[derive(Debug, Clone)]
pub struct OverlayRenderData {
    pub view: OverlayView,
    pub header: String,
    pub caption_total: String,
    pub last_label: String,
    pub session_label: String,
    pub squad_label: String,
    pub last_match_caption: String,
    pub window_title: String,
}

impl OverlayRenderData {
    pub fn from_view(view: OverlayView, lang: &LangCtx) -> Self {
        Self {
            view,
            header: lang.t("COFFRES", "CHESTS", "COFRES").to_string(),
            caption_total: "total".to_string(),
            last_label: lang.t("Dernière", "Last", "Última").to_string(),
            session_label: lang.t("Session", "Session", "Sesión").to_string(),
            squad_label: crate::overlay::squad_label(lang).to_string(),
            last_match_caption: crate::overlay::last_match_caption(lang).to_string(),
            window_title: lang.t("Coffres", "Chests", "Cofres").to_string(),
        }
    }
}

pub type OnMovedCb = Box<dyn Fn(i32, i32) + Send + Sync>;
pub type OnLockCb = Box<dyn Fn(bool) + Send + Sync>;
pub type OnLogCb = Box<dyn Fn(&str, &str) + Send + Sync>;

#[cfg(target_os = "windows")]
mod win {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::ptr::null_mut;
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
    use std::sync::{Arc, Condvar, Mutex, OnceLock};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use crate::overlay::OverlayAppearance;
    use super::{OnLockCb, OnLogCb, OnMovedCb, OverlayRenderData, STYLES};

    type HWND = *mut c_void;
    type HDC = *mut c_void;
    type HFONT = *mut c_void;
    type HBRUSH = *mut c_void;
    type HPEN = *mut c_void;
    type HRGN = *mut c_void;
    type HCURSOR = *mut c_void;
    type HINSTANCE = *mut c_void;
    type HICON = *mut c_void;
    type HMENU = *mut c_void;
    type HGDIOBJ = *mut c_void;
    type HMONITOR = *mut c_void;

    const WS_POPUP: u32 = 0x8000_0000;
    const WS_EX_TOPMOST: u32 = 0x0000_0008;
    const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
    const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
    const WS_EX_LAYERED: u32 = 0x0008_0000;
    const WS_EX_NOACTIVATE: u32 = 0x0800_0000;

    const GWL_EXSTYLE: i32 = -20;
    const HTCLIENT: isize = 1;
    const HTCAPTION: isize = 2;

    const WM_DESTROY: u32 = 0x0002;
    const WM_PAINT: u32 = 0x000F;
    const WM_ERASEBKGND: u32 = 0x0014;
    const WM_SETCURSOR: u32 = 0x0020;
    const WM_NCHITTEST: u32 = 0x0084;
    const WM_NCRBUTTONUP: u32 = 0x00A5;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_EXITSIZEMOVE: u32 = 0x0232;
    const WM_HOTKEY: u32 = 0x0312;
    const WM_PRINTCLIENT: u32 = 0x0318;

    const WM_APP_TOGGLE: u32 = 0x8001;
    const WM_APP_RELAYOUT: u32 = 0x8002;
    const WM_APP_LOCK: u32 = 0x8003;
    const WM_APP_APPEARANCE: u32 = 0x8004;
    const WM_APP_STOP: u32 = 0x8005;

    const MOD_CONTROL: u32 = 0x0002;
    const MOD_SHIFT: u32 = 0x0004;
    const MOD_NOREPEAT: u32 = 0x4000;
    const VK_O: u32 = 0x4F;
    const VK_L: u32 = 0x4C;

    const LWA_COLORKEY: u32 = 0x0001;
    const LWA_ALPHA: u32 = 0x0002;
    const SW_HIDE: i32 = 0;
    const SW_SHOWNOACTIVATE: i32 = 4;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const MONITOR_DEFAULTTONEAREST: u32 = 2;

    const TRANSPARENT: i32 = 1;
    const LOGPIXELSY: i32 = 90;
    const DT_LEFT: u32 = 0x0000;
    const DT_CENTER: u32 = 0x0001;
    const DT_RIGHT: u32 = 0x0002;
    const DT_VCENTER: u32 = 0x0004;
    const DT_BOTTOM: u32 = 0x0008;
    const DT_SINGLELINE: u32 = 0x0020;
    const DT_NOCLIP: u32 = 0x0100;
    const DT_CALCRECT: u32 = 0x0400;
    const DT_NOPREFIX: u32 = 0x0800;

    const ERROR_CLASS_ALREADY_EXISTS: u32 = 1410;
    const KEY_COLOR: u32 = 0x0001_0001;
    const MIN_WIDTH: i32 = 240;
    const PAD: i32 = 16;
    const COL_GAP: i32 = 36;
    const SEP: &str = "  •  ";

    const HWND_TOPMOST: HWND = (-1isize) as HWND;
    const IDC_SIZEALL: *const u16 = 32646usize as *const u16;
    const IDC_HAND: *const u16 = 32649usize as *const u16;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct POINT {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct MSG {
        hwnd: HWND,
        message: u32,
        w_param: usize,
        l_param: isize,
        time: u32,
        pt: POINT,
    }

    #[repr(C)]
    struct PAINTSTRUCT {
        hdc: HDC,
        f_erase: i32,
        rc_paint: RECT,
        f_restore: i32,
        f_inc_update: i32,
        rgb_reserved: [u8; 32],
    }

    #[repr(C)]
    struct MONITORINFO {
        cb_size: u32,
        rc_monitor: RECT,
        rc_work: RECT,
        dw_flags: u32,
    }

    #[repr(C)]
    struct WNDCLASSEXW {
        cb_size: u32,
        style: u32,
        lpfn_wnd_proc: Option<unsafe extern "system" fn(HWND, u32, usize, isize) -> isize>,
        cb_cls_extra: i32,
        cb_wnd_extra: i32,
        h_instance: HINSTANCE,
        h_icon: HICON,
        h_cursor: HCURSOR,
        hbr_background: HBRUSH,
        lpsz_menu_name: *const u16,
        lpsz_class_name: *const u16,
        h_icon_sm: HICON,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetProcessDPIAware() -> i32;
        fn RegisterClassExW(wc: *const WNDCLASSEXW) -> u16;
        fn CreateWindowExW(
            ex_style: u32,
            class_name: *const u16,
            window_name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: HWND,
            menu: HMENU,
            instance: HINSTANCE,
            param: *mut c_void,
        ) -> HWND;
        fn DefWindowProcW(hwnd: HWND, msg: u32, w_param: usize, l_param: isize) -> isize;
        fn GetMessageW(msg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> i32;
        fn TranslateMessage(msg: *const MSG) -> i32;
        fn DispatchMessageW(msg: *const MSG) -> isize;
        fn PostMessageW(hwnd: HWND, msg: u32, w_param: usize, l_param: isize) -> i32;
        fn PostQuitMessage(code: i32);
        fn DestroyWindow(hwnd: HWND) -> i32;
        fn ShowWindow(hwnd: HWND, cmd: i32) -> i32;
        fn IsWindowVisible(hwnd: HWND) -> i32;
        fn SetWindowPos(
            hwnd: HWND,
            after: HWND,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
        fn SetWindowRgn(hwnd: HWND, hrgn: HRGN, redraw: i32) -> i32;
        fn GetWindowRect(hwnd: HWND, rect: *mut RECT) -> i32;
        fn GetClientRect(hwnd: HWND, rect: *mut RECT) -> i32;
        fn InvalidateRect(hwnd: HWND, rect: *const RECT, erase: i32) -> i32;
        fn UpdateWindow(hwnd: HWND) -> i32;
        fn BeginPaint(hwnd: HWND, ps: *mut PAINTSTRUCT) -> HDC;
        fn EndPaint(hwnd: HWND, ps: *const PAINTSTRUCT) -> i32;
        fn GetDC(hwnd: HWND) -> HDC;
        fn ReleaseDC(hwnd: HWND, hdc: HDC) -> i32;
        fn FillRect(hdc: HDC, rect: *const RECT, brush: HBRUSH) -> i32;
        fn DrawTextW(hdc: HDC, text: *const u16, count: i32, rect: *mut RECT, format: u32) -> i32;
        fn LoadCursorW(instance: HINSTANCE, name: *const u16) -> HCURSOR;
        fn SetCursor(cursor: HCURSOR) -> HCURSOR;
        fn SetLayeredWindowAttributes(hwnd: HWND, key: u32, alpha: u8, flags: u32) -> i32;
        fn RegisterHotKey(hwnd: HWND, id: i32, modifiers: u32, vk: u32) -> i32;
        fn UnregisterHotKey(hwnd: HWND, id: i32) -> i32;
        fn MonitorFromWindow(hwnd: HWND, flags: u32) -> HMONITOR;
        fn GetMonitorInfoW(monitor: HMONITOR, info: *mut MONITORINFO) -> i32;

        #[cfg(target_pointer_width = "64")]
        fn GetWindowLongPtrW(hwnd: HWND, index: i32) -> isize;
        #[cfg(target_pointer_width = "64")]
        fn SetWindowLongPtrW(hwnd: HWND, index: i32, value: isize) -> isize;
        #[cfg(target_pointer_width = "32")]
        fn GetWindowLongW(hwnd: HWND, index: i32) -> isize;
        #[cfg(target_pointer_width = "32")]
        fn SetWindowLongW(hwnd: HWND, index: i32, value: isize) -> isize;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn GetDeviceCaps(hdc: HDC, index: i32) -> i32;
        fn CreateFontW(
            height: i32,
            width: i32,
            escapement: i32,
            orientation: i32,
            weight: i32,
            italic: u32,
            underline: u32,
            strikeout: u32,
            charset: u32,
            out_precision: u32,
            clip_precision: u32,
            quality: u32,
            pitch_and_family: u32,
            face_name: *const u16,
        ) -> HFONT;
        fn CreateSolidBrush(color: u32) -> HBRUSH;
        fn CreatePen(style: i32, width: i32, color: u32) -> HPEN;
        fn CreateRoundRectRgn(x1: i32, y1: i32, x2: i32, y2: i32, w: i32, h: i32) -> HRGN;
        fn FrameRgn(hdc: HDC, rgn: HRGN, brush: HBRUSH, w: i32, h: i32) -> i32;
        fn SelectObject(hdc: HDC, obj: HGDIOBJ) -> HGDIOBJ;
        fn DeleteObject(obj: HGDIOBJ) -> i32;
        fn SetBkMode(hdc: HDC, mode: i32) -> i32;
        fn SetTextColor(hdc: HDC, color: u32) -> u32;
        fn Chord(
            hdc: HDC,
            left: i32,
            top: i32,
            right: i32,
            bottom: i32,
            xr1: i32,
            yr1: i32,
            xr2: i32,
            yr2: i32,
        ) -> i32;
        fn RoundRect(
            hdc: HDC,
            left: i32,
            top: i32,
            right: i32,
            bottom: i32,
            w: i32,
            h: i32,
        ) -> i32;
        fn Rectangle(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32) -> i32;
        fn Polygon(hdc: HDC, pts: *const POINT, count: i32) -> i32;
        fn Ellipse(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(name: *const u16) -> HINSTANCE;
        fn GetLastError() -> u32;
    }

    unsafe fn get_ex_style(hwnd: HWND) -> isize {
        #[cfg(target_pointer_width = "64")]
        {
            unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) }
        }
        #[cfg(target_pointer_width = "32")]
        {
            unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) }
        }
    }

    unsafe fn set_ex_style(hwnd: HWND, value: isize) {
        #[cfg(target_pointer_width = "64")]
        {
            unsafe {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, value);
            }
        }
        #[cfg(target_pointer_width = "32")]
        {
            unsafe {
                SetWindowLongW(hwnd, GWL_EXSTYLE, value);
            }
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    const fn rgb(r: u8, g: u8, b: u8) -> u32 {
        (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
    }

    struct SharedOverlay {
        render: OverlayRenderData,
        style: String,
        on_moved: Option<OnMovedCb>,
        on_lock: Option<OnLockCb>,
        on_log: Option<OnLogCb>,
        thread: Option<JoinHandle<()>>,
    }

    static SHARED: OnceLock<Mutex<SharedOverlay>> = OnceLock::new();
    static HWND_ATOM: AtomicIsize = AtomicIsize::new(0);
    static LOCKED: AtomicBool = AtomicBool::new(false);
    static SIZE_PCT: AtomicI32 = AtomicI32::new(100);
    static OPACITY_PCT: AtomicI32 = AtomicI32::new(100);
    static HOTKEY_OK: AtomicBool = AtomicBool::new(false);
    static LOCK_HOTKEY_OK: AtomicBool = AtomicBool::new(false);

    fn shared() -> &'static Mutex<SharedOverlay> {
        SHARED.get_or_init(|| {
            Mutex::new(SharedOverlay {
                render: OverlayRenderData {
                    view: crate::overlay::OverlayView {
                        total: "0".to_string(),
                        current: None,
                        current_caption: "last match".to_string(),
                        current_live: false,
                        badge: None,
                        rows: Vec::new(),
                        treasures: "0".to_string(),
                        session: "0".to_string(),
                        last: None,
                        squad: None,
                    },
                    header: "CHESTS".to_string(),
                    caption_total: "total".to_string(),
                    last_label: "Last".to_string(),
                    session_label: "Session".to_string(),
                    squad_label: "Squad".to_string(),
                    last_match_caption: "last match".to_string(),
                    window_title: "Chests".to_string(),
                },
                style: "hud".to_string(),
                on_moved: None,
                on_lock: None,
                on_log: None,
                thread: None,
            })
        })
    }

    fn current_hwnd() -> HWND {
        HWND_ATOM.load(Ordering::Acquire) as HWND
    }

    fn is_keyed() -> bool {
        shared()
            .lock()
            .map(|s| s.style != "classic")
            .unwrap_or(true)
    }

    fn current_alpha() -> u8 {
        let pct = OPACITY_PCT.load(Ordering::Relaxed);
        ((pct as f64) * 255.0 / 100.0).round().clamp(0.0, 255.0) as u8
    }

    // Thread-local GDI state (only touched on the Win32 overlay thread)
    struct GdiState {
        dpi_scale: f32,
        scale: f32,
        stable_w: i32,
        header_h: i32,
        big_h: i32,
        caption_h: i32,
        row_h: i32,
        col1_w: i32,
        col2_w: i32,
        label_w: i32,
        hud_big_h: i32,
        hud_small_h: i32,
        lock_rect: RECT,
        font_big: HFONT,
        font_small: HFONT,
        font_header: HFONT,
        font_caption: HFONT,
        font_label: HFONT,
        font_icon: HFONT,
        font_hud_big: HFONT,
        font_hud_small: HFONT,
        brush_bg: HBRUSH,
        brush_border: HBRUSH,
        brush_line: HBRUSH,
    }

    impl Default for GdiState {
        fn default() -> Self {
            Self {
                dpi_scale: 1.0,
                scale: 1.0,
                stable_w: 0,
                header_h: 0,
                big_h: 0,
                caption_h: 0,
                row_h: 0,
                col1_w: 0,
                col2_w: 0,
                label_w: 0,
                hud_big_h: 0,
                hud_small_h: 0,
                lock_rect: RECT::default(),
                font_big: null_mut(),
                font_small: null_mut(),
                font_header: null_mut(),
                font_caption: null_mut(),
                font_label: null_mut(),
                font_icon: null_mut(),
                font_hud_big: null_mut(),
                font_hud_small: null_mut(),
                brush_bg: null_mut(),
                brush_border: null_mut(),
                brush_line: null_mut(),
            }
        }
    }

    impl GdiState {
        fn s(&self, v: i32) -> i32 {
            (v as f32 * self.scale).round() as i32
        }

        fn sc(&self, v: f64) -> i32 {
            (v * self.scale as f64).round() as i32
        }

        unsafe fn delete_fonts(&mut self) {
            for f in [
                &mut self.font_big,
                &mut self.font_small,
                &mut self.font_header,
                &mut self.font_caption,
                &mut self.font_label,
                &mut self.font_icon,
                &mut self.font_hud_big,
                &mut self.font_hud_small,
            ] {
                if !f.is_null() {
                    unsafe { DeleteObject(*f) };
                    *f = null_mut();
                }
            }
        }

        unsafe fn delete_brushes(&mut self) {
            for b in [
                &mut self.brush_bg,
                &mut self.brush_border,
                &mut self.brush_line,
            ] {
                if !b.is_null() {
                    unsafe { DeleteObject(*b) };
                    *b = null_mut();
                }
            }
        }

        unsafe fn create_fonts(&mut self) {
            unsafe { self.delete_fonts() };
            let pct = SIZE_PCT.load(Ordering::Relaxed);
            self.scale = self.dpi_scale * (pct as f32) / 100.0;
            let segoe = wide("Segoe UI");
            let mdl2 = wide("Segoe MDL2 Assets");
            let bahn_c = wide("Bahnschrift Condensed");
            let bahn_sc = wide("Bahnschrift SemiCondensed");
            unsafe {
                self.font_big = CreateFontW(-self.s(38), 0, 0, 0, 800, 0, 0, 0, 1, 0, 0, 5, 0, segoe.as_ptr());
                self.font_small = CreateFontW(-self.s(14), 0, 0, 0, 500, 0, 0, 0, 1, 0, 0, 5, 0, segoe.as_ptr());
                self.font_header = CreateFontW(-self.s(12), 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 5, 0, segoe.as_ptr());
                self.font_caption = CreateFontW(-self.s(11), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, segoe.as_ptr());
                self.font_label = CreateFontW(-self.s(13), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, segoe.as_ptr());
                self.font_icon = CreateFontW(-self.s(14), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, mdl2.as_ptr());
                self.font_hud_big = CreateFontW(-self.s(40), 0, 0, 0, 700, 1, 0, 0, 1, 0, 0, 4, 0, bahn_c.as_ptr());
                self.font_hud_small = CreateFontW(-self.s(18), 0, 0, 0, 600, 1, 0, 0, 1, 0, 0, 4, 0, bahn_sc.as_ptr());
            }
        }
    }

    thread_local! {
        static GDI: RefCell<GdiState> = RefCell::new(GdiState::default());
    }

    unsafe fn text_width(hdc: HDC, font: HFONT, text: &str) -> (i32, i32) {
        unsafe {
            SelectObject(hdc, font);
            let mut r = RECT::default();
            let w = wide(if text.is_empty() { " " } else { text });
            DrawTextW(hdc, w.as_ptr(), -1, &mut r, DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX);
            (r.right - r.left, r.bottom - r.top)
        }
    }

    unsafe fn draw_text(
        hdc: HDC,
        font: HFONT,
        color: u32,
        text: &str,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        align: u32,
    ) {
        unsafe {
            SelectObject(hdc, font);
            SetTextColor(hdc, color);
            let mut r = RECT { left, top, right, bottom };
            let w = wide(text);
            DrawTextW(hdc, w.as_ptr(), -1, &mut r, DT_SINGLELINE | DT_NOPREFIX | align);
        }
    }

    unsafe fn draw_outlined(
        hdc: HDC,
        font: HFONT,
        color: u32,
        text: &str,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        align: u32,
        thin: bool,
        scale: f32,
    ) {
        unsafe {
            SelectObject(hdc, font);
            let o = if thin {
                1
            } else {
                ((1.6 * scale as f64).round() as i32).max(1)
            };
            let w = wide(text);
            SetTextColor(hdc, rgb(11, 15, 24));
            for dx in -o..=o {
                for dy in -o..=(o + 1) {
                    if dx.abs() != o && dy.abs() < o {
                        continue;
                    }
                    let mut r = RECT {
                        left: left + dx,
                        top: top + dy,
                        right: right + dx,
                        bottom: bottom + dy,
                    };
                    DrawTextW(hdc, w.as_ptr(), -1, &mut r, DT_SINGLELINE | DT_NOPREFIX | DT_NOCLIP | align);
                }
            }
            SetTextColor(hdc, color);
            let mut rc = RECT { left, top, right, bottom };
            DrawTextW(hdc, w.as_ptr(), -1, &mut rc, DT_SINGLELINE | DT_NOPREFIX | DT_NOCLIP | align);
        }
    }

    unsafe fn draw_chest(
        hdc: HDC,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        body: u32,
        lid: u32,
        latch: u32,
        outline: u32,
        sc_pen: i32,
        sc_4: i32,
        sc_2: i32,
    ) {
        unsafe {
            let pen = CreatePen(0, sc_pen.max(1), outline);
            let old_pen = SelectObject(hdc, pen);
            let lid_h = (h as f64 * 0.42) as i32;
            let b_lid = CreateSolidBrush(lid);
            let old_b = SelectObject(hdc, b_lid);
            Chord(hdc, x, y, x + w, y + 2 * lid_h, x + w, y + lid_h, x, y + lid_h);
            let b_body = CreateSolidBrush(body);
            SelectObject(hdc, b_body);
            RoundRect(hdc, x, y + lid_h, x + w, y + h, sc_4, sc_4);
            let b_latch = CreateSolidBrush(latch);
            SelectObject(hdc, b_latch);
            let lw = (w / 5).max(3);
            Rectangle(
                hdc,
                x + (w - lw) / 2,
                y + lid_h - sc_2,
                x + (w + lw) / 2,
                y + lid_h + (h / 4).max(4),
            );
            SelectObject(hdc, old_b);
            SelectObject(hdc, old_pen);
            DeleteObject(b_lid);
            DeleteObject(b_body);
            DeleteObject(b_latch);
            DeleteObject(pen);
        }
    }

    unsafe fn draw_star(
        hdc: HDC,
        cx: i32,
        cy: i32,
        r: i32,
        fill: u32,
        outline: u32,
        sc_pen: i32,
    ) {
        let mut pts = [POINT::default(); 10];
        for (i, pt) in pts.iter_mut().enumerate() {
            let a = -std::f64::consts::FRAC_PI_2 + (i as f64) * std::f64::consts::PI / 5.0;
            let rr = if i % 2 == 0 {
                r as f64
            } else {
                r as f64 * 0.45
            };
            pt.x = cx + (rr * a.cos()).round() as i32;
            pt.y = cy + (rr * a.sin()).round() as i32;
        }
        unsafe {
            let pen = CreatePen(0, sc_pen.max(1), outline);
            let brush = CreateSolidBrush(fill);
            let old_pen = SelectObject(hdc, pen);
            let old_b = SelectObject(hdc, brush);
            Polygon(hdc, pts.as_ptr(), pts.len() as i32);
            SelectObject(hdc, old_b);
            SelectObject(hdc, old_pen);
            DeleteObject(brush);
            DeleteObject(pen);
        }
    }

    unsafe fn draw_disc(hdc: HDC, x: i32, y: i32, d: i32, fill: u32, outline: u32, pen_w: i32) {
        unsafe {
            let pen = CreatePen(0, pen_w.max(1), outline);
            let brush = CreateSolidBrush(fill);
            let old_pen = SelectObject(hdc, pen);
            let old_b = SelectObject(hdc, brush);
            Ellipse(hdc, x, y, x + d, y + d);
            SelectObject(hdc, old_b);
            SelectObject(hdc, old_pen);
            DeleteObject(brush);
            DeleteObject(pen);
        }
    }

    unsafe fn draw_label_value(
        hdc: HDC,
        font: HFONT,
        label: &str,
        value: &str,
        left: i32,
        right: i32,
        top: i32,
        height: i32,
        value_color: u32,
        scale: f32,
    ) {
        unsafe {
            if value.is_empty() {
                draw_outlined(hdc, font, rgb(255, 233, 166), label, left, top, right, top + height, DT_RIGHT, true, scale);
                return;
            }
            let (value_w, _) = text_width(hdc, font, value);
            draw_outlined(hdc, font, value_color, value, left, top, right, top + height, DT_RIGHT, true, scale);
            let label_right = right - value_w;
            let full_label = format!("{label}{SEP}");
            draw_outlined(hdc, font, rgb(255, 233, 166), &full_label, left, top, label_right, top + height, DT_RIGHT, true, scale);
        }
    }

    unsafe fn apply_lock(hwnd: HWND, locked: bool, notify: bool) {
        LOCKED.store(locked, Ordering::Release);
        let alpha = current_alpha();
        let keyed = is_keyed();
        let layered = locked || alpha < 255 || keyed;
        unsafe {
            let mut ex = get_ex_style(hwnd);
            if layered {
                ex |= WS_EX_LAYERED as isize;
            } else {
                ex &= !(WS_EX_LAYERED as isize);
            }
            if locked {
                ex |= WS_EX_TRANSPARENT as isize;
            } else {
                ex &= !(WS_EX_TRANSPARENT as isize);
            }
            set_ex_style(hwnd, ex);
            if layered {
                let flags = if keyed { LWA_ALPHA | LWA_COLORKEY } else { LWA_ALPHA };
                SetLayeredWindowAttributes(hwnd, KEY_COLOR, alpha, flags);
            }
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            InvalidateRect(hwnd, null_mut(), 1);
        }
        if notify {
            if let Ok(guard) = shared().lock() {
                if let Some(ref cb) = guard.on_lock {
                    cb(locked);
                }
            }
        }
    }

    unsafe fn set_visible(hwnd: HWND, visible: bool) {
        unsafe {
            if visible {
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            } else {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    fn clamp_to_monitor(hwnd: HWND, cur: RECT, w: i32, h: i32) -> (i32, i32) {
        let mut nx = cur.left;
        let mut ny = cur.top;
        unsafe {
            let mut mi = MONITORINFO {
                cb_size: size_of::<MONITORINFO>() as u32,
                rc_monitor: RECT::default(),
                rc_work: RECT::default(),
                dw_flags: 0,
            };
            let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if !mon.is_null() && GetMonitorInfoW(mon, &mut mi) != 0 {
                let work = mi.rc_work;
                if cur.right - cur.left > 0 && (cur.left + cur.right) / 2 > (work.left + work.right) / 2 {
                    nx = cur.right - w;
                }
                nx = nx.clamp(work.left, (work.right - w).max(work.left));
                ny = ny.clamp(work.top, (work.bottom - h).max(work.top));
            }
        }
        (nx, ny)
    }

    fn notify_moved(nx: i32, ny: i32) {
        if let Ok(guard) = shared().lock() {
            if let Some(ref cb) = guard.on_moved {
                cb(nx, ny);
            }
        }
    }

    fn log_msg(cls: &str, msg: &str) {
        if let Ok(guard) = shared().lock() {
            if let Some(ref cb) = guard.on_log {
                cb(cls, msg);
            }
        }
    }

    unsafe fn relayout(hwnd: HWND) {
        let (render, keyed) = match shared().lock() {
            Ok(g) => (g.render.clone(), g.style != "classic"),
            Err(_) => return,
        };
        let v = &render.view;

        GDI.with(|cell| {
            let mut st = cell.borrow_mut();
            let hdc = unsafe { GetDC(hwnd) };
            if hdc.is_null() {
                return;
            }
            let (nx, ny, cur_left, cur_top) = if keyed {
                let pad = st.sc(8.0);
                let lock_w = st.sc(22.0);
                let (big_w, big_h) = unsafe { text_width(hdc, st.font_hud_big, &v.total) };
                st.hud_big_h = big_h;
                let last_val = v.last.as_deref().unwrap_or("—");
                let r1 = format!("{}{SEP}{last_val}", render.last_label);
                let r2 = format!("{}{SEP}{}", render.session_label, v.session);
                let (w1, small_h) = unsafe { text_width(hdc, st.font_hud_small, &r1) };
                st.hud_small_h = small_h;
                let (w2, _) = unsafe { text_width(hdc, st.font_hud_small, &r2) };
                let w3 = if let Some(ref sq) = v.squad {
                    let r3 = format!("{}{SEP}{sq}", render.squad_label);
                    unsafe { text_width(hdc, st.font_hud_small, &r3).0 }
                } else {
                    0
                };
                let (tw, _) = unsafe { text_width(hdc, st.font_hud_small, &v.treasures) };
                let w4 = st.sc(22.0) + tw;
                let rows_w = w1.max(w2).max(w3).max(w4);
                let w = (lock_w + pad + (st.sc(18.0) + st.sc(26.0) + st.sc(8.0) + big_w).max(rows_w) + pad + st.sc(14.0))
                    .max(st.stable_w);
                st.stable_w = w;
                let row_count = if v.squad.is_some() { 4 } else { 3 };
                let h = pad + st.hud_big_h + row_count * (st.hud_small_h + st.sc(2.0)) + pad;
                st.lock_rect = RECT {
                    left: st.sc(2.0),
                    top: 0,
                    right: lock_w,
                    bottom: h,
                };
                let mut cur = RECT::default();
                unsafe { GetWindowRect(hwnd, &mut cur) };
                let (nx, ny) = clamp_to_monitor(hwnd, cur, w, h);
                unsafe {
                    SetWindowPos(hwnd, HWND_TOPMOST, nx, ny, w, h, SWP_NOACTIVATE);
                    SetWindowRgn(hwnd, null_mut(), 1);
                }
                (nx, ny, cur.left, cur.top)
            } else {
                let (hw1, hh) = unsafe { text_width(hdc, st.font_header, &render.header) };
                st.header_h = hh;
                let badge_w = v
                    .badge
                    .as_deref()
                    .map(|b| unsafe { text_width(hdc, st.font_header, b).0 })
                    .unwrap_or(0);
                let header_w = hw1 + st.s(16) + badge_w + st.s(30);

                let (bw, bh) = unsafe { text_width(hdc, st.font_big, &v.total) };
                st.big_h = bh;
                let (cw, ch) = unsafe { text_width(hdc, st.font_caption, &render.caption_total) };
                st.caption_h = ch;
                st.col1_w = bw.max(cw);

                let cur_txt = v.current.as_deref().unwrap_or("—");
                let (c2_big, _) = unsafe { text_width(hdc, st.font_big, cur_txt) };
                let (c2_cap, _) = unsafe { text_width(hdc, st.font_caption, &v.current_caption) };
                let (c2_last, _) = unsafe { text_width(hdc, st.font_caption, &render.last_match_caption) };
                st.col2_w = c2_big.max(c2_cap).max(c2_last);
                let counters_w = st.col1_w + st.s(COL_GAP) + st.col2_w;

                st.label_w = 0;
                let mut value_w = 0;
                st.row_h = 0;
                for row in &v.rows {
                    let (lw, lh) = unsafe { text_width(hdc, st.font_label, &row.label) };
                    st.label_w = st.label_w.max(lw);
                    let (vw, vh) = unsafe { text_width(hdc, st.font_small, &row.value) };
                    let dw = row
                        .detail
                        .as_deref()
                        .map(|d| st.s(6) + unsafe { text_width(hdc, st.font_caption, d).0 })
                        .unwrap_or(0);
                    value_w = value_w.max(vw + dw);
                    st.row_h = st.row_h.max(lh).max(vh);
                }
                let rows_w = if v.rows.is_empty() {
                    0
                } else {
                    st.label_w + st.s(12) + value_w
                };

                let w = st
                    .stable_w
                    .max(st.s(MIN_WIDTH))
                    .max(header_w.max(counters_w).max(rows_w) + 2 * st.s(PAD));
                st.stable_w = w;
                let mut h = st.s(12) + st.header_h + st.s(6) + st.big_h + st.caption_h + st.s(12);
                if !v.rows.is_empty() {
                    h += 1 + st.s(10) + (v.rows.len() as i32) * (st.row_h + st.s(4)) + st.s(4);
                }
                st.lock_rect = RECT {
                    left: w - st.s(PAD) - st.s(18),
                    top: st.s(6),
                    right: w - st.s(6),
                    bottom: st.s(12) + st.header_h + st.s(6),
                };

                let mut cur = RECT::default();
                unsafe { GetWindowRect(hwnd, &mut cur) };
                let (nx, ny) = clamp_to_monitor(hwnd, cur, w, h);
                unsafe {
                    SetWindowPos(hwnd, HWND_TOPMOST, nx, ny, w, h, SWP_NOACTIVATE);
                    let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, st.s(18), st.s(18));
                    SetWindowRgn(hwnd, rgn, 1);
                }
                (nx, ny, cur.left, cur.top)
            };
            unsafe { ReleaseDC(hwnd, hdc) };
            if nx != cur_left || ny != cur_top {
                notify_moved(nx, ny);
            }
        });

        unsafe {
            InvalidateRect(hwnd, null_mut(), 1);
            UpdateWindow(hwnd);
        }
    }

    unsafe fn paint_to(hwnd: HWND, hdc: HDC) {
        let (render, keyed) = match shared().lock() {
            Ok(g) => (g.render.clone(), g.style != "classic"),
            Err(_) => return,
        };
        let v = &render.view;
        let locked = LOCKED.load(Ordering::Acquire);

        GDI.with(|cell| {
            let st = cell.borrow();
            let mut rc = RECT::default();
            unsafe { GetClientRect(hwnd, &mut rc) };

            if keyed {
                unsafe {
                    let key = CreateSolidBrush(KEY_COLOR);
                    FillRect(hdc, &rc, key);
                    DeleteObject(key);
                    SetBkMode(hdc, TRANSPARENT);
                }
                let pad = st.sc(8.0);
                let right = rc.right - pad - st.sc(12.0);
                let white = rgb(255, 255, 255);
                let gold = rgb(255, 210, 63);
                let ink = rgb(11, 15, 24);

                if !locked {
                    unsafe {
                        let pen = CreatePen(0, 1, rgb(58, 66, 86));
                        let brush = CreateSolidBrush(rgb(28, 34, 48));
                        let old_p = SelectObject(hdc, pen);
                        let old_b = SelectObject(hdc, brush);
                        RoundRect(hdc, 0, 0, rc.right, rc.bottom, st.sc(12.0), st.sc(12.0));
                        SelectObject(hdc, old_b);
                        SelectObject(hdc, old_p);
                        DeleteObject(brush);
                        DeleteObject(pen);
                        SelectObject(hdc, st.font_icon);
                        SetTextColor(hdc, rgb(200, 200, 208));
                        let mut lr = st.lock_rect;
                        let icon = wide("\u{E785}");
                        DrawTextW(hdc, icon.as_ptr(), -1, &mut lr, DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_NOPREFIX);
                    }
                }

                let left = st.lock_rect.right;
                let mut y = pad;
                let live_figures = v.current_live
                    && v.current.as_deref().is_some_and(|c| c != "…");

                let (big_w, _) = unsafe { text_width(hdc, st.font_hud_big, &v.total) };
                unsafe {
                    draw_outlined(
                        hdc,
                        st.font_hud_big,
                        if live_figures { gold } else { white },
                        &v.total,
                        left,
                        y,
                        right,
                        y + st.hud_big_h,
                        DT_RIGHT,
                        false,
                        st.scale,
                    );
                }
                let iw = st.sc(26.0);
                let ih = st.sc(22.0);
                let ix = right - big_w - st.sc(8.0) - iw;
                unsafe {
                    draw_chest(
                        hdc,
                        ix,
                        y + (st.hud_big_h - ih) / 2,
                        iw,
                        ih,
                        gold,
                        rgb(224, 165, 38),
                        ink,
                        ink,
                        st.sc(1.5),
                        st.sc(4.0),
                        st.sc(2.0),
                    );
                    if v.current_live {
                        let d = st.sc(11.0);
                        draw_disc(
                            hdc,
                            ix - st.sc(7.0) - d,
                            y + (st.hud_big_h - d) / 2,
                            d,
                            rgb(255, 59, 48),
                            ink,
                            st.sc(1.5),
                        );
                    }
                }
                y += st.hud_big_h + st.sc(2.0);

                let row_h = st.hud_small_h;
                let last_val = v.last.as_deref().unwrap_or("—");
                unsafe {
                    draw_label_value(hdc, st.font_hud_small, &render.last_label, last_val, left, right, y, row_h, white, st.scale);
                    y += row_h + st.sc(2.0);
                    draw_label_value(hdc, st.font_hud_small, &render.session_label, &v.session, left, right, y, row_h, white, st.scale);
                    y += row_h + st.sc(2.0);
                    if let Some(ref sq) = v.squad {
                        draw_label_value(hdc, st.font_hud_small, &render.squad_label, sq, left, right, y, row_h, white, st.scale);
                        y += row_h + st.sc(2.0);
                    }
                    let (tw, _) = text_width(hdc, st.font_hud_small, &v.treasures);
                    draw_outlined(
                        hdc,
                        st.font_hud_small,
                        rgb(214, 190, 255),
                        &v.treasures,
                        left,
                        y,
                        right,
                        y + row_h,
                        DT_RIGHT,
                        true,
                        st.scale,
                    );
                    draw_star(
                        hdc,
                        right - tw - st.sc(12.0),
                        y + row_h / 2,
                        st.sc(8.0),
                        rgb(196, 155, 255),
                        ink,
                        st.sc(1.5),
                    );
                }
            } else {
                unsafe {
                    FillRect(hdc, &rc, st.brush_bg);
                    SetBkMode(hdc, TRANSPARENT);
                }
                let x = st.s(PAD);
                let right = rc.right - st.s(PAD);
                let mut y = st.s(12);

                unsafe {
                    let border = CreateRoundRectRgn(0, 0, rc.right + 1, rc.bottom + 1, st.s(18), st.s(18));
                    FrameRgn(hdc, border, st.brush_border, 1, 1);
                    DeleteObject(border);

                    draw_text(hdc, st.font_header, rgb(150, 150, 162), &render.header, x, y, right, y + st.header_h, DT_LEFT);
                    if let Some(ref badge) = v.badge {
                        draw_text(hdc, st.font_header, rgb(96, 214, 128), badge, x, y, st.lock_rect.left - st.s(6), y + st.header_h, DT_RIGHT);
                    }
                    SelectObject(hdc, st.font_icon);
                    SetTextColor(hdc, if locked { rgb(96, 96, 106) } else { rgb(200, 200, 208) });
                    let mut lr = st.lock_rect;
                    let icon = wide(if locked { "\u{E72E}" } else { "\u{E785}" });
                    DrawTextW(hdc, icon.as_ptr(), -1, &mut lr, DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_NOPREFIX);
                    y += st.header_h + st.s(6);

                    let x2 = x + st.col1_w + st.s(COL_GAP);
                    draw_text(hdc, st.font_big, rgb(242, 184, 75), &v.total, x - st.s(1), y, x + st.col1_w + st.s(4), y + st.big_h, DT_LEFT);
                    let cur_color = if v.current.is_none() {
                        rgb(80, 80, 92)
                    } else if v.current_live {
                        rgb(110, 224, 150)
                    } else {
                        rgb(226, 226, 232)
                    };
                    draw_text(hdc, st.font_big, cur_color, v.current.as_deref().unwrap_or("—"), x2 - st.s(1), y, right + st.s(4), y + st.big_h, DT_LEFT);
                    let sep = RECT {
                        left: x + st.col1_w + st.s(COL_GAP) / 2,
                        top: y + st.s(8),
                        right: x + st.col1_w + st.s(COL_GAP) / 2 + 1,
                        bottom: y + st.big_h + st.caption_h,
                    };
                    FillRect(hdc, &sep, st.brush_line);
                    y += st.big_h;
                    draw_text(hdc, st.font_caption, rgb(130, 130, 142), &render.caption_total, x, y, x + st.col1_w + st.s(4), y + st.caption_h, DT_LEFT);
                    draw_text(hdc, st.font_caption, rgb(130, 130, 142), &v.current_caption, x2, y, right + st.s(4), y + st.caption_h, DT_LEFT);
                    y += st.caption_h + st.s(12);

                    if !v.rows.is_empty() {
                        let line = RECT { left: x, top: y, right, bottom: y + 1 };
                        FillRect(hdc, &line, st.brush_line);
                        y += 1 + st.s(10);
                        for row in &v.rows {
                            draw_text(hdc, st.font_label, rgb(130, 130, 142), &row.label, x, y, x + st.label_w, y + st.row_h, DT_LEFT);
                            let vx = x + st.label_w + st.s(12);
                            let val_color = if row.treasure { rgb(196, 160, 255) } else { rgb(226, 226, 232) };
                            draw_text(hdc, st.font_small, val_color, &row.value, vx, y, right + st.s(4), y + st.row_h, DT_LEFT);
                            if let Some(ref detail) = row.detail {
                                let (vw, _) = text_width(hdc, st.font_small, &row.value);
                                let dx = vx + vw + st.s(6);
                                draw_text(hdc, st.font_caption, rgb(130, 130, 142), detail, dx, y, right + st.s(4), y + st.row_h - st.s(2), DT_BOTTOM);
                            }
                            y += st.row_h + st.s(4);
                        }
                    }
                }
            }
        });
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        w_param: usize,
        l_param: isize,
    ) -> isize {
        match msg {
            WM_NCHITTEST => {
                let mut wr = RECT::default();
                unsafe { GetWindowRect(hwnd, &mut wr) };
                let px = (l_param as i16) as i32 - wr.left;
                let py = ((l_param >> 16) as i16) as i32 - wr.top;
                let on_button = GDI.with(|c| {
                    let lr = c.borrow().lock_rect;
                    px >= lr.left && px < lr.right && py >= lr.top && py < lr.bottom
                });
                return if on_button { HTCLIENT } else { HTCAPTION };
            }
            WM_SETCURSOR => {
                if (l_param & 0xFFFF) == HTCLIENT {
                    unsafe { SetCursor(LoadCursorW(null_mut(), IDC_HAND)) };
                    return 1;
                }
            }
            WM_LBUTTONUP => {
                unsafe { apply_lock(hwnd, true, true) };
                return 0;
            }
            WM_APP_LOCK => {
                let cur = LOCKED.load(Ordering::Acquire);
                unsafe { apply_lock(hwnd, !cur, true) };
                return 0;
            }
            WM_NCRBUTTONUP => {
                unsafe { set_visible(hwnd, false) };
                return 0;
            }
            WM_HOTKEY => {
                if w_param == 2 {
                    let cur = LOCKED.load(Ordering::Acquire);
                    unsafe { apply_lock(hwnd, !cur, true) };
                } else {
                    let vis = unsafe { IsWindowVisible(hwnd) != 0 };
                    unsafe { set_visible(hwnd, !vis) };
                }
                return 0;
            }
            WM_EXITSIZEMOVE => {
                let mut r = RECT::default();
                if unsafe { GetWindowRect(hwnd, &mut r) } != 0 {
                    notify_moved(r.left, r.top);
                }
                return 0;
            }
            WM_APP_STOP => {
                unsafe { DestroyWindow(hwnd) };
                return 0;
            }
            WM_APP_TOGGLE => {
                let vis = unsafe { IsWindowVisible(hwnd) != 0 };
                unsafe { set_visible(hwnd, !vis) };
                return 0;
            }
            WM_APP_RELAYOUT => {
                unsafe { relayout(hwnd) };
                return 0;
            }
            WM_APP_APPEARANCE => {
                GDI.with(|c| {
                    let mut st = c.borrow_mut();
                    unsafe { st.create_fonts() };
                    st.stable_w = 0;
                });
                let cur = LOCKED.load(Ordering::Acquire);
                unsafe {
                    apply_lock(hwnd, cur, false);
                    relayout(hwnd);
                }
                return 0;
            }
            WM_ERASEBKGND => return 1,
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = unsafe { zeroed() };
                let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
                if !hdc.is_null() {
                    unsafe { paint_to(hwnd, hdc) };
                    unsafe { EndPaint(hwnd, &ps) };
                }
                return 0;
            }
            WM_PRINTCLIENT => {
                unsafe { paint_to(hwnd, w_param as HDC) };
                return 0;
            }
            WM_DESTROY => {
                unsafe {
                    UnregisterHotKey(hwnd, 1);
                    UnregisterHotKey(hwnd, 2);
                    GDI.with(|c| {
                        let mut st = c.borrow_mut();
                        st.delete_fonts();
                        st.delete_brushes();
                    });
                    PostQuitMessage(0);
                }
                return 0;
            }
            _ => {}
        }
        unsafe { DefWindowProcW(hwnd, msg, w_param, l_param) }
    }

    pub fn set_style(style: &str) {
        let valid = if STYLES.contains(&style) { style } else { "hud" };
        if let Ok(mut g) = shared().lock() {
            g.style = valid.to_string();
        }
        let hwnd = current_hwnd();
        if !hwnd.is_null() {
            unsafe { PostMessageW(hwnd, WM_APP_APPEARANCE, 0, 0) };
        }
    }

    pub fn set_appearance(size_percent: i32, opacity_percent: i32) {
        SIZE_PCT.store(OverlayAppearance::clamp_size(size_percent), Ordering::Relaxed);
        OPACITY_PCT.store(OverlayAppearance::clamp_opacity(opacity_percent), Ordering::Relaxed);
        let hwnd = current_hwnd();
        if !hwnd.is_null() {
            unsafe { PostMessageW(hwnd, WM_APP_APPEARANCE, 0, 0) };
        }
    }

    pub fn update(data: OverlayRenderData) {
        if let Ok(mut g) = shared().lock() {
            g.render = data;
        }
        let hwnd = current_hwnd();
        if !hwnd.is_null() {
            unsafe { PostMessageW(hwnd, WM_APP_RELAYOUT, 0, 0) };
        }
    }

    pub fn toggle_visible() {
        let hwnd = current_hwnd();
        if !hwnd.is_null() {
            unsafe { PostMessageW(hwnd, WM_APP_TOGGLE, 0, 0) };
        }
    }

    pub fn toggle_lock() {
        let hwnd = current_hwnd();
        if !hwnd.is_null() {
            unsafe { PostMessageW(hwnd, WM_APP_LOCK, 0, 0) };
        }
    }

    pub fn is_running() -> bool {
        !current_hwnd().is_null()
    }

    pub fn is_visible() -> bool {
        let hwnd = current_hwnd();
        !hwnd.is_null() && unsafe { IsWindowVisible(hwnd) != 0 }
    }

    pub fn is_locked() -> bool {
        LOCKED.load(Ordering::Acquire)
    }

    pub fn hotkeys_ok() -> (bool, bool) {
        (
            HOTKEY_OK.load(Ordering::Relaxed),
            LOCK_HOTKEY_OK.load(Ordering::Relaxed),
        )
    }

    pub fn start(
        x: Option<i32>,
        y: Option<i32>,
        locked: bool,
        on_moved: OnMovedCb,
        on_lock: OnLockCb,
        on_log: OnLogCb,
    ) {
        if is_running() {
            return;
        }
        let title = {
            let Ok(mut g) = shared().lock() else { return };
            if g.thread.is_some() {
                return;
            }
            g.on_moved = Some(on_moved);
            g.on_lock = Some(on_lock);
            g.on_log = Some(on_log);
            g.render.window_title.clone()
        };
        LOCKED.store(locked, Ordering::Release);

        let ready = Arc::new((Mutex::new(false), Condvar::new()));
        let ready_clone = Arc::clone(&ready);

        let handle = thread::Builder::new()
            .name("native-overlay".to_string())
            .spawn(move || {
                let signal_ready = || {
                    let (lock, cv) = &*ready_clone;
                    if let Ok(mut done) = lock.lock() {
                        *done = true;
                        cv.notify_all();
                    }
                };

                unsafe {
                    SetProcessDPIAware();
                    let screen_dc = GetDC(null_mut());
                    let dpi = if !screen_dc.is_null() {
                        let d = GetDeviceCaps(screen_dc, LOGPIXELSY);
                        ReleaseDC(null_mut(), screen_dc);
                        if d > 0 { d as f32 / 96.0 } else { 1.0 }
                    } else {
                        1.0
                    };

                    let (init_w, init_h, init_x, init_y) = GDI.with(|c| {
                        let mut st = c.borrow_mut();
                        *st = GdiState::default();
                        st.dpi_scale = dpi;
                        st.create_fonts();
                        st.brush_bg = CreateSolidBrush(rgb(18, 18, 22));
                        st.brush_border = CreateSolidBrush(rgb(58, 58, 66));
                        st.brush_line = CreateSolidBrush(rgb(44, 44, 52));
                        (
                            st.s(MIN_WIDTH),
                            st.s(90),
                            x.unwrap_or_else(|| st.s(24)),
                            y.unwrap_or_else(|| st.s(24)),
                        )
                    });

                    let h_instance = GetModuleHandleW(null_mut());
                    let class_name = wide("ChestCounterOverlay");
                    let wc = WNDCLASSEXW {
                        cb_size: size_of::<WNDCLASSEXW>() as u32,
                        style: 0,
                        lpfn_wnd_proc: Some(wnd_proc),
                        cb_cls_extra: 0,
                        cb_wnd_extra: 0,
                        h_instance,
                        h_icon: null_mut(),
                        h_cursor: LoadCursorW(null_mut(), IDC_SIZEALL),
                        hbr_background: null_mut(),
                        lpsz_menu_name: null_mut(),
                        lpsz_class_name: class_name.as_ptr(),
                        h_icon_sm: null_mut(),
                    };

                    if RegisterClassExW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                        log_msg("err", &format!("Overlay RegisterClassExW failed ({})", GetLastError()));
                        signal_ready();
                        return;
                    }

                    let win_title = wide(&title);
                    let hwnd = CreateWindowExW(
                        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                        class_name.as_ptr(),
                        win_title.as_ptr(),
                        WS_POPUP,
                        init_x,
                        init_y,
                        init_w,
                        init_h,
                        null_mut(),
                        null_mut(),
                        h_instance,
                        null_mut(),
                    );
                    if hwnd.is_null() {
                        log_msg("err", &format!("Overlay CreateWindowExW failed ({})", GetLastError()));
                        signal_ready();
                        return;
                    }

                    HWND_ATOM.store(hwnd as isize, Ordering::Release);
                    apply_lock(hwnd, locked, false);
                    relayout(hwnd);
                    ShowWindow(hwnd, SW_SHOWNOACTIVATE);

                    let hk1 = RegisterHotKey(hwnd, 1, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, VK_O) != 0;
                    let hk2 = RegisterHotKey(hwnd, 2, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, VK_L) != 0;
                    HOTKEY_OK.store(hk1, Ordering::Relaxed);
                    LOCK_HOTKEY_OK.store(hk2, Ordering::Relaxed);

                    signal_ready();

                    let mut msg: MSG = zeroed();
                    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }

                    HWND_ATOM.store(0, Ordering::Release);
                    HOTKEY_OK.store(false, Ordering::Relaxed);
                    LOCK_HOTKEY_OK.store(false, Ordering::Relaxed);
                }
            });

        if let Ok(h) = handle {
            if let Ok(mut g) = shared().lock() {
                g.thread = Some(h);
            }
            let (lock, cv) = &*ready;
            if let Ok(guard) = lock.lock() {
                let _ = cv.wait_timeout_while(guard, Duration::from_secs(5), |done| !*done);
            }
        }
    }

    pub fn stop() {
        let hwnd = current_hwnd();
        if !hwnd.is_null() {
            unsafe { PostMessageW(hwnd, WM_APP_STOP, 0, 0) };
        }
        let handle = shared().lock().ok().and_then(|mut g| g.thread.take());
        if let Some(h) = handle {
            let _ = h.join();
        }
        HWND_ATOM.store(0, Ordering::Release);
    }
}

#[cfg(not(target_os = "windows"))]
mod stub {
    use super::{OnLockCb, OnLogCb, OnMovedCb, OverlayRenderData};

    pub fn set_style(_style: &str) {}
    pub fn set_appearance(_size_percent: i32, _opacity_percent: i32) {}
    pub fn update(_data: OverlayRenderData) {}
    pub fn toggle_visible() {}
    pub fn toggle_lock() {}
    pub fn is_running() -> bool {
        false
    }
    pub fn is_visible() -> bool {
        false
    }
    pub fn is_locked() -> bool {
        false
    }
    pub fn hotkeys_ok() -> (bool, bool) {
        (false, false)
    }
    pub fn start(
        _x: Option<i32>,
        _y: Option<i32>,
        _locked: bool,
        _on_moved: OnMovedCb,
        _on_lock: OnLockCb,
        _on_log: OnLogCb,
    ) {
    }
    pub fn stop() {}
}

#[cfg(target_os = "windows")]
pub use win::*;
#[cfg(not(target_os = "windows"))]
pub use stub::*;
