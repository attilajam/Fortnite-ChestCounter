use chrono::{DateTime, Datelike, FixedOffset, Local, Timelike, Utc, Weekday};

use crate::models::{canonical_treasure_name, chest_kinds};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
    Es,
}

#[derive(Debug, Clone)]
pub struct LangCtx {
    pub lang: Lang,
    pub utc_times: bool,
    pub clock_24: bool,
    /// Optional fixed local timezone offset override (None = use system Local).
    pub fixed_local_offset: Option<FixedOffset>,
}

impl LangCtx {
    pub fn new(code: Option<&str>, utc_times: bool, clock_format: &str) -> Self {
        let lang = match normalize_lang(code) {
            Some("fr") => Lang::Fr,
            Some("es") => Lang::Es,
            _ => Lang::En,
        };
        let clock_24 = match clock_format {
            "12" => false,
            "24" => true,
            _ => default_clock_24(),
        };
        Self {
            lang,
            utc_times,
            clock_24,
            fixed_local_offset: None,
        }
    }

    pub fn code(&self) -> &'static str {
        match self.lang {
            Lang::Fr => "fr",
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    pub fn culture_name(&self) -> &'static str {
        match self.lang {
            Lang::Fr => "fr-FR",
            Lang::En => "en-GB",
            Lang::Es => "es-ES",
        }
    }

    pub fn is_en(&self) -> bool {
        self.lang == Lang::En
    }

    pub fn is_es(&self) -> bool {
        self.lang == Lang::Es
    }

