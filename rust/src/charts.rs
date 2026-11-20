use std::collections::HashMap;
use std::fmt::Write;

use chrono::{DateTime, Duration, FixedOffset, NaiveDate};

use crate::catalog::TreasureCatalog;
use crate::lang::{LangCtx, html_encode};
use crate::models::{MatchRecord, chest_kinds};
use crate::report::pretty;

fn h(s: &str) -> String {
    html_encode(s)
}

fn when(m: &MatchRecord, lang: &LangCtx) -> DateTime<FixedOffset> {
    lang.shown(m.start_utc())
}

fn kind_color(kind: &str) -> &'static str {
    match chest_kinds::canonical(kind) {
        chest_kinds::NORMAL => "#eab308",
        chest_kinds::RARE => "#38bdf8",
        chest_kinds::BURIED => "#fb923c",
        chest_kinds::EXTRACTION => "#c084fc",
        chest_kinds::POSTER_CHEST => "#34d399",
        _ => "#94a3b8",
    }
}

/// Rounds a positive ceiling up to a clean human-readable axis tick maximum.
pub fn nice_ceil(v: f64) -> f64 {
    if v <= 0.0 {
        return 1.0;
    }
    let exp = 10_f64.powf(v.log10().floor());
    let frac = v / exp;
    let nice = if frac <= 1.0 {
        1.0
    } else if frac <= 1.5 {
        1.5
    } else if frac <= 2.0 {
        2.0
    } else if frac <= 2.5 {
        2.5
    } else if frac <= 3.0 {
        3.0
    } else if frac <= 4.0 {
        4.0
    } else if frac <= 5.0 {
        5.0
    } else if frac <= 6.0 {
        6.0
    } else if frac <= 8.0 {
        8.0
    } else {
        10.0
    };
    nice * exp
}

