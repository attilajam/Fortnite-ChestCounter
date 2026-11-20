use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use base64::Engine;
use chrono::{DateTime, Datelike, Duration, FixedOffset, TimeZone, Utc, Weekday};

use crate::catalog::{
    self, TreasureCatalog, TreasureHour, TreasureOdds, TreasurePrize, GOLD_OUTFIT, GIZMO, LOADING_SCREEN,
    PIRATE_SHOP, VEHICLE_DECAL,
};
use crate::charts;
use crate::lang::{dotnet_json_string, html_encode, LangCtx};
use crate::models::{chest_kinds, Config, ManualEntry, MatchRecord, Store, TrackerData};
use crate::overlay::{build_overlay, squad_label, OverlayView};
use crate::profiles::{self, Profile};

const PIRATA_ONE_WOFF2: &[u8] = include_bytes!("../../src/ChestCounter/Assets/PirataOne-latin.woff2");

pub mod pretty {
    use super::*;

    pub fn playlist(playlist: Option<&str>, lang: &LangCtx) -> String {
        let Some(pl) = playlist else {
            return lang
                .t("mode inconnu", "unknown mode", "modo desconocido")
                .to_string();
        };
        if pl.is_empty() {
            return lang
                .t("mode inconnu", "unknown mode", "modo desconocido")
                .to_string();
        }
        let p = pl.to_lowercase();
        let mut parts = Vec::new();
        if p.contains("habanero") || p.contains("ranked") {
            parts.push(lang.t("Classée", "Ranked", "Clasificatoria"));
        }
        if p.contains("nobuild") {
            parts.push(lang.t("Zéro construction", "Zero Build", "Cero construcción"));
        }
        if p.contains("solo") {
            parts.push(lang.t("Solo", "Solo", "Solitario"));
        } else if p.contains("duo") {
            parts.push(lang.t("Duo", "Duos", "Dúos"));
        } else if p.contains("trio") {
            parts.push(lang.t("Trio", "Trios", "Tríos"));
        } else if p.contains("squad") {
            parts.push(lang.t("Section", "Squads", "Escuadrones"));
        }
        if !parts.is_empty() {
            parts.join(" · ")
        } else {
            pl.replace("Playlist_", "")
        }
    }

    pub fn placement(m: &MatchRecord, lang: &LangCtx) -> String {
        let Some(place) = m.placement else {
            return lang
                .t("place inconnue", "unknown placement", "puesto desconocido")
                .to_string();
        };
        if place <= 0 {
            return lang
                .t("place inconnue", "unknown placement", "puesto desconocido")
                .to_string();
        }
        if place == 1 {
            return lang
                .t("Top 1 !", "Victory Royale!", "¡Victoria magistral!")
                .to_string();
        }
        if let Some(total) = m.total_players {
            if total > 0 {
                return format!("{} / {}", lang.ordinal(place), total);
            }
        }
        lang.ordinal(place)
    }

    pub fn clock(seconds: f32) -> String {
        let s = seconds as i32;
        format!("{}:{:02}", s / 60, s % 60)
    }

    pub fn likely(lang: &LangCtx) -> &'static str {
        lang.t("probable", "likely", "probable")
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ScanProgress {
    pub done: usize,
    pub total: usize,
}

pub struct ReportContext<'a> {
    pub now_utc: DateTime<Utc>,
    pub session_start_utc: DateTime<Utc>,
    pub progress: Option<ScanProgress>,
    pub lang: &'a LangCtx,
    pub catalog: &'a TreasureCatalog,
    pub images_folder: Option<&'a Path>,
}

pub fn is_this_session(m: &MatchRecord, session_start_utc: DateTime<Utc>) -> bool {
    let start = m.start_utc();
    let dur_ms = (m.duration_seconds.unwrap_or(0) as f64 * 1000.0).round() as i64;
    let end = start + Duration::milliseconds(dur_ms);
    end >= session_start_utc - Duration::minutes(2)
}

pub fn font_face_css() -> &'static str {
    static FONT_FACE: OnceLock<String> = OnceLock::new();
    FONT_FACE.get_or_init(|| {
        let b64 = base64::engine::general_purpose::STANDARD.encode(PIRATA_ONE_WOFF2);
        format!(
            "@font-face{{font-family:'Pirata One';src:url(data:font/woff2;base64,{b64}) format('woff2');font-display:swap}}"
        )
    })
}

