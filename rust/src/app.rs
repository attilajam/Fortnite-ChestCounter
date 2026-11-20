use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, FixedOffset, Utc};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::{Value, json};
use tao::event_loop::EventLoopProxy;

use crate::catalog::TreasureCatalog;
use crate::gui_page;
use crate::gui_server::{self, GuiServer};
use crate::gui_window::{self, WindowConfig, WindowEventMsg};
use crate::lang::LangCtx;
use crate::models::{Config, ManualEntry, Store, TrackerData, default_demos_folder};
use crate::native_overlay::{self, OverlayRenderData};
use crate::overlay::{OverlayAppearance, build_overlay};
use crate::profiles;
use crate::report::{self, ReportContext, ScanProgress, is_this_session, pretty};
use crate::tracker::{
    LiveUpdateEvent, ScanOutcome, TrackerState, analyze_entry_detached, apply_entry_result,
};

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO_URL: &str = "https://github.com/Attila20/Fortnite-ChestCounter";
const ISSUES_URL: &str = "https://github.com/Attila20/Fortnite-ChestCounter/issues";

#[derive(Debug, Clone)]
pub struct ActivitySpan {
    pub text: String,
    pub class: &'static str,
}

#[derive(Default)]
pub struct ActivityLog {
    lines: VecDeque<Vec<ActivitySpan>>,
}

impl ActivityLog {
    pub fn push(&mut self, class: &'static str, text: impl Into<String>) {
        if self.lines.len() >= 120 {
            self.lines.pop_front();
        }
        self.lines.push_back(vec![ActivitySpan {
            text: text.into(),
            class,
        }]);
    }

    pub fn last_json(&self, max: usize) -> Vec<Value> {
        let skip = self.lines.len().saturating_sub(max);
        self.lines
            .iter()
            .skip(skip)
            .map(|line| {
                Value::Array(
                    line.iter()
                        .map(|s| json!({ "t": s.text, "c": s.class }))
                        .collect(),
                )
            })
            .collect()
    }
}

pub struct AppOptions {
    pub store: Store,
    pub demos_override: Option<String>,
    pub lang_override: Option<String>,
    pub clock_override: Option<String>,
    pub tz_override: Option<FixedOffset>,
    pub utc_override: bool,
    pub now_override: Option<DateTime<Utc>>,
    pub session_start_override: Option<DateTime<Utc>>,
}

struct SharedState {
    store: Store,
    config: Config,
    data: TrackerData,
    catalog: TreasureCatalog,
    lang: LangCtx,
    tz_override: Option<FixedOffset>,
    utc_override: bool,
    clock_override: Option<String>,
    session_start_utc: DateTime<Utc>,
    tracker: TrackerState,
    progress: Option<ScanProgress>,
    report_revision: u64,
    first_run: bool,
    windowed: bool,
    log: ActivityLog,
    demos_dirty: bool,
    window_proxy: Option<EventLoopProxy<WindowEventMsg>>,
}

impl SharedState {
    fn rebuild_lang(&mut self) {
        let utc_times = self.utc_override || self.config.time_zone.eq_ignore_ascii_case("utc");
        let clock = self
            .clock_override
            .as_deref()
            .unwrap_or(&self.config.clock_format);
        let mut lang = LangCtx::new(self.config.language.as_deref(), utc_times, clock);
        if !utc_times {
            lang.fixed_local_offset = self.tz_override;
        }
        self.lang = lang;
    }

