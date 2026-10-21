//! French formatting helpers used by the views.

use std::path::Path;

pub fn bytes(value: u64) -> String {
    const UNITS: [&str; 4] = ["o", "Ko", "Mo", "Go"];
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} o")
    } else if size >= 100.0 {
        format!("{size:.0} {}", UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit]).replace('.', ",")
    }
}

pub fn speed(value: f64) -> String {
    format!("{}/s", bytes(value.max(0.0) as u64))
}

pub fn duration(seconds: u64) -> String {
    if seconds < 60 {
        return format!("{seconds} s");
    }
    let hours = seconds as f64 / 3600.0;
    if hours < 1.0 {
        return format!("{} min", seconds / 60);
    }
    format!("{hours:.1} h").replace('.', ",")
}

pub fn percent(fraction: f32) -> String {
    format!("{:.0} %", fraction.clamp(0.0, 1.0) * 100.0)
}

pub fn relative_day(timestamp: i64, now: i64) -> String {
    let days = (now - timestamp).max(0) / 86_400;
    match days {
        0 => "Aujourd'hui".to_owned(),
        1 => "Hier".to_owned(),
        _ => format!("Il y a {days} jours"),
    }
}

pub fn date(timestamp: i64) -> String {
    let (year, month, day) = civil_from_days(timestamp.div_euclid(86_400));
    const MONTHS: [&str; 12] = [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ];
    format!("{day} {} {year}", MONTHS[(month - 1) as usize])
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m as u32, d as u32)
}

pub fn short_path(path: &Path, max_chars: usize) -> String {
    let mut full = path.display().to_string();
    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        if let Ok(rest) = path.strip_prefix(&home) {
            let rest = rest.display().to_string();
            full = if rest.is_empty() {
                "~".to_owned()
            } else {
                format!("~/{rest}")
            };
        }
    }
    if full.chars().count() <= max_chars {
        return full;
    }

    // Keep both the root and the most specific part readable.
    let chars: Vec<char> = full.chars().collect();
    let keep = max_chars.saturating_sub(1);
    let head = keep / 2;
    let tail = keep - head;
    let start: String = chars.iter().take(head).collect();
    let end: String = chars.iter().skip(chars.len() - tail).collect();
    format!("{start}…{end}")
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_prefix_is_abbreviated() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let path = std::path::PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("RecompLauncher");
        let result = short_path(&path, 80);
        assert!(result.starts_with("~/"));
        assert!(result.contains("RecompLauncher"));
    }

    #[test]
    fn long_paths_keep_both_ends() {
        let result = short_path(Path::new("abcdefghijklmnopqrstuvwxyz"), 10);
        assert_eq!(result.chars().count(), 10);
        assert!(result.starts_with("abcd"));
        assert!(result.ends_with("xyz"));
        assert!(result.contains('…'));
    }
}