pub(crate) const STYLE: &str = concat!(
    ":root,.report-content{",
    "--font:\"Segoe UI Variable Text\",\"Segoe UI\",Inter,system-ui,-apple-system,sans-serif;",
    "--bg:#0b0f14;--surface:#121822;--surface-2:#17202d;--surface-3:#1e293b;",
    "--border:rgba(255,255,255,.08);--border-strong:rgba(255,255,255,.15);",
    "--ink:#e8edf5;--ink-bright:#ffffff;--muted:#8a9ab5;",
    "--gold:#eab308;--gold-ink:#facc15;--gold-soft:rgba(234,179,8,.14);",
    "--boost:#f97316;--boost-ink:#fb923c;--boost-soft:rgba(249,115,22,.15);",
    "--teal:#14b8a6;--teal-ink:#2dd4bf;--teal-soft:rgba(20,184,166,.15);",
    "--star:#c084fc;--star-soft:rgba(192,132,252,.14);",
    "--sky:#38bdf8;--warn:#f87171;--warn-bg:rgba(248,113,113,.11);",
    "--track:rgba(255,255,255,.08);--hover-col:rgba(255,255,255,.045);",
    "--tip-bg:#0d131c;--tip-ink:#f1f5f9;",
    "--shadow:0 1px 2px rgba(0,0,0,.35),0 8px 24px rgba(0,0,0,.22);",
    "--hero-bg:radial-gradient(125% 115% at 0% 0%,rgba(234,179,8,.17) 0%,#121822 60%);",
    "--hero-border:rgba(234,179,8,.34);",
    "--c-vu-bg:rgba(148,163,184,.14);--c-vu-ink:#94a3b8;",
    "--c-u-bg:rgba(234,179,8,.14);--c-u-ink:#eab308;",
    "--c-p-bg:rgba(249,115,22,.16);--c-p-ink:#fb923c;",
    "--c-l-bg:rgba(52,211,153,.15);--c-l-ink:#34d399;",
    "--c-vl-bg:rgba(20,184,166,.18);--c-vl-ink:#2dd4bf;",
    "--c-c-bg:#0f766e;--c-c-ink:#ffffff;",
    "--tag-w-bg:rgba(56,189,248,.16);--tag-w-ink:#7dd3fc;",
    "--tag-s-bg:rgba(192,132,252,.16);--tag-s-ink:#d8b4fe",
    "}\n",
    "@media (prefers-color-scheme:light){:root:not([data-theme=\"dark\"]),.report-content{",
    "--bg:#f6f5f0;--surface:#ffffff;--surface-2:#f1efe7;--surface-3:#e6e2d5;",
    "--border:rgba(15,23,42,.09);--border-strong:rgba(15,23,42,.17);",
    "--ink:#18202c;--ink-bright:#0b0f14;--muted:#5a6577;",
    "--gold:#b47806;--gold-ink:#926004;--gold-soft:rgba(180,120,6,.11);",
    "--boost:#c2410c;--boost-ink:#c2410c;--boost-soft:rgba(194,65,12,.10);",
    "--teal:#0f766e;--teal-ink:#0f766e;--teal-soft:rgba(15,118,110,.11);",
    "--star:#7e22ce;--star-soft:rgba(126,34,206,.10);",
    "--sky:#0284c7;--warn:#dc2626;--warn-bg:rgba(220,38,38,.08);",
    "--track:rgba(15,23,42,.08);--hover-col:rgba(15,23,42,.04);",
    "--tip-bg:#111827;--tip-ink:#f8fafc;",
    "--shadow:0 1px 2px rgba(15,23,42,.05),0 6px 18px rgba(15,23,42,.05);",
    "--hero-bg:radial-gradient(125% 115% at 0% 0%,rgba(234,179,8,.16) 0%,#ffffff 64%);",
    "--hero-border:rgba(180,120,6,.36);",
    "--c-vu-bg:rgba(100,116,139,.12);--c-vu-ink:#475569;",
    "--c-u-bg:rgba(180,120,6,.12);--c-u-ink:#926004;",
    "--c-p-bg:rgba(194,65,12,.12);--c-p-ink:#c2410c;",
    "--c-l-bg:rgba(21,128,61,.12);--c-l-ink:#166534;",
    "--c-vl-bg:rgba(15,118,110,.14);--c-vl-ink:#0f766e;",
    "--c-c-bg:#0f766e;--c-c-ink:#ffffff;",
    "--tag-w-bg:rgba(2,132,199,.12);--tag-w-ink:#0369a1;",
    "--tag-s-bg:rgba(126,34,206,.11);--tag-s-ink:#7e22ce",
    "}}\n",
    ":root[data-theme=\"light\"]{",
    "--bg:#f6f5f0;--surface:#ffffff;--surface-2:#f1efe7;--surface-3:#e6e2d5;",
    "--border:rgba(15,23,42,.09);--border-strong:rgba(15,23,42,.17);",
    "--ink:#18202c;--ink-bright:#0b0f14;--muted:#5a6577;",
    "--gold:#b47806;--gold-ink:#926004;--gold-soft:rgba(180,120,6,.11);",
    "--boost:#c2410c;--boost-ink:#c2410c;--boost-soft:rgba(194,65,12,.10);",
    "--teal:#0f766e;--teal-ink:#0f766e;--teal-soft:rgba(15,118,110,.11);",
    "--star:#7e22ce;--star-soft:rgba(126,34,206,.10);",
    "--sky:#0284c7;--warn:#dc2626;--warn-bg:rgba(220,38,38,.08);",
    "--track:rgba(15,23,42,.08);--hover-col:rgba(15,23,42,.04);",
    "--tip-bg:#111827;--tip-ink:#f8fafc;",
    "--shadow:0 1px 2px rgba(15,23,42,.05),0 6px 18px rgba(15,23,42,.05);",
    "--hero-bg:radial-gradient(125% 115% at 0% 0%,rgba(234,179,8,.16) 0%,#ffffff 64%);",
    "--hero-border:rgba(180,120,6,.36);",
    "--c-vu-bg:rgba(100,116,139,.12);--c-vu-ink:#475569;",
    "--c-u-bg:rgba(180,120,6,.12);--c-u-ink:#926004;",
    "--c-p-bg:rgba(194,65,12,.12);--c-p-ink:#c2410c;",
    "--c-l-bg:rgba(21,128,61,.12);--c-l-ink:#166534;",
    "--c-vl-bg:rgba(15,118,110,.14);--c-vl-ink:#0f766e;",
    "--c-c-bg:#0f766e;--c-c-ink:#ffffff;",
    "--tag-w-bg:rgba(2,132,199,.12);--tag-w-ink:#0369a1;",
    "--tag-s-bg:rgba(126,34,206,.11);--tag-s-ink:#7e22ce",
    "}\n",
    "*{box-sizing:border-box}\n",
    "body,.report-content{margin:0;background:var(--bg);color:var(--ink);font:14px/1.55 var(--font);-webkit-font-smoothing:antialiased;text-rendering:optimizeLegibility}\n",
    "main,.report-layout{max-width:1120px;margin:0 auto;padding:32px 20px 64px;display:flex;flex-direction:column;gap:24px}\n",
    ".pirate{font-family:\"Pirata One\",var(--font),serif;font-weight:400;letter-spacing:.01em}\n",
    ".num{font-variant-numeric:tabular-nums;font-feature-settings:\"tnum\" 1;letter-spacing:-.015em}\n",
    "header{display:flex;flex-wrap:wrap;align-items:center;justify-content:space-between;gap:14px;border-bottom:1px solid var(--border);padding-bottom:16px}\n",
    ".brand{display:flex;align-items:center;gap:13px}\n",
    ".brandmark{width:44px;height:44px;border-radius:12px;background:var(--gold-soft);border:1px solid var(--hero-border);display:grid;place-items:center;flex:none}\n",
    "h1{margin:0;font-size:34px;line-height:1.05;color:var(--ink-bright)}\n",
    ".who{font-size:13px;color:var(--muted);margin-top:2px}.who b{color:var(--gold-ink);font-weight:600}\n",
    ".hdr-r{display:flex;align-items:center;gap:10px}\n",
    ".themebtn{width:34px;height:34px;border-radius:9px;border:1px solid var(--border-strong);background:var(--surface);color:var(--muted);display:grid;place-items:center;cursor:pointer;transition:color .14s,border-color .14s,background .14s}\n",
    ".themebtn:hover{color:var(--ink-bright);border-color:var(--gold);background:var(--surface-2)}\n",
    ".themebtn:focus-visible{outline:2px solid var(--gold);outline-offset:2px}\n",
    ".muted{color:var(--muted)}\n",
    "h2{margin:0 0 10px;font-size:16.5px;font-weight:650;line-height:1.3;letter-spacing:-.01em;color:var(--ink-bright)}\n",
    ".sechead{display:flex;align-items:center;justify-content:space-between;gap:10px;margin-bottom:10px}\n",
    ".sechead h2{margin:0}\n",
    "/* KPI strip */\n",
    ".tiles{display:grid;grid-template-columns:repeat(6,minmax(0,1fr));gap:12px}\n",
    ".tile{background:var(--surface);color:var(--ink);border:1px solid var(--border);border-radius:12px;padding:15px 17px;display:flex;flex-direction:column;justify-content:space-between;gap:4px;box-shadow:var(--shadow)}\n",
    ".tile .v{font-size:30px;font-weight:700;line-height:1.08;color:var(--ink-bright)}\n",
    ".tile .l{font-size:12.5px;color:var(--muted);font-weight:500}\n",
    ".tile .s{font-size:12px;color:var(--muted);margin-top:4px;line-height:1.4}\n",
    ".tile.hero{grid-column:span 2;background:var(--hero-bg);border:1px solid var(--hero-border);position:relative}\n",
    ".tile.hero .k{font-size:11px;font-weight:650;letter-spacing:.11em;text-transform:uppercase;color:var(--gold-ink)}\n",
    ".tile.hero .v{font-size:50px;color:var(--gold-ink);line-height:1.02;letter-spacing:-.025em;margin:2px 0}\n",
    ".tile.hero .l{color:var(--ink);font-size:12.5px;opacity:.88}\n",
    ".editbtn{display:none;position:absolute;top:12px;right:12px;width:34px;height:34px;padding:0;border-radius:9px;border:1px solid var(--hero-border);background:var(--gold-soft);color:var(--gold-ink);cursor:pointer;place-items:center;transition:background .14s}\n",
    ".editbtn:hover{background:rgba(234,179,8,.24)}.editbtn:focus-visible{outline:2px solid var(--gold);outline-offset:2px}\n",
    ".report-content .editbtn{display:grid}\n",
    "@media (max-width:860px){.tiles{grid-template-columns:repeat(2,minmax(0,1fr))}.tile.hero{grid-column:span 2}}\n",
    "/* Surfaces & tables */\n",
    ".card{background:var(--surface);color:var(--ink);border:1px solid var(--border);border-radius:12px;padding:8px 18px;box-shadow:var(--shadow);overflow-x:auto}\n",
    ".two{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:16px}\n",
    "@media (max-width:860px){.two{grid-template-columns:1fr}}\n",
    "table{width:100%;border-collapse:collapse;font-variant-numeric:tabular-nums}\n",
    "th,td{text-align:left;padding:10px 8px;border-bottom:1px solid var(--border);vertical-align:middle}\n",
    "th{white-space:nowrap;font-size:11px;text-transform:uppercase;letter-spacing:.08em;color:var(--muted);font-weight:600}\n",
    "tr:hover td{background:var(--hover-col)}\n",
    "tr:last-child td{border-bottom:none}\n",
    "td.n{text-align:right;font-weight:650;color:var(--ink-bright)}\n",
    "td.pct{text-align:right;color:var(--muted);font-size:12.5px;white-space:nowrap;width:58px}\n",
    "td.nw{white-space:nowrap}\n",
    ".big{font-size:16.5px}\n",
    ".items{color:var(--muted);font-size:12.5px}\n",
    "/* Data visualizations */\n",
    ".viztwo>section{display:flex;flex-direction:column}\n",
    ".viztwo .vizcard{flex:1;display:flex;flex-direction:column}\n",
    ".vizcard{padding:16px 18px 14px;overflow:visible}\n",
    ".vizmeta{display:flex;flex-wrap:wrap;align-items:center;gap:8px 16px;margin-bottom:10px;font-size:12.5px;color:var(--muted)}\n",
    ".vizstat b{color:var(--ink-bright);font-weight:650;font-variant-numeric:tabular-nums}\n",
    ".vizleg{margin-left:auto;display:flex;flex-wrap:wrap;align-items:center;gap:12px;font-size:12px;color:var(--muted)}\n",
    ".vizleg i{font-style:normal;display:inline-flex;align-items:center;gap:5px}\n",
    ".swd{display:inline-block;width:10px;height:10px;border-radius:3px;flex:none}\n",
    ".swd.gold{background:var(--gold)}.swd.boost{background:var(--boost)}\n",
    ".swd.ma{width:13px;height:2.5px;border-radius:2px;background:var(--sky)}\n",
    ".vizbox{position:relative;width:100%}\n",
    ".vizsvg{display:block;width:100%;height:auto;overflow:visible}\n",
    ".gline{stroke:var(--border);stroke-width:1}\n",
    ".gaxis{stroke:var(--border-strong);stroke-width:1}\n",
    ".gtick{fill:var(--muted);font-size:11px;font-family:var(--font);font-variant-numeric:tabular-nums}\n",
    ".gavg{stroke:var(--gold);stroke-width:1.1;opacity:.55}\n",
    ".gavglbl{fill:var(--gold-ink);font-size:10.5px;font-weight:600;font-family:var(--font);opacity:.9}\n",
    ".bbase{fill:var(--gold);opacity:.78}\n",
    ".bbase.peak{fill:var(--gold-ink);opacity:1}\n",
    ".bboost{fill:var(--boost);opacity:.96}\n",
    ".bteal{fill:var(--teal);opacity:.76}\n",
    ".bteal.peak{fill:var(--teal-ink);opacity:1}\n",
    ".bzero{fill:var(--border-strong)}\n",
    ".bpeaklbl{fill:var(--gold-ink);font-size:11px;font-weight:700;font-family:var(--font);font-variant-numeric:tabular-nums}\n",
    ".bpeaklbl.teal{fill:var(--teal-ink)}\n",
    ".gma{stroke:var(--sky);stroke-width:2;stroke-linecap:round;stroke-linejoin:round}\n",
    ".gmadot{fill:var(--sky);stroke:var(--surface);stroke-width:1.6}\n",
    ".svghit{fill:transparent;cursor:crosshair;transition:fill .1s}\n",
    ".svghit:hover,.svghit:focus{fill:var(--hover-col);outline:none}\n",
    ".charttip{position:absolute;z-index:20;pointer-events:none;background:var(--tip-bg);color:var(--tip-ink);border:1px solid var(--border-strong);padding:7px 10px;border-radius:8px;font:500 12px/1.45 var(--font);white-space:pre-line;box-shadow:0 10px 26px rgba(0,0,0,.38);font-variant-numeric:tabular-nums;max-width:250px}\n",
    ".modes{margin-top:12px;padding-top:11px;border-top:1px solid var(--border);display:flex;flex-direction:column;gap:7px}\n",
    ".mrow{display:grid;grid-template-columns:minmax(0,1.25fr) minmax(60px,1fr) auto;align-items:center;gap:10px;font-size:12.5px}\n",
    ".mname{color:var(--ink);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}\n",
    ".mname i{font-style:normal;color:var(--muted);font-size:11.5px}\n",
    ".mbar{display:block;height:6px;border-radius:999px;background:var(--track);overflow:hidden}\n",
    ".mbar i{display:block;height:100%;border-radius:999px;background:var(--teal)}\n",
    ".mval{text-align:right;color:var(--ink-bright);font-weight:650;white-space:nowrap}\n",
    ".mval b{font-weight:400;color:var(--muted);font-size:11.5px}\n",
    ".compbar{display:flex;height:9px;border-radius:999px;overflow:hidden;gap:2px;margin:10px 0 8px;background:var(--track)}\n",
    ".compbar span{display:block;height:100%;min-width:3px;border-radius:2px}\n",
    "/* Your chances */\n",
    ".ch-intro{margin:0 0 12px;color:var(--muted);font-size:13.5px;max-width:760px}\n",
    ".chances2{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:14px}\n",
    "@media (max-width:860px){.chances2{grid-template-columns:1fr}}\n",
    ".hcard{background:var(--surface);color:var(--ink);border:1px solid var(--border);border-radius:12px;padding:16px 18px;display:flex;flex-direction:column;gap:7px;box-shadow:var(--shadow)}\n",
    ".hcard.got{border-color:var(--teal);box-shadow:inset 4px 0 0 var(--teal),var(--shadow);padding-left:20px}\n",
    ".hcard.boosted{border-color:var(--boost);box-shadow:inset 4px 0 0 var(--boost),var(--shadow);padding-left:20px}\n",
    ".htop{display:flex;align-items:center;gap:11px}\n",
    ".cthumb{width:46px;height:46px;border-radius:9px;object-fit:cover;background:var(--surface-2);border:1px solid var(--border);flex:none}\n",
    ".hname{flex:1;min-width:0}\n",
    ".hname .k{font-size:11px;font-weight:600;letter-spacing:.1em;text-transform:uppercase;color:var(--muted)}\n",
    ".ciname{font-weight:650;font-size:15px;color:var(--ink-bright)}\n",
    ".hval{display:flex;align-items:center;gap:10px;flex-wrap:wrap;margin-top:2px}\n",
    ".hval .big{font-size:36px;font-weight:700;line-height:1.05;color:var(--ink-bright)}\n",
    ".hexp{font-size:13.5px;color:var(--ink)}\n",
    ".hcard .s{font-size:12.5px;color:var(--muted)}\n",
    ".cfoot{font-size:12.5px;color:var(--muted);border-top:1px solid var(--border);padding-top:7px;margin-top:2px}\n",
    ".cfoot b{color:var(--ink-bright)}\n",
    ".halfway{margin-top:4px}.hrow{display:flex;justify-content:space-between;gap:8px;font-size:12.5px;color:var(--muted)}\n",
    ".hrow b{color:var(--ink-bright)}\n",
    ".tag{display:inline-block;padding:3px 9px;border-radius:999px;font:650 10.5px var(--font);letter-spacing:.07em;text-transform:uppercase;flex:none}\n",
    ".tag.weekly{background:var(--tag-w-bg);color:var(--tag-w-ink)}\n",
    ".tag.season{background:var(--tag-s-bg);color:var(--tag-s-ink)}\n",
    ".chances2 .gotbadge,.ctable .gotbadge{display:none;padding:2px 8px;border-radius:999px;background:var(--teal-soft);color:var(--teal-ink);font:650 11px var(--font);margin-left:8px;white-space:nowrap}\n",
    ".got .gotbadge{display:inline-block!important}\n",
    ".vchip{display:inline-block;padding:2px 9px;border-radius:999px;font:600 11.5px var(--font);white-space:nowrap}\n",
    ".vchip.vunlikely{background:var(--c-vu-bg);color:var(--c-vu-ink)}\n",
    ".vchip.unlikely{background:var(--c-u-bg);color:var(--c-u-ink)}\n",
    ".vchip.possible{background:var(--c-p-bg);color:var(--c-p-ink)}\n",
    ".vchip.likely{background:var(--c-l-bg);color:var(--c-l-ink)}\n",
    ".vchip.vlikely{background:var(--c-vl-bg);color:var(--c-vl-ink)}\n",
    ".vchip.certain{background:var(--c-c-bg);color:var(--c-c-ink)}\n",
    ".cbar{display:block;height:6px;background:var(--track);border-radius:999px;overflow:hidden;margin-top:5px}\n",
    ".cbar.wide{width:100%}\n",
    ".cbarwrap{position:relative;display:block;padding:5px 0;margin-top:1px;cursor:help;outline:none}\n",
    ".cbarwrap:focus-visible .cbar{outline:2px solid var(--gold);outline-offset:2px}\n",
    ".cbarwrap .cbar{margin-top:0}\n",
    ".cbartip{display:none;position:absolute;z-index:15;bottom:calc(100% + 5px);left:50%;transform:translateX(-50%);width:max-content;max-width:min(330px,82vw);background:var(--tip-bg);color:var(--tip-ink);border:1px solid var(--border-strong);padding:8px 11px;border-radius:9px;font:400 12px/1.48 var(--font);text-align:left;white-space:pre-line;box-shadow:0 10px 26px rgba(0,0,0,.42);pointer-events:none;font-variant-numeric:tabular-nums}\n",
    ".cbartip b{color:#ffffff;font-weight:650}\n",
    ".cbarwrap:hover .cbartip,.cbarwrap:focus .cbartip{display:block}\n",
    ".tcard{overflow:visible}\n",
    ".cbar i{display:block;height:6px;border-radius:999px}\n",
    ".cbar i.vunlikely,.cbar i.unlikely{background:var(--gold)}.cbar i.possible{background:var(--boost)}\n",
    ".cbar i.likely,.cbar i.vlikely{background:var(--teal)}.cbar i.certain{background:#10b981}.cbar i.gold{background:var(--gold)}\n",
    ".cgroup{margin-top:18px}\n",
    ".ghead{display:flex;align-items:center;gap:10px;margin:0 0 8px}\n",
    ".ghead .gt{font-weight:650;font-size:15px;color:var(--ink-bright)}\n",
    ".ghead .gs{font-size:12.5px;color:var(--muted)}.ghead .gs b{color:var(--ink)}\n",
    ".cdbox{margin-left:auto;text-align:right;flex:none}\n",
    ".cdbox .cdl{display:block;font-size:10.5px;letter-spacing:.09em;text-transform:uppercase;color:var(--muted)}\n",
    ".cdbox .cd{font:700 16px var(--font);color:var(--ink-bright)}\n",
    ".cd{font-variant-numeric:tabular-nums}.cd.soon{color:var(--boost-ink)}\n",
    ".ctable:not(.mini){table-layout:fixed;width:100%}\n",
    ".ctable:not(.mini) th:nth-child(1){width:38%}\n",
    ".ctable:not(.mini) th:nth-child(2){width:16%}\n",
    ".ctable:not(.mini) th:nth-child(3){width:32%}\n",
    ".ctable:not(.mini) th:nth-child(4){width:14%}\n",
    ".ctable th.r,.ctable td.n{text-align:right}\n",
    ".ctable td.n{white-space:nowrap;font-weight:650}\n",
    ".ctable td{vertical-align:middle}\n",
    ".tflex{display:flex;align-items:center;gap:10px}\n",
    ".thumb{width:36px;height:36px;border-radius:8px;object-fit:cover;background:var(--surface-2);border:1px solid var(--border);flex:none}\n",
    ".tk{font-weight:600;color:var(--ink-bright)}\n",
    ".iname{display:block;font-size:12px;color:var(--muted)}\n",
    ".tchance{min-width:0}\n",
    ".tcv{display:flex;align-items:center;justify-content:space-between;gap:8px}\n",
    ".tcv b{color:var(--ink-bright)}\n",
    "tr.got .tk{color:var(--teal-ink)}\n",
    "tr.boosted td:first-child{box-shadow:inset 3px 0 0 var(--boost);padding-left:13px}\n",
    ".cmore{margin-top:12px;color:var(--ink)}\n",
    ".cmore summary{cursor:pointer;font-weight:600;font-size:13.5px;color:var(--ink-bright);padding:4px 0}\n",
    ".cmore p,.cmore ul{color:var(--muted);font-size:13.5px;max-width:760px}\n",
    ".cmore li{margin:5px 0}.cmore b{color:var(--ink)}\n",
    ".cmore a{color:var(--gold-ink)}\n",
    ".ctable.mini td,.ctable.mini th{padding-top:7px;padding-bottom:7px}\n",
    "time[title]{border-bottom:1px dotted currentColor;cursor:help}\n",
    ".mo{display:none}\n",
    "@media (max-width:600px){\n",
    "  .ctable th:nth-child(2),.ctable td.pc{display:none}\n",
    "  .ctable:not(.mini) th:nth-child(1){width:42%}\n",
    "  .ctable:not(.mini) th:nth-child(3){width:38%}\n",
    "  .ctable:not(.mini) th:nth-child(4){width:20%}\n",
    "  .iname.mo{display:block}\n",
    "  .thumb{display:none}\n",
    "  .ctable th{white-space:normal;letter-spacing:.04em}\n",
    "  .tcard{padding:4px 10px}\n",
    "  .tcv{flex-wrap:wrap;justify-content:flex-start;gap:2px 8px}\n",
    "  .ctable td.n{white-space:normal}\n",
    "  .tk{display:flex;flex-wrap:wrap;align-items:center;gap:4px}.tk .gotbadge{margin-left:0}\n",
    "  .hval .big{font-size:30px}\n",
    "  .ghead{align-items:flex-start;flex-wrap:wrap}\n",
    "  .cdbox{margin-left:0;text-align:left;width:100%;padding-left:2px}\n",
    "  .vizleg{margin-left:0}\n",
    "}\n",
    ".prize input[data-auto]{cursor:default}\n",
    ".star{color:var(--star);font-weight:600}\n",
    ".gold{color:var(--gold-ink)}\n",
    ".sw{display:inline-block;width:9px;height:9px;border-radius:3px;margin-right:9px;vertical-align:0}\n",
    ".meter{display:block;width:110px;height:6px;background:var(--track);border-radius:999px;overflow:hidden}\n",
    ".meter i{display:block;height:6px;border-radius:999px}\n",
    ".flag{display:flex;gap:10px;align-items:flex-start;background:var(--warn-bg);color:var(--ink);border:1px solid rgba(248,113,113,.32);border-radius:10px;padding:11px 14px;font-size:13px;margin-top:10px}\n",
    "tr.unusual td{background:var(--warn-bg)}\n",
    ".warn{color:var(--warn)}\n",
    "/* Matches filter toolbar */\n",
    ".mtoolbar{display:flex;flex-wrap:wrap;align-items:center;justify-content:space-between;gap:10px;padding:10px 0 10px;border-bottom:1px solid var(--border)}\n",
    ".fchips{display:flex;flex-wrap:wrap;gap:6px}\n",
    ".fchip{height:30px;padding:0 11px;border-radius:999px;border:1px solid var(--border-strong);background:transparent;color:var(--muted);font:600 12px var(--font);cursor:pointer;display:inline-flex;align-items:center;gap:5px;transition:background .12s,color .12s,border-color .12s}\n",
    ".fchip:hover{color:var(--ink-bright);background:var(--surface-2)}\n",
    ".fchip.on{background:var(--gold-soft);border-color:var(--gold);color:var(--gold-ink)}\n",
    ".fchip.boost.on{background:var(--boost-soft);border-color:var(--boost);color:var(--boost-ink)}\n",
    ".fchip.star.on{background:var(--star-soft);border-color:var(--star);color:var(--star)}\n",
    ".msearch{height:30px;width:min(240px,100%);padding:0 12px;border-radius:999px;border:1px solid var(--border-strong);background:var(--surface-2);color:var(--ink-bright);font:12.5px var(--font);outline:none;transition:border-color .14s}\n",
    ".msearch::placeholder{color:var(--muted)}\n",
    ".msearch:focus{border-color:var(--gold)}\n",
    ".btag{display:inline-flex;align-items:center;gap:3px;margin-left:6px;padding:1px 6px;border-radius:999px;background:var(--boost-soft);color:var(--boost-ink);font:700 10.5px var(--font);vertical-align:1px}\n",
    ".rate{display:inline-block;margin-left:6px;font-size:11.5px;font-weight:500;color:var(--muted)}\n",
    "tr.flt,tr.pg{display:none!important}\n",
    "/* Switches & folding sections */\n",
    ".head{display:flex;flex-wrap:wrap;align-items:center;justify-content:space-between;gap:12px;margin-bottom:10px}\n",
    ".head h2{margin:0}\n",
    ".tsw{position:relative;display:inline-flex;align-items:center;gap:9px;min-height:32px;font-size:13px;color:var(--ink);cursor:pointer;user-select:none}\n",
    ".tsw input{position:absolute;opacity:0;width:1px;height:1px;margin:0}\n",
    ".tsw .trk{position:relative;width:36px;height:20px;border-radius:999px;background:var(--surface-3);border:1px solid var(--border-strong);flex:none;transition:background .15s,border-color .15s}\n",
    ".tsw .trk:after{content:\"\";position:absolute;top:2px;left:2px;width:14px;height:14px;border-radius:50%;background:#fff;transition:transform .15s}\n",
    ".tsw input:checked+.trk{background:var(--teal);border-color:var(--teal)}.tsw input:checked+.trk:after{transform:translateX(16px)}\n",
    ".tsw input:focus-visible+.trk{outline:2px solid var(--gold);outline-offset:2px}\n",
    ".tsw .badge{display:inline-block;margin-left:2px;padding:1px 7px;border-radius:999px;background:var(--warn-bg);color:var(--warn);font-size:11px;font-weight:650}\n",
    ".statsbar{display:flex;justify-content:flex-end;margin:-14px 0 -8px}\n",
    ".squadtile,.sq{display:none}\n",
    ":is(body,.report-content).showsquad .squadtile{display:flex}:is(body,.report-content).showsquad .sq{display:table-cell}\n",
    ":is(body,.report-content).showsquad .tiles.hassquad{grid-template-columns:repeat(7,minmax(0,1fr))}\n",
    ".fold .head h2{display:flex;align-items:center;gap:6px}\n",
    ".foldbtn{display:inline-flex;align-items:center;gap:9px;background:none;border:0;padding:4px 0;margin:0;font:inherit;color:inherit;cursor:pointer;text-align:left}\n",
    ".foldbtn:hover{color:var(--gold-ink)}\n",
    ".foldbtn:focus-visible{outline:2px solid var(--gold);outline-offset:4px;border-radius:4px}\n",
    ".chev{width:8px;height:8px;border-right:2px solid var(--muted);border-bottom:2px solid var(--muted);transform:rotate(45deg);margin:0 2px 3px 2px;transition:transform .16s}\n",
    ".fold.closed .chev{transform:rotate(-45deg);margin-bottom:0}\n",
    ".fold.closed .foldbody{display:none}\n",
    ".fold.closed .head{margin-bottom:0}\n",
    ".fold.closed .head>:not(h2){display:none}\n",
    ".team{display:none}\n",
    ":is(body,.report-content).showteam tr.team{display:table-row}\n",
    ":is(body,.report-content).showteam span.team{display:inline}\n",
    "tr.team td{opacity:.78}\n",
    ".by{white-space:nowrap}\n",
    ".by svg{vertical-align:-3px;margin-right:6px}\n",
    "footer{font-size:12.5px;line-height:1.6;color:var(--muted);border-top:1px solid var(--border);padding-top:16px}\n",
    ".loading{display:flex;gap:14px;align-items:center;background:var(--surface);color:var(--ink);border:1px solid var(--hero-border);border-radius:12px;padding:15px 18px;box-shadow:var(--shadow)}\n",
    ".loading .muted{color:var(--muted)}\n",
    ".spin{width:26px;height:26px;flex:none;border:2.5px solid var(--track);border-top-color:var(--gold);border-radius:50%;animation:spin .8s linear infinite}\n",
    "@keyframes spin{to{transform:rotate(360deg)}}\n",
    ".loading .grow{flex:1;min-width:0}\n",
    ".pbar{height:6px;background:var(--track);border-radius:999px;overflow:hidden;margin:8px 0 6px}\n",
    ".pbar i{display:block;height:100%;background:var(--gold);border-radius:999px}\n",
    ".report-content{background:transparent}.report-layout{padding:0;max-width:none}\n",
    ".profiles{margin-top:-12px;font-size:12.5px}\n",
    ".info{position:relative;display:inline-flex;align-items:center;justify-content:center;width:16px;height:16px;margin-left:5px;border-radius:50%;border:1.5px solid var(--muted);color:var(--muted);font:700 10px/1 Georgia,serif;font-style:italic;cursor:help;vertical-align:1px;outline:none;transition:border-color .14s,color .14s}\n",
    ".info:hover,.info:focus{border-color:var(--gold-ink);color:var(--gold-ink)}\n",
    ".infotip{display:none;position:absolute;z-index:20;top:24px;right:-12px;width:300px;max-width:80vw;background:var(--tip-bg);color:var(--tip-ink);border:1px solid var(--border-strong);padding:10px 12px;border-radius:9px;font:400 12.5px/1.48 var(--font);font-style:normal;text-align:left;box-shadow:0 10px 26px rgba(0,0,0,.42)}\n",
    ".info:hover .infotip,.info:focus .infotip{display:block}\n",
    "h2 .info{vertical-align:1px}\n",
    "h2 .infotip{left:-12px;right:auto}\n",
    ".pager{display:flex;flex-wrap:wrap;align-items:center;gap:8px;padding:11px 0 6px;border-top:1px solid var(--border)}\n",
    ".pager[hidden],.pager button[hidden]{display:none}\n",
    ".pager .count{color:var(--muted);font-size:12.5px;margin-right:auto;font-variant-numeric:tabular-nums}\n",
    ".pager button{min-height:32px;padding:5px 12px;border-radius:8px;border:1px solid var(--border-strong);background:var(--surface-2);color:var(--ink);font:600 12.5px var(--font);cursor:pointer;transition:background .12s,border-color .12s}\n",
    ".pager button:hover{background:var(--surface-3);border-color:var(--gold)}\n",
    ".notice{background:var(--surface);color:var(--ink);border:1px solid var(--border);border-left:4px solid var(--gold);border-radius:10px;padding:12px 16px;white-space:pre-line;font-size:13.5px}\n",
    ".notice.warning{border-left-color:var(--warn);background:var(--warn-bg)}\n",
    ".exper{display:inline-block;font:650 10.5px var(--font);letter-spacing:.06em;text-transform:uppercase;padding:2px 8px;border-radius:999px;background:var(--boost-soft);color:var(--boost-ink);vertical-align:1px}\n",
    ".thour{background:var(--surface);color:var(--ink);border:1px solid var(--border);border-left:4px solid var(--gold);border-radius:10px;padding:11px 15px;margin-bottom:12px}\n",
    ".thour.live{border-left-color:var(--boost);border-color:rgba(249,115,22,.35);background:var(--boost-soft)}\n",
    ".thour .tt{font-weight:650;color:var(--ink-bright)}.thour .exp{font-weight:400;color:var(--muted)}\n",
    ".boost{display:inline-block;margin-left:7px;padding:1px 7px;border-radius:999px;background:var(--boost-soft);color:var(--boost-ink);font:700 11px var(--font);letter-spacing:0;text-transform:none;vertical-align:1px}\n",
    ".boost[hidden]{display:none}\n",
    ".boostv{color:var(--boost-ink);font-weight:650}\n",
    ".thour .tb{display:flex;flex-wrap:wrap;gap:6px 18px;font-size:13px;color:var(--muted);margin-top:4px}\n",
    ".thour .tb b{color:var(--ink-bright)}\n",
    ".collection{padding:12px 18px 16px}\n",
    ".wk{padding:10px 0}.wk+.wk{border-top:1px solid var(--border)}\n",
    ".wt{font-weight:650;color:var(--ink-bright);margin-bottom:10px;font-size:14px}.wk.now .wt{color:var(--gold-ink)}\n",
    ".prizes{display:grid;grid-template-columns:repeat(auto-fill,minmax(142px,1fr));gap:10px}\n",
    ".prize{position:relative;display:flex;flex-direction:column;gap:3px;padding:8px;border:1px solid var(--border);border-radius:10px;cursor:pointer;background:var(--surface-2);transition:border-color .14s,transform .14s}\n",
    ".prize:hover{border-color:var(--border-strong)}\n",
    ".prize input{position:absolute;top:12px;left:12px;width:18px;height:18px;accent-color:var(--teal);cursor:pointer;z-index:1}\n",
    ".prize .pimg{display:block;aspect-ratio:1;border-radius:7px;background:var(--surface-3);overflow:hidden;margin-bottom:4px}\n",
    ".prize img{width:100%;height:100%;object-fit:cover;display:block;filter:grayscale(.72);opacity:.68;transition:filter .16s,opacity .16s}\n",
    ".prize .pn{font-size:12.5px;font-weight:600;line-height:1.3;color:var(--ink-bright)}\n",
    ".prize .pk{font-size:11.5px;color:var(--muted)}\n",
    ".prize.got{border-color:var(--teal);background:var(--teal-soft)}\n",
    ".prize.got img{filter:none;opacity:1}"
);

