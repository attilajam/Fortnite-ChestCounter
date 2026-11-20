use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::{Theme, WindowBuilder};
use wry::{NewWindowResponse, WebContext, WebViewBuilder};

use crate::gui_server;

#[derive(Debug, Clone)]
pub enum WindowEventMsg {
    Quit,
    Focus,
}

pub struct WindowConfig {
    pub url: String,
    pub cache_dir: PathBuf,
    pub title: String,
    pub close_question: Arc<dyn Fn() -> (String, String) + Send + Sync>,
    pub quit_flag: Arc<AtomicBool>,
    pub on_proxy: Box<dyn FnOnce(EventLoopProxy<WindowEventMsg>)>,
}

/// Runs the native desktop application window (WebView2 on Windows, WebKitGTK on Linux)
/// on the current (main) thread until closed.
/// Returns `Ok(())` when cleanly exited, or `Err(msg)` if a native webview could not be created
/// (in which case the caller can fall back to opening the local URL in Edge/Chrome/browser).
pub fn run(cfg: WindowConfig) -> Result<(), String> {
    let event_loop = EventLoopBuilder::<WindowEventMsg>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title(&cfg.title)
        .with_inner_size(LogicalSize::new(1200.0, 900.0))
        .with_min_inner_size(LogicalSize::new(560.0, 480.0))
        .with_theme(Some(Theme::Dark))
        .build(&event_loop)
        .map_err(|e| format!("create window: {e}"))?;

    let _ = std::fs::create_dir_all(&cfg.cache_dir);
    let mut web_context = WebContext::new(Some(cfg.cache_dir));

    let builder = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url(&cfg.url)
        .with_background_color((15, 27, 36, 255))
        .with_devtools(false)
        .with_new_window_req_handler(|url, _features| {
            gui_server::open_external(&url);
            NewWindowResponse::Deny
        });

    #[cfg(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    ))]
    let _webview = builder
        .build(&window)
        .map_err(|e| format!("create webview: {e}"))?;

    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    )))]
    let _webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window
            .default_vbox()
            .ok_or_else(|| "missing gtk vbox".to_string())?;
        builder
            .build_gtk(vbox)
            .map_err(|e| format!("create gtk webview: {e}"))?
    };

    (cfg.on_proxy)(proxy);

    let quit_flag = cfg.quit_flag;
    let close_question = cfg.close_question;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        let _ = &_webview;

        match event {
            Event::UserEvent(WindowEventMsg::Quit) => {
                quit_flag.store(true, Ordering::SeqCst);
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(WindowEventMsg::Focus) => {
                window.set_minimized(false);
                window.set_visible(true);
                window.set_focus();
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if quit_flag.load(Ordering::SeqCst) {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                let (title, text) = (close_question)();
                if confirm_close(&window, &title, &text) {
                    quit_flag.store(true, Ordering::SeqCst);
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });
}

#[cfg(target_os = "windows")]
fn confirm_close(window: &tao::window::Window, title: &str, text: &str) -> bool {
    use std::ffi::c_void;
    use tao::platform::windows::WindowExtWindows;

    type HWND = *mut c_void;
    const MB_YESNO: u32 = 0x0000_0004;
    const MB_ICONQUESTION: u32 = 0x0000_0020;
    const MB_DEFBUTTON2: u32 = 0x0000_0100;
    const IDYES: i32 = 6;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: HWND, text: *const u16, caption: *const u16, utype: u32) -> i32;
    }

    let hwnd = window.hwnd() as HWND;
    let w_text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let w_title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        MessageBoxW(
            hwnd,
            w_text.as_ptr(),
            w_title.as_ptr(),
            MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
        ) == IDYES
    }
}

#[cfg(not(target_os = "windows"))]
fn confirm_close(_window: &tao::window::Window, _title: &str, _text: &str) -> bool {
    true
}
