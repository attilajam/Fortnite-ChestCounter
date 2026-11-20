use chrono::{DateTime, Utc};

use crate::lang::LangCtx;
use crate::models::{MatchRecord, TrackerData};
use crate::report::is_this_session;

pub struct OverlayAppearance;

impl OverlayAppearance {
    pub const SIZES: &'static [i32] = &[70, 80, 90, 100, 115, 130, 150, 175];
    pub const OPACITIES: &'static [i32] = &[100, 90, 80, 70, 60, 50, 40, 30];

    pub fn clamp_size(percent: i32) -> i32 {
        percent.clamp(Self::SIZES[0], Self::SIZES[Self::SIZES.len() - 1])
    }

    pub fn clamp_opacity(percent: i32) -> i32 {
        percent.clamp(Self::OPACITIES[Self::OPACITIES.len() - 1], 100)
    }
}

#[derive(Debug, Clone)]
pub struct OverlayRow {
    pub label: String,
    pub value: String,
    pub treasure: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OverlayView {
    pub total: String,
    pub current: Option<String>,
    pub current_caption: String,
    pub current_live: bool,
    pub badge: Option<String>,
    pub rows: Vec<OverlayRow>,
    pub treasures: String,
    pub session: String,
    pub last: Option<String>,
    pub squad: Option<String>,
}

pub fn squad_label(lang: &LangCtx) -> &'static str {
    lang.t("Équipe", "Squad", "Equipo")
}

pub fn last_match_caption(lang: &LangCtx) -> &'static str {
    lang.t("dernière partie", "last match", "última partida")
}

pub fn build_overlay(
    d: &TrackerData,
    live: Option<&MatchRecord>,
    in_progress: bool,
    show_squad: bool,
    session_start_utc: DateTime<Utc>,
    lang: &LangCtx,
) -> OverlayView {
    let live_chests = live.map(|m| m.chests_opened).unwrap_or(0);
    let live_treasures = live.map(|m| m.treasure_finds.len() as i32).unwrap_or(0);

    let total = d.total_chests() + live_chests;
    let team = d.team_chests() + live.map(|m| m.team_chests_opened).unwrap_or(0);
    let squad = if show_squad && team > 0 {
        Some(lang.n((total + team) as i64))
    } else {
        None
    };

    let session: Vec<&MatchRecord> = d
        .matches
        .iter()
        .filter(|m| is_this_session(m, session_start_utc))
        .collect();
    let session_chests: i32 = session.iter().map(|m| m.chests_opened).sum::<i32>() + live_chests;

    let mut rows = Vec::new();
    let session_detail = if !session.is_empty() {
        let avg = session.iter().map(|m| m.chests_opened).sum::<i32>() as f64
            / session.len() as f64;
        let avg_text = (avg.round() as i64).to_string();
        Some(format!(
            "{} · {} {}",
            lang.count_forms(
                session.len() as i32,
                "partie",
                "parties",
                "match",
                "matches",
                "partida",
                "partidas"
            ),
            lang.t("moy.", "avg.", "media"),
            avg_text
        ))
    } else {
        None
    };

    rows.push(OverlayRow {
        label: lang.t("Session", "Session", "Sesión").to_string(),
        value: lang.count(session_chests, "coffre", "chest", "cofre"),
        treasure: false,
        detail: session_detail,
    });

    let last = d
        .matches
        .iter()
        .max_by_key(|m| m.match_start_utc.unwrap_or(m.analyzed_at_utc));
    if let Some(l) = last {
        rows.push(OverlayRow {
            label: lang.t("Dernière", "Last", "Última").to_string(),
            value: lang.count(l.chests_opened, "coffre", "chest", "cofre"),
            treasure: false,
            detail: None,
        });
    }

    let treasures: i32 = d
        .matches
        .iter()
        .map(|m| m.treasure_finds.len() as i32)
        .sum::<i32>()
        + live_treasures;
    if squad.is_some() {
        rows.push(OverlayRow {
            label: squad_label(lang).to_string(),
            value: lang.count(total + team, "coffre", "chest", "cofre"),
            treasure: false,
            detail: None,
        });
    }
    rows.push(OverlayRow {
        label: lang.t("Trésors", "Treasures", "Tesoros").to_string(),
        value: format!("★ {}", lang.n(treasures as i64)),
        treasure: true,
        detail: None,
    });

    let last_str = last.map(|l| lang.n(l.chests_opened as i64));
    let total_str = lang.n(total as i64);
    let treasures_str = lang.n(treasures as i64);
    let session_str = lang.n(session_chests as i64);

    if let Some(lv) = live {
        return OverlayView {
            total: total_str,
            current: Some(lang.n(lv.chests_opened as i64)),
            current_caption: lang
                .t("partie en cours", "current match", "partida actual")
                .to_string(),
            current_live: true,
            badge: Some(
                lang.t("● EN DIRECT", "● LIVE", "● EN DIRECTO")
                    .to_string(),
            ),
            rows,
            treasures: treasures_str,
            session: session_str,
            last: last_str,
            squad,
        };
    }

    if in_progress {
        return OverlayView {
            total: total_str,
            current: Some("…".to_string()),
            current_caption: lang.t("en cours", "in progress", "en curso").to_string(),
            current_live: true,
            badge: Some(
                lang.t(
                    "● PARTIE EN COURS",
                    "● MATCH IN PROGRESS",
                    "● PARTIDA EN CURSO",
                )
                .to_string(),
            ),
            rows,
            treasures: treasures_str,
            session: session_str,
            last: last_str,
            squad,
        };
    }

    OverlayView {
        total: total_str,
        current: last_str.clone(),
        current_caption: last_match_caption(lang).to_string(),
        current_live: false,
        badge: None,
        rows,
        treasures: treasures_str,
        session: session_str,
        last: last_str,
        squad,
    }
}