/// Renders the upgraded 30-day SVG combo chart:
/// - Stacked bars: base-odds chests (gold) + match-start Treasure Hour boosted chests (orange)
/// - 7-day rolling average smooth polyline (sky)
/// - Active-day average horizontal dashed reference line
/// - Clean Y-axis gridlines & labels + interactive hover tooltip
pub fn daily_chart_section(
    matches: &[&MatchRecord],
    now_shown: DateTime<FixedOffset>,
    catalog: &TreasureCatalog,
    lang: &LangCtx,
) -> String {
    let today: NaiveDate = now_shown.date_naive();
    let days: Vec<NaiveDate> = (0..30).map(|i| today - Duration::days(29 - i)).collect();

    let mut base_day = vec![0i32; 30];
    let mut boost_day = vec![0i32; 30];
    let mut matches_day = vec![0i32; 30];

    for m in matches {
        let d = when(m, lang).date_naive();
        if let Some(idx) = days.iter().position(|&x| x == d) {
            matches_day[idx] += 1;
            let is_boosted = m
                .match_start_utc
                .and_then(|s| catalog.treasure_hour_at(s))
                .is_some_and(|hr| hr.boosts.iter().any(|(_, b)| *b > 1.0));
            if is_boosted {
                boost_day[idx] += m.chests_opened;
            } else {
                base_day[idx] += m.chests_opened;
            }
        }
    }

    let totals: Vec<i32> = (0..30).map(|i| base_day[i] + boost_day[i]).collect();
    let sum_30: i64 = totals.iter().map(|&x| x as i64).sum();
    let boosted_30: i64 = boost_day.iter().map(|&x| x as i64).sum();
    let active_days = totals.iter().filter(|&&x| x > 0).count();
    let active_avg = if active_days > 0 {
        sum_30 as f64 / active_days as f64
    } else {
        0.0
    };

    let raw_max = totals.iter().copied().max().unwrap_or(0);
    let best_idx = totals.iter().position(|&x| x == raw_max).unwrap_or(0);
    let y_max = nice_ceil((raw_max.max(4)) as f64);

    // 7-day rolling average
    let mut ma7 = vec![0.0_f64; 30];
    for i in 0usize..30 {
        let start = i.saturating_sub(6);
        let span = (i - start + 1) as f64;
        let s: i32 = totals[start..=i].iter().sum();
        ma7[i] = s as f64 / span;
    }

    // SVG layout coordinates
    let svg_w = 920.0_f64;
    let svg_h = 210.0_f64;
    let pad_l = 46.0_f64;
    let pad_r = 16.0_f64;
    let pad_t = 18.0_f64;
    let pad_b = 26.0_f64;
    let plot_w = svg_w - pad_l - pad_r;
    let plot_h = svg_h - pad_t - pad_b;
    let col_w = plot_w / 30.0;
    let bar_w = (col_w * 0.64).clamp(6.0, 22.0);

    let y_of = |val: f64| -> f64 { pad_t + plot_h - (val / y_max).clamp(0.0, 1.0) * plot_h };

    let mut svg = String::new();
    let _ = write!(
        svg,
        r#"<svg class="vizsvg" viewBox="0 0 {svg_w:.0} {svg_h:.0}" role="img" aria-label="{}">"#,
        h(lang.t("Coffres par jour", "Chests per day", "Cofres por día"))
    );

    // Horizontal dashed gridlines (0, 50%, 100%)
    for step in [0.0_f64, 0.5, 1.0] {
        let val = y_max * step;
        let y = y_of(val);
        let dash = if step == 0.0 { "" } else { r#" stroke-dasharray="3 4""# };
        let cls = if step == 0.0 { "gaxis" } else { "gline" };
        let _ = write!(
            svg,
            r#"<line class="{cls}" x1="{pad_l:.1}" y1="{y:.1}" x2="{x2:.1}" y2="{y:.1}"{dash}/>"#,
            x2 = svg_w - pad_r
        );
        let lbl = lang.n(val.round() as i64);
        let _ = write!(
            svg,
            r#"<text class="gtick" x="{x:.1}" y="{yt:.1}" text-anchor="end">{lbl}</text>"#,
            x = pad_l - 8.0,
            yt = y + 3.5
        );
    }

    // Active-day average reference line
    if active_avg > 0.0 && raw_max > 0 {
        let ay = y_of(active_avg);
        let _ = write!(
            svg,
            r#"<line class="gavg" x1="{pad_l:.1}" y1="{ay:.1}" x2="{x2:.1}" y2="{ay:.1}" stroke-dasharray="5 4"/>"#,
            x2 = svg_w - pad_r
        );
        let avg_lbl = format!(
            "{} {}",
            lang.t("moy.", "avg", "med."),
            lang.n(active_avg.round() as i64)
        );
        let _ = write!(
            svg,
            r#"<text class="gavglbl" x="{x:.1}" y="{yt:.1}" text-anchor="end">{avg_lbl}</text>"#,
            x = svg_w - pad_r - 2.0,
            yt = (ay - 5.0).max(pad_t + 10.0)
        );
    }

    // Stacked bars
    for i in 0..30 {
        let cx = pad_l + (i as f64 + 0.5) * col_w;
        let bx = cx - bar_w * 0.5;
        let tot = totals[i];
        let base = base_day[i];
        let boost = boost_day[i];
        let y0 = y_of(0.0);

        if tot == 0 {
            let _ = write!(
                svg,
                r#"<rect class="bzero" x="{bx:.1}" y="{y:.1}" width="{bar_w:.1}" height="2" rx="1"/>"#,
                y = y0 - 2.0
            );
        } else {
            let y_base = y_of(base as f64);
            let h_base = if base > 0 { (y0 - y_base).max(1.5) } else { 0.0 };
            let y_tot = y_of(tot as f64);
            let h_boost = if boost > 0 { (y_base - y_tot).max(1.5) } else { 0.0 };
            let is_peak = i == best_idx && raw_max > 0;
            let base_cls = if is_peak && boost == 0 { "bbase peak" } else { "bbase" };
            if base > 0 {
                let _ = write!(
                    svg,
                    r#"<rect class="{base_cls}" x="{bx:.1}" y="{y_base:.1}" width="{bar_w:.1}" height="{h_base:.1}" rx="2.5"/>"#
                );
            }
            if boost > 0 {
                let _ = write!(
                    svg,
                    r#"<rect class="bboost" x="{bx:.1}" y="{y_tot:.1}" width="{bar_w:.1}" height="{h_boost:.1}" rx="2.5"/>"#
                );
            }
            if is_peak {
                let _ = write!(
                    svg,
                    r#"<text class="bpeaklbl" x="{cx:.1}" y="{yt:.1}" text-anchor="middle">{}</text>"#,
                    lang.n(tot as i64),
                    yt = (y_tot - 5.0).max(11.0)
                );
            }
        }
    }

    // 7-day rolling average polyline (only if at least 2 active days)
    if active_days >= 2 {
        let mut pts = String::new();
        for (i, &val) in ma7.iter().enumerate() {
            let cx = pad_l + (i as f64 + 0.5) * col_w;
            let cy = y_of(val);
            if i > 0 {
                pts.push(' ');
            }
            let _ = write!(pts, "{cx:.1},{cy:.1}");
        }
        let _ = write!(
            svg,
            r#"<polyline class="gma" fill="none" points="{pts}"/>"#
        );
        if let Some(&last_ma) = ma7.last() {
            let lx = pad_l + 29.5 * col_w;
            let ly = y_of(last_ma);
            let _ = write!(
                svg,
                r#"<circle class="gmadot" cx="{lx:.1}" cy="{ly:.1}" r="3.5"/>"#
            );
        }
    }

    // X-axis date ticks (every 5 days + today)
    for idx in [0usize, 5, 10, 15, 20, 25, 29] {
        let cx = pad_l + (idx as f64 + 0.5) * col_w;
        let lbl = if idx == 29 {
            lang.t("auj.", "today", "hoy").to_string()
        } else {
            lang.day_axis_date(days[idx])
        };
        let anchor = if idx == 0 {
            "start"
        } else if idx == 29 {
            "end"
        } else {
            "middle"
        };
        let tx = if idx == 0 {
            pad_l
        } else if idx == 29 {
            svg_w - pad_r
        } else {
            cx
        };
        let _ = write!(
            svg,
            r#"<text class="gtick" x="{tx:.1}" y="{y:.1}" text-anchor="{anchor}">{}</text>"#,
            h(&lbl),
            y = svg_h - 6.0
        );
    }

    // Interactive invisible hit-columns for instant hover tooltips
    for i in 0..30 {
        let rx = pad_l + i as f64 * col_w;
        let tot = totals[i];
        let m_cnt = matches_day[i];
        let boost = boost_day[i];
        let mut lines = vec![format!(
            "{} · {}",
            lang.day_tip_date(days[i]),
            lang.count(tot, "coffre", "chest", "cofre")
        )];
        if m_cnt > 0 {
            let avg_m = tot as f64 / m_cnt as f64;
            lines.push(format!(
                "{} · {} {}",
                lang.count_forms(m_cnt, "partie", "parties", "match", "matches", "partida", "partidas"),
                lang.format_float(avg_m, 1, false),
                lang.t("/ partie", "/ match", "/ partida")
            ));
        }
        if boost > 0 {
            lines.push(format!(
                "⚡ {} {}",
                lang.n(boost as i64),
                lang.t("Heure du trésor", "Treasure Hour", "Hora del tesoro")
            ));
        }
        let tip = h(&lines.join("\n"));
        let _ = write!(
            svg,
            r#"<rect class="svghit" tabindex="0" x="{rx:.1}" y="{pad_t:.1}" width="{col_w:.1}" height="{plot_h:.1}" data-tip="{tip}"/>"#
        );
    }

    svg.push_str("</svg>");

    // Header legend & summary pills
    let mut meta = String::new();
    let _ = write!(
        meta,
        r#"<div class="vizmeta"><span class="vizstat"><b>{}</b> {}</span>"#,
        lang.n(sum_30),
        lang.t("sur 30 j", "in 30d", "en 30 d")
    );
    if active_days > 0 {
        let _ = write!(
            meta,
            r#"<span class="vizstat"><b>{}</b> {}</span>"#,
            lang.n(active_avg.round() as i64),
            lang.t("/ jour actif", "/ active day", "/ día activo")
        );
    }
    if raw_max > 0 {
        let _ = write!(
            meta,
            r#"<span class="vizstat">{} <b>{}</b> ({})</span>"#,
            lang.t("pic", "peak", "pico"),
            lang.n(raw_max as i64),
            h(&lang.day_axis_date(days[best_idx]))
        );
    }
    meta.push_str(r#"<span class="vizleg">"#);
    let _ = write!(
        meta,
        r#"<i><b class="swd gold"></b>{}</i>"#,
        lang.t("Normal", "Base", "Normal")
    );
    if boosted_30 > 0 {
        let _ = write!(
            meta,
            r#"<i><b class="swd boost"></b>{} ({})</i>"#,
            lang.t("Heure du trésor", "Treasure Hour", "Hora del tesoro"),
            lang.n(boosted_30)
        );
    }
    if active_days >= 2 {
        let _ = write!(
            meta,
            r#"<i><b class="swd ma"></b>{}</i>"#,
            lang.t("Moy. 7 j", "7d trend", "Tend. 7 d")
        );
    }
    meta.push_str("</span></div>");

    format!(
        r#"<section><div class="sechead"><h2>{}</h2></div><div class="card vizcard">{meta}<div class="vizbox">{svg}<div class="charttip" hidden></div></div></div></section>"#,
        lang.t("Coffres par jour", "Chests per day", "Cofres por día")
    )
}

/// Renders the two-panel data-visualization insights row:
/// 1. Chests-per-match distribution histogram (with median & mean callouts)
/// 2. Survival-adjusted chest yield by match phase (chests/min across 2-min intervals) + playlist efficiency comparison
pub fn insights_section(matches: &[&MatchRecord], lang: &LangCtx) -> String {
    if matches.len() < 3 {
        return String::new();
    }

    let hist_card = distribution_card(matches, lang);
    let timing_card = timing_and_modes_card(matches, lang);
    format!(r#"<div class="two viztwo">{hist_card}{timing_card}</div>"#)
}

fn distribution_card(matches: &[&MatchRecord], lang: &LangCtx) -> String {
    let mut vals: Vec<i32> = matches.iter().map(|m| m.chests_opened).collect();
    vals.sort_unstable();
    let n = vals.len();
    let max_v = vals.last().copied().unwrap_or(0);
    let mean = vals.iter().map(|&v| v as f64).sum::<f64>() / n as f64;
    let median = if n % 2 == 1 {
        vals[n / 2] as f64
    } else {
        (vals[n / 2 - 1] + vals[n / 2]) as f64 / 2.0
    };

    // Choose clean bin size targeting ~7..11 bins
    let step: i32 = if max_v <= 20 {
        3
    } else if max_v <= 40 {
        5
    } else if max_v <= 100 {
        10
    } else if max_v <= 180 {
        20
    } else {
        25
    };
    let num_bins = ((max_v / step) + 1).clamp(4, 12) as usize;
    let mut bins = vec![0usize; num_bins];
    for &v in &vals {
        let idx = ((v.max(0) / step) as usize).min(num_bins - 1);
        bins[idx] += 1;
    }
    let max_bin = bins.iter().copied().max().unwrap_or(1).max(1);
    let y_max = nice_ceil(max_bin as f64);

    let svg_w = 460.0_f64;
    let svg_h = 185.0_f64;
    let pad_l = 36.0_f64;
    let pad_r = 12.0_f64;
    let pad_t = 18.0_f64;
    let pad_b = 26.0_f64;
    let plot_w = svg_w - pad_l - pad_r;
    let plot_h = svg_h - pad_t - pad_b;
    let col_w = plot_w / num_bins as f64;
    let bar_w = (col_w * 0.72).clamp(10.0, 38.0);
    let y_of = |v: f64| pad_t + plot_h - (v / y_max).clamp(0.0, 1.0) * plot_h;
    let med_bin = ((median as i32 / step) as usize).min(num_bins - 1);

    let mut svg = String::new();
    let _ = write!(
        svg,
        r#"<svg class="vizsvg" viewBox="0 0 {svg_w:.0} {svg_h:.0}" role="img" aria-label="{}">"#,
        h(lang.t(
            "Répartition par partie",
            "Chests per match distribution",
            "Distribución por partida"
        ))
    );

    for s in [0.0_f64, 0.5, 1.0] {
        let val = y_max * s;
        let y = y_of(val);
        let dash = if s == 0.0 { "" } else { r#" stroke-dasharray="3 4""# };
        let cls = if s == 0.0 { "gaxis" } else { "gline" };
        let _ = write!(
            svg,
            r#"<line class="{cls}" x1="{pad_l:.1}" y1="{y:.1}" x2="{x2:.1}" y2="{y:.1}"{dash}/>"#,
            x2 = svg_w - pad_r
        );
        let _ = write!(
            svg,
            r#"<text class="gtick" x="{x:.1}" y="{yt:.1}" text-anchor="end">{}</text>"#,
            lang.n(val.round() as i64),
            x = pad_l - 6.0,
            yt = y + 3.5
        );
    }

    for (i, &cnt) in bins.iter().enumerate() {
        let cx = pad_l + (i as f64 + 0.5) * col_w;
        let bx = cx - bar_w * 0.5;
        let y0 = y_of(0.0);
        let y1 = y_of(cnt as f64);
        let h_bar = (y0 - y1).max(if cnt > 0 { 2.0 } else { 0.0 });
        let cls = if i == med_bin { "bbase peak" } else { "bbase" };
        if cnt > 0 {
            let _ = write!(
                svg,
                r#"<rect class="{cls}" x="{bx:.1}" y="{y1:.1}" width="{bar_w:.1}" height="{h_bar:.1}" rx="3"/>"#
            );
            if cnt == max_bin {
                let _ = write!(
                    svg,
                    r#"<text class="bpeaklbl" x="{cx:.1}" y="{yt:.1}" text-anchor="middle">{cnt}</text>"#,
                    yt = (y1 - 5.0).max(11.0)
                );
            }
        } else {
            let _ = write!(
                svg,
                r#"<rect class="bzero" x="{bx:.1}" y="{y:.1}" width="{bar_w:.1}" height="2" rx="1"/>"#,
                y = y0 - 2.0
            );
        }

        let lo = i as i32 * step;
        let hi = lo + step - 1;
        let lbl = if i + 1 == num_bins {
            format!("{lo}+")
        } else {
            format!("{lo}–{hi}")
        };
        let _ = write!(
            svg,
            r#"<text class="gtick" x="{cx:.1}" y="{y:.1}" text-anchor="middle">{lbl}</text>"#,
            y = svg_h - 6.0
        );

        let pct = 100.0 * cnt as f64 / n as f64;
        let tip = h(&format!(
            "{lbl} {}\n{} ({}%)",
            lang.t("coffres", "chests", "cofres"),
            lang.count_forms(
                cnt as i32,
                "partie",
                "parties",
                "match",
                "matches",
                "partida",
                "partidas"
            ),
            lang.format_float(pct, 1, false)
        ));
        let rx = pad_l + i as f64 * col_w;
        let _ = write!(
            svg,
            r#"<rect class="svghit" tabindex="0" x="{rx:.1}" y="{pad_t:.1}" width="{col_w:.1}" height="{plot_h:.1}" data-tip="{tip}"/>"#
        );
    }

    svg.push_str("</svg>");

    let p90 = vals[((n as f64 * 0.9).floor() as usize).min(n - 1)];
    let meta = format!(
        r#"<div class="vizmeta"><span class="vizstat">{} <b>{}</b></span><span class="vizstat">{} <b>{}</b></span><span class="vizstat">P90 <b>{}</b></span></div>"#,
        lang.t("médiane", "median", "mediana"),
        lang.format_float(median, 1, false),
        lang.t("moy.", "mean", "media"),
        lang.format_float(mean, 1, false),
        lang.n(p90 as i64),
    );

    format!(
        r#"<section><div class="sechead"><h2>{}</h2></div><div class="card vizcard">{meta}<div class="vizbox">{svg}<div class="charttip" hidden></div></div></div></section>"#,
        lang.t(
            "Répartition par partie",
            "Chests per match distribution",
            "Distribución por partida"
        )
    )
}

fn timing_and_modes_card(matches: &[&MatchRecord], lang: &LangCtx) -> String {
    // 10 bins of 2 minutes each: 0-2m, 2-4m, ..., 16-18m, 18m+
    const NUM_BINS: usize = 10;
    const BIN_SECS: f64 = 120.0;

    let mut chests_in_bin = [0usize; NUM_BINS];
    let mut mins_in_bin = [0.0_f64; NUM_BINS];
    let mut total_dur_secs = 0i64;
    let mut total_chests_timed = 0i64;

    for m in matches {
        let max_chest_s = m
            .chests
            .iter()
            .map(|c| c.time_seconds as f64)
            .fold(0.0_f64, f64::max);
        let dur_s = m
            .duration_seconds
            .map(|d| d as f64)
            .filter(|&d| d > 0.0)
            .unwrap_or(max_chest_s);
        if dur_s > 0.0 {
            total_dur_secs += dur_s.round() as i64;
            total_chests_timed += m.chests_opened as i64;
            for b in 0..NUM_BINS {
                let lo = b as f64 * BIN_SECS;
                let hi = if b + 1 == NUM_BINS {
                    dur_s.max(lo)
                } else {
                    (b + 1) as f64 * BIN_SECS
                };
                if dur_s > lo {
                    let overlap = (dur_s.min(hi) - lo).max(0.0);
                    mins_in_bin[b] += overlap / 60.0;
                }
            }
        }
        for c in &m.chests {
            let t = c.time_seconds.max(0.0) as f64;
            let idx = ((t / BIN_SECS).floor() as usize).min(NUM_BINS - 1);
            chests_in_bin[idx] += 1;
        }
    }

    let rates: Vec<f64> = (0..NUM_BINS)
        .map(|b| {
            if mins_in_bin[b] >= 1.0 {
                chests_in_bin[b] as f64 / mins_in_bin[b]
            } else {
                0.0
            }
        })
        .collect();

    let max_rate = rates.iter().copied().fold(0.0_f64, f64::max);
    let y_max = nice_ceil(max_rate.max(1.0));
    let peak_idx = rates
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
        .unwrap_or(0);

    let svg_w = 460.0_f64;
    let svg_h = 185.0_f64;
    let pad_l = 36.0_f64;
    let pad_r = 12.0_f64;
    let pad_t = 18.0_f64;
    let pad_b = 26.0_f64;
    let plot_w = svg_w - pad_l - pad_r;
    let plot_h = svg_h - pad_t - pad_b;
    let col_w = plot_w / NUM_BINS as f64;
    let bar_w = (col_w * 0.68).clamp(10.0, 32.0);
    let y_of = |v: f64| pad_t + plot_h - (v / y_max).clamp(0.0, 1.0) * plot_h;

    let mut svg = String::new();
    let _ = write!(
        svg,
        r#"<svg class="vizsvg" viewBox="0 0 {svg_w:.0} {svg_h:.0}" role="img" aria-label="{}">"#,
        h(lang.t(
            "Rythme par phase de partie",
            "Loot rate by match phase",
            "Ritmo por fase de partida"
        ))
    );

    for s in [0.0_f64, 0.5, 1.0] {
        let val = y_max * s;
        let y = y_of(val);
        let dash = if s == 0.0 { "" } else { r#" stroke-dasharray="3 4""# };
        let cls = if s == 0.0 { "gaxis" } else { "gline" };
        let _ = write!(
            svg,
            r#"<line class="{cls}" x1="{pad_l:.1}" y1="{y:.1}" x2="{x2:.1}" y2="{y:.1}"{dash}/>"#,
            x2 = svg_w - pad_r
        );
        let lbl = if val >= 10.0 || val == 0.0 {
            lang.n(val.round() as i64)
        } else {
            lang.format_float(val, 1, false)
        };
        let _ = write!(
            svg,
            r#"<text class="gtick" x="{x:.1}" y="{yt:.1}" text-anchor="end">{lbl}</text>"#,
            x = pad_l - 6.0,
            yt = y + 3.5
        );
    }

    for i in 0..NUM_BINS {
        let cx = pad_l + (i as f64 + 0.5) * col_w;
        let bx = cx - bar_w * 0.5;
        let r = rates[i];
        let y0 = y_of(0.0);
        let y1 = y_of(r);
        let h_bar = (y0 - y1).max(if r > 0.0 { 2.0 } else { 0.0 });
        let cls = if i == peak_idx && max_rate > 0.0 {
            "bteal peak"
        } else {
            "bteal"
        };
        if r > 0.0 {
            let _ = write!(
                svg,
                r#"<rect class="{cls}" x="{bx:.1}" y="{y1:.1}" width="{bar_w:.1}" height="{h_bar:.1}" rx="3"/>"#
            );
            if i == peak_idx {
                let _ = write!(
                    svg,
                    r#"<text class="bpeaklbl teal" x="{cx:.1}" y="{yt:.1}" text-anchor="middle">{}</text>"#,
                    lang.format_float(r, 1, false),
                    yt = (y1 - 5.0).max(11.0)
                );
            }
        } else {
            let _ = write!(
                svg,
                r#"<rect class="bzero" x="{bx:.1}" y="{y:.1}" width="{bar_w:.1}" height="2" rx="1"/>"#,
                y = y0 - 2.0
            );
        }

        let lo_m = i * 2;
        let lbl = if i + 1 == NUM_BINS {
            format!("{lo_m}m+")
        } else {
            format!("{lo_m}–{}m", lo_m + 2)
        };
        if i % 2 == 0 || i + 1 == NUM_BINS {
            let _ = write!(
                svg,
                r#"<text class="gtick" x="{cx:.1}" y="{y:.1}" text-anchor="middle">{lbl}</text>"#,
                y = svg_h - 6.0
            );
        }

        let tip = h(&format!(
            "{lbl} · {} {}\n{}",
            lang.format_float(r, 2, false),
            lang.t("coffres/min", "chests/min", "cofres/min"),
            lang.count(chests_in_bin[i] as i32, "coffre", "chest", "cofre")
        ));
        let rx = pad_l + i as f64 * col_w;
        let _ = write!(
            svg,
            r#"<rect class="svghit" tabindex="0" x="{rx:.1}" y="{pad_t:.1}" width="{col_w:.1}" height="{plot_h:.1}" data-tip="{tip}"/>"#
        );
    }

    svg.push_str("</svg>");

    let per_hour = if total_dur_secs > 0 {
        total_chests_timed as f64 * 3600.0 / total_dur_secs as f64
    } else {
        0.0
    };
    let peak_lbl = if peak_idx + 1 == NUM_BINS {
        format!("{}m+", peak_idx * 2)
    } else {
        format!("{}–{}m", peak_idx * 2, peak_idx * 2 + 2)
    };
    let mut meta = String::from(r#"<div class="vizmeta">"#);
    if per_hour > 0.0 {
        let _ = write!(
            meta,
            r#"<span class="vizstat"><b>{}</b> {}</span>"#,
            lang.n(per_hour.round() as i64),
            lang.t("coffres / h", "chests / hr", "cofres / h")
        );
    }
    if max_rate > 0.0 {
        let _ = write!(
            meta,
            r#"<span class="vizstat">{} <b>{peak_lbl}</b> ({} / min)</span>"#,
            lang.t("pic", "peak", "pico"),
            lang.format_float(max_rate, 1, false)
        );
    }
    meta.push_str("</div>");

    // Compact mode efficiency comparison if >1 playlist was played
    let mut by_mode: HashMap<String, (usize, i64, i64)> = HashMap::new();
    for m in matches {
        let label = pretty::playlist(m.playlist.as_deref(), lang);
        let e = by_mode.entry(label).or_insert((0, 0, 0));
        e.0 += 1;
        e.1 += m.chests_opened as i64;
        e.2 += m.duration_seconds.unwrap_or(0).max(0) as i64;
    }
    let mut modes_html = String::new();
    if by_mode.len() > 1 {
        let mut rows: Vec<(String, usize, f64, f64)> = by_mode
            .into_iter()
            .map(|(name, (cnt, chests, secs))| {
                let avg_m = chests as f64 / cnt as f64;
                let per_h = if secs > 0 {
                    chests as f64 * 3600.0 / secs as f64
                } else {
                    0.0
                };
                (name, cnt, avg_m, per_h)
            })
            .collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1));
        rows.truncate(4);
        let max_avg = rows.iter().map(|r| r.2).fold(1.0_f64, f64::max);
        modes_html.push_str(r#"<div class="modes">"#);
        for (name, cnt, avg_m, per_h) in rows {
            let w = (100.0 * avg_m / max_avg).clamp(4.0, 100.0);
            let hr_s = if per_h > 0.0 {
                format!(" · {}/h", lang.n(per_h.round() as i64))
            } else {
                String::new()
            };
            let _ = write!(
                modes_html,
                r#"<div class="mrow"><span class="mname">{} <i>({})</i></span><span class="mbar"><i style="width:{w:.1}%"></i></span><span class="mval num">{}<b>{hr_s}</b></span></div>"#,
                h(&name),
                cnt,
                lang.format_float(avg_m, 1, false),
            );
        }
        modes_html.push_str("</div>");
    }

    format!(
        r#"<section><div class="sechead"><h2>{}</h2></div><div class="card vizcard">{meta}<div class="vizbox">{svg}<div class="charttip" hidden></div></div>{modes_html}</div></section>"#,
        lang.t(
            "Rythme par phase de partie",
            "Loot rate by match phase",
            "Ritmo por fase de partida"
        )
    )
}

/// Proportional 100% stacked composition bar for "By chest type".
pub fn chest_composition_bar(by_kind: &[(String, usize)], lang: &LangCtx) -> String {
    let total: usize = by_kind.iter().map(|x| x.1).sum();
    if total == 0 {
        return String::new();
    }
    let mut out = String::from(r#"<div class="compbar" aria-hidden="true">"#);
    for (k, count) in by_kind {
        let pct = 100.0 * (*count as f64) / (total as f64);
        if pct < 0.3 {
            continue;
        }
        let color = kind_color(k);
        let title = h(&format!(
            "{}: {} ({}%)",
            lang.name(k),
            lang.n(*count as i64),
            lang.format_float(pct, 1, false)
        ));
        let _ = write!(
            out,
            r#"<span style="width:{pct:.2}%;background:{color}" title="{title}"></span>"#
        );
    }
    out.push_str("</div>");
    out
}