    fn app_name(&self) -> &'static str {
        self.lang.t(
            "Compteur de coffres Fortnite",
            "Fortnite Chest Counter",
            "Contador de cofres de Fortnite",
        )
    }

    fn report_context<'a>(&'a self, images: &'a Path) -> ReportContext<'a> {
        ReportContext {
            now_utc: Utc::now(),
            session_start_utc: self.session_start_utc,
            progress: self.progress,
            lang: &self.lang,
            catalog: &self.catalog,
            images_folder: images.is_dir().then_some(images),
        }
    }

    fn write_reports(&mut self) {
        let images = self.store.images_folder();
        let ctx = self.report_context(&images);
        let _ = report::write_all(&self.store, &self.config, &self.data, &ctx);
        self.report_revision += 1;
    }

    fn build_report_fragment(&self) -> String {
        let images = self.store.images_folder();
        report::report_fragment(&self.config, &self.data, &self.report_context(&images))
    }

    fn update_overlay(&self) {
        let view = profiles::view(&self.data, &self.config, &self.lang);
        let ov = build_overlay(
            &view,
            self.tracker.live_match.as_ref(),
            self.tracker.is_live(),
            self.config.show_squad_total,
            self.session_start_utc,
            &self.lang,
        );
        let _ = report::write_overlay_file(&self.store, &ov, &self.config.overlay_style, &self.lang);
        if native_overlay::is_running() {
            native_overlay::update(OverlayRenderData::from_view(ov, &self.lang));
        }
    }

    fn build_gui_state(&self) -> Value {
        let view = profiles::view(&self.data, &self.config, &self.lang);
        let profile = profiles::current(&self.data, &self.config, &self.lang);
        let demos_exists = !self.config.demos_folder.is_empty()
            && Path::new(&self.config.demos_folder).is_dir();

        let (status_kind, status_text) = if self.tracker.is_live() {
            let txt = if let Some(ref live) = self.tracker.live_match {
                let c = self.lang.count(live.chests_opened, "coffre", "chest", "cofre");
                format!(
                    "{}{c}",
                    self.lang
                        .t("Partie en cours : ", "Match in progress: ", "Partida en curso: ")
                )
            } else {
                self.lang
                    .t("Partie en cours", "Match in progress", "Partida en curso")
                    .to_string()
            };
            ("live", txt)
        } else if self.tracker.is_finishing
            || self.tracker.has_pending
            || self.progress.is_some()
        {
            (
                "busy",
                self.lang
                    .t(
                        "Comptage des coffres…",
                        "Counting the chests…",
                        "Contando los cofres…",
                    )
                    .to_string(),
            )
        } else if !demos_exists {
            (
                "warn",
                self.lang
                    .t(
                        "Dossier des replays introuvable",
                        "Replays folder not found",
                        "Carpeta de repeticiones no encontrada",
                    )
                    .to_string(),
            )
        } else {
            (
                "",
                self.lang
                    .t(
                        "En attente d'une partie",
                        "Waiting for a match",
                        "Esperando una partida",
                    )
                    .to_string(),
            )
        };

        let last = view
            .matches
            .iter()
            .max_by_key(|m| m.match_start_utc.unwrap_or(m.analyzed_at_utc));
        let session_matches: Vec<_> = view
            .matches
            .iter()
            .filter(|m| is_this_session(m, self.session_start_utc))
            .collect();
        let session_chests: i32 = session_matches.iter().map(|m| m.chests_opened).sum();
        let treasures: usize = view.matches.iter().map(|m| m.treasure_finds.len()).sum();
        let avg = if !view.matches.is_empty() {
            self.lang.format_float(
                view.counted_chests() as f64 / view.matches.len() as f64,
                1,
                false,
            )
        } else {
            "0".to_string()
        };

        let manual_history: Vec<Value> = profiles::manual_history(&self.data, &self.config, &self.lang)
            .into_iter()
            .map(|e| {
                let what = if e.kind == ManualEntry::ESTIMATE {
                    let n = self.lang.n(e.career_matches.unwrap_or(0) as i64);
                    format!(
                        "{} · {} {}",
                        self.lang.t("estimation", "estimate", "estimación"),
                        n,
                        self.lang.t("parties", "matches", "partidas")
                    )
                } else if e.kind == ManualEntry::EARLIER {
                    self.lang
                        .t("ajouts précédents", "earlier additions", "añadidos anteriores")
                        .to_string()
                } else if let Some(wf) = e.week_from_utc {
                    if let Some(wk) = self.catalog.weeks.iter().find(|w| w.from == wf) {
                        format!(
                            "{} {}",
                            self.lang.t("semaine", "week", "semana"),
                            wk.number
                        )
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                let when = e
                    .added_utc
                    .map(|t| self.lang.date_time_short(self.lang.shown(t)))
                    .unwrap_or_default();
                json!({
                    "id": e.id,
                    "chests": self.lang.signed_n(e.chests as i64),
                    "what": what,
                    "when": when,
                })
            })
            .collect();

        let last_json = last.map(|m| {
            json!({
                "chests": self.lang.n(m.chests_opened as i64),
                "when": self.lang.date_time_short(self.lang.shown(m.start_utc())),
                "mode": pretty::playlist(m.playlist.as_deref(), &self.lang),
            })
        });

        let squad_json = if self.config.show_squad_total && view.team_chests() > 0 {
            Some(json!({
                "total": self.lang.n(view.squad_chests() as i64),
                "team": self.lang.n(view.team_chests() as i64),
            }))
        } else {
            None
        };

        let profs_json: Vec<Value> = profiles::list(&self.data, &self.lang)
            .into_iter()
            .map(|p| {
                json!({
                    "key": p.key,
                    "name": p.name,
                    "matches": self.lang.n(p.matches as i64),
                    "chests": self.lang.n(p.chests as i64),
                })
            })
            .collect();

        let now_utc = Utc::now();
        let notices_json: Vec<Value> = self
            .catalog
            .active_notices(now_utc)
            .into_iter()
            .filter(|n| !self.config.dismissed_notices.contains(&n.id))
            .map(|n| {
                json!({
                    "id": n.id,
                    "level": n.level,
                    "title": n.title_in(self.lang.code()),
                    "text": n.text_in(self.lang.code()),
                    "link": n.link,
                })
            })
            .collect();

        let overlay_full = fs::canonicalize(self.store.overlay_path())
            .unwrap_or_else(|_| self.store.overlay_path())
            .to_string_lossy()
            .into_owned();

        let err_log = self.store.error_log_path();
        let err_log_str = if err_log.exists() {
            Some(err_log.to_string_lossy().into_owned())
        } else {
            None
        };

        let local_zone = self.lang.local_offset_string(now_utc);
        let manual_week = self.catalog.week_of(now_utc).map(|w| w.number);
        let manual_text = if view.manual_adjust > 0 {
            format!("+{}", self.lang.n(view.manual_adjust as i64))
        } else {
            self.lang.n(view.manual_adjust as i64)
        };

        json!({
            "txt": gui_page::texts(&self.lang),
            "lang": self.lang.code(),
            "appName": self.app_name(),
            "version": APP_VERSION,
            "player": profile.as_ref().map(|p| p.name.as_str()),
            "firstRun": self.first_run,
            "autoClose": false,
            "status": { "kind": status_kind, "text": status_text },
            "total": self.lang.n(view.total_chests() as i64),
            "manual": view.manual_adjust,
            "manualText": manual_text,
            "manualWeek": manual_week,
            "manualHistory": manual_history,
            "session": self.lang.n(session_chests as i64),
            "sessionMatches": self.lang.n(session_matches.len() as i64),
            "last": last_json,
            "squad": squad_json,
            "treasures": self.lang.n(treasures as i64),
            "matches": self.lang.n(view.matches.len() as i64),
            "average": avg,
            "progress": self.progress.map(|p| json!({ "done": p.done, "total": p.total })),
            "update": Value::Null,
            "whatsNew": Value::Null,
            "updatesOn": false,
            "activity": self.log.last_json(60),
            "overlay": {
                "show": self.config.show_overlay,
                "running": native_overlay::is_running(),
                "visible": native_overlay::is_visible(),
                "locked": native_overlay::is_locked(),
                "style": self.config.overlay_style,
                "size": self.config.overlay_size,
                "opacity": self.config.overlay_opacity,
                "sizes": OverlayAppearance::SIZES,
                "opacities": OverlayAppearance::OPACITIES,
                "squad": self.config.show_squad_total,
            },
            "profiles": profs_json,
            "profileKey": profile.map(|p| p.key),
            "settings": {
                "language": self.lang.code(),
                "languages": [["fr", "Français"], ["en", "English"], ["es", "Español"]],
                "windowed": self.windowed,
                "minimizeToTray": self.config.minimize_to_tray,
                "startWithFortnite": false,
                "canStartWithFortnite": false,
                "updateMode": self.config.update_mode,
                "timeZone": self.config.time_zone,
                "clockFormat": self.config.clock_format,
                "localZone": local_zone,
                "windowsClock": if self.lang.clock_24 { "24" } else { "12" },
                "demos": self.config.demos_folder,
                "demosExists": demos_exists,
            },
            "paths": {
                "data": self.store.folder.to_string_lossy(),
                "overlay": overlay_full,
                "errorLog": err_log_str,
            },
            "canUninstall": false,
            "notices": notices_json,
            "reportRevision": self.report_revision,
        })
    }
}

struct WakeSignal {
    lock: Mutex<bool>,
    cv: Condvar,
}

impl WakeSignal {
    fn new() -> Self {
        Self {
            lock: Mutex::new(false),
            cv: Condvar::new(),
        }
    }

    fn wake(&self) {
        if let Ok(mut flag) = self.lock.lock() {
            *flag = true;
            self.cv.notify_all();
        }
    }

    fn wait_timeout(&self, dur: Duration) -> bool {
        let Ok(mut flag) = self.lock.lock() else {
            thread::sleep(dur);
            return false;
        };
        if *flag {
            *flag = false;
            return true;
        }
        let (mut guard, _) = self.cv.wait_timeout(flag, dur).unwrap();
        let was_woken = *guard;
        *guard = false;
        was_woken
    }
}

fn start_overlay_for(shared: &Arc<Mutex<SharedState>>) {
    let (x, y, locked, style, size, opacity, render_data) = {
        let Ok(mut st) = shared.lock() else { return };
        if !native_overlay::STYLES.contains(&st.config.overlay_style.as_str()) {
            st.config.overlay_style = "hud".to_string();
        }
        let view = profiles::view(&st.data, &st.config, &st.lang);
        let ov = build_overlay(
            &view,
            st.tracker.live_match.as_ref(),
            st.tracker.is_live(),
            st.config.show_squad_total,
            st.session_start_utc,
            &st.lang,
        );
        let rd = OverlayRenderData::from_view(ov, &st.lang);
        (
            st.config.overlay_x,
            st.config.overlay_y,
            st.config.overlay_locked,
            st.config.overlay_style.clone(),
            st.config.overlay_size,
            st.config.overlay_opacity,
            rd,
        )
    };

    native_overlay::set_appearance(size, opacity);
    native_overlay::set_style(&style);
    native_overlay::update(render_data);

    let s_moved = Arc::clone(shared);
    let s_lock = Arc::clone(shared);
    let s_log = Arc::clone(shared);
    native_overlay::start(
        x,
        y,
        locked,
        Box::new(move |nx, ny| {
            if let Ok(mut st) = s_moved.lock() {
                st.config.overlay_x = Some(nx);
                st.config.overlay_y = Some(ny);
                let _ = st.store.save_config(&st.config);
            }
        }),
        Box::new(move |is_locked| {
            if let Ok(mut st) = s_lock.lock() {
                st.config.overlay_locked = is_locked;
                let _ = st.store.save_config(&st.config);
                let msg = if is_locked {
                    st.lang.t(
                        "Overlay verrouillé : la souris passe au travers (Ctrl+Maj+L pour déverrouiller).",
                        "Overlay locked: the mouse goes through it (Ctrl+Shift+L to unlock).",
                        "Overlay bloqueado: el ratón lo atraviesa (Ctrl+Mayús+L para desbloquear).",
                    )
                } else {
                    st.lang.t(
                        "Overlay déverrouillé : clic gauche maintenu pour le déplacer.",
                        "Overlay unlocked: hold left mouse button to move it.",
                        "Overlay desbloqueado: mantén clic izquierdo para moverlo.",
                    )
                };
                st.log.push("mut", msg);
            }
        }),
        Box::new(move |cls, msg| {
            if let Ok(mut st) = s_log.lock() {
                let c = match cls {
                    "err" => "err",
                    "warn" => "warn",
                    "ok" => "ok",
                    "info" => "info",
                    "tre" => "tre",
                    _ => "mut",
                };
                st.log.push(c, msg);
            }
        }),
    );
}

fn run_gui_action(
    shared: &Arc<Mutex<SharedState>>,
    wake: &Arc<WakeSignal>,
    quit: &Arc<AtomicBool>,
    name: &str,
    value: Value,
) -> Value {
    let as_bool = || value.as_bool().unwrap_or(false);
    let as_str = || match &value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    let as_i32 = || {
        value
            .as_i64()
            .map(|n| n as i32)
            .or_else(|| profiles::parse_estimate_number(&as_str()))
            .unwrap_or(0)
    };

    let mut start_ov = false;
    let mut stop_ov = false;
    let mut wake_now = false;
    let mut settings_changed = true;
    let mut result = Value::Null;

    {
        let Ok(mut st) = shared.lock() else {
            return json!({ "error": "lock error" });
        };

        match name {
            "dismissWelcome" => {
                st.first_run = false;
                settings_changed = false;
            }
            "dismissNotice" => {
                let id = as_str();
                if st
                    .catalog
                    .notices
                    .iter()
                    .any(|n| n.id == id && n.level == "info")
                    && !st.config.dismissed_notices.contains(&id)
                {
                    st.config.dismissed_notices.push(id);
                    let _ = st.store.save_config(&st.config);
                }
                settings_changed = false;
            }
            "toggleOverlayVisible" => {
                native_overlay::toggle_visible();
                settings_changed = false;
            }
            "toggleOverlayLock" => {
                native_overlay::toggle_lock();
                settings_changed = false;
            }
            "showOverlay" => {
                let want = as_bool();
                if want != st.config.show_overlay {
                    st.config.show_overlay = want;
                    if want {
                        start_ov = true;
                    } else {
                        stop_ov = true;
                    }
                }
            }
            "overlayStyle" => {
                let s = as_str();
                if native_overlay::STYLES.contains(&s.as_str()) {
                    st.config.overlay_style = s.clone();
                    native_overlay::set_style(&s);
                }
            }
            "overlaySize" => {
                st.config.overlay_size = OverlayAppearance::clamp_size(as_i32());
                native_overlay::set_appearance(st.config.overlay_size, st.config.overlay_opacity);
            }
            "overlayOpacity" => {
                st.config.overlay_opacity = OverlayAppearance::clamp_opacity(as_i32());
                native_overlay::set_appearance(st.config.overlay_size, st.config.overlay_opacity);
            }
            "showSquadTotal" => {
                st.config.show_squad_total = as_bool();
            }
            "showSquadInReport" => {
                st.config.show_squad_in_report = as_bool();
                let _ = st.store.save_config(&st.config);
                settings_changed = false;
            }
            "showTeamInReport" => {
                st.config.show_team_treasures_in_report = as_bool();
                let _ = st.store.save_config(&st.config);
                settings_changed = false;
            }
            "language" => {
                let s = as_str();
                if let Some(norm) = crate::lang::normalize_lang(Some(&s)) {
                    st.config.language = Some(norm.to_string());
                    st.rebuild_lang();
                }
            }
            "profile" => {
                let k = as_str();
                if profiles::list(&st.data, &st.lang).iter().any(|p| p.key == k) {
                    st.config.profile = Some(k);
                }
            }
            "minimizeToTray" => {
                st.config.minimize_to_tray = as_bool();
            }
            "timeZone" => {
                st.config.time_zone = as_str();
                st.utc_override = false;
                st.rebuild_lang();
            }
            "clockFormat" => {
                st.config.clock_format = as_str();
                st.clock_override = None;
                st.rebuild_lang();
            }
            "demosFolder" => {
                let folder = as_str().trim().trim_matches('"').to_string();
                if !folder.is_empty() && folder != st.config.demos_folder {
                    st.config.demos_folder = folder;
                    st.demos_dirty = true;
                    wake_now = true;
                }
            }
            "manualChests" => {
                let view = profiles::view(&st.data, &st.config, &st.lang);
                let clamped = as_i32().clamp(-view.counted_chests(), 10_000_000);
                let cfg = st.config.clone();
                let lang = st.lang.clone();
                profiles::set_manual(&mut st.data, &cfg, &lang, clamped);
                let _ = st.store.save_data(&st.data);
            }
            "manualAdd" | "manualEstimate" => {
                let view = profiles::view(&st.data, &st.config, &st.lang);
                let (raw_chests, career, this_week) = if let Some(obj) = value.as_object() {
                    (
                        obj.get("chests").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                        obj.get("career").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                        obj.get("thisWeek").and_then(|v| v.as_bool()).unwrap_or(false),
                    )
                } else {
                    (as_i32(), 0, false)
                };
                let prev_est: i32 = if name == "manualEstimate" {
                    profiles::manual_history(&st.data, &st.config, &st.lang)
                        .into_iter()
                        .filter(|e| e.kind == ManualEntry::ESTIMATE)
                        .map(|e| e.chests)
                        .sum()
                } else {
                    0
                };
                let current = view.manual_adjust - prev_est;
                let chests = raw_chests.clamp(-view.counted_chests() - current, 10_000_000 - current);
                if chests != 0 || prev_est != 0 {
                    let week = if this_week && name == "manualAdd" {
                        st.catalog.week_of(Utc::now()).map(|w| w.from)
                    } else {
                        None
                    };
                    let cfg = st.config.clone();
                    let lang = st.lang.clone();
                    profiles::add_manual(
                        &mut st.data,
                        &cfg,
                        &lang,
                        chests,
                        if name == "manualEstimate" { Some(career) } else { None },
                        week,
                    );
                    let _ = st.store.save_data(&st.data);
                }
            }
            "manualRemove" => {
                let id = as_str();
                let cfg = st.config.clone();
                let lang = st.lang.clone();
                if profiles::remove_manual(&mut st.data, &cfg, &lang, &id) {
                    let _ = st.store.save_data(&st.data);
                }
            }
            "estimate" => {
                let view = profiles::view(&st.data, &st.config, &st.lang);
                settings_changed = false;
                if view.matches.len() < profiles::MIN_COUNTED_MATCHES {
                    let tmpl = gui_page::texts(&st.lang)["estimateTooFew"];
                    result = json!({
                        "error": tmpl.replace("{0}", &profiles::MIN_COUNTED_MATCHES.to_string())
                    });
                } else if let Some(est) = profiles::estimate_from_career(&view, as_i32()) {
                    let avg = st.lang.format_float(est.average, 1, false);
                    let cm = st.lang.n(est.career_matches as i64);
                    let ct = st.lang.n(est.counted_matches as i64);
                    let ch = st.lang.n(est.chests as i64);
                    let text = match st.lang.code() {
                        "en" => format!(
                            "({cm} − {ct} already counted) × {avg} chests on average = {ch} chests (replaces the previous estimate)"
                        ),
                        "es" => format!(
                            "({cm} − {ct} ya contadas) × {avg} cofres de media = {ch} cofres (sustituye la estimación anterior)"
                        ),
                        _ => format!(
                            "({cm} − {ct} déjà comptées) × {avg} coffres en moyenne = {ch} coffres (remplace l'estimation précédente)"
                        ),
                    };
                    result = json!({
                        "chests": est.chests,
                        "career": est.career_matches,
                        "chestsText": ch,
                        "text": text,
                    });
                } else {
                    result = json!({
                        "error": gui_page::texts(&st.lang)["estimateNothing"]
                    });
                }
            }
            "obtained" => {
                if let Some(obj) = value.as_object() {
                    if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
                        let on = obj.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
                        let cfg = st.config.clone();
                        let lang = st.lang.clone();
                        profiles::set_obtained(&mut st.data, &cfg, &lang, id, on);
                        let _ = st.store.save_data(&st.data);
                        st.write_reports();
                    }
                }
                settings_changed = false;
            }
            "copyObsPath" => {
                let p = fs::canonicalize(st.store.overlay_path())
                    .unwrap_or_else(|_| st.store.overlay_path())
                    .to_string_lossy()
                    .into_owned();
                gui_server::copy_to_clipboard(&p);
                settings_changed = false;
            }
            "openDataFolder" => {
                let p = st.store.folder.to_string_lossy().into_owned();
                gui_server::open_external(&p);
                settings_changed = false;
            }
            "openErrorLog" => {
                let p = st.store.error_log_path();
                if p.exists() {
                    gui_server::open_external(&p.to_string_lossy());
                }
                settings_changed = false;
            }
            "openIssues" => {
                gui_server::open_external(ISSUES_URL);
                settings_changed = false;
            }
            "openRepo" => {
                gui_server::open_external(REPO_URL);
                settings_changed = false;
            }
            "quit" => {
                quit.store(true, Ordering::SeqCst);
                if let Some(ref proxy) = st.window_proxy {
                    let _ = proxy.send_event(WindowEventMsg::Quit);
                }
                wake_now = true;
                settings_changed = false;
            }
            _ => {
                settings_changed = false;
            }
        }

        if settings_changed {
            let _ = st.store.save_config(&st.config);
            st.write_reports();
            st.update_overlay();
        }
    }

    if start_ov {
        start_overlay_for(shared);
    } else if stop_ov {
        native_overlay::stop();
    }
    if wake_now {
        wake.wake();
    }

    result
}

