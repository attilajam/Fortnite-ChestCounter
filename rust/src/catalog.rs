use std::collections::HashMap;
use std::fs;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use regex::Regex;
use serde_json::Value;

use crate::lang::LangCtx;
use crate::models::flex_utc;

pub const GOLD_OUTFIT: &str = "outfit";
pub const PIRATE_SHOP: &str = "pirate-shop";
pub const VEHICLE_DECAL: &str = "car";
pub const PICKAXE_OR_BACK_BLING: &str = "pickaxe";
pub const JAM_TRACK: &str = "jam";
pub const LOADING_SCREEN: &str = "loading";
pub const GIZMO: &str = "gizmo";

const ALL_KINDS: &[&str] = &[
    GOLD_OUTFIT,
    PIRATE_SHOP,
    VEHICLE_DECAL,
    PICKAXE_OR_BACK_BLING,
    JAM_TRACK,
    LOADING_SCREEN,
    GIZMO,
];

pub const DEFAULT_ODDS_SOURCE: &str = "https://x.com/FortniteStatus/status/2119202610322227327";
pub const DEFAULT_HOURS_SOURCE: &str = "https://fortnite.bot";
pub const APP_VERSION: (u32, u32, u32) = (1, 5, 1);

const EMBEDDED_CONTENT_JSON: &str = include_str!("../../content/content.json");

#[derive(Debug, Clone)]
pub struct TreasurePrize {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub image: Option<String>,
    pub prize_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TreasureWeek {
    pub number: i32,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub title: String,
    pub prizes: Vec<TreasurePrize>,
}

#[derive(Debug, Clone)]
pub struct TreasureHour {
    pub start_utc: DateTime<Utc>,
    pub end_utc: DateTime<Utc>,
    /// Preserves insertion order from content.json for exact JSON output parity.
    pub boosts: Vec<(String, f64)>,
    pub confirmed: bool,
}

impl TreasureHour {
    pub fn boost_for(&self, kind: &str) -> f64 {
        self.boosts
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, v)| *v)
            .unwrap_or(1.0)
    }
}

#[derive(Debug, Clone)]
pub struct TreasureOdds {
    pub kind: String,
    pub one_in: i32,
    pub weekly: bool,
}

#[derive(Debug, Clone)]
pub struct Notice {
    pub id: String,
    pub from_utc: DateTime<Utc>,
    pub to_utc: DateTime<Utc>,
    pub level: String,
    pub text: HashMap<String, String>,
    pub title: HashMap<String, String>,
    pub link: Option<String>,
    pub min_version: Option<(u32, u32, u32)>,
    pub max_version: Option<(u32, u32, u32)>,
}

impl Notice {
    pub fn text_in(&self, lang: &str) -> &str {
        pick_lang(&self.text, lang)
    }

    pub fn title_in(&self, lang: &str) -> &str {
        pick_lang(&self.title, lang)
    }

    pub fn concerns(&self, v: (u32, u32, u32)) -> bool {
        self.min_version.map_or(true, |min| v >= min)
            && self.max_version.map_or(true, |max| v <= max)
    }
}

fn pick_lang<'a>(map: &'a HashMap<String, String>, lang: &str) -> &'a str {
    if let Some(s) = map.get(lang) {
        return s.as_str();
    }
    if let Some(s) = map.get("en") {
        return s.as_str();
    }
    map.values().next().map(|s| s.as_str()).unwrap_or("")
}

#[derive(Debug, Clone)]
pub struct TreasureCatalog {
    pub revision: i32,
    pub odds: Vec<TreasureOdds>,
    pub pirate_shop_access: TreasurePrize,
    pub weeks: Vec<TreasureWeek>,
    pub treasure_hours: Vec<TreasureHour>,
    pub notices: Vec<Notice>,
    pub odds_source: String,
    pub hours_source: String,
}

impl TreasureCatalog {
    pub fn load_embedded() -> Self {
        Self::parse(EMBEDDED_CONTENT_JSON)
            .expect("embedded content.json must be valid")
    }

    pub fn load(data_folder: &Path) -> Self {
        let mut catalog = Self::load_embedded();
        let cached_path = data_folder.join("content.json");
        if let Ok(json) = fs::read_to_string(&cached_path) {
            let json = json.strip_prefix('\u{feff}').unwrap_or(&json);
            if let Ok(cached) = Self::parse(json) {
                if cached.revision > catalog.revision {
                    catalog = cached;
                }
            }
        }
        catalog
    }

