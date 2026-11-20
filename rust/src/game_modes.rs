use std::sync::OnceLock;

use regex::Regex;

use crate::lang::LangCtx;
use crate::models::MatchRecord;

pub const SEASON_MAPS: &[&str] = &["/Delilah/Maps/Delilah_Terrain"];

fn br_playlist_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^Playlist_(Default(Solo|Duo|Duos|Trio|Trios|Squad|Squads)|Trios|NoBuildBR_(Solo|Duo|Duos|Trio|Trios|Squad|Squads))$",
        )
        .unwrap()
    })
}

pub fn is_season_map(map: Option<&str>) -> bool {
    let Some(map) = map else { return false };
    SEASON_MAPS.iter().any(|m| m.eq_ignore_ascii_case(map))
}

pub fn is_battle_royale_playlist(playlist: Option<&str>) -> bool {
    let Some(p) = playlist else { return false };
    br_playlist_regex().is_match(p)
}

pub fn counts(m: &MatchRecord) -> bool {
    !m.is_custom_match
        && is_battle_royale_playlist(m.playlist.as_deref())
        && (m.map.is_none() || is_season_map(m.map.as_deref()))
}

pub fn why_not(m: &MatchRecord, lang: &LangCtx) -> Option<String> {
    if counts(m) {
        return None;
    }
    let mode = m
        .playlist
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| lang.t("mode inconnu", "unknown mode", "modo desconocido"));
    if m.is_custom_match {
        return Some(match lang.code() {
            "en" => format!("custom match ({mode})"),
            "es" => format!("partida personalizada ({mode})"),
            _ => format!("partie personnalisée ({mode})"),
        });
    }
    if let Some(map) = m.map.as_deref() {
        if !is_season_map(Some(map)) {
            return Some(match lang.code() {
                "en" => format!("not on the season's map ({mode}, map {map})"),
                "es" => format!("no es el mapa de la temporada ({mode}, mapa {map})"),
                _ => format!("pas sur la carte de la saison ({mode}, carte {map})"),
            });
        }
    }
    Some(match lang.code() {
        "en" => format!("not the regular Battle Royale ({mode})"),
        "es" => format!("no es el Battle Royale normal ({mode})"),
        _ => format!("pas le Battle Royale classique ({mode})"),
    })
}
