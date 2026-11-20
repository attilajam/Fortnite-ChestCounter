use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::catalog::TreasureCatalog;
use crate::lang::LangCtx;

/// Deserializes ISO-8601 / RFC-3339 timestamps with or without a timezone suffix (treating naive as UTC).
pub mod flex_utc {
    use super::*;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_utc(&s).ok_or_else(|| serde::de::Error::custom(format!("invalid datetime: {s}")))
    }

    pub fn serialize<S>(dt: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&dt.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true))
    }

    pub fn parse_utc(s: &str) -> Option<DateTime<Utc>> {
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Some(dt.with_timezone(&Utc));
        }
        for fmt in [
            "%Y-%m-%dT%H:%M:%S%.f",
            "%Y-%m-%dT%H:%M:%S",
            "%Y-%m-%d %H:%M:%S%.f",
            "%Y-%m-%d %H:%M:%S",
        ] {
            if let Ok(naive) = NaiveDateTime::parse_from_str(s, fmt) {
                return Some(DateTime::from_naive_utc_and_offset(naive, Utc));
            }
        }
        None
    }
}

pub mod opt_flex_utc {
    use super::*;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = Option::<String>::deserialize(deserializer)?;
        match opt {
            None => Ok(None),
            Some(s) if s.is_empty() => Ok(None),
            Some(s) => flex_utc::parse_utc(&s)
                .map(Some)
                .ok_or_else(|| serde::de::Error::custom(format!("invalid datetime: {s}"))),
        }
    }

    pub fn serialize<S>(dt: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match dt {
            Some(d) => flex_utc::serialize(d, serializer),
            None => serializer.serialize_none(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct ChestRecord {
    pub time_seconds: f32,
    pub kind: String,
    pub blueprint: String,
    pub destroyed: bool,
    pub items: Vec<String>,
    pub treasure_items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct TreasureFind {
    pub item: String,
    pub raw_item: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prize_id: Option<String>,
    pub from_golden_chest: bool,
    pub from_prize_handler: bool,
    pub time_exact: bool,
    pub source_destroyed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub obtained_at_seconds: Option<f32>,
    pub source_kind: String,
    pub source_blueprint: String,
    pub source_is_chest: bool,
    pub source_certain: bool,
    pub also_possible: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teammate: Option<String>,
}

impl Default for TreasureFind {
    fn default() -> Self {
        Self {
            item: String::new(),
            raw_item: String::new(),
            prize_id: None,
            from_golden_chest: false,
            from_prize_handler: false,
            time_exact: false,
            source_destroyed: false,
            obtained_at_seconds: None,
            source_kind: String::new(),
            source_blueprint: String::new(),
            source_is_chest: true,
            source_certain: false,
            also_possible: Vec::new(),
            teammate: None,
        }
    }
}

impl TreasureFind {
    pub fn display_name(&self, lang: &LangCtx, catalog: &TreasureCatalog) -> String {
        if let Some(ref pid) = self.prize_id {
            treasure_friendly(pid, lang, catalog)
        } else {
            lang.name(&self.item)
        }
    }

    pub fn source_label(&self, lang: &LangCtx) -> String {
        if self.source_destroyed {
            format!(
                "{} ({})",
                lang.name(&self.source_kind),
                lang.t("détruit", "destroyed", "destruido")
            )
        } else {
            lang.name(&self.source_kind)
        }
    }
}

pub const UNKNOWN_PREFIX: &str = "Unknown:";
const OLD_UNKNOWN_PREFIX: &str = "Inconnu:";
pub const UNKNOWN_FR: &str = "Trésor (type inconnu)";
pub const UNKNOWN_EN: &str = "Treasure (unknown type)";
pub const UNKNOWN_ES: &str = "Tesoro (tipo desconocido)";
const OLD_UNKNOWN_NAME: &str = "Gizmo de trésor";

pub fn canonical_treasure_name(stored: &str) -> &str {
    if stored == OLD_UNKNOWN_NAME {
        UNKNOWN_FR
    } else {
        stored
    }
}

pub fn is_guid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    for (i, &c) in b.iter().enumerate() {
        if matches!(i, 8 | 13 | 18 | 23) {
            if c != b'-' {
                return false;
            }
        } else if !c.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn known_prize(id: &str, lang: &LangCtx, catalog: &TreasureCatalog) -> Option<String> {
    let prize = catalog.find(id)?;
    if prize.kind == crate::catalog::PIRATE_SHOP {
        return Some(catalog.kind_name(&prize.kind, lang).to_string());
    }
    let kind = catalog.kind_name(&prize.kind, lang);
    let kind_display = if lang.code() == "en" {
        kind.to_string()
    } else {
        let mut chars = kind.chars();
        match chars.next() {
            Some(first) => format!("{}{}", first.to_lowercase(), chars.as_str()),
            None => String::new(),
        }
    };
    Some(format!("{} ({})", prize.name, kind_display))
}

pub fn treasure_friendly(item: &str, lang: &LangCtx, catalog: &TreasureCatalog) -> String {
    if !item.contains(':') {
        if let Some(bare_id) = known_prize(item, lang, catalog) {
            return bare_id;
        }
    }
    if is_guid(item) {
        return format!(
            "{} ({})",
            lang.t(
                "Récompense de coffre doré",
                "Golden chest reward",
                "Recompensa de cofre dorado"
            ),
            &item[..8]
        );
    }
    if let Some(colon) = item.find(':') {
        if let Some(known) = known_prize(item, lang, catalog) {
            return known;
        }
        let lower = item.to_ascii_lowercase();
        if lower.starts_with(&UNKNOWN_PREFIX.to_ascii_lowercase())
            || lower.starts_with(&OLD_UNKNOWN_PREFIX.to_ascii_lowercase())
        {
            return lang.t(UNKNOWN_FR, UNKNOWN_EN, UNKNOWN_ES).to_string();
        }
        return format!(
            "{} ({})",
            lang.t(
                "Récompense de coffre doré",
                "Golden chest reward",
                "Recompensa de cofre dorado"
            ),
            &item[colon + 1..]
        );
    }

    let i = item.to_ascii_lowercase();
    if i.contains("flameceleb") {
        return "Feu d'artifice doré".to_string();
    }
    if i.contains("relicextractor") {
        return "Extracteur portable".to_string();
    }
    if i.contains("relicfinder") {
        return "Localisateur d'esprit".to_string();
    }
    if i.contains("luckylocator") {
        return "Localisateur chanceux".to_string();
    }
    if i.contains("smugglerextractionkey") || i.contains("extractionaccelerat") {
        return "Accélérateur d'extraction".to_string();
    }
    item.to_string()
}

pub mod chest_kinds {
    pub const NORMAL: &str = "Coffre";
    pub const RARE: &str = "Coffre rare";
    pub const BURIED: &str = "Coffre enterré";
    pub const EXTRACTION: &str = "Coffre d'esprit (rare)";
    pub const POSTER_CHEST: &str = "Coffre des affiches d'esprit";
    pub const OTHER: &str = "Autre coffre";

    pub const CANNON: &str = "Canon à butin";
    pub const AMMO_BOX: &str = "Caisse de munitions";
    pub const FOOD_BOX: &str = "Caisse de nourriture";
    pub const OTHER_CONTAINER: &str = "Autre conteneur";

    pub fn canonical(kind: &str) -> &str {
        match kind {
            "Coffre d'extraction / reliques" | "Coffre de chambre forte" => EXTRACTION,
            _ => kind,
        }
    }

    pub fn is_chest(kind: &str) -> bool {
        matches!(
            canonical(kind),
            NORMAL | RARE | BURIED | EXTRACTION | POSTER_CHEST | OTHER
        )
    }

    pub fn classify(blueprint: &str) -> &'static str {
        let b = blueprint.to_ascii_lowercase();
        if b.contains("lootcannon") || b.contains("cannon") {
            return CANNON;
        }
        if b.contains("ammo") {
            return AMMO_BOX;
        }
        if b.contains("foodbox") || b.contains("food_box") {
            return FOOD_BOX;
        }
        if b.contains("rarechest") || b.contains("legendary") || b.contains("rare_chest") {
            return RARE;
        }
        if b.contains("buried") {
            return BURIED;
        }
        if b.contains("spriteposter") || b.contains("booty") {
            return POSTER_CHEST;
        }
        if b.contains("relic") || b.contains("extraction") {
            return EXTRACTION;
        }
        if b.contains("tiered_chest") || b.contains("normalchest") || b.contains("standardchest") {
            return NORMAL;
        }
        if b.contains("chest") {
            OTHER
        } else {
            OTHER_CONTAINER
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ManualEntry {
    pub id: String,
    pub chests: i32,
    #[serde(with = "opt_flex_utc", skip_serializing_if = "Option::is_none")]
    pub added_utc: Option<DateTime<Utc>>,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub career_matches: Option<i32>,
    #[serde(with = "opt_flex_utc", skip_serializing_if = "Option::is_none")]
    pub week_from_utc: Option<DateTime<Utc>>,
}

impl ManualEntry {
    pub const HAND: &'static str = "hand";
    pub const ESTIMATE: &'static str = "estimate";
    pub const EARLIER: &'static str = "earlier";
}

impl Default for ManualEntry {
    fn default() -> Self {
        Self {
            id: String::new(),
            chests: 0,
            added_utc: None,
            kind: Self::HAND.to_string(),
            career_matches: None,
            week_from_utc: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct TeamChestCount {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epic_id: Option<String>,
    pub is_bot: bool,
    pub chests: i32,
    pub destroyed: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct SeenPrize {
    pub player: String,
    pub prize_id: String,
    pub time_seconds: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct MatchRecord {
    pub session_id: String,
    pub replay_file: String,
    #[serde(with = "flex_utc")]
    pub analyzed_at_utc: DateTime<Utc>,
    #[serde(with = "opt_flex_utc", skip_serializing_if = "Option::is_none")]
    pub match_start_utc: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epic_id: Option<String>,
    pub tracked_by_username: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_tracked_owner: Option<String>,
    pub is_replay_owner: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_players: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eliminations: Option<i32>,
    pub chests_opened: i32,
    pub ammo_boxes_opened: i32,
    pub other_containers_opened: i32,
    pub other_interactions: HashMap<String, i32>,
    pub analyzer_version: i32,
    pub is_battle_royale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<String>,
    pub is_custom_match: bool,
    pub chests: Vec<ChestRecord>,
    pub treasure_finds: Vec<TreasureFind>,
    pub team_treasure_finds: Vec<TreasureFind>,
    pub seen_prizes: Vec<SeenPrize>,
    pub team_chests: Vec<TeamChestCount>,
    pub team_chests_opened: i32,
    pub vehicle_seconds: i32,
    pub vehicle_meters: i32,
    pub unrecorded_vehicle_loot: i32,
}

impl Default for MatchRecord {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            replay_file: String::new(),
            analyzed_at_utc: Utc::now(),
            match_start_utc: None,
            duration_seconds: None,
            playlist: None,
            game_version: None,
            player_name: None,
            epic_id: None,
            tracked_by_username: false,
            not_tracked_owner: None,
            is_replay_owner: false,
            placement: None,
            total_players: None,
            eliminations: None,
            chests_opened: 0,
            ammo_boxes_opened: 0,
            other_containers_opened: 0,
            other_interactions: HashMap::new(),
            analyzer_version: 1,
            is_battle_royale: true,
            map: None,
            is_custom_match: false,
            chests: Vec::new(),
            treasure_finds: Vec::new(),
            team_treasure_finds: Vec::new(),
            seen_prizes: Vec::new(),
            team_chests: Vec::new(),
            team_chests_opened: 0,
            vehicle_seconds: 0,
            vehicle_meters: 0,
            unrecorded_vehicle_loot: 0,
        }
    }
}

impl MatchRecord {
    pub fn vehicle_chests_missing(&self) -> bool {
        self.unrecorded_vehicle_loot >= 3
    }

    pub fn start_utc(&self) -> DateTime<Utc> {
        self.match_start_utc.unwrap_or(self.analyzed_at_utc)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub demos_folder: String,
    pub start_with_fortnite: bool,
    pub show_overlay: bool,
    pub minimize_to_tray: bool,
    pub update_mode: String,
    pub time_zone: String,
    pub clock_format: String,
    pub dismissed_notices: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_version: Option<String>,
    pub overlay_style: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay_x: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay_y: Option<i32>,
    pub overlay_locked: bool,
    pub overlay_size: i32,
    pub overlay_opacity: i32,
    pub live_update: bool,
    pub live_interval_seconds: i32,
    pub show_squad_total: bool,
    pub show_squad_in_report: bool,
    pub show_team_treasures_in_report: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: None,
            profile: None,
            demos_folder: String::new(),
            start_with_fortnite: false,
            show_overlay: true,
            minimize_to_tray: false,
            update_mode: "prompt".to_string(),
            time_zone: "local".to_string(),
            clock_format: "auto".to_string(),
            dismissed_notices: Vec::new(),
            last_version: None,
            overlay_style: "hud".to_string(),
            overlay_x: None,
            overlay_y: None,
            overlay_locked: false,
            overlay_size: 100,
            overlay_opacity: 100,
            live_update: true,
            live_interval_seconds: 30,
            show_squad_total: false,
            show_squad_in_report: false,
            show_team_treasures_in_report: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct TrackerData {
    pub version: i32,
    pub analyzer_version: i32,
    #[serde(with = "flex_utc")]
    pub created_utc: DateTime<Utc>,
    pub matches: Vec<MatchRecord>,
    pub processed_files: HashMap<String, String>,
    pub manual_chests: HashMap<String, i32>,
    pub manual_history: HashMap<String, Vec<ManualEntry>>,
    pub obtained: HashMap<String, Vec<String>>,

    #[serde(skip)]
    pub obtained_ids: Vec<String>,
    #[serde(skip)]
    pub manual_adjust: i32,
    #[serde(skip)]
    pub manual_entries: Vec<ManualEntry>,
}

impl Default for TrackerData {
    fn default() -> Self {
        Self {
            version: 1,
            analyzer_version: 1,
            created_utc: Utc::now(),
            matches: Vec::new(),
            processed_files: HashMap::new(),
            manual_chests: HashMap::new(),
            manual_history: HashMap::new(),
            obtained: HashMap::new(),
            obtained_ids: Vec::new(),
            manual_adjust: 0,
            manual_entries: Vec::new(),
        }
    }
}

impl TrackerData {
    pub fn counted_chests(&self) -> i32 {
        self.matches.iter().map(|m| m.chests_opened).sum()
    }

    pub fn total_chests(&self) -> i32 {
        (self.counted_chests() + self.manual_adjust).max(0)
    }

    pub fn team_chests(&self) -> i32 {
        self.matches.iter().map(|m| m.team_chests_opened).sum()
    }

    pub fn squad_chests(&self) -> i32 {
        self.total_chests() + self.team_chests()
    }
}

pub struct Store {
    pub folder: PathBuf,
}

/// Default data folder matching C# `Store.DefaultFolder()`:
/// `%APPDATA%\ChestCounter` on Windows, `$XDG_DATA_HOME/ChestCounter` or `~/.local/share/ChestCounter` elsewhere.
pub fn default_data_folder() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.trim().is_empty() {
                return PathBuf::from(appdata).join("ChestCounter");
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            if !xdg.trim().is_empty() {
                return PathBuf::from(xdg).join("ChestCounter");
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            if !home.trim().is_empty() {
                return PathBuf::from(home).join(".local").join("share").join("ChestCounter");
            }
        }
    }
    PathBuf::from("ChestCounter")
}

/// Default Fortnite replays folder: `%LOCALAPPDATA%\FortniteGame\Saved\Demos` on Windows.
pub fn default_demos_folder() -> String {
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            if !local.trim().is_empty() {
                return PathBuf::from(local)
                    .join("FortniteGame")
                    .join("Saved")
                    .join("Demos")
                    .to_string_lossy()
                    .into_owned();
            }
        }
    }
    String::new()
}

impl Store {
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
        }
    }

    pub fn config_exists(&self) -> bool {
        self.config_path().exists()
    }

    pub fn error_log_path(&self) -> PathBuf {
        self.folder.join("errors.log")
    }

    pub fn webview_cache_dir(&self) -> PathBuf {
        self.folder.join("webview")
    }

    pub fn config_path(&self) -> PathBuf {
        self.folder.join("config.json")
    }

    pub fn data_path(&self) -> PathBuf {
        self.folder.join("data.json")
    }

    pub fn report_path(&self) -> PathBuf {
        self.folder.join("report.html")
    }

    pub fn overlay_path(&self) -> PathBuf {
        self.folder.join("overlay.html")
    }

    pub fn csv_path(&self) -> PathBuf {
        self.folder.join("matches.csv")
    }

    pub fn images_folder(&self) -> PathBuf {
        self.folder.join("images")
    }

    pub fn load_config(&self) -> Result<Config, String> {
        let p = self.config_path();
        if !p.exists() {
            return Ok(Config::default());
        }
        let s = fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
        let s = s.strip_prefix('\u{feff}').unwrap_or(&s);
        serde_json::from_str(s).map_err(|e| format!("parse {}: {e}", p.display()))
    }

    pub fn load_data(&self) -> Result<TrackerData, String> {
        let p = self.data_path();
        if !p.exists() {
            return Ok(TrackerData::default());
        }
        let s = fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
        let s = s.strip_prefix('\u{feff}').unwrap_or(&s);
        serde_json::from_str(s).map_err(|e| format!("parse {}: {e}", p.display()))
    }

    pub fn save_data(&self, data: &TrackerData) -> Result<(), String> {
        fs::create_dir_all(&self.folder)
            .map_err(|e| format!("mkdir {}: {e}", self.folder.display()))?;
        let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
        write_atomic(&self.data_path(), &json)
    }

    pub fn save_config(&self, config: &Config) -> Result<(), String> {
        fs::create_dir_all(&self.folder)
            .map_err(|e| format!("mkdir {}: {e}", self.folder.display()))?;
        let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
        write_atomic(&self.config_path(), &json)
    }
}

pub fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
    }
    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    fs::write(&tmp, content.as_bytes()).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    for attempt in 0..5 {
        match fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) if attempt < 4 => {
                let _ = e;
                std::thread::sleep(std::time::Duration::from_millis(25 * (attempt + 1)));
            }
            Err(e) => return Err(format!("rename {}: {e}", path.display())),
        }
    }
    Ok(())
}