fn spawn_watcher_thread(
    shared: Arc<Mutex<SharedState>>,
    wake: Arc<WakeSignal>,
    quit: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("replay-watcher".to_string())
        .spawn(move || {
            let mut fs_watcher: Option<RecommendedWatcher> = None;
            let mut watched_dir = String::new();
            let fs_fired = Arc::new(AtomicBool::new(false));

            let attach_watcher = |dir: &str,
                                  current: &mut Option<RecommendedWatcher>,
                                  watched: &mut String,
                                  wake_sig: &Arc<WakeSignal>,
                                  fired: &Arc<AtomicBool>| {
                if dir == watched.as_str() && current.is_some() {
                    return;
                }
                *current = None;
                watched.clear();
                if dir.is_empty() || !Path::new(dir).is_dir() {
                    return;
                }
                let w_sig = Arc::clone(wake_sig);
                let w_fired = Arc::clone(fired);
                if let Ok(mut w) = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                    if let Ok(ev) = res {
                        let relevant = matches!(
                            ev.kind,
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any
                        ) && ev.paths.iter().any(|p| {
                            p.extension()
                                .and_then(|e| e.to_str())
                                .is_some_and(|e| e.eq_ignore_ascii_case("replay"))
                        });
                        if relevant {
                            w_fired.store(true, Ordering::Relaxed);
                            w_sig.wake();
                        }
                    }
                }) {
                    if w.watch(Path::new(dir), RecursiveMode::NonRecursive).is_ok() {
                        *current = Some(w);
                        *watched = dir.to_string();
                    }
                }
            };

            let mut next_scan = Instant::now() + Duration::from_secs(1);

            while !quit.load(Ordering::SeqCst) {
                // Attach or re-attach FS watcher if demos folder changed or appeared
                let demos_dir = {
                    let Ok(mut st) = shared.lock() else { break };
                    if st.demos_dirty {
                        st.demos_dirty = false;
                        fs_watcher = None;
                        watched_dir.clear();
                        next_scan = Instant::now();
                    }
                    st.config.demos_folder.clone()
                };
                attach_watcher(&demos_dir, &mut fs_watcher, &mut watched_dir, &wake, &fs_fired);

                if Instant::now() >= next_scan {
                    // 1. Collect ready replays under lock
                    let (ready, catalog, lang) = {
                        let Ok(mut st) = shared.lock() else { break };
                        let SharedState {
                            config,
                            data,
                            tracker,
                            catalog,
                            lang,
                            ..
                        } = &mut *st;
                        let ready = tracker.collect_ready(&config.demos_folder, data, false);
                        (ready, catalog.clone(), lang.clone())
                    };

                    // 2. Analyze each ready replay WITHOUT holding the lock (keeps UI responsive!)
                    if !ready.is_empty() {
                        let batch = ready.len() >= 2;
                        let mut added_any = false;
                        for (idx, entry) in ready.iter().enumerate() {
                            if quit.load(Ordering::SeqCst) {
                                break;
                            }
                            if batch {
                                if let Ok(mut st) = shared.lock() {
                                    st.progress = Some(ScanProgress {
                                        done: idx,
                                        total: ready.len(),
                                    });
                                    st.write_reports();
                                }
                            } else if let Ok(mut st) = shared.lock() {
                                let fname = entry
                                    .path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("replay.replay");
                                let mb = st.lang.format_float(
                                    entry.len as f64 / (1024.0 * 1024.0),
                                    1,
                                    false,
                                );
                                let msg = match st.lang.code() {
                                    "en" => format!("Analyzing {fname} ({mb} MB)…"),
                                    "es" => format!("Analizando {fname} ({mb} MB)…"),
                                    _ => format!("Analyse de {fname} ({mb} Mo)…"),
                                };
                                st.log.push("mut", msg);
                            }

                            let analyzed = analyze_entry_detached(entry, &catalog, &lang);

                            if let Ok(mut st) = shared.lock() {
                                let SharedState {
                                    store,
                                    config,
                                    data,
                                    lang,
                                    tracker,
                                    log,
                                    ..
                                } = &mut *st;
                                let outcome =
                                    apply_entry_result(entry, analyzed, store, config, data, lang);
                                if tracker.is_live() {
                                    tracker.clear_live();
                                }
                                match &outcome {
                                    ScanOutcome::Added { record, updated } => {
                                        added_any = true;
                                        let c = lang.count(
                                            record.chests_opened,
                                            "coffre",
                                            "chest",
                                            "cofre",
                                        );
                                        let verb = if *updated {
                                            lang.t("Mis à jour", "Updated", "Actualizado")
                                        } else {
                                            lang.t("Compté", "Counted", "Contado")
                                        };
                                        let tr = if !record.treasure_finds.is_empty() {
                                            format!(
                                                " · ★ {}",
                                                record.treasure_finds.len()
                                            )
                                        } else {
                                            String::new()
                                        };
                                        log.push(
                                            if !record.treasure_finds.is_empty() { "tre" } else { "ok" },
                                            format!("{verb} : {c}{tr} ({})", record.replay_file),
                                        );
                                    }
                                    ScanOutcome::Skipped { file, reason } => {
                                        log.push("mut", format!("{file} — {reason}"));
                                    }
                                    ScanOutcome::Error { file, message } => {
                                        log.push("err", format!("{file} — {message}"));
                                    }
                                }
                            }
                        }

                        if let Ok(mut st) = shared.lock() {
                            st.progress = None;
                            if added_any || batch {
                                st.write_reports();
                            }
                            st.update_overlay();
                        }
                    } else if let Ok(mut st) = shared.lock() {
                        if st.progress.is_some() {
                            st.progress = None;
                            st.write_reports();
                        }
                    }

                    // 3. Check live match in progress & update overlay
                    let live_event = {
                        let Ok(mut st) = shared.lock() else { break };
                        let SharedState {
                            config,
                            data,
                            catalog,
                            lang,
                            tracker,
                            ..
                        } = &mut *st;
                        tracker.update_live(config, data, catalog, lang)
                    };
                    if let Some(ev) = live_event {
                        if let Ok(mut st) = shared.lock() {
                            match ev {
                                LiveUpdateEvent::MatchStarted { counted, map } => {
                                    let msg = if counted {
                                        st.lang
                                            .t(
                                                "Partie en cours… (ses coffres seront comptés à la fin)",
                                                "Match in progress… (its chests will be counted at the end)",
                                                "Partida en curso… (sus cofres se contarán al final)",
                                            )
                                            .to_string()
                                    } else {
                                        format!(
                                            "{} ({})",
                                            st.lang.t(
                                                "Partie en cours dans un autre mode",
                                                "Match in progress in another mode",
                                                "Partida en curso en otro modo",
                                            ),
                                            map.as_deref().unwrap_or("?")
                                        )
                                    };
                                    st.log.push("mut", msg);
                                }
                                LiveUpdateEvent::CountsChanged { chests, treasures } => {
                                    let c = st.lang.count(chests, "coffre", "chest", "cofre");
                                    let tr = if treasures > 0 {
                                        format!(" · ★ {treasures}")
                                    } else {
                                        String::new()
                                    };
                                    st.log.push("info", format!("● {c}{tr}"));
                                }
                                LiveUpdateEvent::Cleared => {}
                            }
                            st.update_overlay();
                        }
                    }

                    // Compute next scan interval
                    let wait_secs = {
                        let Ok(st) = shared.lock() else { break };
                        if st.tracker.is_finishing {
                            2
                        } else if st.tracker.has_pending || st.tracker.is_live() {
                            15
                        } else if fs_watcher.is_some() {
                            60
                        } else {
                            10
                        }
                    };
                    next_scan = Instant::now() + Duration::from_secs(wait_secs);
                }

                let now = Instant::now();
                let sleep_for = next_scan.saturating_duration_since(now);
                if sleep_for > Duration::ZERO && wake.wait_timeout(sleep_for) {
                    if fs_fired.swap(false, Ordering::Relaxed) {
                        let soon = Instant::now() + Duration::from_secs(2);
                        if soon < next_scan {
                            next_scan = soon;
                        }
                    }
                }
            }
        })
        .expect("spawn replay-watcher thread")
}