    pub fn parse(json: &str) -> Result<Self, String> {
        let root: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let schema = root
            .get("schema")
            .and_then(Value::as_i64)
            .ok_or("missing schema")?;
        if schema != 1 {
            return Err(format!("unsupported schema {schema}"));
        }
        let revision = root
            .get("revision")
            .and_then(Value::as_i64)
            .ok_or("missing revision")? as i32;
        if revision < 1 {
            return Err("invalid revision".to_string());
        }

        let safe_id = Regex::new(r"^[a-z0-9][a-z0-9-]{0,39}$").unwrap();
        let safe_prize_id = Regex::new(r"^[A-Za-z0-9:_.\-]{1,80}$").unwrap();

        let parse_when = |v: &Value| -> Result<DateTime<Utc>, String> {
            let s = v.as_str().ok_or("expected date string")?;
            flex_utc::parse_utc(s).ok_or_else(|| format!("invalid date: {s}"))
        };

        let parse_kind = |v: &Value| -> Result<String, String> {
            let s = v.as_str().ok_or("expected kind")?;
            if ALL_KINDS.contains(&s) {
                Ok(s.to_string())
            } else {
                Err(format!("unknown kind: {s}"))
            }
        };

        let parse_prize = |e: &Value| -> Result<TreasurePrize, String> {
            let id = e.get("id").and_then(Value::as_str).ok_or("missing prize id")?;
            if !safe_id.is_match(id) {
                return Err(format!("invalid prize id: {id}"));
            }
            let kind = parse_kind(e.get("kind").ok_or("missing prize kind")?)?;
            let name = e
                .get("name")
                .and_then(Value::as_str)
                .ok_or("missing prize name")?;
            if name.len() > 80 {
                return Err("prize name too long".to_string());
            }
            let image = match e.get("image").and_then(Value::as_str) {
                Some(img) => {
                    if img.len() > 300 || !img.starts_with("https://") {
                        return Err("invalid image url".to_string());
                    }
                    Some(img.to_string())
                }
                None => None,
            };
            let prize_ids = match e.get("prizeIds").and_then(Value::as_array) {
                Some(arr) => {
                    let mut out = Vec::new();
                    for item in arr.iter().take(6) {
                        let s = item.as_str().unwrap_or("");
                        if !safe_prize_id.is_match(s) {
                            return Err(format!("invalid prizeId: {s}"));
                        }
                        out.push(s.to_string());
                    }
                    out
                }
                None => Vec::new(),
            };
            Ok(TreasurePrize {
                id: id.to_string(),
                kind,
                name: name.to_string(),
                image,
                prize_ids,
            })
        };

        let odds_arr = root
            .get("odds")
            .and_then(Value::as_array)
            .ok_or("missing odds")?;
        let mut odds = Vec::new();
        for o in odds_arr.iter().take(20) {
            let one_in = o
                .get("oneIn")
                .and_then(Value::as_i64)
                .ok_or("missing oneIn")? as i32;
            if !(1..=100_000_000).contains(&one_in) {
                return Err("oneIn out of range".to_string());
            }
            let kind = parse_kind(o.get("kind").ok_or("missing odds kind")?)?;
            let weekly = o.get("weekly").and_then(Value::as_bool).unwrap_or(false);
            odds.push(TreasureOdds {
                kind,
                one_in,
                weekly,
            });
        }
        for k in ALL_KINDS {
            if !odds.iter().any(|o| o.kind == *k) {
                return Err(format!("missing odds for {k}"));
            }
        }

        let weeks_arr = root
            .get("weeks")
            .and_then(Value::as_array)
            .ok_or("missing weeks")?;
        let mut weeks = Vec::new();
        for w in weeks_arr.iter().take(30) {
            let number = w
                .get("number")
                .and_then(Value::as_i64)
                .ok_or("missing week number")? as i32;
            let from = parse_when(w.get("start").ok_or("missing week start")?)?;
            let to = parse_when(w.get("end").ok_or("missing week end")?)?;
            if to <= from {
                return Err("invalid week dates".to_string());
            }
            let title = w
                .get("title")
                .and_then(Value::as_str)
                .ok_or("missing week title")?;
            let mut prizes = Vec::new();
            if let Some(pa) = w.get("prizes").and_then(Value::as_array) {
                for p in pa.iter().take(20) {
                    prizes.push(parse_prize(p)?);
                }
            }
            weeks.push(TreasureWeek {
                number,
                from,
                to,
                title: title.to_string(),
                prizes,
            });
        }

        let mut treasure_hours = Vec::new();
        if let Some(th_arr) = root.get("treasureHours").and_then(Value::as_array) {
            for h in th_arr.iter().take(300) {
                let start_utc = parse_when(h.get("start").ok_or("missing hour start")?)?;
                let end_utc = parse_when(h.get("end").ok_or("missing hour end")?)?;
                if end_utc <= start_utc || end_utc - start_utc > Duration::hours(24) {
                    return Err("invalid treasure hour dates".to_string());
                }
                let boosts_obj = h
                    .get("boosts")
                    .and_then(Value::as_object)
                    .ok_or("missing boosts")?;
                let mut boosts = Vec::new();
                for (k, v) in boosts_obj {
                    if !ALL_KINDS.contains(&k.as_str()) {
                        return Err(format!("invalid boost kind {k}"));
                    }
                    let val = v.as_f64().ok_or("invalid boost value")?;
                    if !(1.0..=100.0).contains(&val) {
                        return Err("boost out of range".to_string());
                    }
                    boosts.push((k.clone(), val));
                }
                let confirmed = h.get("confirmed").and_then(Value::as_bool).unwrap_or(false);
                treasure_hours.push(TreasureHour {
                    start_utc,
                    end_utc,
                    boosts,
                    confirmed,
                });
            }
        }

        let mut notices = Vec::new();
        if let Some(nt_arr) = root.get("notices").and_then(Value::as_array) {
            for n in nt_arr.iter().take(10) {
                let id = n.get("id").and_then(Value::as_str).ok_or("missing notice id")?;
                if !safe_id.is_match(id) {
                    return Err("invalid notice id".to_string());
                }
                let from_utc = parse_when(n.get("from").ok_or("missing notice from")?)?;
                let to_utc = parse_when(n.get("to").ok_or("missing notice to")?)?;
                let level = n
                    .get("level")
                    .and_then(Value::as_str)
                    .ok_or("missing notice level")?;
                if !matches!(level, "info" | "warning") {
                    return Err("invalid notice level".to_string());
                }
                let text = parse_langs(n.get("text").ok_or("missing notice text")?, 600)?;
                let title = match n.get("title") {
                    Some(t) => parse_langs(t, 100)?,
                    None => HashMap::new(),
                };
                let link = match n.get("link").and_then(Value::as_str) {
                    Some(url) if url.len() <= 300 && url.starts_with("https://") => {
                        Some(url.to_string())
                    }
                    Some(_) => return Err("invalid notice link".to_string()),
                    None => None,
                };
                let min_version = n
                    .get("minVersion")
                    .and_then(Value::as_str)
                    .map(parse_version)
                    .transpose()?;
                let max_version = n
                    .get("maxVersion")
                    .and_then(Value::as_str)
                    .map(parse_version)
                    .transpose()?;
                notices.push(Notice {
                    id: id.to_string(),
                    from_utc,
                    to_utc,
                    level: level.to_string(),
                    text,
                    title,
                    link,
                    min_version,
                    max_version,
                });
            }
        }

        let pirate_shop_access = parse_prize(root.get("pirateShop").ok_or("missing pirateShop")?)?;
        if pirate_shop_access.kind != PIRATE_SHOP {
            return Err("invalid pirateShop kind".to_string());
        }

        let source = |name: &str, fallback: &str| -> Result<String, String> {
            match root.get("sources").and_then(|s| s.get(name)).and_then(Value::as_str) {
                Some(url) if url.len() <= 300 && url.starts_with("https://") => Ok(url.to_string()),
                Some(_) => Err(format!("invalid sources.{name}")),
                None => Ok(fallback.to_string()),
            }
        };

        Ok(Self {
            revision,
            odds,
            pirate_shop_access,
            weeks,
            treasure_hours,
            notices,
            odds_source: source("odds", DEFAULT_ODDS_SOURCE)?,
            hours_source: source("treasureHours", DEFAULT_HOURS_SOURCE)?,
        })
    }

