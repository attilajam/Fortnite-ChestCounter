use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};

use crate::catalog::TreasureCatalog;
use crate::game_modes;
use crate::lang::LangCtx;
use crate::models::{Config, MatchRecord, Store, TrackerData};
use crate::profiles;
use crate::replay::{CURRENT_VERSION, ReplayHeaderProbe, analyze_replay_file};
use crate::report::{self, ReportContext, ScanProgress};

const QUIET_PERIOD: Duration = Duration::from_secs(10);
const FINALIZED_QUIET_PERIOD: Duration = Duration::from_secs(4);
const OLD_REPLAY_AGE: Duration = Duration::from_secs(60);
const ABANDONED_LIVE_AGE: Duration = Duration::from_secs(600);

/// Converts a `SystemTime` to .NET UTC ticks (100-ns intervals since 0001-01-01T00:00:00Z).
pub fn system_time_to_dotnet_ticks(t: SystemTime) -> i64 {
    const UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => {
            UNIX_EPOCH_TICKS
                + (d.as_secs() as i64) * 10_000_000
                + (d.subsec_nanos() as i64) / 100
        }
        Err(_) => UNIX_EPOCH_TICKS,
    }
}

/// Exact match for C# `Tracker.FileKey(FileInfo)`.
pub fn file_key(path: &Path, len: u64, mtime: SystemTime) -> String {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let ticks = system_time_to_dotnet_ticks(mtime);
    format!("{name}|{len}|{ticks}")
}

#[derive(Debug, Clone)]
pub struct ReplayEntry {
    pub path: PathBuf,
    pub len: u64,
    pub mtime: SystemTime,
}

#[derive(Debug, Clone)]
pub enum ScanOutcome {
    Added { record: MatchRecord, updated: bool },
    Skipped { file: String, reason: String },
    Error { file: String, message: String },
}

#[derive(Debug, Clone)]
pub enum LiveUpdateEvent {
    MatchStarted { counted: bool, map: Option<String> },
    CountsChanged { chests: i32, treasures: usize },
    Cleared,
}

/// Stateful replay tracker that can live inside an app loop without holding borrows.
#[derive(Default)]
pub struct TrackerState {
    last_seen_size: HashMap<PathBuf, u64>,
    pub has_pending: bool,
    pub is_finishing: bool,
    pub new_profiles_found: bool,

    // Live match state
    live_file: Option<PathBuf>,
    live_size: Option<u64>,
    live_parsed_utc: Option<DateTime<Utc>>,
    pub live_match: Option<MatchRecord>,
    live_not_counted: bool,
    live_map_known: bool,
}

