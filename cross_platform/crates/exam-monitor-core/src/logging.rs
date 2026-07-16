use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Per-session evidence log: an `events.csv` audit trail plus periodic
/// screenshot dumps, written under a timestamped folder so a proctor has a
/// record after the exam. Dependency-free (no chrono) — timestamps are
/// formatted from the system clock with a small civil-date routine.
pub struct SessionLogger {
    dir: PathBuf,
    events: Mutex<File>,
}

impl SessionLogger {
    /// Create `<base_dir>/<room>_<start-epoch>/` with an events.csv header.
    pub fn new(base_dir: &Path, room: &str) -> std::io::Result<Self> {
        let dir = base_dir.join(format!("{}_{}", sanitize(room), now_secs()));
        fs::create_dir_all(&dir)?;
        let mut events = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("events.csv"))?;
        writeln!(events, "timestamp_utc,epoch_ms,event,name,student_id")?;
        Ok(Self {
            dir,
            events: Mutex::new(events),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Append one audit row. Errors are swallowed — logging must never take
    /// down a live exam.
    pub fn event(&self, kind: &str, name: &str, student_id: &str) {
        let ms = now_ms();
        let line = format!(
            "{},{},{},{},{}\n",
            format_utc(ms),
            ms,
            kind,
            csv_escape(name),
            csv_escape(student_id),
        );
        if let Ok(mut file) = self.events.lock() {
            let _ = file.write_all(line.as_bytes());
        }
    }

    /// Write one JPEG frame as `<label>_<epoch>.jpg`.
    pub fn save_frame(&self, label: &str, jpeg: &[u8]) {
        let name = format!("{}_{}.jpg", sanitize(label), now_secs());
        let _ = fs::write(self.dir.join(name), jpeg);
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

fn sanitize(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if cleaned.is_empty() {
        String::from("unknown")
    } else {
        cleaned
    }
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Epoch milliseconds -> `YYYY-MM-DD HH:MM:SSZ` (UTC), via Howard Hinnant's
/// days-from-civil algorithm. Avoids pulling in a date crate.
fn format_utc(ms: u128) -> String {
    let secs = (ms / 1000) as i64;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (hour, minute, second) = (tod / 3600, (tod % 3600) / 60, tod % 60);

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };

    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_epoch_as_utc() {
        // 1_700_000_000_000 ms == 2023-11-14 22:13:20 UTC
        assert_eq!(format_utc(1_700_000_000_000), "2023-11-14 22:13:20Z");
    }

    #[test]
    fn csv_escape_quotes_commas() {
        assert_eq!(csv_escape("Doe, Jane"), "\"Doe, Jane\"");
        assert_eq!(csv_escape("plain"), "plain");
    }

    #[test]
    fn sanitize_strips_path_separators() {
        assert_eq!(sanitize("../etc/passwd"), "___etc_passwd");
        assert_eq!(sanitize(""), "unknown");
    }
}