    pub fn active_notices(&self, utc: DateTime<Utc>) -> Vec<&Notice> {
        self.notices
            .iter()
            .filter(|n| utc >= n.from_utc && utc < n.to_utc && n.concerns(APP_VERSION))
            .collect()
    }

    pub fn find(&self, prize_id: &str) -> Option<&TreasurePrize> {
        if self
            .pirate_shop_access
            .prize_ids
            .iter()
            .any(|id| id.eq_ignore_ascii_case(prize_id))
        {
            return Some(&self.pirate_shop_access);
        }
        self.weeks
            .iter()
            .flat_map(|w| w.prizes.iter())
            .find(|p| p.prize_ids.iter().any(|id| id.eq_ignore_ascii_case(prize_id)))
    }

    pub fn treasure_hour_at(&self, utc: DateTime<Utc>) -> Option<&TreasureHour> {
        self.treasure_hours
            .iter()
            .find(|h| utc >= h.start_utc && utc < h.end_utc)
    }

    pub fn boost(&self, kind: &str, utc: DateTime<Utc>) -> f64 {
        let Some(h) = self.treasure_hour_at(utc) else {
            return 1.0;
        };
        let b = h.boost_for(kind);
        if b > 0.0 { b } else { 1.0 }
    }

    pub fn week_of(&self, utc: DateTime<Utc>) -> Option<&TreasureWeek> {
        self.weeks.iter().find(|w| utc >= w.from && utc < w.to)
    }