    pub fn t<'a>(&self, fr: &'a str, en: &'a str, es: &'a str) -> &'a str {
        match self.lang {
            Lang::En => en,
            Lang::Es => es,
            Lang::Fr => fr,
        }
    }

    pub fn thousands_sep(&self) -> &'static str {
        match self.lang {
            Lang::En => ",",
            Lang::Fr => "\u{202F}",
            Lang::Es => ".",
        }
    }

    pub fn decimal_sep(&self) -> &'static str {
        match self.lang {
            Lang::En => ".",
            Lang::Fr | Lang::Es => ",",
        }
    }

    /// Formats an integer with .NET `N0` culture formatting.
    pub fn n(&self, n: i64) -> String {
        let neg = n < 0;
        let digits = n.unsigned_abs().to_string();
        let sep = self.thousands_sep();
        let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
        if neg {
            out.push('-');
        }
        let len = digits.len();
        for (i, ch) in digits.chars().enumerate() {
            if i > 0 && (len - i) % 3 == 0 {
                out.push_str(sep);
            }
            out.push(ch);
        }
        out
    }

    /// Formats a signed manual adjustment like .NET `+#,0;-#,0`.
    pub fn signed_n(&self, n: i64) -> String {
        if n >= 0 {
            format!("+{}", self.n(n))
        } else {
            self.n(n)
        }
    }

    /// Formats a floating-point number with `decimals` decimal places and optional grouping (like .NET `F{d}` or `N{d}`).
    /// Uses round-to-even (banker's rounding) to match .NET `double.ToString`.
    pub fn format_float(&self, value: f64, decimals: usize, grouped: bool) -> String {
        let scale = 10_f64.powi(decimals as i32);
        let scaled = (value * scale).round_ties_even();
        let neg = scaled < 0.0;
        let abs_int = scaled.abs() as u128;
        let divisor = 10_u128.pow(decimals as u32);
        let int_part = abs_int / divisor;
        let frac_part = abs_int % divisor;

        let int_str = if grouped {
            format_u128_grouped(int_part, self.thousands_sep())
        } else {
            int_part.to_string()
        };

        let mut out = String::new();
        if neg && abs_int != 0 {
            out.push('-');
        }
        out.push_str(&int_str);
        if decimals > 0 {
            out.push_str(self.decimal_sep());
            out.push_str(&format!("{:0width$}", frac_part, width = decimals));
        }
        out
    }

    pub fn count(&self, n: i32, fr: &str, en: &str, es: &str) -> String {
        self.count_forms(
            n,
            fr,
            &format!("{fr}s"),
            en,
            &format!("{en}s"),
            es,
            &format!("{es}s"),
        )
    }

    pub fn count_forms(
        &self,
        n: i32,
        fr_one: &str,
        fr_many: &str,
        en_one: &str,
        en_many: &str,
        es_one: &str,
        es_many: &str,
    ) -> String {
        match self.lang {
            Lang::En => format!("{} {}", self.n(n as i64), if n == 1 { en_one } else { en_many }),
            Lang::Es => format!("{} {}", self.n(n as i64), if n == 1 { es_one } else { es_many }),
            Lang::Fr => format!("{} {}", self.n(n as i64), if n > 1 { fr_many } else { fr_one }),
        }
    }

    pub fn ordinal(&self, n: i32) -> String {
        match self.lang {
            Lang::Es => format!("{n}.º"),
            Lang::Fr => {
                if n == 1 {
                    "1er".to_string()
                } else {
                    format!("{n}e")
                }
            }
            Lang::En => {
                let suffix = match n % 100 {
                    11 | 12 | 13 => "th",
                    _ => match n % 10 {
                        1 => "st",
                        2 => "nd",
                        3 => "rd",
                        _ => "th",
                    },
                };
                format!("{n}{suffix}")
            }
        }
    }

    pub fn to_local(&self, utc: DateTime<Utc>) -> DateTime<FixedOffset> {
        if let Some(off) = self.fixed_local_offset {
            utc.with_timezone(&off)
        } else {
            let local = utc.with_timezone(&Local);
            local.fixed_offset()
        }
    }

    pub fn shown(&self, utc: DateTime<Utc>) -> DateTime<FixedOffset> {
        if self.utc_times {
            utc.with_timezone(&FixedOffset::east_opt(0).unwrap())
        } else {
            self.to_local(utc)
        }
    }

    pub fn zone(&self) -> &'static str {
        if self.utc_times { " UTC" } else { "" }
    }

    pub fn local_offset_string(&self, now_utc: DateTime<Utc>) -> String {
        let off = self.to_local(now_utc).offset().local_minus_utc();
        if off == 0 {
            return "UTC".to_string();
        }
        let sign = if off < 0 { "−" } else { "+" };
        let abs = off.unsigned_abs();
        let hours = abs / 3600;
        let mins = (abs % 3600) / 60;
        if mins != 0 {
            format!("UTC{sign}{hours}:{mins:02}")
        } else {
            format!("UTC{sign}{hours}")
        }
    }

    pub fn format_time(&self, dt: DateTime<FixedOffset>, seconds: bool) -> String {
        if self.clock_24 {
            if seconds {
                format!("{:02}:{:02}:{:02}", dt.hour(), dt.minute(), dt.second())
            } else {
                format!("{:02}:{:02}", dt.hour(), dt.minute())
            }
        } else {
            let (is_pm, hour12) = dt.hour12();
            let ampm = match self.lang {
                Lang::En => {
                    if is_pm { "pm" } else { "am" }
                }
                Lang::Fr => {
                    if is_pm { "PM" } else { "AM" }
                }
                Lang::Es => {
                    if is_pm { "p. m." } else { "a. m." }
                }
            };
            if seconds {
                format!("{hour12}:{:02}:{:02} {ampm}", dt.minute(), dt.second())
            } else {
                format!("{hour12}:{:02} {ampm}", dt.minute())
            }
        }
    }

    pub fn clock(&self, shown: DateTime<FixedOffset>, seconds: bool) -> String {
        format!("{}{}", self.format_time(shown, seconds), self.zone())
    }

    pub fn date_time_short(&self, shown: DateTime<FixedOffset>) -> String {
        let date = if self.is_en() {
            format!("{} {} ", shown.day(), self.month_short(shown.month()))
        } else {
            format!("{:02}/{:02} ", shown.day(), shown.month())
        };
        format!("{date}{}{}", self.format_time(shown, false), self.zone())
    }

    /// Short date and time ("Nov 8, 6:00 am" / "8 nov. 06:00")
    pub fn short_when(&self, utc: DateTime<Utc>) -> String {
        let shown = self.shown(utc);
        let date = match self.lang {
            Lang::En => format!("{} {}, ", self.month_short(shown.month()), shown.day()),
            Lang::Fr | Lang::Es => format!("{} {} ", shown.day(), self.month_short(shown.month())),
        };
        format!("{date}{}{}", self.format_time(shown, false), self.zone())
    }

    /// Formats updated date: "19 Nov 2026" (en) or "19/11/2026" (fr/es).
    pub fn updated_date(&self, shown: DateTime<FixedOffset>) -> String {
        if self.is_en() {
            format!(
                "{} {} {}",
                shown.day(),
                self.month_short(shown.month()),
                shown.year()
            )
        } else {
            format!("{:02}/{:02}/{:04}", shown.day(), shown.month(), shown.year())
        }
    }

    /// Chart tooltip date: "ddd d MMM" (en) or "ddd dd/MM" (fr/es).
    pub fn day_tip_date(&self, d: chrono::NaiveDate) -> String {
        let wd = self.weekday_short(d.weekday());
        if self.is_en() {
            format!("{wd} {} {}", d.day(), self.month_short(d.month()))
        } else {
            format!("{wd} {:02}/{:02}", d.day(), d.month())
        }
    }

    /// Chart axis date: "d MMM" (en) or "dd/MM" (fr/es).
    pub fn day_axis_date(&self, d: chrono::NaiveDate) -> String {
        if self.is_en() {
            format!("{} {}", d.day(), self.month_short(d.month()))
        } else {
            format!("{:02}/{:02}", d.day(), d.month())
        }
    }

    /// Long date pattern (`D` in .NET)
    pub fn long_date(&self, dt: DateTime<FixedOffset>) -> String {
        let wd = self.weekday_full(dt.weekday());
        let mon = self.month_full(dt.month());
        match self.lang {
            Lang::En => format!("{wd}, {} {mon} {}", dt.day(), dt.year()),
            Lang::Fr => format!("{wd} {} {mon} {}", dt.day(), dt.year()),
            Lang::Es => format!("{wd}, {} de {mon} de {}", dt.day(), dt.year()),
        }
    }

    pub fn time_el_text(&self, shown: DateTime<FixedOffset>, format: &str) -> String {
        match format {
            "date" => match self.lang {
                Lang::En => format!("{} {}", self.month_full(shown.month()), shown.day()),
                Lang::Fr => format!("{} {}", shown.day(), self.month_full(shown.month())),
                Lang::Es => format!("{} de {}", shown.day(), self.month_full(shown.month())),
            },
            "weekdayTime" => format!("{} {}", self.weekday_full(shown.weekday()), self.clock(shown, false)),
            _ => match self.lang {
                Lang::En => format!(
                    "{}, {} {}, {}",
                    self.weekday_full(shown.weekday()),
                    self.month_short(shown.month()),
                    shown.day(),
                    self.clock(shown, false)
                ),
                Lang::Fr | Lang::Es => format!(
                    "{} {} {}, {}",
                    self.weekday_full(shown.weekday()),
                    shown.day(),
                    self.month_short(shown.month()),
                    self.clock(shown, false)
                ),
            },
        }
    }

    pub fn month_short(&self, month: u32) -> &'static str {
        let idx = (month.saturating_sub(1) as usize).min(11);
        match self.lang {
            Lang::En => [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ][idx],
            Lang::Fr => [
                "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.",
                "nov.", "déc.",
            ][idx],
            Lang::Es => [
                "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic",
            ][idx],
        }
    }

    pub fn month_full(&self, month: u32) -> &'static str {
        let idx = (month.saturating_sub(1) as usize).min(11);
        match self.lang {
            Lang::En => [
                "January", "February", "March", "April", "May", "June", "July", "August",
                "September", "October", "November", "December",
            ][idx],
            Lang::Fr => [
                "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août",
                "septembre", "octobre", "novembre", "décembre",
            ][idx],
            Lang::Es => [
                "enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto",
                "septiembre", "octubre", "noviembre", "diciembre",
            ][idx],
        }
    }

    pub fn weekday_short(&self, wd: Weekday) -> &'static str {
        match self.lang {
            Lang::En => match wd {
                Weekday::Mon => "Mon",
                Weekday::Tue => "Tue",
                Weekday::Wed => "Wed",
                Weekday::Thu => "Thu",
                Weekday::Fri => "Fri",
                Weekday::Sat => "Sat",
                Weekday::Sun => "Sun",
            },
            Lang::Fr => match wd {
                Weekday::Mon => "lun.",
                Weekday::Tue => "mar.",
                Weekday::Wed => "mer.",
                Weekday::Thu => "jeu.",
                Weekday::Fri => "ven.",
                Weekday::Sat => "sam.",
                Weekday::Sun => "dim.",
            },
            Lang::Es => match wd {
                Weekday::Mon => "lun",
                Weekday::Tue => "mar",
                Weekday::Wed => "mié",
                Weekday::Thu => "jue",
                Weekday::Fri => "vie",
                Weekday::Sat => "sáb",
                Weekday::Sun => "dom",
            },
        }
    }

    pub fn weekday_full(&self, wd: Weekday) -> &'static str {
        match self.lang {
            Lang::En => match wd {
                Weekday::Mon => "Monday",
                Weekday::Tue => "Tuesday",
                Weekday::Wed => "Wednesday",
                Weekday::Thu => "Thursday",
                Weekday::Fri => "Friday",
                Weekday::Sat => "Saturday",
                Weekday::Sun => "Sunday",
            },
            Lang::Fr => match wd {
                Weekday::Mon => "lundi",
                Weekday::Tue => "mardi",
                Weekday::Wed => "mercredi",
                Weekday::Thu => "jeudi",
                Weekday::Fri => "vendredi",
                Weekday::Sat => "samedi",
                Weekday::Sun => "dimanche",
            },
            Lang::Es => match wd {
                Weekday::Mon => "lunes",
                Weekday::Tue => "martes",
                Weekday::Wed => "miércoles",
                Weekday::Thu => "jueves",
                Weekday::Fri => "viernes",
                Weekday::Sat => "sábado",
                Weekday::Sun => "domingo",
            },
        }
    }

    /// Display name of a chest kind or treasure item stored in French.
    pub fn name(&self, stored: &str) -> String {
        let s = chest_kinds::canonical(stored);
        let s = canonical_treasure_name(s);
        if s.is_empty() || self.lang == Lang::Fr {
            return s.to_string();
        }
        let lower = s.to_lowercase();
        let entry = match lower.as_str() {
            "coffre" => Some(("Chest", "Cofre")),
            "coffre rare" => Some(("Rare chest", "Cofre raro")),
            "coffre enterré" => Some(("Buried chest", "Cofre enterrado")),
            "coffre d'esprit (rare)" => Some(("Sprite chest (rare)", "Cofre de espíritu (raro)")),
            "coffre des affiches d'esprit" => {
                Some(("Sprite poster chest", "Cofre de los carteles de espíritu"))
            }
            "autre coffre" => Some(("Other chest", "Otro cofre")),
            "canon à butin" => Some(("Loot cannon", "Cañón de botín")),
            "caisse de munitions" => Some(("Ammo box", "Caja de munición")),
            "caisse de nourriture" => Some(("Food box", "Caja de comida")),
            "autre conteneur" => Some(("Other container", "Otro contenedor")),
            "trésor" => Some(("Treasure", "Tesoro")),
            "trésor (type inconnu)" => Some(("Treasure (unknown type)", "Tesoro (tipo desconocido)")),
            "feu d'artifice doré" => Some(("Golden firework", "Fuego artificial dorado")),
            "extracteur portable" => Some(("Portable extractor", "Extractor portátil")),
            "localisateur d'esprit" => Some(("Sprite locator", "Localizador de espíritus")),
            "localisateur chanceux" => Some(("Lucky locator", "Localizador de la suerte")),
            "accélérateur d'extraction" => {
                Some(("Extraction accelerator", "Acelerador de extracción"))
            }
            "inconnu" => Some(("unknown", "desconocido")),
            _ => None,
        };
        match entry {
            Some((en, es)) => (if self.is_en() { en } else { es }).to_string(),
            None => s.to_string(),
        }
    }
}