const CHEST_ICON: &str = r##"<span class="brandmark" aria-hidden="true"><svg width="26" height="26" viewBox="0 0 56 56"><rect x="6" y="21" width="44" height="27" rx="5" fill="#422006" stroke="#eab308" stroke-width="3.2"/><path d="M6 26 Q28 5 50 26" fill="#713f12" stroke="#eab308" stroke-width="3.2"/><rect x="23.5" y="24" width="9" height="10" rx="2.5" fill="#facc15"/><line x1="6" y1="35" x2="50" y2="35" stroke="#eab308" stroke-width="2.2"/></svg></span>"##;
const THEME_ICON: &str = r##"<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M4.93 4.93l1.41 1.41m11.32 11.32l1.41 1.41M2 12h2m16 0h2M4.93 19.07l1.41-1.41m11.32-11.32l1.41-1.41"/></svg>"##;
const TEAM_ICON: &str = r##"<svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true"><circle cx="9" cy="8" r="3.2" fill="none" stroke="currentColor" stroke-width="2"/><path d="M3 20c0-3.5 2.7-6 6-6s6 2.5 6 6" fill="none" stroke="currentColor" stroke-width="2"/><circle cx="17" cy="9" r="2.6" fill="none" stroke="currentColor" stroke-width="2"/><path d="M16 14c3 0 5 2.2 5 5" fill="none" stroke="currentColor" stroke-width="2"/></svg>"##;
const FLAG_ICON: &str = r##"<svg width="17" height="17" viewBox="0 0 24 24" aria-hidden="true" style="flex:none;margin-top:2px"><path d="M5 21V4M5 4h11l-2 4 2 4H5" fill="none" stroke="var(--warn)" stroke-width="2" stroke-linejoin="round"/></svg>"##;
pub const PENCIL_ICON: &str = r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 20h4L19 9l-4-4L4 16z"/><path d="M14 6l4 4"/></svg>"#;

fn kind_color(kind: &str) -> &'static str {
    match kind {
        chest_kinds::NORMAL => "#eab308",
        chest_kinds::RARE => "#38bdf8",
        chest_kinds::BURIED => "#fb923c",
        chest_kinds::EXTRACTION => "#c084fc",
        chest_kinds::POSTER_CHEST => "#34d399",
        chest_kinds::OTHER => "#94a3b8",
        _ => "#94a3b8",
    }
}

fn h(s: &str) -> String {
    html_encode(s)
}

fn when(m: &MatchRecord, lang: &LangCtx) -> DateTime<FixedOffset> {
    lang.shown(m.start_utc())
}

fn to_crlf(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 256);
    let mut prev_cr = false;
    for ch in s.chars() {
        if ch == '\n' && !prev_cr {
            out.push('\r');
        }
        out.push(ch);
        prev_cr = ch == '\r';
    }
    out
}

fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    crate::models::write_atomic(path, content)
}

pub fn write_overlay_file(store: &Store, ov_view: &OverlayView, style: &str, lang: &LangCtx) -> Result<(), String> {
    let ov = overlay(ov_view, style, lang);
    write_atomic(&store.overlay_path(), &ov)
}

pub fn write_all(store: &Store, config: &Config, data: &TrackerData, ctx: &ReportContext<'_>) -> Result<(), String> {
    let view = profiles::view(data, config, ctx.lang);
    let profs = profiles::list(data, ctx.lang);
    let rep = report(config, &view, &profs, ctx);
    write_atomic(&store.report_path(), &rep)?;
    let ov_view = build_overlay(
        &view,
        None,
        false,
        config.show_squad_total,
        ctx.session_start_utc,
        ctx.lang,
    );
    let ov = overlay(&ov_view, &config.overlay_style, ctx.lang);
    write_atomic(&store.overlay_path(), &ov)?;
    let csv_text = csv(data, ctx.lang, ctx.catalog);
    write_atomic(&store.csv_path(), &csv_text)?;
    Ok(())
}

pub fn report(
    config: &Config,
    data: &TrackerData,
    profiles_list: &[Profile],
    ctx: &ReportContext<'_>,
) -> String {
    render_report(config, data, profiles_list, ctx, false)
}

pub fn report_fragment(config: &Config, data: &TrackerData, ctx: &ReportContext<'_>) -> String {
    let view = profiles::view(data, config, ctx.lang);
    render_report(config, &view, &[], ctx, true)
}