    pub fn hunt_start(&self) -> DateTime<Utc> {
        self.weeks
            .first()
            .map(|w| w.from)
            .unwrap_or(DateTime::<Utc>::MIN_UTC)
    }

    pub fn shown_week(&self, utc: DateTime<Utc>) -> Option<&TreasureWeek> {
        self.week_of(utc)
            .or_else(|| self.weeks.iter().rev().find(|w| w.to <= utc))
    }

    pub fn kind_name<'a>(&self, kind: &str, lang: &'a LangCtx) -> &'a str {
        match kind {
            GOLD_OUTFIT => lang.t("Tenue en or", "Gold Outfit", "Traje de oro"),
            PIRATE_SHOP => lang.t(
                "Accès à la boutique pirate",
                "Pirate Shop Access",
                "Acceso a la tienda pirata",
            ),
            VEHICLE_DECAL => lang.t(
                "Véhicule et autocollant",
                "Vehicle & Decal",
                "Vehículo y adhesivo",
            ),
            PICKAXE_OR_BACK_BLING => lang.t(
                "Pioche ou sac à dos",
                "Pickaxe or Back Bling",
                "Pico o mochila",
            ),
            JAM_TRACK => lang.t("Piste musicale", "Jam Track", "Pista musical"),
            LOADING_SCREEN => lang.t("Écran de chargement", "Loading Screen", "Pantalla de carga"),
            _ => lang.t(
                "Gizmo (n'importe lequel)",
                "Gizmo (any)",
                "Artilugio (cualquiera)",
            ),
        }
    }

    pub fn log_never(&self, odds: &TreasureOdds, chest_times_utc: &[DateTime<Utc>], plain_chests: i64) -> f64 {
        let p = 1.0 / odds.one_in as f64;
        let mut sum = plain_chests.max(0) as f64 * (1.0 - p).ln();
        for &t in chest_times_utc {
            let boosted = (p * self.boost(&odds.kind, t)).min(0.999);
            sum += (1.0 - boosted).ln();
        }
        sum
    }

    pub fn expected(&self, odds: &TreasureOdds, chest_times_utc: &[DateTime<Utc>], plain_chests: i64) -> f64 {
        let p = 1.0 / odds.one_in as f64;
        let mut sum = plain_chests.max(0) as f64 * p;
        for &t in chest_times_utc {
            sum += (p * self.boost(&odds.kind, t)).min(0.999);
        }
        sum
    }

    pub fn chests_for(one_in: i32, chance: f64) -> i64 {
        ((1.0 - chance).ln() / (1.0 - 1.0 / one_in as f64).ln()).ceil() as i64
    }
}

fn parse_langs(e: &Value, max: usize) -> Result<HashMap<String, String>, String> {
    let obj = e.as_object().ok_or("expected langs object")?;
    let mut map = HashMap::new();
    for (k, v) in obj {
        if matches!(k.as_str(), "fr" | "en" | "es") {
            let s = v.as_str().unwrap_or("");
            if s.len() > max {
                return Err("notice text too long".to_string());
            }
            map.insert(k.clone(), s.to_string());
        }
    }
    Ok(map)
}

fn parse_version(s: &str) -> Result<(u32, u32, u32), String> {
    let parts: Vec<u32> = s
        .split('.')
        .map(|p| p.parse::<u32>().map_err(|_| format!("invalid version: {s}")))
        .collect::<Result<Vec<_>, _>>()?;
    match parts.len() {
        2 => Ok((parts[0], parts[1], 0)),
        3 | 4 => Ok((parts[0], parts[1], parts[2])),
        _ => Err(format!("invalid version: {s}")),
    }
}

/// Local filename for a treasure image URL (`TreasureImages.LocalName`).
pub fn image_local_name(url: &str) -> Option<String> {
    // Strip scheme + host
    let after_scheme = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let path_and_query = after_scheme.split_once('/')?.1;
    let (path, query) = match path_and_query.split_once('?') {
        Some((p, q)) => (p, q),
        None => (path_and_query, ""),
    };
    let file_name = path.rsplit('/').next()?;
    let (stem, ext) = match file_name.rfind('.') {
        Some(idx) => (&file_name[..idx], &file_name[idx..]),
        None => (file_name, ""),
    };
    let clean_query: String = query
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if clean_query.is_empty() {
        Some(format!("{stem}{ext}"))
    } else {
        Some(format!("{stem}-{clean_query}{ext}"))
    }
}

/// Relative `images/<file>` src if the image exists in `images_folder`.
pub fn image_src(images_folder: Option<&Path>, url: Option<&str>) -> Option<String> {
    let folder = images_folder?;
    let url = url?;
    let name = image_local_name(url)?;
    if folder.join(&name).exists() {
        Some(format!("images/{name}"))
    } else {
        None
    }
}
