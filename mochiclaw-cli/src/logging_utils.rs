//! Log file utilities

use std::path::PathBuf;
use time::OffsetDateTime;

/// Clean up log files older than max_age_days
pub fn cleanup_old_logs(log_dir: &PathBuf, max_age_days: u32) {
    let log_path = if log_dir.is_absolute() {
        log_dir.clone()
    } else {
        // Relative to current dir
        std::env::current_dir().unwrap_or_default().join(log_dir)
    };

    if !log_path.exists() {
        return;
    }

    let cutoff = OffsetDateTime::now_utc() - time::Duration::days(max_age_days as i64);
    let prefix = "mochiclaw";
    let date_format = time::format_description::parse("[year]-[month]-[day]").unwrap();

    if let Ok(entries) = std::fs::read_dir(&log_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };

            // Match files like mochiclaw.2026-03-28.log
            if let Some(date_str) = file_name.strip_prefix(prefix) {
                let date_str = date_str.trim_start_matches('.');
                // Parse date from filename (format: YYYY-MM-DD)
                let date_part = date_str.split('.').next().unwrap_or(date_str);
                if let Ok(parsed_date) = time::Date::parse(date_part, &date_format) {
                    let offset_date = parsed_date.with_hms(0, 0, 0).unwrap().assume_utc();
                    if offset_date < cutoff {
                        tracing::info!("removing old log file: {}", path.display());
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
    }
}

/// Resolve log directory relative to config file location
pub fn resolve_log_dir(log_dir: Option<&str>, config_path: &PathBuf) -> Option<PathBuf> {
    log_dir.map(|dir| {
        let path = PathBuf::from(dir);
        if path.is_absolute() {
            path
        } else {
            config_path.parent().unwrap_or(&PathBuf::from(".")).join(path)
        }
    })
}