impl TrackerState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_live(&self) -> bool {
        self.live_file.is_some() && !self.live_not_counted
    }

    pub fn clear_live(&mut self) {
        self.live_not_counted = false;
        self.live_map_known = false;
        self.live_file = None;
        self.live_size = None;
        self.live_match = None;
    }

    /// Clears stale "autre joueur" entries and, if analyzer version bumped, clears processed files for re-analysis.
    pub fn upgrade_if_needed(&self, store: &Store, data: &mut TrackerData) -> bool {
        data.processed_files
            .retain(|_, v| !v.starts_with("autre joueur"));
        if data.analyzer_version >= CURRENT_VERSION {
            return false;
        }
        let had_files = !data.processed_files.is_empty();
        data.analyzer_version = CURRENT_VERSION;
        if had_files {
            data.processed_files.clear();
            let _ = store.save_data(data);
        }
        had_files
    }

    pub fn replay_files(&self, demos_folder: &str) -> Vec<ReplayEntry> {
        if demos_folder.trim().is_empty() {
            return Vec::new();
        }
        let dir = Path::new(demos_folder);
        let Ok(rd) = fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut entries = Vec::new();
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let is_replay = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("replay"));
            if !is_replay {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
                entries.push(ReplayEntry {
                    path,
                    len: meta.len(),
                    mtime,
                });
            }
        }
        entries.sort_by_key(|e| e.mtime);
        entries
    }

    pub fn unprocessed_files(&self, demos_folder: &str, data: &TrackerData) -> Vec<ReplayEntry> {
        self.replay_files(demos_folder)
            .into_iter()
            .filter(|e| {
                let key = file_key(&e.path, e.len, e.mtime);
                !data.processed_files.contains_key(&key)
            })
            .collect()
    }

    pub fn backlog_count(&self, demos_folder: &str, data: &TrackerData) -> usize {
        self.unprocessed_files(demos_folder, data)
            .into_iter()
            .filter(|e| ReplayHeaderProbe::is_live(&e.path) != Some(true))
            .count()
    }

    fn is_ready(&mut self, entry: &ReplayEntry, ignore_quiet_period: bool) -> bool {
        let live = ReplayHeaderProbe::is_live(&entry.path);
        let quiet_for = SystemTime::now()
            .duration_since(entry.mtime)
            .unwrap_or_default();

        if live == Some(true) {
            return quiet_for > ABANDONED_LIVE_AGE;
        }
        if live == Some(false) && (ignore_quiet_period || quiet_for > OLD_REPLAY_AGE) {
            return true;
        }

        let stable = self
            .last_seen_size
            .get(&entry.path)
            .is_some_and(|&prev| prev == entry.len);
        self.last_seen_size.insert(entry.path.clone(), entry.len);

        if live.is_none() {
            return stable && quiet_for > Duration::from_secs(120);
        }
        if !ReplayHeaderProbe::key_missing(&entry.path) {
            self.is_finishing = true;
            return stable && quiet_for >= FINALIZED_QUIET_PERIOD;
        }
        stable && quiet_for > QUIET_PERIOD
    }

    /// Finds ready replays to analyze on this tick.
    pub fn collect_ready(
        &mut self,
        demos_folder: &str,
        data: &TrackerData,
        ignore_quiet_period: bool,
    ) -> Vec<ReplayEntry> {
        self.has_pending = false;
        self.is_finishing = false;

        let mut ready = Vec::new();
        for entry in self.unprocessed_files(demos_folder, data) {
            if !entry.path.exists() {
                continue;
            }
            if !self.is_ready(&entry, ignore_quiet_period) {
                self.has_pending = true;
                continue;
            }
            ready.push(entry);
        }
        ready
    }

    /// Processes a single replay file and updates `data` + `config` + disk storage.
    pub fn process_entry(
        &mut self,
        entry: &ReplayEntry,
        store: &Store,
        config: &mut Config,
        data: &mut TrackerData,
        catalog: &TreasureCatalog,
        lang: &LangCtx,
    ) -> ScanOutcome {
        let outcome = process_single_entry(entry, store, config, data, catalog, lang);
        if self.live_file.as_ref() == Some(&entry.path) {
            self.clear_live();
        }
        outcome
    }

    /// Checks whether a live replay is currently being recorded and optionally parses live counts.
    pub fn update_live(
        &mut self,
        config: &Config,
        data: &TrackerData,
        catalog: &TreasureCatalog,
        lang: &LangCtx,
    ) -> Option<LiveUpdateEvent> {
        let now_sys = SystemTime::now();
        let file = self
            .unprocessed_files(&config.demos_folder, data)
            .into_iter()
            .filter(|f| {
                now_sys
                    .duration_since(f.mtime)
                    .unwrap_or(Duration::MAX)
                    < Duration::from_secs(180)
            })
            .rev()
            .find(|f| ReplayHeaderProbe::is_live(&f.path) == Some(true));

        let Some(file) = file else {
            if let Some(ref lf) = self.live_file {
                let stale = fs::metadata(lf)
                    .and_then(|m| m.modified())
                    .map(|mt| {
                        now_sys.duration_since(mt).unwrap_or(Duration::MAX)
                            > Duration::from_secs(300)
                    })
                    .unwrap_or(true);
                if stale {
                    self.clear_live();
                    return Some(LiveUpdateEvent::Cleared);
                }
            }
            return None;
        };

        let mut started = false;
        if self.live_file.as_ref() != Some(&file.path) {
            self.clear_live();
            self.live_file = Some(file.path.clone());
            started = true;
        }

        if !self.live_map_known {
            let map = ReplayHeaderProbe::map(&file.path);
            if map.is_none() && !started {
                return None;
            }
            self.live_map_known = map.is_some();
            self.live_not_counted = map
                .as_deref()
                .is_some_and(|m| !game_modes::is_season_map(Some(m)));
            return Some(LiveUpdateEvent::MatchStarted {
                counted: !self.live_not_counted,
                map,
            });
        }

        if self.live_not_counted {
            return None;
        }

        if !config.live_update || ReplayHeaderProbe::key_missing(&file.path) {
            return if started {
                Some(LiveUpdateEvent::MatchStarted {
                    counted: true,
                    map: None,
                })
            } else {
                None
            };
        }

        let interval = chrono::Duration::seconds(config.live_interval_seconds.max(10) as i64);
        let now_utc = Utc::now();
        if self.live_size == Some(file.len)
            || self
                .live_parsed_utc
                .is_some_and(|t| now_utc.signed_duration_since(t) < interval)
        {
            return None;
        }

        self.live_parsed_utc = Some(now_utc);
        let Ok(record) = analyze_replay_file(&file.path, None, catalog, lang) else {
            return None;
        };
        self.live_size = Some(file.len);

        let next = if game_modes::counts(&record) {
            Some(record)
        } else {
            None
        };
        let changed = match (&self.live_match, &next) {
            (None, None) => false,
            (Some(a), Some(b)) => {
                a.chests_opened != b.chests_opened
                    || a.team_chests_opened != b.team_chests_opened
                    || a.treasure_finds.len() != b.treasure_finds.len()
            }
            _ => true,
        };
        self.live_match = next;
        if changed {
            if let Some(ref lv) = self.live_match {
                return Some(LiveUpdateEvent::CountsChanged {
                    chests: lv.chests_opened,
                    treasures: lv.treasure_finds.len(),
                });
            }
            return Some(LiveUpdateEvent::Cleared);
        }
        None
    }

    /// Convenience synchronous scan (used by CLI `--once` / `--watch`).
    pub fn scan(
        &mut self,
        store: &Store,
        config: &mut Config,
        data: &mut TrackerData,
        catalog: &TreasureCatalog,
        ignore_quiet_period: bool,
        report_ctx: &ReportContext<'_>,
        write_reports: bool,
    ) -> Vec<ScanOutcome> {
        let ready = self.collect_ready(&config.demos_folder, data, ignore_quiet_period);
        if ready.is_empty() {
            return Vec::new();
        }
        let batch = ready.len() >= 2;
        let profiles_before = profiles::list(data, report_ctx.lang).len();
        let mut outcomes = Vec::with_capacity(ready.len());
        let mut added_any = false;
        for (i, entry) in ready.iter().enumerate() {
            if batch && write_reports {
                let ctx_prog = ReportContext {
                    progress: Some(ScanProgress {
                        done: i,
                        total: ready.len(),
                    }),
                    ..*report_ctx
                };
                let _ = report::write_all(store, config, data, &ctx_prog);
            }
            let outcome = self.process_entry(entry, store, config, data, catalog, report_ctx.lang);
            if matches!(outcome, ScanOutcome::Added { .. }) {
                added_any = true;
            }
            outcomes.push(outcome);
        }
        if batch && profiles_before < 2 && profiles::list(data, report_ctx.lang).len() >= 2 {
            self.new_profiles_found = true;
        }
        if write_reports && (added_any || batch) {
            let _ = report::write_all(store, config, data, report_ctx);
        }
        outcomes
    }
}

