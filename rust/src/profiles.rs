use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};

use crate::game_modes;
use crate::lang::LangCtx;
use crate::models::{Config, ManualEntry, MatchRecord, TrackerData};

#[derive(Debug, Clone)]
pub struct Profile {
    pub key: String,
    pub name: String,
    pub matches: usize,
    pub chests: i32,
    pub last_played: DateTime<Utc>,
}

pub fn key(m: &MatchRecord) -> String {
    if let Some(ref epic_id) = m.epic_id {
        if !epic_id.is_empty() {
            return epic_id.clone();
        }
    }
    m.player_name.clone().unwrap_or_default()
}

pub fn counted(d: &TrackerData) -> impl Iterator<Item = &MatchRecord> {
    d.matches.iter().filter(|m| game_modes::counts(m))
}

pub fn list(d: &TrackerData, lang: &LangCtx) -> Vec<Profile> {
    let mut groups: HashMap<String, Vec<&MatchRecord>> = HashMap::new();
    for m in counted(d) {
        groups.entry(key(m)).or_default().push(m);
    }

    let mut profiles: Vec<Profile> = groups
        .into_iter()
        .map(|(k, mut ms)| {
            ms.sort_by(|a, b| b.start_utc().cmp(&a.start_utc()));
            let last_played = ms[0].start_utc();
            let name = ms
                .iter()
                .filter_map(|m| m.player_name.as_deref())
                .find(|n| !n.trim().is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    lang.t("Compte inconnu", "Unknown account", "Cuenta desconocida")
                        .to_string()
                });
            let chests: i32 = ms.iter().map(|m| m.chests_opened).sum();
            Profile {
                key: k,
                name,
                matches: ms.len(),
                chests,
                last_played,
            }
        })
        .collect();

    profiles.sort_by(|a, b| b.last_played.cmp(&a.last_played));
    profiles
}

pub fn current(d: &TrackerData, c: &Config, lang: &LangCtx) -> Option<Profile> {
    let all = list(d, lang);
    if let Some(ref prof) = c.profile {
        if let Some(found) = all.iter().find(|p| &p.key == prof) {
            return Some(found.clone());
        }
    }
    all.into_iter().next()
}

pub fn manual(d: &TrackerData, profile_key: Option<&str>) -> i32 {
    let base = d.manual_chests.get("").copied().unwrap_or(0);
    let extra = match profile_key {
        Some(k) if !k.is_empty() => d.manual_chests.get(k).copied().unwrap_or(0),
        _ => 0,
    };
    base + extra
}

fn explain(list: &mut Vec<ManualEntry>, total: i32) {
    let sum: i32 = list.iter().map(|e| e.chests).sum();
    let missing = total - sum;
    if missing == 0 {
        return;
    }
    if let Some(idx) = list.iter().position(|e| e.kind == ManualEntry::EARLIER) {
        list[idx].chests += missing;
        if list[idx].chests == 0 {
            list.remove(idx);
        }
    } else {
        list.push(ManualEntry {
            id: ManualEntry::EARLIER.to_string(),
            chests: missing,
            added_utc: None,
            kind: ManualEntry::EARLIER.to_string(),
            career_matches: None,
            week_from_utc: None,
        });
    }
}

pub fn manual_history(d: &TrackerData, c: &Config, lang: &LangCtx) -> Vec<ManualEntry> {
    let k = current(d, c, lang).map(|p| p.key).unwrap_or_default();
    let mut entries = d.manual_history.get(&k).cloned().unwrap_or_default();
    explain(&mut entries, manual(d, Some(&k)));
    entries.sort_by(|a, b| {
        let a_earlier = a.kind == ManualEntry::EARLIER;
        let b_earlier = b.kind == ManualEntry::EARLIER;
        a_earlier
            .cmp(&b_earlier)
            .then_with(|| b.added_utc.cmp(&a.added_utc))
    });
    entries
}

fn settle(d: &mut TrackerData, c: &Config, lang: &LangCtx) -> (String, Vec<ManualEntry>) {
    let k = current(d, c, lang).map(|p| p.key).unwrap_or_default();
    let total = manual(d, Some(&k));
    let mut entries = d.manual_history.get(&k).cloned().unwrap_or_default();
    if !k.is_empty() {
        d.manual_chests.remove("");
        d.manual_history.remove("");
    }
    explain(&mut entries, total);
    (k, entries)
}

fn keep(d: &mut TrackerData, k: String, entries: Vec<ManualEntry>) {
    let total: i32 = entries.iter().map(|e| e.chests).sum();
    if total == 0 {
        d.manual_chests.remove(&k);
    } else {
        d.manual_chests.insert(k.clone(), total);
    }
    if entries.is_empty() {
        d.manual_history.remove(&k);
    } else {
        d.manual_history.insert(k, entries);
    }
}

