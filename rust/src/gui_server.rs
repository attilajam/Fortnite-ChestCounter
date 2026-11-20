use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde_json::Value;

use crate::gui_page;

const PREFERRED_PORT: u16 = 47613;

pub type StateFn = Arc<dyn Fn() -> Value + Send + Sync>;
pub type ActionFn = Arc<dyn Fn(&str, Value) -> Value + Send + Sync>;
pub type ReportFn = Arc<dyn Fn() -> Option<String> + Send + Sync>;

pub struct GuiServer {
    pub port: u16,
    pub token: String,
    stop: Arc<AtomicBool>,
}

impl GuiServer {
    pub fn start(
        state_fn: StateFn,
        action_fn: ActionFn,
        report_fn: ReportFn,
        data_folder: PathBuf,
        report_path: PathBuf,
        overlay_path: PathBuf,
    ) -> Result<Self, String> {
        let listener = TcpListener::bind(("127.0.0.1", PREFERRED_PORT))
            .or_else(|_| TcpListener::bind(("127.0.0.1", 0)))
            .map_err(|e| format!("bind local GUI server: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("local_addr: {e}"))?
            .port();
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("set_nonblocking: {e}"))?;

        let token = generate_token();
        let stop = Arc::new(AtomicBool::new(false));

        let srv_token = token.clone();
        let srv_stop = Arc::clone(&stop);
        let refresh_re = Arc::new(
            Regex::new(r#"<meta http-equiv="refresh"[^>]*>"#).expect("valid regex"),
        );

        thread::Builder::new()
            .name("gui-server".to_string())
            .spawn(move || {
                while !srv_stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let st = Arc::clone(&state_fn);
                            let act = Arc::clone(&action_fn);
                            let fragment = Arc::clone(&report_fn);
                            let tok = srv_token.clone();
                            let df = data_folder.clone();
                            let rp = report_path.clone();
                            let op = overlay_path.clone();
                            let re = Arc::clone(&refresh_re);
                            thread::spawn(move || {
                                serve_client(stream, port, &tok, &st, &act, &fragment, &df, &rp, &op, &re);
                            });
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(25));
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| format!("spawn gui-server: {e}"))?;

        Ok(Self { port, token, stop })
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/?t={}", self.port, self.token)
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Fallback browser opener: tries Edge/Chrome `--app=` on Windows, else default browser.
    pub fn open_fallback(&self) {
        open_app_or_browser(&self.url());
    }
}

impl Drop for GuiServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn generate_token() -> String {
    let mut bytes = [0u8; 16];
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let pid = std::process::id() as u64;
    let mut s0 = now.as_nanos() as u64 ^ pid.wrapping_mul(0x9e3779b97f4a7c15);
    let mut s1 = (now.as_nanos() >> 64) as u64 ^ (&bytes as *const _ as usize as u64);
    if s0 == 0 && s1 == 0 {
        s0 = 0x1234_5678_9abc_def0;
    }
    // SplitMix64 mixer (plus /dev/urandom on unix if available)
    #[cfg(unix)]
    {
        if let Ok(mut f) = fs::File::open("/dev/urandom") {
            let _ = f.read_exact(&mut bytes);
            return bytes.iter().map(|b| format!("{b:02x}")).collect();
        }
    }
    for chunk in bytes.chunks_exact_mut(8) {
        s0 = s0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = s0 ^ s1;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^= z >> 31;
        s1 = s1.rotate_left(17) ^ z;
        chunk.copy_from_slice(&z.to_le_bytes());
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[allow(clippy::too_many_arguments)]
fn serve_client(
    mut stream: TcpStream,
    port: u16,
    token: &str,
    state_fn: &StateFn,
    action_fn: &ActionFn,
    report_fn: &ReportFn,
    data_folder: &Path,
    report_path: &Path,
    overlay_path: &Path,
    refresh_re: &Regex,
) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let Some((method, target, headers, body)) = read_http_request(&mut stream) else {
        return;
    };

    let host = headers.get("host").map(|s| s.as_str()).unwrap_or("");
    let expected_ip = format!("127.0.0.1:{port}");
    let expected_lh = format!("localhost:{port}");
    if host != expected_ip && host != expected_lh {
        write_response(&mut stream, 403, "text/plain", b"Forbidden");
        return;
    }

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target.as_str(), ""),
    };
    let token_param = format!("t={token}");
    let token_ok = headers.get("x-token").map(|s| s.as_str()) == Some(token)
        || query.split('&').any(|part| part == token_param);

    if method == "GET" && path == "/" && token_ok {
        let html = gui_page::html(token);
        write_response(&mut stream, 200, "text/html; charset=utf-8", html.as_bytes());
    } else if method == "GET" && path == "/api/state" && token_ok {
        let val = state_fn();
        let bytes = serde_json::to_vec(&val).unwrap_or_else(|_| b"{}".to_vec());
        write_response(&mut stream, 200, "application/json; charset=utf-8", &bytes);
    } else if method == "POST" && path == "/api/action" && token_ok {
        let parsed: Value = if body.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_slice(&body).unwrap_or_else(|_| serde_json::json!({}))
        };
        let name = parsed
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let val = parsed.get("value").cloned().unwrap_or(Value::Null);
        let result = action_fn(name, val);
        let resp = serde_json::json!({ "ok": true, "result": result });
        let bytes = serde_json::to_vec(&resp).unwrap_or_else(|_| b"{\"ok\":true}".to_vec());
        write_response(&mut stream, 200, "application/json; charset=utf-8", &bytes);
    } else if method == "GET" && path == "/api/report" && token_ok {
        if let Some(html) = report_fn() {
            write_response(&mut stream, 200, "text/html; charset=utf-8", html.as_bytes());
        } else {
            write_response(&mut stream, 503, "text/plain", b"Report unavailable");
        }
    } else if method == "GET" && path == "/report" && token_ok && report_path.exists() {
        if let Ok(html) = fs::read_to_string(report_path) {
            let cleaned = refresh_re.replace_all(&html, "");
            write_response(
                &mut stream,
                200,
                "text/html; charset=utf-8",
                cleaned.as_bytes(),
            );
        } else {
            write_response(&mut stream, 404, "text/plain", b"Not found");
        }
    } else if method == "GET" && path == "/overlay" && token_ok && overlay_path.exists() {
        if let Ok(bytes) = fs::read(overlay_path) {
            write_response(&mut stream, 200, "text/html; charset=utf-8", &bytes);
        } else {
            write_response(&mut stream, 404, "text/plain", b"Not found");
        }
    } else if method == "GET" && path.starts_with("/images/") {
        let raw = percent_decode(&path["/images/".len()..]);
        let fname = Path::new(&raw)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let file = data_folder.join("images").join(fname);
        if !fname.is_empty() && file.is_file() {
            if let Ok(bytes) = fs::read(&file) {
                let ctype = if fname.to_ascii_lowercase().ends_with(".png") {
                    "image/png"
                } else {
                    "image/webp"
                };
                write_response(&mut stream, 200, ctype, &bytes);
                return;
            }
        }
        write_response(&mut stream, 404, "text/plain", b"Not found");
    } else {
        write_response(
            &mut stream,
            if token_ok { 404 } else { 403 },
            "text/plain",
            b"No",
        );
    }
}

fn read_http_request(
    stream: &mut TcpStream,
) -> Option<(String, String, HashMap<String, String>, Vec<u8>)> {
    let mut buf = Vec::with_capacity(1024);
    let mut one = [0u8; 1];
    while buf.len() < 32768 {
        match stream.read(&mut one) {
            Ok(1) => buf.push(one[0]),
            _ => return None,
        }
        let n = buf.len();
        if n >= 4 && &buf[n - 4..] == b"\r\n\r\n" {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.split("\r\n");
    let first_line = lines.next()?;
    let mut parts = first_line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();

    let mut headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    let content_len: usize = headers
        .get("content-length")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0)
        .min(1 << 20);
    let mut body = vec![0u8; content_len];
    if content_len > 0 && stream.read_exact(&mut body).is_err() {
        return None;
    }

    Some((method, target, headers, body))
}

fn write_response(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) {
    let reason = match status {
        200 => "OK",
        403 => "Forbidden",
        503 => "Service Unavailable",
        _ => "Not Found",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(val) = u8::from_str_radix(hex, 16) {
                    out.push(val);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn open_external(target: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", target])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(target).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    }
}

pub fn open_app_or_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let candidates = [
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        ];
        for exe in candidates {
            if Path::new(exe).exists() {
                if std::process::Command::new(exe)
                    .arg(format!("--app={url}"))
                    .arg("--window-size=1180,860")
                    .spawn()
                    .is_ok()
                {
                    return;
                }
            }
        }
    }
    open_external(url);
}

pub fn copy_to_clipboard(text: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::c_void;
        use std::ptr::{copy_nonoverlapping, null_mut};

        const CF_UNICODETEXT: u32 = 13;
        const GMEM_MOVEABLE: u32 = 0x0002;

        #[link(name = "user32")]
        unsafe extern "system" {
            fn OpenClipboard(hwnd: *mut c_void) -> i32;
            fn EmptyClipboard() -> i32;
            fn SetClipboardData(format: u32, mem: *mut c_void) -> *mut c_void;
            fn CloseClipboard() -> i32;
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GlobalAlloc(flags: u32, bytes: usize) -> *mut c_void;
            fn GlobalLock(mem: *mut c_void) -> *mut c_void;
            fn GlobalUnlock(mem: *mut c_void) -> i32;
            fn GlobalFree(mem: *mut c_void) -> *mut c_void;
        }

        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let byte_len = wide.len() * std::mem::size_of::<u16>();
        unsafe {
            if OpenClipboard(null_mut()) == 0 {
                return false;
            }
            let _ = EmptyClipboard();
            let hmem = GlobalAlloc(GMEM_MOVEABLE, byte_len);
            if hmem.is_null() {
                let _ = CloseClipboard();
                return false;
            }
            let dst = GlobalLock(hmem) as *mut u16;
            if dst.is_null() {
                let _ = GlobalFree(hmem);
                let _ = CloseClipboard();
                return false;
            }
            copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
            let _ = GlobalUnlock(hmem);
            let ok = !SetClipboardData(CF_UNICODETEXT, hmem).is_null();
            if !ok {
                let _ = GlobalFree(hmem);
            }
            let _ = CloseClipboard();
            ok
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = text;
        false
    }
}