pub fn normalize_lang(code: Option<&str>) -> Option<&'static str> {
    let s = code?.trim().to_lowercase();
    match s.as_str() {
        "fr" | "fra" | "fre" | "french" | "français" | "francais" | "francés" | "frances" => {
            Some("fr")
        }
        "en" | "eng" | "english" | "anglais" | "inglés" | "ingles" => Some("en"),
        "es" | "spa" | "spanish" | "español" | "espanol" | "espagnol" => Some("es"),
        _ => None,
    }
}

fn default_clock_24() -> bool {
    let loc = std::env::var("LC_TIME")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();
    !loc.starts_with("en_US")
}

fn format_u128_grouped(n: u128, sep: &str) -> String {
    let digits = n.to_string();
    let len = digits.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push_str(sep);
        }
        out.push(ch);
    }
    out
}

/// Exact match for .NET's `WebUtility.HtmlEncode`:
/// Encodes `&`, `<`, `>`, `"`, `'`, and Latin-1 Supplement chars U+00A0..=U+00FF as `&#NNN;`,
/// as well as non-BMP characters (U+10000+) as `&#NNNNN;`.
pub fn html_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if (c as u32) >= 160 && (c as u32) < 256 => {
                out.push_str(&format!("&#{};", c as u32));
            }
            c if (c as u32) > 0xFFFF => {
                out.push_str(&format!("&#{};", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Exact match for .NET's `System.Text.Json.JsonSerializer.Serialize` default string escaping
/// (escapes `<`, `>`, `&`, `'`, `+`, backticks, and non-ASCII as `\uXXXX` uppercase hex).
pub fn dotnet_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003C"),
            '>' => out.push_str("\\u003E"),
            '&' => out.push_str("\\u0026"),
            '\'' => out.push_str("\\u0027"),
            '+' => out.push_str("\\u002B"),
            '`' => out.push_str("\\u0060"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c if (c as u32) >= 0x7F => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{:04X}", u));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
