#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, Utc};

use chest_counter::app::{self, AppOptions};
use chest_counter::catalog::TreasureCatalog;
use chest_counter::lang::LangCtx;
use chest_counter::models::{Store, default_data_folder, default_demos_folder};
use chest_counter::profiles;
use chest_counter::replay::analyze_replay_file;
use chest_counter::report::{self, ReportContext};
use chest_counter::tracker::{ScanOutcome, TrackerState};

#[cfg(target_os = "windows")]
fn attach_parent_console() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(dwProcessId: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(target_os = "windows"))]
fn attach_parent_console() {}

fn print_help() {
    println!(
        "chest-counter — Fortnite Battle Royale chest & treasure counter\n\n\
         USAGE:\n  \
           chest-counter [OPTIONS]\n\n\
         DEFAULT (no mode flag):\n  \
           Opens the desktop application window, starts the always-on-top\n  \
           Windows overlay (when enabled), and automatically watches and\n  \
           analyzes Fortnite .replay files.\n\n\
         OPTIONS:\n  \
           --data <DIR>              Data folder [default: %APPDATA%\\ChestCounter on Windows]\n  \
           --demos <DIR>             Override Fortnite Demos folder\n  \
           --lang <fr|en|es>         Override display language\n  \
           --report-only             Headless: only regenerate report.html, overlay.html, matches.csv\n  \
           --once                    Headless: scan demos once, update data.json & reports, then exit\n  \
           --watch [SECS]            Headless: continuously watch demos folder (default interval: 4s)\n  \
           --analyze <FILE.replay>   Headless: analyze a single .replay file and print JSON\n  \
           --clock <auto|12|24>      Clock format (default: auto)\n  \
           --tz <local|utc|±HH:MM>   Display timezone (default: local)\n  \
           --now <RFC3339>           Override current UTC timestamp (for deterministic reports)\n  \
           --session-start <RFC3339> Override session start UTC timestamp (for overlay)\n  \
           -h, --help                Show this help"
    );
}