/// Launches the full desktop application (GUI window + Windows native overlay + automatic replay watcher).
pub fn run_desktop(opts: AppOptions) -> Result<(), String> {
    let first_run = !opts.store.config_exists();
    let mut config = opts.store.load_config()?;
    if let Some(d) = opts.demos_override {
        config.demos_folder = d;
    }
    if config.demos_folder.trim().is_empty() {
        config.demos_folder = default_demos_folder();
    }
    if let Some(l) = opts.lang_override {
        config.language = Some(l);
    }

    let utc_times = opts.utc_override || config.time_zone.eq_ignore_ascii_case("utc");
    let clock = opts
        .clock_override
        .as_deref()
        .unwrap_or(&config.clock_format);
    let mut lang = LangCtx::new(config.language.as_deref(), utc_times, clock);
    if !utc_times {
        lang.fixed_local_offset = opts.tz_override;
    }

    let catalog = TreasureCatalog::load(&opts.store.folder);
    let mut data = opts.store.load_data()?;
    let tracker = TrackerState::new();
    let reanalyzing = tracker.upgrade_if_needed(&opts.store, &mut data);
    let _ = opts.store.save_config(&config);

    let mut log = ActivityLog::default();
    if reanalyzing {
        log.push(
            "info",
            lang.t(
                "Nouvelle version de l'analyse : les replays encore présents vont être réanalysés (sans doublon).",
                "New analysis version: the replays still on disk will be analyzed again (no duplicates).",
                "Nueva versión del análisis: las repeticiones del disco se analizarán de nuevo (sin duplicados).",
            ),
        );
    }
    if tracker.replay_files(&config.demos_folder).is_empty() {
        let dir_exists = !config.demos_folder.is_empty() && Path::new(&config.demos_folder).is_dir();
        log.push(
            "warn",
            if dir_exists {
                lang.t(
                    "Aucun replay pour l'instant : c'est normal si tu n'as pas encore joué depuis que les replays sont activés.",
                    "No replay yet: that is expected if you have not played since replays were turned on.",
                    "Aún no hay repeticiones: es normal si no has jugado desde que activaste las repeticiones.",
                )
            } else {
                lang.t(
                    "Le dossier des replays n'existe pas encore : Fortnite le crée à la première partie enregistrée.",
                    "The replays folder does not exist yet: Fortnite creates it with the first recorded match.",
                    "La carpeta de repeticiones aún no existe: Fortnite la crea con la primera partida grabada.",
                )
            },
        );
        log.push(
            "mut",
            lang.t(
                "Vérifie dans Fortnite : Paramètres → Jeu → Rediffusions → Enregistrer les rediffusions.",
                "Check in Fortnite: Settings → Game → Replays → Record Replays.",
                "Compruébalo en Fortnite: Ajustes → Juego → Repeticiones → Grabar repeticiones.",
            ),
        );
    }

    let backlog = tracker.backlog_count(&config.demos_folder, &data);
    let initial_progress = if backlog > 1 {
        Some(ScanProgress {
            done: 0,
            total: backlog,
        })
    } else {
        None
    };

    let now_utc = opts.now_override.unwrap_or_else(Utc::now);
    let session_start_utc = opts.session_start_override.unwrap_or(now_utc);

    let data_folder = opts.store.folder.clone();
    let report_path = opts.store.report_path();
    let overlay_path = opts.store.overlay_path();
    let webview_cache = opts.store.webview_cache_dir();
    let show_overlay_initially = config.show_overlay;

    let shared = Arc::new(Mutex::new(SharedState {
        store: opts.store,
        config,
        data,
        catalog,
        lang,
        tz_override: opts.tz_override,
        utc_override: opts.utc_override,
        clock_override: opts.clock_override,
        session_start_utc,
        tracker,
        progress: initial_progress,
        report_revision: 0,
        first_run,
        windowed: true,
        log,
        demos_dirty: false,
        window_proxy: None,
    }));

    // Write initial report/overlay/csv before opening UI
    {
        let mut st = shared.lock().unwrap();
        st.write_reports();
    }

    // Start native Windows always-on-top overlay if enabled
    if show_overlay_initially {
        start_overlay_for(&shared);
    }

    let quit = Arc::new(AtomicBool::new(false));
    let wake = Arc::new(WakeSignal::new());

    // Start local HTTP GUI server
    let s_state = Arc::clone(&shared);
    let s_report = Arc::clone(&shared);
    let s_action = Arc::clone(&shared);
    let wake_act = Arc::clone(&wake);
    let quit_act = Arc::clone(&quit);
    let server = GuiServer::start(
        Arc::new(move || {
            s_state
                .lock()
                .map(|st| st.build_gui_state())
                .unwrap_or_else(|_| json!({}))
        }),
        Arc::new(move |name, val| run_gui_action(&s_action, &wake_act, &quit_act, name, val)),
        Arc::new(move || s_report.lock().ok().map(|st| st.build_report_fragment())),
        data_folder,
        report_path,
        overlay_path,
    )?;

    // Start background replay watcher thread
    let watcher_handle = spawn_watcher_thread(Arc::clone(&shared), Arc::clone(&wake), Arc::clone(&quit));

    // Build window config
    let title = {
        let st = shared.lock().unwrap();
        format!("{} v{APP_VERSION}", st.app_name())
    };
    let s_close = Arc::clone(&shared);
    let s_proxy = Arc::clone(&shared);
    let win_cfg = WindowConfig {
        url: server.url(),
        cache_dir: webview_cache,
        title,
        close_question: Arc::new(move || {
            let Ok(st) = s_close.lock() else {
                return ("Chest Counter".to_string(), "Close?".to_string());
            };
            (
                st.app_name().to_string(),
                st.lang
                    .t(
                        "Fermer le compteur de coffres ?\n\nTes prochaines parties ne seront comptées qu'à sa prochaine ouverture.",
                        "Close the chest counter?\n\nYour next matches will only be counted when it is opened again.",
                        "¿Cerrar el contador de cofres?\n\nTus próximas partidas solo se contarán cuando se vuelva a abrir.",
                    )
                    .to_string(),
            )
        }),
        quit_flag: Arc::clone(&quit),
        on_proxy: Box::new(move |proxy| {
            if let Ok(mut st) = s_proxy.lock() {
                st.window_proxy = Some(proxy);
            }
        }),
    };

    // Run native desktop window on the main thread; if WebView2/GTK isn't available, fall back to browser
    if let Err(_err) = gui_window::run(win_cfg) {
        if let Ok(mut st) = shared.lock() {
            st.windowed = false;
        }
        server.open_fallback();
        while !quit.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(250));
        }
    }

    // Clean shutdown
    quit.store(true, Ordering::SeqCst);
    wake.wake();
    native_overlay::stop();
    server.stop();
    let _ = watcher_handle.join();
    Ok(())
}