/// Analyzes and records one replay file without holding `TrackerState` locked.
pub fn analyze_entry_detached(
    entry: &ReplayEntry,
    catalog: &TreasureCatalog,
    lang: &LangCtx,
) -> Result<Option<MatchRecord>, String> {
    if ReplayHeaderProbe::is_live(&entry.path) == Some(true)
        && ReplayHeaderProbe::key_missing(&entry.path)
    {
        return Ok(None); // incomplete live replay
    }
    let mut record = analyze_replay_file(&entry.path, None, catalog, lang)?;
    record.replay_file = entry
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("replay.replay")
        .to_string();
    Ok(Some(record))
}

/// Applies the result of `analyze_entry_detached` to `data` and `config` and saves to disk.
pub fn apply_entry_result(
    entry: &ReplayEntry,
    analyzed: Result<Option<MatchRecord>, String>,
    store: &Store,
    config: &mut Config,
    data: &mut TrackerData,
    lang: &LangCtx,
) -> ScanOutcome {
    let key = file_key(&entry.path, entry.len, entry.mtime);
    let fname = entry
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("replay.replay")
        .to_string();

    let mut record = match analyzed {
        Ok(None) => {
            data.processed_files
                .insert(key, "incomplete".to_string());
            let _ = store.save_data(data);
            return ScanOutcome::Skipped {
                file: fname,
                reason: lang
                    .t(
                        "replay incomplet (Fortnite s'est fermé pendant la partie) : cette partie ne peut pas être comptée",
                        "incomplete replay (Fortnite closed during the match): this match cannot be counted",
                        "repetición incompleta (Fortnite se cerró durante la partida): esta partida no se puede contar",
                    )
                    .to_string(),
            };
        }
        Ok(Some(r)) => r,
        Err(msg) => {
            data.processed_files
                .insert(key, format!("error: {msg}"));
            let _ = store.save_data(data);
            return ScanOutcome::Error {
                file: fname,
                message: msg,
            };
        }
    };

    record.replay_file = fname.clone();

    if let Some(reason) = game_modes::why_not(&record, lang) {
        data.matches
            .retain(|m| m.session_id != record.session_id);
        let pl = record.playlist.as_deref().unwrap_or("?");
        data.processed_files
            .insert(key, format!("autre mode: {pl}"));
        let _ = store.save_data(data);
        return ScanOutcome::Skipped {
            file: fname,
            reason,
        };
    }

    if let Some(pos) = data
        .matches
        .iter()
        .position(|m| m.session_id == record.session_id)
    {
        if data.matches[pos].analyzer_version >= record.analyzer_version {
            data.processed_files
                .insert(key, record.session_id.clone());
            let _ = store.save_data(data);
            return ScanOutcome::Skipped {
                file: fname,
                reason: lang
                    .t(
                        "partie déjà comptée (même partie, fichier renommé)",
                        "match already counted (same match, renamed file)",
                        "partida ya contada (misma partida, archivo renombrado)",
                    )
                    .to_string(),
            };
        }
        record.analyzed_at_utc = data.matches[pos].analyzed_at_utc;
        data.matches.remove(pos);
        let updated_rec = record.clone();
        data.matches.push(record);
        data.processed_files
            .insert(key, updated_rec.session_id.clone());
        let _ = store.save_data(data);
        return ScanOutcome::Added {
            record: updated_rec,
            updated: true,
        };
    }

    if config.profile.as_deref().unwrap_or("").is_empty() {
        config.profile = Some(profiles::key(&record));
        let _ = store.save_config(config);
    } else if let Some(current) = profiles::current(data, config, lang) {
        let end: DateTime<Utc> = record.match_start_utc.unwrap_or(record.analyzed_at_utc)
            + chrono::Duration::seconds(record.duration_seconds.unwrap_or(0) as i64);
        if profiles::key(&record) != current.key
            && Utc::now().signed_duration_since(end) < chrono::Duration::minutes(15)
        {
            config.profile = Some(profiles::key(&record));
            let _ = store.save_config(config);
        }
    }

    let out_rec = record.clone();
    data.matches.push(record);
    data.processed_files
        .insert(key, out_rec.session_id.clone());
    let _ = store.save_data(data);

    ScanOutcome::Added {
        record: out_rec,
        updated: false,
    }
}

fn process_single_entry(
    entry: &ReplayEntry,
    store: &Store,
    config: &mut Config,
    data: &mut TrackerData,
    catalog: &TreasureCatalog,
    lang: &LangCtx,
) -> ScanOutcome {
    let analyzed = analyze_entry_detached(entry, catalog, lang);
    apply_entry_result(entry, analyzed, store, config, data, lang)
}