fn parse_offset(spec: &str) -> Result<Option<FixedOffset>, String> {
    if spec.eq_ignore_ascii_case("local") {
        return Ok(None);
    }
    if spec.eq_ignore_ascii_case("utc") || spec == "Z" {
        return Ok(Some(FixedOffset::east_opt(0).unwrap()));
    }
    let sign = if let Some(rest) = spec.strip_prefix('+') {
        (1, rest)
    } else if let Some(rest) = spec.strip_prefix('-') {
        (-1, rest)
    } else {
        return Err(format!(
            "Invalid --tz value '{spec}' (expected local, utc, or ±HH:MM)"
        ));
    };
    let mut parts = sign.1.split(':');
    let hh: i32 = parts
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("Invalid timezone hours in '{spec}'"))?;
    let mm: i32 = parts
        .next()
        .map(|s| s.parse().ok())
        .unwrap_or(Some(0))
        .ok_or_else(|| format!("Invalid timezone minutes in '{spec}'"))?;
    let secs = sign.0 * (hh * 3600 + mm * 60);
    FixedOffset::east_opt(secs)
        .map(Some)
        .ok_or_else(|| format!("Timezone offset out of range: '{spec}'"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let is_cli_mode = args.iter().any(|a| {
        matches!(
            a.as_str(),
            "-h" | "--help" | "--once" | "--report-only" | "--watch" | "--analyze"
        )
    });
    if is_cli_mode {
        attach_parent_console();
    }
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            if !is_cli_mode {
                attach_parent_console();
            }
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

enum RunMode {
    Desktop,
    ReportOnly,
    Once,
    Watch(u64),
    Analyze(PathBuf),
}

fn run(args: Vec<String>) -> Result<(), String> {
    let mut data_dir: Option<PathBuf> = None;
    let mut demos_override: Option<String> = None;
    let mut lang_override: Option<String> = None;
    let mut mode = RunMode::Desktop;
    let mut clock_override: Option<String> = None;
    let mut tz_spec = "local".to_string();
    let mut now_override: Option<DateTime<Utc>> = None;
    let mut session_start_override: Option<DateTime<Utc>> = None;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "--data" => {
                i += 1;
                data_dir = Some(PathBuf::from(
                    args.get(i).ok_or("--data requires a directory")?,
                ));
            }
            "--demos" => {
                i += 1;
                demos_override = Some(args.get(i).ok_or("--demos requires a directory")?.clone());
            }
            "--lang" => {
                i += 1;
                lang_override = Some(args.get(i).ok_or("--lang requires fr|en|es")?.clone());
            }
            "--report-only" => {
                mode = RunMode::ReportOnly;
            }
            "--once" => {
                mode = RunMode::Once;
            }
            "--watch" => {
                let secs = if let Some(next) = args.get(i + 1) {
                    if let Ok(s) = next.parse::<u64>() {
                        i += 1;
                        s.max(1)
                    } else {
                        4
                    }
                } else {
                    4
                };
                mode = RunMode::Watch(secs);
            }
            "--analyze" => {
                i += 1;
                mode = RunMode::Analyze(PathBuf::from(
                    args.get(i).ok_or("--analyze requires a file")?,
                ));
            }
            "--clock" => {
                i += 1;
                clock_override = Some(args.get(i).ok_or("--clock requires auto|12|24")?.clone());
            }
            "--tz" => {
                i += 1;
                tz_spec = args
                    .get(i)
                    .ok_or("--tz requires local|utc|±HH:MM")?
                    .clone();
            }
            "--now" => {
                i += 1;
                let s = args.get(i).ok_or("--now requires an RFC3339 timestamp")?;
                now_override = Some(
                    DateTime::parse_from_rfc3339(s)
                        .map_err(|e| format!("Invalid --now timestamp '{s}': {e}"))?
                        .with_timezone(&Utc),
                );
            }
            "--session-start" => {
                i += 1;
                let s = args
                    .get(i)
                    .ok_or("--session-start requires an RFC3339 timestamp")?;
                session_start_override = Some(
                    DateTime::parse_from_rfc3339(s)
                        .map_err(|e| format!("Invalid --session-start timestamp '{s}': {e}"))?
                        .with_timezone(&Utc),
                );
            }
            other => return Err(format!("Unknown argument: {other} (try --help)")),
        }
        i += 1;
    }

    let utc_override = tz_spec.eq_ignore_ascii_case("utc");
    let tz_override = if utc_override {
        None
    } else {
        parse_offset(&tz_spec)?
    };
    let resolved_data_dir = data_dir.unwrap_or_else(default_data_folder);
    let store = Store::new(resolved_data_dir);

    if matches!(mode, RunMode::Desktop) {
        return app::run_desktop(AppOptions {
            store,
            demos_override,
            lang_override,
            clock_override,
            tz_override,
            utc_override,
            now_override,
            session_start_override,
        });
    }

    let mut config = store.load_config()?;
    if let Some(d) = demos_override {
        config.demos_folder = d;
    }
    if config.demos_folder.trim().is_empty() {
        config.demos_folder = default_demos_folder();
    }
    if let Some(l) = lang_override {
        config.language = Some(l);
    }

    let utc_times = utc_override || config.time_zone.eq_ignore_ascii_case("utc");
    let clock_format = clock_override.as_deref().unwrap_or(&config.clock_format);
    let mut lang = LangCtx::new(config.language.as_deref(), utc_times, clock_format);
    if !utc_times {
        lang.fixed_local_offset = tz_override;
    }

    let catalog = TreasureCatalog::load(&store.folder);

    if let RunMode::Analyze(replay_path) = mode {
        let rec = analyze_replay_file(&replay_path, None, &catalog, &lang)?;
        let json = serde_json::to_string_pretty(&rec).map_err(|e| e.to_string())?;
        println!("{json}");
        return Ok(());
    }

    let mut data = store.load_data()?;
    let now_utc = now_override.unwrap_or_else(Utc::now);
    let session_start_utc = session_start_override.unwrap_or(now_utc);
    let images_dir = store.images_folder();

    let report_ctx = ReportContext {
        now_utc,
        session_start_utc,
        progress: None,
        lang: &lang,
        catalog: &catalog,
        images_folder: if images_dir.exists() {
            Some(&images_dir)
        } else {
            None
        },
    };

    match mode {
        RunMode::ReportOnly => {
            report::write_all(&store, &config, &data, &report_ctx)?;
            print_summary(&data, &config, &lang);
            Ok(())
        }
        RunMode::Once => {
            let mut tracker = TrackerState::new();
            let outcomes =
                tracker.scan(&store, &mut config, &mut data, &catalog, true, &report_ctx, false);
            for o in &outcomes {
                print_outcome(o);
            }
            report::write_all(&store, &config, &data, &report_ctx)?;
            print_summary(&data, &config, &lang);
            Ok(())
        }
        RunMode::Watch(interval) => {
            report::write_all(&store, &config, &data, &report_ctx)?;
            print_summary(&data, &config, &lang);
            let mut tracker = TrackerState::new();
            loop {
                let live_ctx = ReportContext {
                    now_utc: Utc::now(),
                    session_start_utc,
                    progress: None,
                    lang: &lang,
                    catalog: &catalog,
                    images_folder: if images_dir.exists() {
                        Some(&images_dir)
                    } else {
                        None
                    },
                };
                let outcomes = tracker.scan(
                    &store,
                    &mut config,
                    &mut data,
                    &catalog,
                    false,
                    &live_ctx,
                    true,
                );
                for o in &outcomes {
                    print_outcome(o);
                }
                thread::sleep(Duration::from_secs(interval));
            }
        }
        RunMode::Desktop | RunMode::Analyze(_) => unreachable!(),
    }
}

fn print_outcome(o: &ScanOutcome) {
    match o {
        ScanOutcome::Added { record, updated } => {
            let tag = if *updated { "updated" } else { "added" };
            println!(
                "[{tag}] {} — {} chests, {} treasures",
                record.replay_file,
                record.chests_opened,
                record.treasure_finds.len()
            );
        }
        ScanOutcome::Skipped { file, reason } => {
            println!("[skipped] {file} — {reason}");
        }
        ScanOutcome::Error { file, message } => {
            eprintln!("[error] {file} — {message}");
        }
    }
}

fn print_summary(
    data: &chest_counter::models::TrackerData,
    config: &chest_counter::models::Config,
    lang: &LangCtx,
) {
    let view = profiles::view(data, config, lang);
    let profile = profiles::current(data, config, lang);
    let treasures: usize = view.matches.iter().map(|m| m.treasure_finds.len()).sum();
    if let Some(p) = profile {
        print!("{} | ", p.name);
    }
    println!(
        "Matches: {} | Chests: {} | Treasures: {}",
        lang.n(view.matches.len() as i64),
        lang.n(view.total_chests() as i64),
        lang.n(treasures as i64),
    );
}