fn render_report(
    config: &Config,
    data: &TrackerData,
    profiles_list: &[Profile],
    ctx: &ReportContext<'_>,
    in_app: bool,
) -> String {
    let lang = ctx.lang;
    let progress = ctx.progress;

    let mut matches: Vec<&MatchRecord> = data.matches.iter().collect();
    // OrderByDescending(When) — stable sort preserving relative order of ties
    matches.sort_by(|a, b| when(b, lang).cmp(&when(a, lang)));

    let total_chests: i32 = matches.iter().map(|m| m.chests_opened).sum();
    let grand_total = data.total_chests();
    let team_chests = data.team_chests();
    let avg = if !matches.is_empty() {
        total_chests as f64 / matches.len() as f64
    } else {
        0.0
    };
    let best: i32 = matches.iter().map(|m| m.chests_opened).max().unwrap_or(0);
    let treasures: usize = matches.iter().map(|m| m.treasure_finds.len()).sum();
    let team_treasures: usize = matches.iter().map(|m| m.team_treasure_finds.len()).sum();
    let session: i32 = matches
        .iter()
        .filter(|m| is_this_session(m, ctx.session_start_utc))
        .map(|m| m.chests_opened)
        .sum();
    let player = matches
        .iter()
        .filter_map(|m| m.player_name.as_deref())
        .find(|n| !n.is_empty())
        .unwrap_or("");
    let every: i32 = if treasures > 0 {
        (total_chests as f64 / treasures as f64).round_ties_even() as i32
    } else {
        0
    };

    // Overall chest yield per hour across matches with duration metadata
    let mut dur_secs_sum: i64 = 0;
    let mut dur_chests_sum: i64 = 0;
    for m in &matches {
        if let Some(d) = m.duration_seconds {
            if d > 0 {
                dur_secs_sum += d as i64;
                dur_chests_sum += m.chests_opened as i64;
            }
        }
    }
    let avg_sub_str = if dur_secs_sum >= 60 && dur_chests_sum > 0 {
        let per_h = dur_chests_sum as f64 * 3600.0 / dur_secs_sum as f64;
        Some(format!(
            "{} {}",
            lang.n(per_h.round() as i64),
            lang.t("coffres / h", "chests / hr", "cofres / h")
        ))
    } else {
        None
    };

    let mut sb = String::with_capacity(64 * 1024);
    let title = lang.t("Compteur de coffres", "Chest Counter", "Contador de cofres");
    let now_shown = lang.shown(ctx.now_utc);
    let updated_date = lang.updated_date(now_shown);
    let updated_clock = lang.clock(now_shown, true);
    let updated = match lang.code() {
        "en" => format!("Updated {updated_date} at {updated_clock} · auto refresh"),
        "es" => format!("Actualizado el {updated_date} a las {updated_clock} · actualización automática"),
        _ => format!("Mis à jour le {updated_date} à {updated_clock} · actualisation auto"),
    };
    let theme_title = h(lang.t("Changer de thème", "Switch theme", "Cambiar tema"));

    let tile = |value: &str, label: &str, sub: Option<&str>| -> String {
        match sub {
            Some(s) => format!(
                r#"<div class="tile"><div class="v num">{value}</div><div class="l">{label}</div><div class="s">{s}</div></div>"#
            ),
            None => format!(
                r#"<div class="tile"><div class="v num">{value}</div><div class="l">{label}</div></div>"#
            ),
        }
    };

    let edit_title = h(lang.t(
        "Modifier mon nombre de coffres",
        "Edit my number of chests",
        "Modificar mi número de cofres",
    ));
    let hero_sub = {
        let mut s = match lang.code() {
            "en" => format!(
                "opened or destroyed by you · {} since the tool started",
                lang.n(session as i64)
            ),
            "es" => format!(
                "abiertos o destruidos por ti · {} desde que se abrió",
                lang.n(session as i64)
            ),
            _ => format!(
                "ouverts ou détruits par toi · {} depuis le lancement",
                lang.n(session as i64)
            ),
        };
        if data.manual_adjust != 0 {
            s.push_str("<br>");
            let adj = lang.signed_n(data.manual_adjust as i64);
            s.push_str(&match lang.code() {
                "en" => format!("{adj} of them added by hand"),
                "es" => format!("de ellos, {adj} añadidos a mano"),
                _ => format!("dont {adj} ajoutés à la main"),
            });
        }
        s
    };

    let squad_tile = if team_chests > 0 {
        let squad_val = lang.n((grand_total + team_chests) as i64);
        let squad_lbl = format!(
            "{}{}",
            lang.t("total de l'équipe", "squad total", "total del equipo"),
            squad_info(lang)
        );
        let squad_sub = match lang.code() {
            "en" => format!("you + {} by your teammates", lang.n(team_chests as i64)),
            "es" => format!("tú + {} de tus compañeros", lang.n(team_chests as i64)),
            _ => format!("toi + {} par tes coéquipiers", lang.n(team_chests as i64)),
        };
        tile(&squad_val, &squad_lbl, Some(&squad_sub))
            .replacen(r#"<div class="tile">"#, r#"<div class="tile squadtile">"#, 1)
    } else {
        String::new()
    };

    let treasure_sub_str: String = {
        let mut parts = String::new();
        if every > 0 {
            parts.push_str(&match lang.code() {
                "en" => format!("1 every {} chests", lang.n(every as i64)),
                "es" => format!("1 cada {} cofres", lang.n(every as i64)),
                _ => format!("1 tous les {} coffres", lang.n(every as i64)),
            });
        }
        if team_treasures > 0 {
            if every > 0 {
                parts.push_str("<br>");
            }
            parts.push_str(&match lang.code() {
                "en" => format!("+{team_treasures} by the team (estimate)"),
                "es" => format!("+{team_treasures} del equipo (estimación)"),
                _ => format!("+{team_treasures} par l'équipe (estimation)"),
            });
        }
        parts
    };
    let treasure_sub_opt = if treasure_sub_str.is_empty() {
        None
    } else {
        Some(treasure_sub_str.as_str())
    };

    let stats_bar = if team_chests > 0 {
        let lbl = format!(
            r#"{} <span class="badge">{}</span>"#,
            lang.t(
                "Afficher le total d'équipe",
                "Show the squad total",
                "Mostrar el total del equipo"
            ),
            lang.t("imprécis", "inaccurate", "impreciso")
        );
        format!(r#"<div class="statsbar">{}</div>"#, switch_html("showsquad", &lbl))
    } else {
        String::new()
    };

    let loading = loading_card(progress, lang);
    let show_squad = u8::from(config.show_squad_in_report);
    let show_team = u8::from(config.show_team_treasures_in_report);
    if in_app {
        sb.push_str(&format!(
            r#"<div class="report-content" lang="{code}" data-showsquad="{show_squad}" data-showteam="{show_team}"><div class="report-layout">{loading}"#,
            code = lang.code(),
        ));
    } else {
        sb.push_str(&format!(
            "<!doctype html><html lang=\"{code}\"><head><meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n\
<meta http-equiv=\"refresh\" content=\"{refresh}\">\n\
<title>{title}</title><style>{font}{style}</style></head><body data-showsquad=\"{show_squad}\" data-showteam=\"{show_team}\"><main>\n\
<header><div class=\"brand\">{chest_icon}<div><h1 class=\"pirate\">{title}</h1>{who}</div></div>\n\
<div class=\"hdr-r\"><span class=\"muted\" style=\"font-size:12.5px\">{updated}</span><button type=\"button\" class=\"themebtn\" id=\"themebtn\" title=\"{theme_title}\" aria-label=\"{theme_title}\">{theme_icon}</button></div></header>\n\
{loading}{notices}{profiles_line}\n\
\n",
            code = lang.code(),
            refresh = if progress.is_some() { 2 } else { 15 },
            font = font_face_css(),
            style = STYLE,
            chest_icon = CHEST_ICON,
            theme_icon = THEME_ICON,
            who = if !player.is_empty() {
                format!(r#"<div class="who"><b>{}</b></div>"#, h(player))
            } else {
                String::new()
            },
            notices = notice_cards(ctx),
            profiles_line = profiles_line(profiles_list, player, lang),
        ));
    }

    sb.push_str(&format!(
        "<section class=\"tiles{hassquad}\">
  <div class=\"tile hero\"><button type=\"button\" class=\"editbtn\" id=\"editchests\" title=\"{edit_title}\" aria-label=\"{edit_title}\">{pencil}</button><div class=\"k\">{hero_k}</div><div class=\"v num\">{grand_total}</div><div class=\"l\">{hero_sub}</div></div>
  {squad_tile}
  {t_matches}
  {t_avg}
  {t_best}
  {t_treasures}\n\
</section>\n\
{stats_bar}",
        hassquad = if team_chests > 0 { " hassquad" } else { "" },
        edit_title = edit_title,
        pencil = PENCIL_ICON,
        hero_k = lang.t("Coffres au total", "Chests in total", "Cofres en total"),
        grand_total = lang.n(grand_total as i64),
        hero_sub = hero_sub,
        squad_tile = squad_tile,
        t_matches = tile(
            &lang.n(matches.len() as i64),
            lang.t("parties analysées", "matches analyzed", "partidas analizadas"),
            None
        ),
        t_avg = tile(
            &lang.format_float(avg, 1, false),
            lang.t("coffres par partie", "chests per match", "cofres por partida"),
            avg_sub_str.as_deref()
        ),
        t_best = tile(
            &lang.n(best as i64),
            lang.t("record en une partie", "best match", "récord en una partida"),
            None
        ),
        t_treasures = tile(
            &lang.n(treasures as i64),
            lang.t("trésors trouvés", "treasures found", "tesoros encontrados"),
            treasure_sub_opt
        ),
        stats_bar = stats_bar,
    ));

    // 30-day SVG combo chart (stacked base + Treasure Hour boosted + 7d rolling trend + active avg)
    sb.push_str(&charts::daily_chart_section(&matches, now_shown, ctx.catalog, lang));

    // Analytical insights row (chests/match histogram + loot rate by match phase & playlist efficiency)
    sb.push_str(&charts::insights_section(&matches, lang));

    // By chest type | Where your treasures come from
    let mut by_kind_order: Vec<String> = Vec::new();
    let mut by_kind_counts: HashMap<String, usize> = HashMap::new();
    for m in &matches {
        for c in &m.chests {
            let k = chest_kinds::canonical(&c.kind).to_string();
            if !by_kind_counts.contains_key(&k) {
                by_kind_order.push(k.clone());
            }
            *by_kind_counts.entry(k).or_insert(0) += 1;
        }
    }
    let mut by_kind: Vec<(String, usize)> = by_kind_order
        .into_iter()
        .map(|k| {
            let cnt = by_kind_counts[&k];
            (k, cnt)
        })
        .collect();
    by_kind.sort_by(|a, b| b.1.cmp(&a.1));
    let kind_total: usize = by_kind.iter().map(|x| x.1).sum();
    let kind_max = by_kind.iter().map(|x| x.1).max().unwrap_or(1).max(1);

    let mut other_order: Vec<String> = Vec::new();
    let mut other_counts: HashMap<String, i32> = HashMap::new();
    for m in &matches {
        if !m.other_interactions.is_empty() {
            for (k, &v) in &m.other_interactions {
                if !other_counts.contains_key(k) {
                    other_order.push(k.clone());
                }
                *other_counts.entry(k.clone()).or_insert(0) += v;
            }
        } else {
            for (k, v) in [
                (chest_kinds::AMMO_BOX, m.ammo_boxes_opened),
                (chest_kinds::OTHER_CONTAINER, m.other_containers_opened),
            ] {
                let ks = k.to_string();
                if !other_counts.contains_key(&ks) {
                    other_order.push(ks.clone());
                }
                *other_counts.entry(ks).or_insert(0) += v;
            }
        }
    }
    let mut other_kinds: Vec<(String, i32)> = other_order
        .into_iter()
        .map(|k| {
            let cnt = other_counts[&k];
            (k, cnt)
        })
        .filter(|x| x.1 > 0)
        .collect();
    other_kinds.sort_by(|a, b| b.1.cmp(&a.1));

    let comp_bar = charts::chest_composition_bar(&by_kind, lang);
    sb.push_str(&format!(
        r#"<div class="two"><section><h2>{}</h2><div class="card">{comp_bar}<table>"#,
        lang.t("Par type de coffre", "By chest type", "Por tipo de cofre")
    ));
    for (k, count) in &by_kind {
        let color = kind_color(k);
        let w = ((100.0 * *count as f64 / kind_max as f64).round_ties_even() as i32).max(2);
        let share = if kind_total > 0 {
            let p = 100.0 * *count as f64 / kind_total as f64;
            format!("{}%", lang.format_float(p, 1, false))
        } else {
            String::new()
        };
        sb.push_str(&format!(
            r#"<tr><td><span class="sw" style="background:{color}"></span>{name}</td><td style="width:120px"><span class="meter"><i style="width:{w}%;background:{color}"></i></span></td><td class="pct num">{share}</td><td class="n num big">{cnt}</td></tr>"#,
            name = h(&lang.name(k)),
            cnt = lang.n(*count as i64),
        ));
    }
    let destroyed_total: usize = matches
        .iter()
        .map(|m| m.chests.iter().filter(|c| c.destroyed).count())
        .sum();
    let mut notes = Vec::new();
    if destroyed_total > 0 {
        let dn = lang.n(destroyed_total as i64);
        notes.push(match lang.code() {
            "en" => format!("of which {dn} destroyed (broken instead of opened)"),
            "es" => format!("de ellos, {dn} destruidos (rotos en lugar de abiertos)"),
            _ => format!("dont {dn} détruits (cassés au lieu d'être ouverts)"),
        });
    }
    for (kind, count) in &other_kinds {
        sb.push_str(&format!(
            r#"<tr class="items"><td>{name} {hors}</td><td></td><td></td><td class="n num">{cnt}</td></tr>"#,
            name = h(&lang.name(kind)),
            hors = lang.t("(hors total)", "(not in the total)", "(fuera del total)"),
            cnt = lang.n(*count as i64),
        ));
    }
    sb.push_str("</table>");
    if !notes.is_empty() {
        sb.push_str(&format!(
            r#"<div class="items" style="padding:10px 0 8px">{}</div>"#,
            notes.join(" · ")
        ));
    }
    sb.push_str("</div></section>");

    // Where your treasures come from
    let mut treasure_rows: Vec<(&MatchRecord, &crate::models::TreasureFind, bool)> = Vec::new();
    let mut team_rows: Vec<(&MatchRecord, &crate::models::TreasureFind, bool)> = Vec::new();
    for m in &matches {
        for f in &m.treasure_finds {
            treasure_rows.push((m, f, false));
        }
        for f in &m.team_treasure_finds {
            team_rows.push((m, f, true));
        }
    }

    sb.push_str(&format!(
        r#"<section><h2>{}</h2><div class="card"><table>"#,
        lang.t(
            "D'où viennent tes trésors",
            "Where your treasures come from",
            "De dónde vienen tus tesoros"
        )
    ));
    if treasure_rows.is_empty() {
        sb.push_str(&format!(
            r#"<tr><td class="items">{}</td></tr>"#,
            lang.t(
                "Pas encore de trésor.",
                "No treasure yet.",
                "Todavía ningún tesoro."
            )
        ));
    }

    let mut src_order: Vec<String> = Vec::new();
    let mut src_groups: HashMap<String, (usize, bool)> = HashMap::new();
    for &(_, f, _) in &treasure_rows {
        let lbl = f.source_label(lang);
        if let Some(entry) = src_groups.get_mut(&lbl) {
            entry.0 += 1;
        } else {
            src_order.push(lbl.clone());
            src_groups.insert(lbl, (1, f.source_is_chest));
        }
    }
    let mut src_sorted: Vec<(String, usize, bool)> = src_order
        .into_iter()
        .map(|k| {
            let (cnt, is_chest) = src_groups[&k];
            (k, cnt, is_chest)
        })
        .collect();
    src_sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let src_total: usize = src_sorted.iter().map(|x| x.1).sum();
    let src_max = src_sorted.iter().map(|x| x.1).max().unwrap_or(1).max(1);

    for (key, cnt, is_chest) in &src_sorted {
        let w = ((100.0 * *cnt as f64 / src_max as f64).round_ties_even() as i32).max(3);
        let share = if src_total > 0 {
            format!("{}%", lang.format_float(100.0 * *cnt as f64 / src_total as f64, 1, false))
        } else {
            String::new()
        };
        if *is_chest {
            sb.push_str(&format!(
                r#"<tr><td>{}</td><td style="width:120px"><span class="meter"><i style="width:{w}%;background:var(--star)"></i></span></td><td class="pct num">{share}</td><td class="n num big">{}</td></tr>"#,
                h(key),
                cnt
            ));
        } else {
            sb.push_str(&format!(
                r#"<tr class="unusual"><td class="warn">{} <span style="font-size:12px">· {}</span></td><td style="width:120px"><span class="meter"><i style="width:{w}%;background:var(--warn)"></i></span></td><td class="pct num warn">{share}</td><td class="n num big warn">{}</td></tr>"#,
                h(key),
                lang.t("fiabilité faible", "low reliability", "fiabilidad baja"),
                cnt
            ));
        }
    }
    sb.push_str("</table></div>");

    let unusual_count = treasure_rows
        .iter()
        .filter(|r| !r.1.source_is_chest)
        .count();
    if unusual_count > 0 {
        let count_str = lang.count_forms(
            unusual_count as i32,
            "trésor obtenu",
            "trésors obtenus",
            "treasure",
            "treasures",
            "tesoro obtenido",
            "tesoros obtenidos",
        );
        let msg = match lang.code() {
            "en" => format!("<b>{count_str} obtained other than from a chest.</b> Low reliability: most likely a mistake in the way the replays are analyzed."),
            "es" => format!("<b>{count_str} de otra forma que con un cofre.</b> Fiabilidad baja: muy probablemente un error en la forma de analizar las repeticiones."),
            _ => format!("<b>{count_str} autrement qu'avec un coffre.</b> Fiabilité faible : il s'agit très probablement d'une erreur dans la façon dont les replays sont analysés."),
        };
        sb.push_str(&format!(r#"<div class="flag">{FLAG_ICON}<span>{msg}</span></div>"#));
    }
    sb.push_str("</section></div>");

    sb.push_str(&chances_section(grand_total, &matches, &data.manual_entries, ctx));
    sb.push_str(&seen_section(&matches, ctx));
    sb.push_str(&collection_section(&matches, &data.obtained_ids, ctx));

    // Treasures table
    if !treasure_rows.is_empty() || !team_rows.is_empty() {
        let names_info = lang.t(
            "Pourquoi certains trésors ont un nom et d'autres non ? Un trésor majeur (tenue, pioche, piste, écran, véhicule, accès à la boutique pirate) sort d'un coffre doré qui porte son identifiant exact : il est nommé à coup sûr (★★), à condition que ce coffre soit dans ton replay, donc à moins d'environ 150 m de toi. Un gizmo est nommé quand il apparaît dans tes mains. Sinon le replay dit seulement qu'un trésor a été reçu, sans dire lequel : « Trésor (type inconnu) ». C'est toujours le cas pour un trésor trouvé par un coéquipier loin de toi.",
            "Why do some treasures have a name and others not? A major treasure (outfit, pickaxe, track, loading screen, vehicle, Pirate Shop Access) comes out of a golden chest that carries its exact id: it is named for sure (★★), as long as that chest is in your replay, that is within about 150 m of you. A gizmo is named when it shows up in your hands. Otherwise the replay only says that a treasure was received, not which one: \"Treasure (unknown type)\". That is always the case for a treasure found by a teammate far from you.",
            "¿Por qué unos tesoros tienen nombre y otros no? Un tesoro mayor (traje, pico, pista, pantalla de carga, vehículo, acceso a la tienda pirata) sale de un cofre dorado que lleva su identificador exacto: se nombra con seguridad (★★), siempre que ese cofre esté en tu repetición, es decir a menos de unos 150 m de ti. Un artilugio se nombra cuando aparece en tus manos. Si no, la repetición solo dice que se recibió un tesoro, no cuál: «Tesoro (tipo desconocido)». Siempre es el caso de un tesoro encontrado por un compañero lejos de ti.",
        );
        let team_switch = if !team_rows.is_empty() {
            let lbl = match lang.code() {
                "en" => format!("Also show the team (+{}, estimate)", team_rows.len()),
                "es" => format!("Mostrar también el equipo (+{}, estimación)", team_rows.len()),
                _ => format!("Afficher aussi l'équipe (+{}, estimation)", team_rows.len()),
            };
            switch_html("showteam", &lbl)
        } else {
            String::new()
        };
        let fold_title = format!(
            r#"{} · <span class="num">{}</span>"#,
            lang.t("Trésors trouvés", "Treasures found", "Tesoros encontrados"),
            treasures
        );
        sb.push_str(&fold_start(
            "treasures",
            &fold_title,
            false,
            &info_html(names_info),
            &team_switch,
        ));
        sb.push_str(&format!(
            r#"<div class="card"><table id="t-treasures"><thead><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>"#,
            lang.t("Partie", "Match", "Partida"),
            lang.t("Trésor", "Treasure", "Tesoro"),
            lang.t("Obtenu à", "Obtained at", "Momento"),
            lang.t("Source", "Source", "Origen"),
            lang.t("Par", "By", "Por")
        ));

        let mut all_t: Vec<(&MatchRecord, &crate::models::TreasureFind, bool)> =
            treasure_rows.iter().chain(team_rows.iter()).copied().collect();
        all_t.sort_by(|a, b| {
            when(b.0, lang).cmp(&when(a.0, lang)).then_with(|| {
                let ta = a.1.obtained_at_seconds.unwrap_or(0.0);
                let tb = b.1.obtained_at_seconds.unwrap_or(0.0);
                ta.total_cmp(&tb)
            })
        });

        for (m, f, team) in all_t {
            let when_s = match f.obtained_at_seconds {
                Some(t) => {
                    if f.time_exact {
                        pretty::clock(t)
                    } else {
                        format!(
                            "{} {}",
                            lang.t("vers", "around", "hacia los"),
                            pretty::clock(t)
                        )
                    }
                }
                None => "?".to_string(),
            };
            let source = if team && f.teammate.as_deref().unwrap_or("").is_empty() {
                format!(
                    r#"<span class="items">{}</span>"#,
                    lang.t(
                        "coffre hors de ta vue",
                        "chest out of your sight",
                        "cofre fuera de tu vista"
                    )
                )
            } else if !f.source_is_chest {
                format!(
                    r#"<span class="warn">{} · {}</span>"#,
                    h(&f.source_label(lang)),
                    lang.t("fiabilité faible", "low reliability", "fiabilidad baja")
                )
            } else if f.source_certain {
                h(&f.source_label(lang))
            } else {
                format!(
                    r#"{} <span class="items">({})</span>"#,
                    h(&f.source_label(lang)),
                    pretty::likely(lang)
                )
            };
            let who = if !team {
                lang.t("toi", "you", "tú").to_string()
            } else {
                let tm = f.teammate.as_deref().unwrap_or("");
                let name = if tm.is_empty() {
                    lang.t("un coéquipier", "a teammate", "un compañero")
                        .to_string()
                } else {
                    h(tm)
                };
                format!("{TEAM_ICON}{name}")
            };
            let cls = if team {
                "r team"
            } else if f.source_is_chest {
                "r"
            } else {
                "r unusual"
            };
            let stars = if f.from_golden_chest {
                r#"<span class="gold">★★</span>"#
            } else {
                r#"<span class="star">★</span>"#
            };
            sb.push_str(&format!(
                r#"<tr class="{cls}"><td class="nw num items">{dt}</td><td>{stars} <b>{disp}</b></td><td class="nw num">{when_s}</td><td>{source}</td><td class="by">{who}</td></tr>"#,
                dt = lang.date_time_short(when(m, lang)),
                disp = h(&f.display_name(lang, ctx.catalog)),
            ));
        }
        sb.push_str(&format!(
            "</tbody></table>{}</div>{FOLD_END}",
            pager("t-treasures", lang)
        ));
    }

    // Latest matches (with filter chips, search, Treasure Hour badges & chests/min rate)
    let is_match_boosted = |m: &MatchRecord| -> bool {
        m.match_start_utc
            .and_then(|s| ctx.catalog.treasure_hour_at(s))
            .is_some_and(|hr| hr.boosts.iter().any(|(_, b)| *b > 1.0))
    };
    let boosted_matches = matches.iter().filter(|m| is_match_boosted(m)).count();
    let treasure_matches = matches.iter().filter(|m| !m.treasure_finds.is_empty()).count();

    let matches_title = format!(
        r#"{} · <span class="num">{}</span>"#,
        lang.t("Dernières parties", "Latest matches", "Últimas partidas"),
        lang.n(matches.len() as i64)
    );
    sb.push_str(&fold_start("matches", &matches_title, false, "", ""));
    let sq_th = if team_chests > 0 {
        format!(
            r#"<th class="sq" style="text-align:right">{}</th>"#,
            h(squad_label(lang))
        )
    } else {
        String::new()
    };

    sb.push_str(r#"<div class="card">"#);
    if !matches.is_empty() {
        sb.push_str(r#"<div class="mtoolbar" data-for="t-matches"><div class="fchips">"#);
        sb.push_str(&format!(
            r#"<button type="button" class="fchip on" data-f="all">{} ({})</button>"#,
            lang.t("Toutes", "All", "Todas"),
            lang.n(matches.len() as i64)
        ));
        if boosted_matches > 0 {
            sb.push_str(&format!(
                r#"<button type="button" class="fchip boost" data-f="boost">⚡ {} ({})</button>"#,
                lang.t("Heure du trésor", "Treasure Hour", "Hora del tesoro"),
                lang.n(boosted_matches as i64)
            ));
        }
        if treasure_matches > 0 {
            sb.push_str(&format!(
                r#"<button type="button" class="fchip star" data-f="tre">★ {} ({})</button>"#,
                lang.t("Avec trésor", "Treasures", "Con tesoro"),
                lang.n(treasure_matches as i64)
            ));
        }
        let search_ph = h(lang.t(
            "Filtrer par mode, date, trésor…",
            "Filter by mode, date, treasure…",
            "Filtrar por modo, fecha, tesoro…",
        ));
        sb.push_str(&format!(
            r#"</div><input type="search" class="msearch" id="m-search" placeholder="{search_ph}" aria-label="{search_ph}"></div>"#
        ));
    }

    sb.push_str(&format!(
        r#"<table id="t-matches"><thead><tr><th>{}</th><th>{}</th><th>{}</th><th style="text-align:right">{}</th>{sq_th}<th>{}</th></tr></thead><tbody>"#,
        lang.t("Date", "Date", "Fecha"),
        lang.t("Mode", "Mode", "Modo"),
        lang.t("Place", "Placement", "Puesto"),
        lang.t("Coffres", "Chests", "Cofres"),
        lang.t("Trésor", "Treasure", "Tesoro"),
    ));

    let vwarn = vehicle_warning(lang);
    let boost_tip = h(lang.t(
        "Partie lancée pendant une Heure du trésor",
        "Match started during Treasure Hour",
        "Partida iniciada durante la Hora del tesoro",
    ));
    for m in &matches {
        let mut treasure = m
            .treasure_finds
            .iter()
            .map(|f| {
                let cls = if f.from_golden_chest { "gold" } else { "star" };
                let stars = if f.from_golden_chest { "★★" } else { "★" };
                format!(
                    r#"<span class="{cls}">{stars} {}</span>"#,
                    h(&f.display_name(lang, ctx.catalog))
                )
            })
            .collect::<Vec<_>>()
            .join("<br>");
        for (i, f) in m.team_treasure_finds.iter().enumerate() {
            let br = if i == 0 && m.treasure_finds.is_empty() {
                ""
            } else {
                "<br>"
            };
            treasure.push_str(&format!(
                r#"<span class="team">{br}{TEAM_ICON} ★ {}</span>"#,
                h(&f.display_name(lang, ctx.catalog))
            ));
        }
        let win = m.placement == Some(1);
        let win_attr = if win {
            r#" class="nw num gold" style="font-weight:700""#
        } else {
            r#" class="nw num""#
        };
        let veh_warn = if m.vehicle_chests_missing() {
            format!(r#" <span class="warn" title="{}">⚠</span>"#, h(vwarn))
        } else {
            String::new()
        };
        let boosted = is_match_boosted(m);
        let btag = if boosted {
            format!(r#"<span class="btag" title="{boost_tip}">⚡</span>"#)
        } else {
            String::new()
        };
        let rate_html = match m.duration_seconds {
            Some(d) if d >= 60 && m.chests_opened > 0 => {
                let cpm = m.chests_opened as f64 * 60.0 / d as f64;
                format!(r#"<span class="rate">{}/m</span>"#, lang.format_float(cpm, 1, false))
            }
            _ => String::new(),
        };
        let sq_td = if team_chests > 0 {
            let title_str = m
                .team_chests
                .iter()
                .filter(|x| x.chests > 0)
                .map(|x| format!("{} {}", x.name, x.chests))
                .collect::<Vec<_>>()
                .join(", ");
            let val_str = if !m.team_chests.is_empty() {
                format!("+{}", m.team_chests_opened)
            } else {
                String::new()
            };
            format!(
                r#"<td class="n num sq" title="{}">{val_str}</td>"#,
                h(&title_str)
            )
        } else {
            String::new()
        };
        let data_attrs = format!(
            "{}{}",
            if boosted { " data-boost=\"1\"" } else { "" },
            if !m.treasure_finds.is_empty() { " data-tre=\"1\"" } else { "" }
        );
        sb.push_str(&format!(
            r#"<tr class="r"{data_attrs}><td class="nw num items">{dt}</td><td>{pl}{btag}</td><td{win_attr}>{place}</td><td class="n num big">{chests}{rate_html}{veh_warn}</td>{sq_td}<td class="items">{treasure}</td></tr>"#,
            dt = lang.date_time_short(when(m, lang)),
            pl = h(&pretty::playlist(m.playlist.as_deref(), lang)),
            place = h(&pretty::placement(m, lang)),
            chests = m.chests_opened,
        ));
    }
    sb.push_str(&format!("</tbody></table>{}</div>", pager("t-matches", lang)));
    let blind = matches.iter().filter(|m| m.vehicle_chests_missing()).count();
    if blind > 0 {
        let blind_cnt = lang.count_forms(
            blind as i32,
            "partie",
            "parties",
            "match",
            "matches",
            "partida",
            "partidas",
        );
        sb.push_str(&format!(
            r#"<div class="flag">{FLAG_ICON}<span><b>⚠ {blind_cnt}</b> · {}</span></div>"#,
            h(vwarn)
        ));
    }
    sb.push_str(FOLD_END);

    if in_app {
        sb.push_str("</div></div>");
        return sb;
    }

    sb.push_str(lang.t(
        "<footer>Compte les coffres que tu as ouverts ou détruits toi-même, d'après tes replays (« rediffusions » dans Fortnite), analysés après chaque partie. Les caisses de munitions et de nourriture ne sont pas comptées. ★★ = coffre doré de la chasse au trésor, détecté de façon certaine. ★ = trésor reçu grâce à un de tes coffres (moment exact), nommé quand l'objet est identifiable. L'icône équipe marque un trésor trouvé par un coéquipier (estimation : déduit des trésors que tu reçois sans avoir ouvert de coffre, car la chasse au trésor les partage avec toute l'équipe) ; il n'entre pas dans tes totaux. Le total de l'équipe ajoute aux tiens les coffres ouverts ou détruits par tes coéquipiers, comptés de la même façon dans tes replays ; un coffre ouvert à plus de 150 m de toi peut manquer, car le replay ne l'enregistre pas.</footer>",
        "<footer>Counts the chests you opened or destroyed yourself, from your replays, analyzed after each match. Ammo boxes and food boxes are not counted. ★★ = golden treasure hunt chest, detected for sure. ★ = treasure received from one of your own chests (exact moment), named when the item can be identified. The team icon marks a treasure found by a teammate (estimate: deduced from the treasures you receive without opening a chest, as the treasure hunt shares them with the whole team); it is not in your totals. The squad total adds the chests opened or destroyed by your teammates, counted the same way in your replays; a chest opened more than 150 m away from you can be missing, as the replay does not record it.</footer>",
        "<footer>Cuenta los cofres que has abierto o destruido tú mismo, a partir de tus repeticiones, analizadas después de cada partida. Las cajas de munición y de comida no se cuentan. ★★ = cofre dorado de la búsqueda del tesoro, detectado con seguridad. ★ = tesoro recibido de uno de tus cofres (momento exacto), con nombre cuando el objeto se puede identificar. El icono de equipo marca un tesoro encontrado por un compañero (estimación: deducido de los tesoros que recibes sin abrir un cofre, ya que la búsqueda del tesoro los comparte con todo el equipo); no cuenta en tus totales. El total del equipo añade a los tuyos los cofres abiertos o destruidos por tus compañeros, contados igual en tus repeticiones; un cofre abierto a más de 150 m de ti puede faltar, porque la repetición no lo graba.</footer>",
    ));
    sb.push_str("<script>(function(){\n");
    sb.push_str(REPORT_SCRIPT);
    sb.push('\n');
    sb.push_str(TREASURE_HOUR_SCRIPT);
    sb.push_str(STANDALONE_REPORT_INIT);
    sb.push_str("})();</script></main></body></html>");
    to_crlf(&sb)
}

pub(crate) const REPORT_SCRIPT: &str = r#"function initReport(root,surface,post){
function get(k){try{return localStorage.getItem(k)}catch(e){return null}}
function put(k,v){try{localStorage.setItem(k,v)}catch(e){}}
// Interactive SVG chart hover tooltips
[].forEach.call(root.querySelectorAll('.vizbox'),function(box){
  var tip=box.querySelector('.charttip');if(!tip)return;
  function show(hit){
    var txt=hit.getAttribute('data-tip');if(!txt)return;
    tip.textContent=txt;tip.hidden=false;
    var br=box.getBoundingClientRect(),hr=hit.getBoundingClientRect();
    var cx=(hr.left+hr.width/2)-br.left,tw=tip.offsetWidth,th=tip.offsetHeight;
    var left=Math.max(4,Math.min(br.width-tw-4,cx-tw/2));
    var top=-th-6;
    tip.style.left=left+'px';tip.style.top=top+'px';
  }
  function hide(){tip.hidden=true}
  [].forEach.call(box.querySelectorAll('.svghit'),function(hit){
    hit.addEventListener('mouseenter',function(){show(hit)});
    hit.addEventListener('focus',function(){show(hit)});
    hit.addEventListener('mouseleave',hide);
    hit.addEventListener('blur',hide);
  });
});
var pagers=[];
[].forEach.call(root.querySelectorAll('.pager'),function(box){
  var t=root.getElementById(box.getAttribute('data-for'));if(!t)return;
  var step=+box.getAttribute('data-step')||10,key='chestcounter-rows-'+t.id,n=parseInt(get(key),10);if(!(n>=step))n=step;
  var more=box.querySelector('.more'),all=box.querySelector('.all'),less=box.querySelector('.less'),info=box.querySelector('.count');
  function rows(){return [].filter.call(t.querySelectorAll('tr.r'),function(r){return !r.classList.contains('flt')&&(!r.classList.contains('team')||surface.classList.contains('showteam'))})}
  function apply(){
    [].forEach.call(t.querySelectorAll('tr.r'),function(r){r.classList.remove('pg')});
    var v=rows();v.forEach(function(r,i){if(i>=n)r.classList.add('pg')});
    var shown=Math.min(n,v.length);
    info.textContent=info.getAttribute('data-fmt').replace('{0}',shown).replace('{1}',v.length);
    more.hidden=all.hidden=shown>=v.length;less.hidden=n<=step;box.hidden=v.length<=step;
  }
  more.onclick=function(){n+=step*2;put(key,n);apply()};
  all.onclick=function(){n=1e9;put(key,n);apply()};
  less.onclick=function(){n=step;put(key,n);apply()};
  apply();pagers.push(apply);
});
// Match table filter chips + search
[].forEach.call(root.querySelectorAll('.mtoolbar'),function(bar){
  var t=root.getElementById(bar.getAttribute('data-for'));if(!t)return;
  var chips=bar.querySelectorAll('.fchip'),inp=bar.querySelector('.msearch'),mode='all';
  function run(){
    var q=(inp?inp.value:'').trim().toLowerCase();
    [].forEach.call(t.querySelectorAll('tr.r'),function(r){
      var ok=true;
      if(mode==='boost')ok=r.getAttribute('data-boost')==='1';
      else if(mode==='tre')ok=r.getAttribute('data-tre')==='1';
      if(ok&&q)ok=r.textContent.toLowerCase().indexOf(q)!==-1;
      r.classList.toggle('flt',!ok);
    });
    pagers.forEach(function(p){p()});
  }
  [].forEach.call(chips,function(c){
    c.onclick=function(){
      mode=c.getAttribute('data-f')||'all';
      [].forEach.call(chips,function(x){x.classList.toggle('on',x===c)});
      run();
    };
  });
  if(inp)inp.addEventListener('input',run);
});
// Switches: the counter's setting, or (report opened as a file) what this browser remembers.
function toggle(id,cls,action){
  var b=root.getElementById(id),key='chestcounter-'+cls,v=surface.getAttribute('data-'+cls)==='1';
  if(!post){var saved=get(key);if(saved!==null)v=saved==='1'}
  function set(x){surface.classList.toggle(cls,x);if(b)b.checked=x;pagers.forEach(function(p){p()})}
  set(v);
  if(b)b.addEventListener('change',function(){set(b.checked);put(key,b.checked?'1':'0');if(post)post(action,b.checked)});
}
toggle('showteam','showteam','showTeamInReport');
toggle('showsquad','showsquad','showSquadInReport');
// Folding sections: the title opens or closes the whole section.
[].forEach.call(root.querySelectorAll('section.fold'),function(s){
  var key='chestcounter-fold-'+s.getAttribute('data-fold'),btn=s.querySelector('.foldbtn'),open=s.getAttribute('data-open')==='1';
  var saved=get(key);if(saved!==null)open=saved==='1';
  function set(o){s.classList.toggle('closed',!o);btn.setAttribute('aria-expanded',o?'true':'false')}
  set(open);
  btn.addEventListener('click',function(){var o=s.classList.contains('closed');set(o);put(key,o?'1':'0')});
});
var ok={};try{ok=JSON.parse(get('chestcounter-obtained')||'{}')||{}}catch(e){}
function send(id,on){if(post)post('obtained',{id:id,on:on})}
var boxes=root.querySelectorAll('input[data-prize]');
function refresh(){
  var n=0;[].forEach.call(boxes,function(c){c.parentNode.classList.toggle('got',c.checked);if(c.checked)n++});
  var cc=root.getElementById('colcount');if(cc)cc.textContent=n+'/'+boxes.length;
  [].forEach.call(root.querySelectorAll('[data-items]'),function(card){
    var got=card.getAttribute('data-items').split(' ').some(function(id){var c=root.querySelector('input[data-prize="'+id+'"]');return c&&c.checked});
    card.classList.toggle('got',got);
  });
}
[].forEach.call(boxes,function(c){
  if(c.hasAttribute('data-auto'))return;if(!c.hasAttribute('data-saved'))c.checked=!!ok[c.getAttribute('data-prize')];
  c.addEventListener('change',function(){var id=c.getAttribute('data-prize');if(c.checked)ok[id]=1;else delete ok[id];put('chestcounter-obtained',JSON.stringify(ok));send(id,c.checked);refresh()});
});
refresh();
}"#;

const STANDALONE_REPORT_INIT: &str = r#"
// Standalone theme toggle
var savedTheme;try{savedTheme=localStorage.getItem('chestcounter-theme')}catch(e){}
if(savedTheme==='light'||savedTheme==='dark')document.documentElement.setAttribute('data-theme',savedTheme);
var tb=document.getElementById('themebtn');
if(tb)tb.onclick=function(){
  var cur=document.documentElement.getAttribute('data-theme');
  var isLight=cur==='light'||(!cur&&window.matchMedia&&window.matchMedia('(prefers-color-scheme:light)').matches);
  var next=isLight?'dark':'light';
  document.documentElement.setAttribute('data-theme',next);
  try{localStorage.setItem('chestcounter-theme',next)}catch(e){}
};
var tok=new URLSearchParams(location.search).get('t');
var post=tok?function(action,value){
  fetch('/api/action',{method:'POST',headers:{'X-Token':tok,'Content-Type':'application/json'},body:JSON.stringify({action:action,value:value})}).catch(function(){});
}:null;
initReport(document,document.body,post);
startTreasureHour(document);
"#;

const FOLD_END: &str = "</div></section>";

fn fold_start(key: &str, title: &str, open: bool, info: &str, right: &str) -> String {
    format!(
        r#"<section class="fold{closed}" data-fold="{key}" data-open="{dopen}"><div class="head"><h2><button type="button" class="foldbtn" aria-expanded="{expanded}"><span class="chev" aria-hidden="true"></span>{title}</button>{info}</h2>{right}</div><div class="foldbody">"#,
        closed = if open { "" } else { " closed" },
        dopen = if open { 1 } else { 0 },
        expanded = if open { "true" } else { "false" },
    )
}

fn switch_html(id: &str, label: &str) -> String {
    format!(
        r#"<label class="tsw"><input type="checkbox" id="{id}"><span class="trk" aria-hidden="true"></span><span>{label}</span></label>"#
    )
}

fn pager(table_id: &str, lang: &LangCtx) -> String {
    format!(
        r#"<div class="pager" data-for="{table_id}" data-step="10" hidden><span class="count" data-fmt="{fmt}"></span><button type="button" class="more">{more}</button><button type="button" class="all">{all}</button><button type="button" class="less" hidden>{less}</button></div>"#,
        fmt = h(lang.t("{0} sur {1}", "{0} of {1}", "{0} de {1}")),
        more = lang.t("Afficher 20 de plus", "Show 20 more", "Mostrar 20 más"),
        all = lang.t("Tout afficher", "Show all", "Mostrar todo"),
        less = lang.t("Réduire", "Show less", "Reducir"),
    )
}

fn info_html(text: &str) -> String {
    let enc = h(text);
    format!(
        r#"<span class="info" tabindex="0" role="note" aria-label="{enc}">i<span class="infotip">{enc}</span></span>"#
    )
}

fn squad_info(lang: &LangCtx) -> String {
    let text = lang.t(
        "Pourquoi ce nombre est bas ? Il vient de TES replays. Fortnite n'y enregistre que ce qui se passe à environ 150 m autour de toi : un coffre ouvert par un coéquipier plus loin n'y figure pas et ne peut pas être compté. Rien non plus après ton départ de la partie (élimination puis retour au salon). Plus vous lootez ensemble, plus le nombre est juste. Les coffres comptés, eux, le sont avec les mêmes règles que les tiens.",
        "Why is this number low? It comes from YOUR replays. Fortnite only records what happens within about 150 m of you: a chest a teammate opens farther away is not in it and cannot be counted. Nothing either once you leave the match (eliminated, back to the lobby). The more you loot together, the closer it gets. The chests that are counted are counted with the same rules as yours.",
        "¿Por qué este número es bajo? Viene de TUS repeticiones. Fortnite solo graba lo que pasa a unos 150 m a tu alrededor: un cofre que un compañero abre más lejos no está en ella y no se puede contar. Tampoco nada después de que salgas de la partida (eliminado, de vuelta al lobby). Cuanto más looteéis juntos, más exacto es. Los cofres contados se cuentan con las mismas reglas que los tuyos.",
    );
    info_html(text)
}

fn vehicle_warning(lang: &LangCtx) -> &'static str {
    lang.t(
        "Fortnite n'a pas enregistré ce que ton véhicule a détruit dans ce replay : du butin de coffres a été vu le long de ton trajet sans aucun coffre enregistré, ces coffres ne peuvent pas être comptés. Désactive « Enregistrer des rediffusions de haute qualité » dans Fortnite (Paramètres → Jeu → Rediffusions) puis relance le jeu.",
        "Fortnite did not record what your vehicle destroyed in this replay: chest loot was seen along your drive with no chest recorded, so those chests cannot be counted. Turn off \"Record High Quality Replays\" in Fortnite (Settings → Game → Replays), then restart the game.",
        "Fortnite no grabó lo que destruyó tu vehículo en esta repetición: se vio botín de cofres a lo largo de tu trayecto sin ningún cofre grabado, así que esos cofres no se pueden contar. Desactiva «Grabar repeticiones de alta calidad» en Fortnite (Ajustes → Juego → Repeticiones) y reinicia el juego.",
    )
}

fn notice_cards(ctx: &ReportContext<'_>) -> String {
    let lang = ctx.lang;
    let mut out = String::new();
    for n in ctx.catalog.active_notices(ctx.now_utc) {
        let cls = if n.level == "warning" { " warning" } else { "" };
        let title_str = n.title_in(lang.code());
        let title_html = if !title_str.is_empty() {
            format!("<b>{}</b><br>", h(title_str))
        } else {
            String::new()
        };
        let link_html = match &n.link {
            Some(url) => format!(
                r#"<br><a href="{}" target="_blank" rel="noopener">{}</a>"#,
                h(url),
                lang.t("En savoir plus", "More info", "Más información")
            ),
            None => String::new(),
        };
        out.push_str(&format!(
            r#"<div class="notice{cls}">{title_html}{}{link_html}</div>"#,
            h(n.text_in(lang.code()))
        ));
    }
    out
}

fn loading_card(p: Option<ScanProgress>, lang: &LangCtx) -> String {
    let Some(p) = p else {
        return String::new();
    };
    let pct = if p.total > 0 {
        (100.0 * p.done as f64 / p.total as f64).round_ties_even() as i32
    } else {
        0
    };
    match lang.code() {
        "en" => format!(
            r#"<div class="loading"><div class="spin"></div><div class="grow"><b>Analyzing your replays… {} / {}</b><div class="pbar"><i style="width:{pct}%"></i></div><span class="muted">This page updates by itself: the numbers below grow as the replays are read.</span></div></div>"#,
            p.done, p.total
        ),
        "es" => format!(
            r#"<div class="loading"><div class="spin"></div><div class="grow"><b>Analizando tus repeticiones… {} / {}</b><div class="pbar"><i style="width:{pct}%"></i></div><span class="muted">Esta página se actualiza sola: las cifras de abajo aumentan a medida que se leen las repeticiones.</span></div></div>"#,
            p.done, p.total
        ),
        _ => format!(
            r#"<div class="loading"><div class="spin"></div><div class="grow"><b>Analyse de tes replays… {} / {}</b><div class="pbar"><i style="width:{pct}%"></i></div><span class="muted">Cette page se met à jour toute seule : les chiffres ci-dessous augmentent au fil de la lecture des replays.</span></div></div>"#,
            p.done, p.total
        ),
    }
}

fn profiles_line(profiles_list: &[Profile], player: &str, lang: &LangCtx) -> String {
    let others: Vec<String> = profiles_list
        .iter()
        .filter(|p| p.name != player)
        .map(|p| h(&p.name))
        .collect();
    if others.is_empty() {
        return String::new();
    }
    format!(
        r#"<div class="profiles muted">{}{} · {}</div>"#,
        lang.t(
            "Autres profils sur ce PC : ",
            "Other profiles on this PC: ",
            "Otros perfiles en este PC: "
        ),
        others.join(", "),
        lang.t(
            "« Changer de profil » dans le compteur",
            "\"Switch profile\" in the counter",
            "«Cambiar de perfil» en el contador"
        )
    )
}

#[derive(Debug, Clone, Copy)]
struct ChestOpening<'a> {
    opened_utc: DateTime<Utc>,
    treasure_hour: Option<&'a TreasureHour>,
}

#[derive(Debug, Clone)]
struct Chance {
    p: f64,
    log_never: f64,
    average: f64,
    chests: i32,
    base_chests: i64,
    boosted_chests: Vec<(f64, i64)>,
}

impl Chance {
    fn of(odds: &TreasureOdds, openings: &[ChestOpening<'_>], plain: i64) -> Self {
        let p = 1.0 / odds.one_in as f64;
        let plain = plain.max(0);
        let mut log_never = if plain == 0 {
            0.0
        } else {
            plain as f64 * (1.0 - p).ln()
        };
        let mut average = plain as f64 * p;
        let mut base_chests = plain;
        let mut boosted_chests: Vec<(f64, i64)> = Vec::new();
        for opening in openings {
            let raw_boost = opening
                .treasure_hour
                .map(|h| h.boost_for(&odds.kind))
                .unwrap_or(1.0);
            let boost = if raw_boost > 1.0 { raw_boost } else { 1.0 };
            if boost == 1.0 {
                base_chests += 1;
            } else if let Some((_, count)) = boosted_chests
                .iter_mut()
                .find(|(rate, _)| (*rate - boost).abs() < 1e-9)
            {
                *count += 1;
            } else {
                boosted_chests.push((boost, 1));
            }
            let per_chest = (p * boost).min(0.999);
            log_never += (1.0 - per_chest).ln();
            average += per_chest;
        }
        boosted_chests.sort_by(|a, b| a.0.total_cmp(&b.0));
        Self {
            p: -(log_never.exp() - 1.0),
            log_never,
            average,
            chests: (openings.len() as i64 + plain) as i32,
            base_chests,
            boosted_chests,
        }
    }
}

fn percent(p: f64, lang: &LangCtx) -> String {
    let v = p * 100.0;
    if v >= 99.95 {
        let most = lang.format_float(99.9, 1, false);
        return if lang.code() == "en" {
            format!(">{most}%")
        } else {
            format!("> {most} %")
        };
    }
    let decimals = if v >= 10.0 {
        1
    } else if v <= 0.0 {
        0
    } else {
        ((-v.log10()).ceil() as i32 + 1).clamp(1, 8) as usize
    };
    let text = lang.format_float(v, decimals, false);
    if lang.code() == "en" {
        format!("{text}%")
    } else {
        format!("{text} %")
    }
}

fn one_in_huge(log_chance: f64, lang: &LangCtx) -> String {
    let log10 = -log_chance / 10.0_f64.ln();
    if log10 < 9.0 {
        let v = (-log_chance).exp().round_ties_even() as i64;
        return lang.n(v);
    }
    let exp = log10.floor() as i32;
    let mantissa = 10.0_f64.powf(log10 - exp as f64);
    const SUP: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    let exp_sup: String = exp
        .to_string()
        .chars()
        .map(|d| SUP[(d as u8 - b'0') as usize])
        .collect();
    format!("{} × 10{exp_sup}", lang.format_float(mantissa, 1, false))
}

fn percent_html(c: &Chance, lang: &LangCtx) -> String {
    let text = percent(c.p, lang);
    if c.p * 100.0 < 99.95 {
        return text;
    }
    let x = one_in_huge(c.log_never, lang);
    let never = match lang.code() {
        "en" => format!("never had it: 1 chance in {x}"),
        "es" => format!("no haberlo conseguido: 1 entre {x}"),
        _ => format!("ne pas l'avoir eu : 1 chance sur {x}"),
    };
    format!(r#"<span title="{}">{text}</span>"#, h(&never))
}

fn verdict(p: f64, lang: &LangCtx) -> (&'static str, &'static str) {
    let pct = p * 100.0;
    if pct >= 99.95 {
        ("certain", lang.t("Presque sûr", "Almost certain", "Casi seguro"))
    } else if pct >= 95.0 {
        ("vlikely", lang.t("Très probable", "Very likely", "Muy probable"))
    } else if pct >= 75.0 {
        ("likely", lang.t("Probable", "Likely", "Probable"))
    } else if pct >= 25.0 {
        ("possible", lang.t("Peut-être", "Maybe", "Quizás"))
    } else if pct >= 5.0 {
        ("unlikely", lang.t("Peu probable", "Unlikely", "Poco probable"))
    } else {
        ("vunlikely", lang.t("Très peu probable", "Very unlikely", "Muy poco probable"))
    }
}

fn expected_html(average: f64, lang: &LangCtx) -> String {
    if average < 0.05 {
        format!("&lt; {}", lang.format_float(0.1, 1, true))
    } else if average >= 10.0 {
        format!("≈ {}", lang.format_float(average, 0, true))
    } else {
        format!("≈ {}", lang.format_float(average, 1, true))
    }
}

fn countdown_html(utc: DateTime<Utc>) -> String {
    format!(
        r#"<span class="cd" data-end="{}"></span>"#,
        utc.format("%Y-%m-%dT%H:%M:%SZ")
    )
}

fn time_el(utc: DateTime<Utc>, format_kind: &str, now_utc: DateTime<Utc>, lang: &LangCtx) -> String {
    let shown = lang.shown(utc);
    let text = lang.time_el_text(shown, format_kind);
    let local = lang.to_local(utc);
    let utc_fixed = utc.with_timezone(&FixedOffset::east_opt(0).unwrap());
    let tip = format!(
        "{}, {} ({}) · {}, {} UTC",
        lang.long_date(local),
        lang.format_time(local, false),
        lang.local_offset_string(now_utc),
        lang.long_date(utc_fixed),
        lang.format_time(utc_fixed, false),
    );
    format!(
        r#"<time datetime="{}" title="{}">{}</time>"#,
        utc.format("%Y-%m-%dT%H:%M:%SZ"),
        h(&tip),
        h(&text)
    )
}

fn next_regular_hours(now: DateTime<Utc>) -> Vec<DateTime<Utc>> {
    let next_for = |target_day: Weekday, hour: u32| -> DateTime<Utc> {
        // C# DayOfWeek: Sunday=0, Monday=1, ..., Saturday=6
        let csharp_dow = |wd: Weekday| -> i64 { wd.num_days_from_sunday() as i64 };
        let delta = (csharp_dow(target_day) - csharp_dow(now.weekday()) + 7) % 7;
        let base = Utc
            .with_ymd_and_hms(now.year(), now.month(), now.day(), hour, 0, 0)
            .single()
            .unwrap()
            + Duration::days(delta);
        if base + Duration::hours(2) > now {
            base
        } else {
            base + Duration::days(7)
        }
    };
    let mut out = vec![next_for(Weekday::Sat, 19), next_for(Weekday::Sun, 2)];
    out.sort();
    out
}

fn format_dotnet_r(v: f64) -> String {
    // Match .NET double.ToString("R", CultureInfo.InvariantCulture)
    if v == 0.0 {
        return "0".to_string();
    }
    let abs = v.abs();
    if abs < 1e-4 || abs >= 1e15 {
        // .NET uses uppercase E with signed 2-digit exponent for small/large values
        let s = format!("{v:e}");
        if let Some((mant, exp_str)) = s.split_once('e') {
            let exp: i32 = exp_str.parse().unwrap_or(0);
            if exp < 0 {
                return format!("{mant}E-{:02}", -exp);
            } else {
                return format!("{mant}E+{:02}", exp);
            }
        }
    }
    format!("{v}")
}

fn chances_section(
    chests: i32,
    matches: &[&MatchRecord],
    manual_entries: &[ManualEntry],
    ctx: &ReportContext<'_>,
) -> String {
    let lang = ctx.lang;
    let catalog = ctx.catalog;
    let now = ctx.now_utc;
    let Some(last_week) = catalog.weeks.last() else {
        return String::new();
    };
    if now >= last_week.to + Duration::days(14) {
        return String::new();
    }

    let mut sb = String::new();
    let mut chest_openings: Vec<ChestOpening<'_>> = Vec::new();
    for m in matches {
        // Boosted odds apply when the match itself started during a Treasure Hour,
        // not based on when each individual chest was opened.
        let treasure_hour = m
            .match_start_utc
            .and_then(|start| catalog.treasure_hour_at(start));
        let start = m.start_utc();
        for c in &m.chests {
            let secs = c.time_seconds as f64;
            let nanos = (secs * 1_000_000_000.0).round() as i64;
            chest_openings.push(ChestOpening {
                opened_utc: start + Duration::nanoseconds(nanos),
                treasure_hour,
            });
        }
    }

    let hunt_start = catalog.hunt_start();
    let hunt_end = last_week.to;
    let mut hunt_openings: Vec<ChestOpening<'_>> = chest_openings
        .iter()
        .copied()
        .filter(|c| c.opened_utc >= hunt_start)
        .collect();
    hunt_openings.sort_by_key(|c| c.opened_utc);

    let manual: i64 = chests as i64 - chest_openings.len() as i64;
    let season_chests: i64 = (hunt_openings.len() as i64 + manual).max(0);
    let week = catalog.shown_week(now);
    let week_openings: Vec<ChestOpening<'_>> = match week {
        Some(w) => hunt_openings
            .iter()
            .copied()
            .filter(|c| c.opened_utc >= w.from && c.opened_utc < w.to)
            .collect(),
        None => Vec::new(),
    };
    let week_manual: i64 = match week {
        Some(w) => manual_entries
            .iter()
            .filter(|e| e.week_from_utc == Some(w.from))
            .map(|e| e.chests as i64)
            .sum::<i64>()
            .max(0),
        None => 0,
    };
    let week_chests: i64 = week_openings.len() as i64 + week_manual;
    let boosted: usize = hunt_openings
        .iter()
        .filter(|c| {
            c.treasure_hour
                .is_some_and(|h| h.boosts.iter().any(|(_, b)| *b > 1.0))
        })
        .count();

    let season_chance = |odds: &TreasureOdds| -> Chance {
        if manual >= 0 {
            Chance::of(odds, &hunt_openings, manual)
        } else {
            Chance::of(odds, &[], season_chests)
        }
    };
    let week_chance = |odds: &TreasureOdds| -> Chance {
        Chance::of(odds, &week_openings, week_manual)
    };
    let shown_chance = |odds: &TreasureOdds| -> Chance {
        if odds.weekly {
            week_chance(odds)
        } else {
            season_chance(odds)
        }
    };
    let odds_of = |kind: &str| -> &TreasureOdds {
        catalog.odds.iter().find(|o| o.kind == kind).unwrap()
    };
    let prize_of = |kind: &str| -> Option<&TreasurePrize> {
        week.and_then(|w| w.prizes.iter().find(|p| p.kind == kind))
    };

    let per_day_fn = |count: i64, from: DateTime<Utc>, to: DateTime<Utc>| -> f64 {
        let d = (to - from).num_nanoseconds().unwrap_or(0) as f64 / 86_400_000_000_000.0;
        if d >= 1.0 {
            count as f64 / d
        } else {
            0.0
        }
    };
    let season_pace = if !hunt_openings.is_empty() {
        per_day_fn(hunt_openings.len() as i64, hunt_openings[0].opened_utc, now)
    } else {
        0.0
    };
    let mut week_pace = match week {
        Some(w) => per_day_fn(week_chests, w.from, now),
        None => 0.0,
    };
    if week_pace <= 0.0 {
        week_pace = season_pace;
    }

    let projection = |odds: &TreasureOdds, c: &Chance, weekly: bool| -> String {
        let end = if weekly {
            week.map(|w| w.to).unwrap_or(now)
        } else {
            hunt_end
        };
        let pace = if weekly { week_pace } else { season_pace };
        if pace <= 0.0 || now >= end || c.p >= 0.75 {
            return String::new();
        }
        let rem_days = (end - now).num_nanoseconds().unwrap_or(0) as f64 / 86_400_000_000_000.0;
        let log = c.log_never + pace * rem_days * (-1.0 / odds.one_in as f64).ln_1p();
        let p_html = percent_html(
            &Chance {
                p: -(log.exp() - 1.0),
                log_never: log,
                average: 0.0,
                chests: 0,
                base_chests: 0,
                boosted_chests: Vec::new(),
            },
            lang,
        );
        if weekly {
            match lang.code() {
                "en" => format!("≈ {p_html} by the end of the week at your pace"),
                "es" => format!("≈ {p_html} al final de la semana a tu ritmo"),
                _ => format!("≈ {p_html} d'ici la fin de la semaine à ton rythme"),
            }
        } else {
            match lang.code() {
                "en" => format!("≈ {p_html} by the end of the hunt at your pace"),
                "es" => format!("≈ {p_html} al final de la búsqueda a tu ritmo"),
                _ => format!("≈ {p_html} d'ici la fin de la chasse à ton rythme"),
            }
        }
    };

    let mut gizmo_times: Vec<DateTime<Utc>> = Vec::new();
    for m in matches {
        let start = m.start_utc();
        for f in &m.treasure_finds {
            if !f.from_golden_chest {
                let nanos = (f.obtained_at_seconds.unwrap_or(0.0) as f64 * 1_000_000_000.0).round() as i64;
                let t = start + Duration::nanoseconds(nanos);
                if t >= hunt_start {
                    gizmo_times.push(t);
                }
            }
        }
    }
    gizmo_times.sort();

    let tag = |weekly: bool| -> String {
        if weekly {
            format!(
                r#"<span class="tag weekly">{}</span>"#,
                lang.t("Hebdo", "Weekly", "Semanal")
            )
        } else {
            format!(
                r#"<span class="tag season">{}</span>"#,
                lang.t("Saison", "Season", "Temporada")
            )
        }
    };
    let chip = |p: f64| -> String {
        let (cls, text) = verdict(p, lang);
        format!(r#"<span class="vchip {cls}">{text}</span>"#)
    };
    let one_in_i64 = |n: i64| -> String {
        let ns = lang.n(n);
        match lang.code() {
            "en" => format!("1 in {ns}"),
            "es" => format!("1 entre {ns}"),
            _ => format!("1 sur {ns}"),
        }
    };
    let bar = |c: &Chance, odds: &TreasureOdds, halfway_pct: Option<f64>, wide: bool| -> String {
        let (cls, raw_pct) = match halfway_pct {
            Some(pct) => ("gold", pct),
            None => (verdict(c.p, lang).0, c.p * 100.0),
        };
        let w = raw_pct.clamp(0.8, 100.0);
        let wide_cls = if wide { " wide" } else { "" };
        let title = catalog.kind_name(&odds.kind, lang);
        let base_lbl = lang.t("cote normale", "base odds", "probabilidad base");
        let base_cnt = lang.count_forms(
            c.base_chests.clamp(0, i32::MAX as i64) as i32,
            "coffre",
            "coffres",
            "chest",
            "chests",
            "cofre",
            "cofres",
        );
        let base_line = format!(
            "{base_cnt} \u{00b7} {base_lbl} ({})",
            one_in_i64(odds.one_in as i64)
        );
        let mut plain_lines = vec![title.to_string(), base_line.clone()];
        let mut html_lines = vec![format!("<b>{}</b>", h(title)), h(&base_line)];
        for &(boost, count) in &c.boosted_chests {
            let per_chest = (boost / odds.one_in as f64).min(0.999);
            let inverse = 1.0 / per_chest;
            let rounded = inverse.round() as i64;
            let approx = if (inverse - inverse.round()).abs() > 1e-8 {
                "\u{2248} "
            } else {
                ""
            };
            let mut rate = boost.to_string();
            if lang.code() != "en" {
                rate = rate.replace('.', ",");
            }
            let cnt_str = lang.count_forms(
                count.clamp(0, i32::MAX as i64) as i32,
                "coffre",
                "coffres",
                "chest",
                "chests",
                "cofre",
                "cofres",
            );
            let line = format!("{cnt_str} \u{00b7} \u{00d7}{rate} ({approx}{})", one_in_i64(rounded));
            html_lines.push(h(&line));
            plain_lines.push(line);
        }
        let aria = h(&plain_lines.join("\n"));
        let tip_html = html_lines.join("\n");
        format!(
            r#"<span class="cbarwrap" tabindex="0" role="img" aria-label="{aria}"><span class="cbar{wide_cls}"><i class="{cls}" style="width:{w:.1}%"></i></span><span class="cbartip">{tip_html}</span></span>"#
        )
    };
    let img = |prize: Option<&TreasurePrize>, cls: &str| -> String {
        match catalog::image_src(ctx.images_folder, prize.and_then(|p| p.image.as_deref())) {
            Some(src) => format!(
                r#"<img class="{cls}" src="{}" alt="" onerror="this.style.visibility='hidden'">"#,
                h(&src)
            ),
            None => String::new(),
        }
    };
    let one_in = |n: i32| -> String {
        let ns = lang.n(n as i64);
        match lang.code() {
            "en" => format!("1 in {ns}"),
            "es" => format!("1 entre {ns}"),
            _ => format!("1 sur {ns}"),
        }
    };
    let got_badge = || -> String {
        format!(
            r#"<span class="gotbadge">✓ {}</span>"#,
            lang.t("Obtenu", "Got it", "Conseguido")
        )
    };

    let info = lang.t(
        "La chance que chaque trésor soit déjà tombé pour toi au moins une fois. Détails : « Comment c'est calculé ? » en bas.",
        "The chance each treasure has already dropped for you at least once. Details: \"How is this calculated?\" at the bottom.",
        "La probabilidad de que cada tesoro ya te haya tocado al menos una vez. Detalles: «¿Cómo se calcula?» abajo.",
    );
    sb.push_str(&fold_start(
        "chances",
        lang.t("Tes chances", "Your chances", "Tus probabilidades"),
        true,
        &info_html(info),
        "",
    ));
    sb.push_str(&treasure_hour_banner(ctx));
    sb.push_str(&format!(
        r#"<p class="ch-intro">{}</p>"#,
        lang.t(
            "Chaque coffre est un nouveau tirage : tes chances ne s'accumulent pas. Ces chiffres montrent la chance qu'un trésor soit <b>déjà tombé pour toi au moins une fois</b>, avec les coffres que tu as ouverts.",
            "Every chest is a fresh roll: your odds don't build up. These numbers show the chance a treasure has <b>already dropped for you at least once</b>, from the chests you've opened.",
            "Cada cofre es una tirada nueva: tus probabilidades no se acumulan. Estas cifras muestran la probabilidad de que un tesoro <b>ya te haya tocado al menos una vez</b>, con los cofres que has abierto.",
        )
    ));

    // Top two cards: gold outfit & pirate shop access
    let card = |kind: &str| -> String {
        let odds = odds_of(kind);
        let weekly = odds.weekly;
        let c = shown_chance(odds);
        let prize = if weekly {
            prize_of(kind)
        } else {
            Some(&catalog.pirate_shop_access)
        };
        let items = prize.map(|p| p.id.as_str()).unwrap_or("");
        let name = if weekly {
            prize
                .map(|p| p.name.as_str())
                .unwrap_or_else(|| catalog.kind_name(kind, lang))
        } else {
            lang.t("Toute la saison", "Lasts all season", "Toda la temporada")
        };
        let about = if c.p > 0.0 && c.p < 0.5 {
            let inv = lang.n((1.0 / c.p).round_ties_even() as i64);
            format!(
                " · {}",
                match lang.code() {
                    "en" => format!("about 1 in {inv}"),
                    "es" => format!("aproximadamente 1 entre {inv}"),
                    _ => format!("environ 1 sur {inv}"),
                }
            )
        } else {
            String::new()
        };
        let scope = if weekly {
            lang.t(
                "chance qu'il soit tombé pour toi cette semaine",
                "chance it dropped for you this week",
                "probabilidad de que te haya tocado esta semana",
            )
        } else {
            lang.t(
                "chance qu'il soit tombé pour toi cette saison",
                "chance it dropped for you this season",
                "probabilidad de que te haya tocado esta temporada",
            )
        };
        let line = match lang.code() {
            "en" => format!(
                "Each chest: {} · {} chests {}",
                one_in(odds.one_in),
                lang.n(c.chests as i64),
                if weekly { "this week" } else { "this season" }
            ),
            "es" => format!(
                "Cada cofre: {} · {} cofres {}",
                one_in(odds.one_in),
                lang.n(c.chests as i64),
                if weekly { "esta semana" } else { "esta temporada" }
            ),
            _ => format!(
                "Chaque coffre : {} · {} coffres {}",
                one_in(odds.one_in),
                lang.n(c.chests as i64),
                if weekly { "cette semaine" } else { "cette saison" }
            ),
        };
        let mut extra = String::new();
        if weekly {
            extra.push_str(&bar(&c, odds, None, true));
            let s = season_chance(odds);
            let s_pct = percent_html(&s, lang);
            let s_n = lang.n(s.chests as i64);
            extra.push_str(&format!(
                r#"<div class="cfoot">{}</div>"#,
                match lang.code() {
                    "en" => format!("All weeks combined: <b>{s_pct}</b> over {s_n} chests"),
                    "es" => format!("Todas las semanas juntas: <b>{s_pct}</b> en {s_n} cofres"),
                    _ => format!("Toutes les semaines réunies : <b>{s_pct}</b> sur {s_n} coffres"),
                }
            ));
            if let Some(w) = week {
                if now < w.to {
                    extra.push_str(&format!(
                        r#"<div class="cfoot">{} <b>{}</b></div>"#,
                        lang.t("Cette tenue part dans", "This outfit leaves in", "Este traje se va en"),
                        countdown_html(w.to)
                    ));
                }
            }
        } else {
            let half = TreasureCatalog::chests_for(odds.one_in, 0.5);
            let prog = (c.chests as f64 * 100.0 / half as f64).clamp(0.8, 100.0);
            extra.push_str(&format!(
                r#"<div class="halfway"><div class="hrow"><span>{lbl}</span><span><b>{cur}</b> / {half_s} {chests_w}</span></div>{bar_html}</div>"#,
                lbl = lang.t(
                    "Vers une chance sur deux",
                    "Halfway to a 50/50 chance",
                    "Hacia una probabilidad del 50 %"
                ),
                cur = lang.n(c.chests as i64),
                half_s = lang.n(half.min(i32::MAX as i64)),
                chests_w = lang.t("coffres", "chests", "cofres"),
                bar_html = bar(&c, odds, Some(prog), true),
            ));
        }
        let proj = projection(odds, &c, weekly);
        if !proj.is_empty() {
            extra.push_str(&format!(r#"<div class="cfoot">{proj}</div>"#));
        }
        format!(
            "<div class=\"hcard\" data-items=\"{items}\" data-kind=\"{kind}\" data-p=\"{dp}\">\n\
<div class=\"htop\">{img_html}<div class=\"hname\"><div class=\"k\">{kname}<span class=\"boost\" hidden></span></div><div class=\"ciname\">{name_h}</div></div>{tag_html}{gb}</div>\n\
<div class=\"hval\"><span class=\"big num\">{pct}</span>{chip_html}</div>\n\
<div class=\"hexp\">{scope}{about}</div>\n\
<div class=\"s sline\">{line}</div>{extra}</div>",
            dp = format_dotnet_r(c.p),
            img_html = img(prize, "cthumb"),
            kname = h(catalog.kind_name(kind, lang)),
            name_h = h(name),
            tag_html = tag(weekly),
            gb = got_badge(),
            pct = percent_html(&c, lang),
            chip_html = chip(c.p),
        )
    };

    sb.push_str(r#"<div class="chances2">"#);
    sb.push_str(&card(GOLD_OUTFIT));
    sb.push_str(&card(PIRATE_SHOP));
    sb.push_str("</div>");

    // Weekly and Season tables
    let head = format!(
        r#"<tr><th>{}</th><th class="r">{}</th><th>{}</th><th class="r">{}</th></tr>"#,
        lang.t("Trésor", "Treasure", "Tesoro"),
        lang.t("Chaque coffre", "Each chest", "Cada cofre"),
        lang.t(
            "Chance de l'avoir eu",
            "Chance you've had it",
            "Probabilidad de tenerlo"
        ),
        lang.t("Attendu", "Expected", "Esperado")
    );

    let row = |odds: &TreasureOdds, note: &str| -> String {
        let c = shown_chance(odds);
        let prize = if odds.weekly {
            prize_of(&odds.kind)
        } else {
            None
        };
        let name = prize
            .map(|p| format!(r#"<span class="iname">{}</span>"#, h(&p.name)))
            .unwrap_or_default();
        let proj = if odds.weekly {
            projection(odds, &c, true)
        } else {
            String::new()
        };
        let mut below = String::new();
        for x in [note, proj.as_str()] {
            if !x.is_empty() {
                below.push_str(&format!(r#"<span class="iname">{x}</span>"#));
            }
        }
        let items_attr = prize
            .map(|p| format!(r#" data-items="{}""#, p.id))
            .unwrap_or_default();
        format!(
            r#"<tr data-kind="{kind}"{items_attr}><td><div class="tflex">{img_html}<div><div class="tk">{kname}<span class="boost" hidden></span>{gb}</div>{name}<span class="iname mo">{oi} {pc}</span>{below}</div></div></td><td class="n pc">{oi}</td><td class="tchance"><div class="tcv">{ch}<b>{pct}</b></div>{bar_html}</td><td class="n">{exp}</td></tr>"#,
            kind = odds.kind,
            img_html = img(prize, "thumb"),
            kname = h(catalog.kind_name(&odds.kind, lang)),
            gb = got_badge(),
            oi = one_in(odds.one_in),
            pc = lang.t("par coffre", "each chest", "por cofre"),
            ch = chip(c.p),
            pct = percent_html(&c, lang),
            bar_html = bar(&c, odds, None, false),
            exp = expected_html(c.average, lang),
        )
    };

    if let Some(w) = week {
        let leaves = if now < w.to {
            format!(
                r#" · {} {}</div></div><div class="cdbox"><span class="cdl">{}</span>{}</div></div>"#,
                lang.t("part", "leaves", "se va"),
                time_el(w.to, "dayTime", now, lang),
                lang.t("Part dans", "Leaves in", "Se va en"),
                countdown_html(w.to),
            )
        } else {
            "</div></div></div>".to_string()
        };
        let wm_str = if week_manual > 0 {
            let n_wm = lang.n(week_manual);
            match lang.code() {
                "en" => format!(", including {n_wm} added by hand"),
                "es" => format!(", incluidos {n_wm} añadidos a mano"),
                _ => format!(", dont {n_wm} ajoutés à la main"),
            }
        } else {
            String::new()
        };
        sb.push_str(&format!(
            r#"<div class="cgroup"><div class="ghead">{tag_w}<div><div class="gt">{wk_lbl} {num} · {title}</div><div class="gs"><b>{cnt}</b> {this_wk}{wm_str}{leaves}"#,
            tag_w = tag(true),
            wk_lbl = lang.t("Semaine", "Week", "Semana"),
            num = w.number,
            title = h(&w.title),
            cnt = lang.count_forms(
                week_chests as i32,
                "coffre",
                "coffres",
                "chest",
                "chests",
                "cofre",
                "cofres"
            ),
            this_wk = lang.t("cette semaine", "this week", "esta semana"),
        ));
        sb.push_str(&format!(
            r#"<div class="card tcard"><table class="ctable">{head}"#
        ));
        for odds in catalog
            .odds
            .iter()
            .filter(|o| o.weekly && o.kind != GOLD_OUTFIT)
        {
            sb.push_str(&row(odds, ""));
        }
        sb.push_str("</table></div></div>");
    }

    let by_hand = if manual > 0 {
        let nm = lang.n(manual);
        match lang.code() {
            "en" => format!(", including {nm} added by hand"),
            "es" => format!(", incluidos {nm} añadidos a mano"),
            _ => format!(", dont {nm} ajoutés à la main"),
        }
    } else {
        String::new()
    };
    sb.push_str(&format!(
        r#"<div class="cgroup"><div class="ghead">{tag_s}<div><div class="gt">{all_s}</div><div class="gs"><b>{cnt}</b> {since} ({start_el}){by_hand}</div></div></div>"#,
        tag_s = tag(false),
        all_s = lang.t("Toute la saison", "All season", "Toda la temporada"),
        cnt = lang.count_forms(
            season_chests as i32,
            "coffre",
            "coffres",
            "chest",
            "chests",
            "cofre",
            "cofres"
        ),
        since = lang.t(
            "depuis le début de la chasse",
            "since the hunt started",
            "desde el inicio de la búsqueda"
        ),
        start_el = time_el(hunt_start, "date", now, lang),
    ));
    sb.push_str(&format!(
        r#"<div class="card tcard"><table class="ctable">{head}"#
    ));
    for odds in catalog
        .odds
        .iter()
        .filter(|o| !o.weekly && o.kind != PIRATE_SHOP)
    {
        let mut note = String::new();
        if odds.kind == GIZMO && !gizmo_times.is_empty() {
            let last_g = *gizmo_times.last().unwrap();
            let since = hunt_openings.iter().filter(|c| c.opened_utc > last_g).count() as i32;
            let ng = lang.n(gizmo_times.len() as i64);
            note = match lang.code() {
                "en" => format!(
                    "You found {ng} · last one {} ago",
                    lang.count_forms(since, "", "", "chest", "chests", "", "")
                ),
                "es" => format!(
                    "Has encontrado {ng} · el último hace {}",
                    lang.count_forms(since, "", "", "", "", "cofre", "cofres")
                ),
                _ => format!(
                    "Tu en as trouvé {ng} · le dernier il y a {}",
                    lang.count_forms(since, "coffre", "coffres", "", "", "", "")
                ),
            };
        }
        sb.push_str(&row(odds, &note));
    }
    sb.push_str("</table></div></div>");

    // Folded: weekly treasures all weeks combined
    sb.push_str(&format!(
        r#"<details class="cmore"><summary>{}</summary>"#,
        lang.t(
            "Trésors hebdo, toutes les semaines réunies",
            "Weekly treasures, all weeks combined",
            "Tesoros semanales, todas las semanas juntas"
        )
    ));
    let n_season = lang.n(season_chests);
    sb.push_str(&format!(
        "<p>{}</p>",
        match lang.code() {
            "en" => format!("The chance you've had at least one of them from <i>any</i> week, over all {n_season} chests of the hunt."),
            "es" => format!("La probabilidad de haber conseguido al menos uno, de <i>cualquier</i> semana, en los {n_season} cofres de la búsqueda."),
            _ => format!("La chance d'en avoir eu au moins un, de <i>n'importe quelle</i> semaine, sur les {n_season} coffres de la chasse."),
        }
    ));
    sb.push_str(&format!(
        r#"<div class="card tcard"><table class="ctable mini"><tr><th>{}</th><th class="r">{}</th><th class="r">{}</th></tr>"#,
        lang.t("Trésor", "Treasure", "Tesoro"),
        lang.t("Chance", "Chance", "Probabilidad"),
        lang.t("Attendu", "Expected", "Esperado"),
    ));
    for odds in catalog.odds.iter().filter(|o| o.weekly) {
        let s = season_chance(odds);
        sb.push_str(&format!(
            r#"<tr><td>{}</td><td class="n">{}</td><td class="n">{}</td></tr>"#,
            h(catalog.kind_name(&odds.kind, lang)),
            percent_html(&s, lang),
            expected_html(s.average, lang),
        ));
    }
    sb.push_str("</table></div></details>");

    // Folded: How is this calculated?
    let hours = next_regular_hours(now);
    let hours_text = if !hours.is_empty() {
        let els: Vec<String> = hours
            .iter()
            .map(|&t| time_el(t, "weekdayTime", now, lang))
            .collect();
        match lang.code() {
            "en" => format!(" They run every {}, for 2 hours.", els.join(" and ")),
            "es" => format!(" Son cada {}, durante 2 horas.", els.join(" y ")),
            _ => format!(" Elles ont lieu chaque {}, pendant 2 heures.", els.join(" et ")),
        }
    } else {
        String::new()
    };
    let link = |url: &str, text: &str| -> String {
        format!(
            r#"<a href="{}" target="_blank" rel="noopener">{}</a>"#,
            h(url),
            h(text)
        )
    };
    let ls_one_in = one_in(odds_of(LOADING_SCREEN).one_in);
    let n_boosted = lang.n(boosted as i64);
    let odds_link = link(&catalog.odds_source, "Epic Games (Fortnite Status)");
    let hours_link = link(&catalog.hours_source, "fortnite.bot");

    sb.push_str(&format!(
        r#"<details class="cmore"><summary>{}</summary><ul>"#,
        lang.t(
            "Comment c'est calculé ?",
            "How is this calculated?",
            "¿Cómo se calcula?"
        )
    ));
    sb.push_str(&match lang.code() {
        "en" => format!(
            "<li><b>Each chest is independent.</b> A loading screen is {ls_one_in} on every chest, no matter how many you've opened.</li>\
<li><b>Chance you've had it</b> = 1 − (1 − 1/X)<sup>chests</sup>. It gets close to 100% but never reaches it.</li>\
<li><b>Weekly</b> treasures change every Sunday, and a chest only gives that week's one, so only this week's chests count.</li>\
<li><b>Season</b> treasures (Pirate Shop Access, gizmos) count every chest since the hunt started, including the ones you added by hand. Those have no date, so they count toward a week only if you ticked \"Opened this week\".</li>\
<li><b>Treasure Hours</b> boost some odds (for example ×10 for vehicles).{hours_text} Chests from matches started during one count with the boosted odds ({n_boosted} of yours).</li>\
<li><b>Expected</b> is how many you'd get on average with these chests. <b>At your pace</b> is the same chance at the end of the week (or of the hunt) if you keep opening as many chests a day.</li>\
<li>Odds published by {odds_link}, Treasure Hour times from {hours_link}.</li>"
        ),
        "es" => format!(
            "<li><b>Cada cofre es independiente.</b> Una pantalla de carga es {ls_one_in} en cada cofre, sin importar cuántos hayas abierto.</li>\
<li><b>Probabilidad de tenerlo</b> = 1 − (1 − 1/X)<sup>cofres</sup>. Se acerca al 100 % sin llegar nunca.</li>\
<li><b>Semanal</b>: estos tesoros cambian cada domingo y un cofre solo da el de su semana, así que solo cuentan los cofres de esta semana.</li>\
<li><b>Temporada</b>: el acceso a la tienda pirata y los artilugios cuentan todos los cofres desde el inicio de la búsqueda, incluidos los añadidos a mano. Esos no tienen fecha: solo cuentan para una semana si marcaste «Abiertos esta semana».</li>\
<li><b>Horas del tesoro</b>: algunas probabilidades se multiplican (por ejemplo ×10 para los vehículos).{hours_text} Los cofres de partidas empezadas durante una de ellas cuentan con la probabilidad aumentada ({n_boosted} de los tuyos).</li>\
<li><b>Esperado</b>: cuántos tendrías de media con esos cofres. <b>A tu ritmo</b>: la misma probabilidad al final de la semana (o de la búsqueda) si sigues abriendo tantos cofres al día.</li>\
<li>Probabilidades publicadas por {odds_link}, horarios de las horas del tesoro según {hours_link}.</li>"
        ),
        _ => format!(
            "<li><b>Chaque coffre est indépendant.</b> Un écran de chargement, c'est {ls_one_in} à chaque coffre, peu importe combien tu en as ouvert.</li>\
<li><b>Chance de l'avoir eu</b> = 1 − (1 − 1/X)<sup>coffres</sup>. Elle se rapproche de 100 % sans jamais l'atteindre.</li>\
<li><b>Hebdo</b> : ces trésors changent chaque dimanche, et un coffre ne donne que celui de sa semaine : seuls les coffres de cette semaine comptent.</li>\
<li><b>Saison</b> : l'accès à la boutique pirate et les gizmos comptent tous les coffres depuis le début de la chasse, y compris ceux ajoutés à la main. Ceux-là n'ont pas de date : ils ne comptent pour une semaine que si tu as coché « Ouverts cette semaine ».</li>\
<li><b>Heures du trésor</b> : certaines chances sont multipliées (par exemple ×10 pour les véhicules).{hours_text} Les coffres des parties lancées pendant l'une d'elles comptent avec les chances augmentées ({n_boosted} des tiens).</li>\
<li><b>Attendu</b> : combien tu en aurais en moyenne avec ces coffres. <b>À ton rythme</b> : la même chance à la fin de la semaine (ou de la chasse) si tu continues à ouvrir autant de coffres par jour.</li>\
<li>Cotes publiées par {odds_link}, horaires des heures du trésor d'après {hours_link}.</li>"
        ),
    });
    sb.push_str(&format!("</ul></details>{FOLD_END}"));
    sb
}

fn collection_section(
    matches: &[&MatchRecord],
    obtained: &[String],
    ctx: &ReportContext<'_>,
) -> String {
    let lang = ctx.lang;
    let catalog = ctx.catalog;
    let now = ctx.now_utc;
    let Some(last_week) = catalog.weeks.last() else {
        return String::new();
    };
    if now >= last_week.to + Duration::days(14) {
        return String::new();
    }

    let ticked: HashSet<&str> = obtained.iter().map(|s| s.as_str()).collect();
    let seen: HashSet<String> = matches
        .iter()
        .flat_map(|m| m.treasure_finds.iter().chain(m.team_treasure_finds.iter()))
        .filter_map(|f| f.prize_id.as_deref())
        .map(|id| id.to_lowercase())
        .collect();

    let prize_html = |p: &TreasurePrize| -> String {
        let auto = p
            .prize_ids
            .iter()
            .any(|id| seen.contains(&id.to_lowercase()));
        let saved = !auto && ticked.contains(p.id.as_str());
        let img = match catalog::image_src(ctx.images_folder, p.image.as_deref()) {
            Some(src) => format!(
                r#"<img src="{}" alt="" loading="lazy" onerror="this.remove()">"#,
                h(&src)
            ),
            None => String::new(),
        };
        let input_extra = if auto {
            format!(
                r#" checked data-auto="1" onclick="return false" title="{}""#,
                h(lang.t(
                    "Coché tout seul : vu dans tes replays",
                    "Ticked by itself: seen in your replays",
                    "Marcado solo: visto en tus repeticiones"
                ))
            )
        } else if saved {
            r#" checked data-saved="1""#.to_string()
        } else {
            String::new()
        };
        let auto_suffix = if auto {
            format!(
                " · {}",
                lang.t(
                    "vu dans tes replays",
                    "seen in your replays",
                    "visto en tus repeticiones"
                )
            )
        } else {
            String::new()
        };
        format!(
            r#"<label class="prize{got}"><input type="checkbox" data-prize="{id}"{input_extra}><span class="pimg">{img}</span><span class="pn">{name}</span><span class="pk">{kind}{auto_suffix}</span></label>"#,
            got = if auto { " got" } else { "" },
            id = p.id,
            name = h(&p.name),
            kind = h(catalog.kind_name(&p.kind, lang)),
        )
    };

    let collection_info = lang.t(
        "Coche ce que tu as déjà obtenu : c'est enregistré dans ce navigateur seulement. Un trésor sorti d'un coffre doré est coché tout seul quand le compteur le reconnaît. À part les gizmos, on ne peut pas obtenir deux fois le même trésor. Images : fortnite.bot, téléchargées une seule fois par le compteur et gardées dans son dossier de données.",
        "Tick what you already obtained: it is saved in this browser only. A treasure from a golden chest is ticked by itself when the counter recognizes it. Apart from gizmos, the same treasure cannot be obtained twice. Images: fortnite.bot, downloaded once by the counter and kept in its data folder.",
        "Marca lo que ya has conseguido: se guarda solo en este navegador. Un tesoro de un cofre dorado se marca solo cuando el contador lo reconoce. Salvo los artilugios, no se puede conseguir dos veces el mismo tesoro. Imágenes: fortnite.bot, descargadas una sola vez por el contador y guardadas en su carpeta de datos.",
    );

    let mut sb = String::new();
    let title = format!(
        r#"{} · <span class="num" id="colcount"></span>"#,
        lang.t("Ta collection", "Your collection", "Tu colección")
    );
    sb.push_str(&fold_start(
        "collection",
        &title,
        false,
        &info_html(collection_info),
        "",
    ));
    sb.push_str(r#"<div class="card collection">"#);
    sb.push_str(&format!(
        r#"<div class="wk"><div class="wt">{}</div><div class="prizes">{}</div></div>"#,
        lang.t("Toute la saison", "All season", "Toda la temporada"),
        prize_html(&catalog.pirate_shop_access)
    ));
    for w in &catalog.weeks {
        let current = now >= w.from && now < w.to;
        let dates = format!("{} – {}", lang.short_when(w.from), lang.short_when(w.to));
        let cur_suffix = if current {
            format!(" · {}", lang.t("cette semaine", "this week", "esta semana"))
        } else {
            String::new()
        };
        sb.push_str(&format!(
            r#"<div class="wk{now_cls}"><div class="wt">{wk_lbl} {num} · {title} <span class="items">{dates}{cur_suffix}</span></div>"#,
            now_cls = if current { " now" } else { "" },
            wk_lbl = lang.t("Semaine", "Week", "Semana"),
            num = w.number,
            title = h(&w.title),
        ));
        if !w.prizes.is_empty() {
            let items: String = w.prizes.iter().map(prize_html).collect();
            sb.push_str(&format!(r#"<div class="prizes">{items}</div></div>"#));
        } else {
            sb.push_str(&format!(
                r#"<div class="items" style="padding:4px 0 8px">{}</div></div>"#,
                lang.t(
                    "Récompenses pas encore dévoilées : elles arriveront avec une mise à jour du compteur.",
                    "Prizes not revealed yet: they will come with an update of the counter.",
                    "Recompensas aún no reveladas: llegarán con una actualización del contador."
                )
            ));
        }
    }
    sb.push_str("</div>");
    sb.push_str(FOLD_END);
    sb
}

fn format_json_number(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn treasure_hour_banner(ctx: &ReportContext<'_>) -> String {
    let lang = ctx.lang;
    let catalog = ctx.catalog;

    // Build exact System.Text.Json JSON string matching ReportWriter.cs
    let mut j = String::with_capacity(4096);
    j.push_str("{\"hours\":[");
    for (idx, h_item) in catalog.treasure_hours.iter().enumerate() {
        if idx > 0 {
            j.push(',');
        }
        j.push_str("{\"s\":");
        j.push_str(&h_item.start_utc.timestamp_millis().to_string());
        j.push_str(",\"e\":");
        j.push_str(&h_item.end_utc.timestamp_millis().to_string());
        j.push_str(",\"b\":{");
        for (bi, (k, v)) in h_item.boosts.iter().enumerate() {
            if bi > 0 {
                j.push(',');
            }
            j.push_str(&dotnet_json_string(k));
            j.push(':');
            j.push_str(&format_json_number(*v));
        }
        j.push_str("},\"c\":");
        j.push_str(if h_item.confirmed { "true" } else { "false" });
        j.push('}');
    }
    j.push_str("],\"odds\":{");
    for (idx, o) in catalog.odds.iter().enumerate() {
        if idx > 0 {
            j.push(',');
        }
        j.push_str(&dotnet_json_string(&o.kind));
        j.push(':');
        j.push_str(&o.one_in.to_string());
    }
    j.push_str("},\"names\":{");
    for (idx, kind) in [GOLD_OUTFIT, PIRATE_SHOP, VEHICLE_DECAL].iter().enumerate() {
        if idx > 0 {
            j.push(',');
        }
        j.push_str(&dotnet_json_string(kind));
        j.push(':');
        j.push_str(&dotnet_json_string(catalog.kind_name(kind, lang)));
    }
    j.push_str("},\"text\":{");
    let text_fields: [(&str, &str); 11] = [
        (
            "live",
            lang.t(
                "Heure du trésor en cours, jusqu'à {0}",
                "Treasure Hour live now, until {0}",
                "Hora del tesoro en curso, hasta las {0}",
            ),
        ),
        (
            "next",
            lang.t(
                "Prochaine heure du trésor : {0}",
                "Next Treasure Hour: {0}",
                "Próxima hora del tesoro: {0}",
            ),
        ),
        (
            "endsIn",
            lang.t("se termine dans {0}", "ends in {0}", "termina en {0}"),
        ),
        ("startsIn", lang.t("dans {0}", "in {0}", "en {0}")),
        (
            "expected",
            lang.t(
                "prévue, pas encore annoncée",
                "expected, not announced yet",
                "prevista, aún no anunciada",
            ),
        ),
        (
            "perChest",
            lang.t(
                "1 sur {0} par coffre",
                "1 in {0} per chest",
                "1 entre {0} por cofre",
            ),
        ),
        (
            "oneIn",
            lang.t("1 sur {0}", "1 in {0}", "1 entre {0}"),
        ),
        (
            "boostPerChest",
            lang.t(
                "1 sur {0} à chaque coffre pendant l'heure du trésor",
                "1 in {0} per chest during the Treasure Hour",
                "1 entre {0} por cofre durante la hora del tesoro",
            ),
        ),
        (
            "boostHalf",
            lang.t(
                " · une chance sur deux dans {0} coffres de plus à ce rythme",
                " · one chance in two within {0} more chests at this rate",
                " · una de cada dos en {0} cofres más a este ritmo",
            ),
        ),
        ("now", lang.t("maintenant", "now", "ahora")),
        ("locale", lang.culture_name()),
    ];
    for (idx, (k, v)) in text_fields.iter().enumerate() {
        if idx == 9 {
            // units comes right after boostHalf, before now & locale!
            j.push_str(",\"units\":[");
            let units_str = lang.t("j h min s", "d h m s", "d h m s");
            for (ui, u) in units_str.split(' ').enumerate() {
                if ui > 0 {
                    j.push(',');
                }
                j.push_str(&dotnet_json_string(u));
            }
            j.push(']');
        }
        if idx > 0 {
            j.push(',');
        }
        j.push_str(&dotnet_json_string(k));
        j.push(':');
        j.push_str(&dotnet_json_string(v));
    }
    j.push_str("},\"clock\":{\"utc\":");
    j.push_str(if lang.utc_times { "true" } else { "false" });
    j.push_str(",\"h12\":");
    j.push_str(if !lang.clock_24 { "true" } else { "false" });
    j.push_str("}}");

    format!(
        "<div class=\"thour\" id=\"thour\" hidden></div>\n\
<script type=\"application/json\" id=\"thdata\">{j}</script>"
    )
}

pub(crate) const TREASURE_HOUR_SCRIPT: &str = r#"function startTreasureHour(root){
var data=root.getElementById('thdata'),el=root.getElementById('thour');
if(!data||!el)return;
var d=JSON.parse(data.textContent);
function f(n){return Math.round(n).toLocaleString(d.text.locale)}
function t(ms,withDay){var o=withDay?{weekday:'long',day:'numeric',month:'short',hour:'numeric',minute:'2-digit'}:{hour:'numeric',minute:'2-digit'};o.hour12=d.clock.h12;if(d.clock.utc)o.timeZone='UTC';return new Date(ms).toLocaleString(d.text.locale,o)+(d.clock.utc?' UTC':'')}
function iso(ms){return new Date(ms).toISOString().slice(0,19)+'Z'}
function cd(ms){return '<span class="cd" data-end="'+iso(ms)+'"></span>'}
function boosts(h,live){return Object.keys(h.b).map(function(k){return '<span><b>'+d.names[k]+' ×'+h.b[k].toLocaleString(d.text.locale)+'</b>'+(live?' · '+d.text.perChest.replace('{0}',f(d.odds[k]/h.b[k])):'')+'</span>'}).join('')}
function draw(){
  var now=Date.now(),cur=d.hours.filter(function(h){return now>=h.s&&now<h.e})[0],nxt=d.hours.filter(function(h){return h.s>now}).sort(function(a,b){return a.s-b.s})[0];
  if(cur){el.className='thour live';el.innerHTML='<div class="tt">⚡ '+d.text.live.replace('{0}',t(cur.e,false))+' · '+d.text.endsIn.replace('{0}',cd(cur.e))+'</div><div class="tb">'+boosts(cur,true)+'</div>';el.hidden=false}
  else if(nxt){el.className='thour';el.innerHTML='<div class="tt">'+d.text.next.replace('{0}',t(nxt.s,true))+' · '+d.text.startsIn.replace('{0}',cd(nxt.s))+(nxt.c?'':' <span class="exp">('+d.text.expected+')</span>')+'</div><div class="tb">'+boosts(nxt,false)+'</div>';el.hidden=false}
  else el.hidden=true;
  boost(cur);tick();
}
// During a Treasure Hour, the odds of the boosted kinds (cards and table) show the boosted rate, in orange.
function boost(cur){
  [].forEach.call(root.querySelectorAll('[data-kind]'),function(x){
    var k=x.getAttribute('data-kind'),b=cur&&cur.b[k]>1?cur.b[k]:0,badge=x.querySelector('.boost'),line=x.querySelector('.sline'),pc=x.querySelector('.pc');
    x.classList.toggle('boosted',!!b);
    if(badge){badge.hidden=!b;badge.textContent=b?'⚡ ×'+b.toLocaleString(d.text.locale):''}
    if(line){
      if(!line.hasAttribute('data-n'))line.setAttribute('data-n',line.innerHTML);
      if(b){var one=d.odds[k]/b,p=+x.getAttribute('data-p'),s=d.text.boostPerChest.replace('{0}',f(one));
        if(p<0.5)s+=d.text.boostHalf.replace('{0}',f(Math.ceil(Math.log(0.5/(1-p))/Math.log(1-1/one))));
        line.innerHTML='<span class="boostv">'+s+'</span>'}
      else line.innerHTML=line.getAttribute('data-n');
    }
    if(pc){if(!pc.hasAttribute('data-n'))pc.setAttribute('data-n',pc.innerHTML);pc.innerHTML=b?'<span class="boostv">'+d.text.oneIn.replace('{0}',f(d.odds[k]/b))+'</span>':pc.getAttribute('data-n')}
  });
}
// Every countdown of the page: "3d 11h 58m", with seconds in the last hour, orange in the last day.
function tick(){
  var now=Date.now(),u=d.text.units;
  [].forEach.call(root.querySelectorAll('.cd'),function(x){
    var ms=new Date(x.getAttribute('data-end'))-now;
    if(ms<=0){x.textContent=d.text.now;x.classList.add('done');return}
    var s=Math.floor(ms/1000),dd=Math.floor(s/86400),h=Math.floor(s%86400/3600),m=Math.floor(s%3600/60),sec=s%60;
    x.textContent=dd>0?dd+u[0]+' '+h+u[1]+' '+m+u[2]:h>0?h+u[1]+' '+m+u[2]+' '+sec+u[3]:m+u[2]+' '+sec+u[3];
    x.classList.toggle('soon',ms<86400000);
  });
}
draw();
var updateTimer=setInterval(draw,30000),countdownTimer=setInterval(tick,1000);
return function(){clearInterval(updateTimer);clearInterval(countdownTimer)};
}"#;

fn seen_section(matches: &[&MatchRecord], ctx: &ReportContext<'_>) -> String {
    let lang = ctx.lang;
    let catalog = ctx.catalog;

    let mut rows: Vec<(&MatchRecord, &crate::models::SeenPrize, &TreasurePrize)> = Vec::new();
    for m in matches {
        for s in &m.seen_prizes {
            if let Some(p) = catalog.find(&s.prize_id) {
                if p.kind == PIRATE_SHOP || p.kind == GOLD_OUTFIT {
                    rows.push((m, s, p));
                }
            }
        }
    }
    rows.sort_by(|a, b| {
        when(b.0, lang)
            .cmp(&when(a.0, lang))
            .then_with(|| a.1.time_seconds.total_cmp(&b.1.time_seconds))
    });

    let info = lang.t(
        "Un coffre doré est envoyé à tous les joueurs à environ 150 m, avec le trésor et le joueur qui l'a ouvert : ces trésors-là ont été vus dans tes replays. Un joueur plus loin de toi n'y figure pas.",
        "A golden chest is sent to every player within about 150 m, with the treasure and the player who opened it: these were seen in your replays. A player farther away from you is not in them.",
        "Un cofre dorado se envía a todos los jugadores a unos 150 m, con el tesoro y el jugador que lo abrió: estos se vieron en tus repeticiones. Un jugador más lejos de ti no aparece.",
    );

    let mut sb = String::new();
    let title = format!(
        r#"{} · <span class="num">{}</span>"#,
        lang.t(
            "Ils l'ont eu dans tes parties",
            "They got it in your matches",
            "Lo consiguieron en tus partidas"
        ),
        rows.len()
    );
    let badge_info = format!(
        r#" <span class="exper">{}</span>{}"#,
        lang.t("expérimental", "experimental", "experimental"),
        info_html(info)
    );
    sb.push_str(&fold_start("seen", &title, false, &badge_info, ""));
    sb.push_str(r#"<div class="card">"#);
    if rows.is_empty() {
        sb.push_str(&format!(
            r#"<p class="items" style="margin:8px 0">{}</p></div>{FOLD_END}"#,
            lang.t(
                "Aucun pour l'instant : quand un joueur à moins de 150 m de toi décroche l'accès à la boutique pirate ou une tenue en or, il apparaît ici avec la partie, son pseudo et le moment.",
                "None yet: when a player within 150 m of you gets the Pirate Shop Access or a gold outfit, they show up here with the match, their name and the moment.",
                "Ninguno por ahora: cuando un jugador a menos de 150 m de ti consigue el acceso a la tienda pirata o un traje de oro, aparece aquí con la partida, su nombre y el momento.",
            )
        ));
        return sb;
    }
    sb.push_str(&format!(
        r#"<table id="t-seen"><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr>"#,
        lang.t("Partie", "Match", "Partida"),
        lang.t("Joueur", "Player", "Jugador"),
        lang.t("Trésor", "Treasure", "Tesoro"),
        lang.t("Moment", "Time", "Momento"),
    ));
    for (m, s, prize) in rows {
        let name = if prize.kind == PIRATE_SHOP {
            catalog.kind_name(&prize.kind, lang)
        } else {
            prize.name.as_str()
        };
        sb.push_str(&format!(
            r#"<tr class="r"><td class="nw num items">{}</td><td><b>{}</b></td><td><span class="gold">★★</span> {}</td><td class="nw num">{}</td></tr>"#,
            lang.date_time_short(when(m, lang)),
            h(&s.player),
            h(name),
            pretty::clock(s.time_seconds),
        ));
    }
    sb.push_str(&format!("</table>{}</div>{FOLD_END}", pager("t-seen", lang)));
    sb
}

pub fn overlay(v: &OverlayView, style: &str, lang: &LangCtx) -> String {
    let raw = if style == "classic" {
        overlay_classic(v, lang)
    } else {
        overlay_hud(v, lang)
    };
    to_crlf(&raw)
}

fn overlay_hud(v: &OverlayView, lang: &LangCtx) -> String {
    let row = |label: &str, value: &str, cls: &str| -> String {
        let val_part = if !value.is_empty() {
            format!(r#"<span class="sep">•</span><b>{}</b>"#, h(value))
        } else {
            String::new()
        };
        format!(r#"<div class="r {cls}"><span>{}</span>{val_part}</div>"#, h(label))
    };
    let state = row(
        lang.t("Dernière", "Last", "Última"),
        v.last.as_deref().unwrap_or("—"),
        "",
    );
    let squad = match &v.squad {
        Some(sq) => row(squad_label(lang), sq, ""),
        None => String::new(),
    };
    let live_cls = if v.current_live && v.current.as_deref().is_some_and(|c| c != "…") {
        " live"
    } else {
        ""
    };
    let dot = if v.current_live {
        r#"<i class="dot"></i>"#
    } else {
        ""
    };

    format!(
        "<!doctype html><html lang=\"{code}\"><head><meta charset=\"utf-8\"><meta http-equiv=\"refresh\" content=\"5\">\n\
<title>{title}</title>\n\
<style>\n\
html,body{{margin:0;background:transparent}}\n\
body{{padding:10px 14px;display:inline-block;color:#fff;font-family:Bahnschrift,\"Segoe UI\",sans-serif;font-style:italic;font-weight:600;text-align:right;\n     -webkit-text-stroke:4px #0b0f18;paint-order:stroke fill;letter-spacing:.01em}}\n\
.total{{display:flex;align-items:center;justify-content:flex-end;gap:10px;font-size:46px;font-weight:700;font-stretch:condensed;line-height:1.05}}\n\
.total.live{{color:#ffd23f}}\n\
.total svg{{width:30px;height:26px;flex:none}}\n\
.r{{display:flex;align-items:center;justify-content:flex-end;gap:8px;font-size:21px;font-stretch:semi-condensed;line-height:1.3}}\n\
.r.gold b{{color:#ffd23f}}\n\
.sep{{opacity:.9}}\n\
.dot{{width:13px;height:13px;border-radius:50%;background:#ff3b30;box-shadow:0 0 0 2px #0b0f18;flex:none}}\n\
.t{{color:#d6beff}}.t .star{{color:#c49bff}}\n\
</style></head><body>\n\
<div class=\"total{live_cls}\">{dot}<svg viewBox=\"0 0 30 26\" aria-hidden=\"true\"><rect x=\"2\" y=\"10\" width=\"26\" height=\"14\" rx=\"2\" fill=\"#ffd23f\" stroke=\"#0b0f18\" stroke-width=\"2.5\"/><path d=\"M2 12 Q15 -2 28 12\" fill=\"#e0a526\" stroke=\"#0b0f18\" stroke-width=\"2.5\"/><rect x=\"12.5\" y=\"12\" width=\"5\" height=\"6\" rx=\"1\" fill=\"#0b0f18\"/></svg>{total}</div>\n\
{state}\n\
{session_row}\n\
{squad}\n\
<div class=\"r t\"><span class=\"star\">★</span><b>{treasures}</b></div>\n\
</body></html>",
        code = lang.code(),
        title = lang.t("Overlay coffres", "Chest overlay", "Overlay de cofres"),
        total = h(&v.total),
        session_row = row(lang.t("Session", "Session", "Sesión"), &v.session, ""),
        treasures = h(&v.treasures),
    )
}

fn overlay_classic(v: &OverlayView, lang: &LangCtx) -> String {
    let mut rows = String::new();
    for r in &v.rows {
        let cls = if r.treasure { " t" } else { "" };
        let detail = match &r.detail {
            Some(d) => format!(" <small>{}</small>", h(d)),
            None => String::new(),
        };
        rows.push_str(&format!(
            r#"<div class="row{cls}"><span>{}</span><b>{}{detail}</b></div>"#,
            h(&r.label),
            h(&r.value),
        ));
    }
    let badge = match &v.badge {
        Some(b) => format!(r#"<span class="live">{}</span>"#, h(b)),
        None => String::new(),
    };
    let cur_cls = if v.current.is_none() {
        "none"
    } else if v.current_live {
        "cur"
    } else {
        ""
    };
    format!(
        "<!doctype html><html lang=\"{code}\"><head><meta charset=\"utf-8\"><meta http-equiv=\"refresh\" content=\"5\">\n\
<title>{title}</title>\n\
<style>\n\
html,body{{margin:0;background:transparent;color:#e2e2e8;font:500 16px/1.3 \"Segoe UI\",system-ui,sans-serif}}\n\
.box{{display:inline-block;min-width:260px;padding:14px 18px 12px;border-radius:16px;background:rgba(18,18,22,.86);border:1px solid #3a3a42;text-shadow:0 1px 3px rgba(0,0,0,.6)}}\n\
.head{{display:flex;justify-content:space-between;gap:16px;font:700 12px \"Segoe UI\",sans-serif;letter-spacing:.08em;color:#9696a2}}\n\
.live{{color:#60d680}}\n\
.counters{{display:flex;align-items:stretch;gap:18px;margin:4px 0 10px}}\n\
.counters>div{{display:flex;flex-direction:column}}\n\
.sep{{width:1px;background:#2c2c34;margin:8px 0 2px}}\n\
.big{{font-size:40px;font-weight:800;line-height:1.1;font-variant-numeric:tabular-nums}}\n\
.total{{color:#f2b84b}}.cur{{color:#6ee096}}.none{{color:#50505c}}\n\
.cap{{font-size:11px;font-weight:600;color:#82828e}}\n\
.rows{{border-top:1px solid #2c2c34;padding-top:8px;display:grid;grid-template-columns:auto 1fr;gap:4px 12px}}\n\
.row{{display:contents}}.row span{{font-size:13px;font-weight:600;color:#82828e}}.row b{{font-weight:500}}\n\
.row.t b{{color:#c4a0ff}}.row small{{font-size:11px;font-weight:600;color:#82828e}}\n\
</style></head><body><div class=\"box\">\n\
<div class=\"head\"><span>{hdr}</span>{badge}</div>\n\
<div class=\"counters\">
  <div><span class=\"big total\">{total}</span><span class=\"cap\">total</span></div>
  <div class=\"sep\"></div>
  <div><span class=\"big {cur_cls}\">{cur}</span><span class=\"cap\">{cap}</span></div>\n\
</div>\n\
<div class=\"rows\">{rows}</div>\n\
</div></body></html>",
        code = lang.code(),
        title = lang.t("Overlay coffres", "Chest overlay", "Overlay de cofres"),
        hdr = lang.t("COFFRES", "CHESTS", "COFRES"),
        total = h(&v.total),
        cur = h(v.current.as_deref().unwrap_or("—")),
        cap = h(&v.current_caption),
    )
}

pub fn csv(data: &TrackerData, lang: &LangCtx, catalog: &TreasureCatalog) -> String {
    let mut sb = String::new();
    sb.push_str(lang.t(
        "date;mode;joueur;place;joueurs;coffres;coffres_coequipiers;caisses_munitions;tresors;fichier",
        "date;mode;player;placement;players;chests;teammate_chests;ammo_boxes;treasures;file",
        "fecha;modo;jugador;puesto;jugadores;cofres;cofres_companeros;cajas_municion;tesoros;archivo",
    ));
    sb.push_str("\r\n");

    let mut ms: Vec<&MatchRecord> = profiles::counted(data).collect();
    ms.sort_by(|a, b| when(a, lang).cmp(&when(b, lang)));

    for m in ms {
        let t = m
            .treasure_finds
            .iter()
            .map(|f| f.display_name(lang, catalog))
            .collect::<Vec<_>>()
            .join(" + ");
        let place = m.placement.map(|p| p.to_string()).unwrap_or_default();
        let total = m.total_players.map(|p| p.to_string()).unwrap_or_default();
        sb.push_str(&format!(
            "{};{};{};{};{};{};{};{};{};{}\r\n",
            when(m, lang).format("%Y-%m-%d %H:%M"),
            pretty::playlist(m.playlist.as_deref(), lang),
            m.player_name.as_deref().unwrap_or(""),
            place,
            total,
            m.chests_opened,
            m.team_chests_opened,
            m.ammo_boxes_opened,
            t,
            m.replay_file,
        ));
    }
    sb
}