fn next_manual_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(1);
    let t = Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{:08x}", ((t ^ (n.wrapping_mul(0x9e3779b97f4a7c15))) & 0xffff_ffff) as u32)
}

pub fn set_obtained(d: &mut TrackerData, c: &Config, lang: &LangCtx, prize_id: &str, on: bool) {
    let k = current(d, c, lang).map(|p| p.key).unwrap_or_default();
    let list = d.obtained.entry(k.clone()).or_default();
    list.retain(|x| x != prize_id);
    if on {
        list.push(prize_id.to_string());
    }
    if !k.is_empty() {
        if let Some(early) = d.obtained.get_mut("") {
            early.retain(|x| x != prize_id);
        }
    }
}

pub fn set_manual(d: &mut TrackerData, c: &Config, lang: &LangCtx, chests: i32) {
    let k = current(d, c, lang).map(|p| p.key).unwrap_or_default();
    d.manual_chests.remove("");
    d.manual_history.remove("");
    d.manual_history.remove(&k);
    if chests == 0 {
        d.manual_chests.remove(&k);
    } else {
        d.manual_chests.insert(k, chests);
    }
}

pub fn add_manual(
    d: &mut TrackerData,
    c: &Config,
    lang: &LangCtx,
    chests: i32,
    career_matches: Option<i32>,
    week_from_utc: Option<DateTime<Utc>>,
) {
    let (k, mut entries) = settle(d, c, lang);
    if career_matches.is_some() {
        entries.retain(|e| e.kind != ManualEntry::ESTIMATE);
    }
    entries.push(ManualEntry {
        id: next_manual_id(),
        chests,
        added_utc: Some(Utc::now()),
        kind: if career_matches.is_some() {
            ManualEntry::ESTIMATE.to_string()
        } else {
            ManualEntry::HAND.to_string()
        },
        career_matches,
        week_from_utc: if career_matches.is_none() {
            week_from_utc
        } else {
            None
        },
    });
    keep(d, k, entries);
}

pub fn remove_manual(d: &mut TrackerData, c: &Config, lang: &LangCtx, id: &str) -> bool {
    let (k, mut entries) = settle(d, c, lang);
    let before = entries.len();
    entries.retain(|e| e.id != id);
    let removed = entries.len() != before;
    keep(d, k, entries);
    removed
}

pub const MIN_COUNTED_MATCHES: usize = 10;

#[derive(Debug, Clone)]
pub struct ChestEstimateResult {
    pub career_matches: i32,
    pub counted_matches: i32,
    pub missing_matches: i32,
    pub average: f64,
    pub chests: i32,
}

pub fn estimate_from_career(view: &TrackerData, career_matches: i32) -> Option<ChestEstimateResult> {
    let counted = view.matches.len();
    if counted < MIN_COUNTED_MATCHES || career_matches <= counted as i32 {
        return None;
    }
    let average = view.counted_chests() as f64 / counted as f64;
    let missing = career_matches - counted as i32;
    let chests = ((missing as f64) * average).round() as i32;
    Some(ChestEstimateResult {
        career_matches,
        counted_matches: counted as i32,
        missing_matches: missing,
        average,
        chests,
    })
}

pub fn parse_estimate_number(text: &str) -> Option<i32> {
    let digits: String = text
        .chars()
        .filter(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect();
    digits.parse::<i32>().ok()
}

pub fn view(d: &TrackerData, c: &Config, lang: &LangCtx) -> TrackerData {
    let cur = current(d, c, lang);
    let entries = manual_history(d, c, lang);
    match cur {
        None => TrackerData {
            version: d.version,
            analyzer_version: d.analyzer_version,
            created_utc: d.created_utc,
            matches: counted(d).cloned().collect(),
            processed_files: d.processed_files.clone(),
            manual_chests: d.manual_chests.clone(),
            manual_history: d.manual_history.clone(),
            obtained: d.obtained.clone(),
            manual_adjust: manual(d, None),
            manual_entries: entries,
            obtained_ids: d.obtained.get("").cloned().unwrap_or_default(),
        },
        Some(p) => {
            let mut seen = HashSet::new();
            let mut obtained_ids = Vec::new();
            for src in [d.obtained.get(&p.key), d.obtained.get("")]
                .into_iter()
                .flatten()
            {
                for id in src {
                    if seen.insert(id.clone()) {
                        obtained_ids.push(id.clone());
                    }
                }
            }
            TrackerData {
                version: d.version,
                analyzer_version: d.analyzer_version,
                created_utc: d.created_utc,
                matches: counted(d).filter(|m| key(m) == p.key).cloned().collect(),
                processed_files: d.processed_files.clone(),
                manual_chests: d.manual_chests.clone(),
                manual_history: d.manual_history.clone(),
                obtained: d.obtained.clone(),
                manual_adjust: manual(d, Some(&p.key)),
                manual_entries: entries,
                obtained_ids,
            }
        }
    }
}
