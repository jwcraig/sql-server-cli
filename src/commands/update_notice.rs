use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::cli::{CliArgs, CommandKind};

const CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 24);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateCache {
    last_checked_unix: u64,
}

pub(crate) fn maybe_emit(args: &CliArgs) {
    if args.quiet || args.output.json {
        return;
    }

    if matches!(args.command, CommandKind::Update(_)) {
        return;
    }

    if !io::stderr().is_terminal() {
        return;
    }

    let settings = match crate::app_settings::load_settings() {
        Ok(settings) => settings,
        Err(err) => {
            tracing::debug!("Skipping update notice; failed reading settings: {err}");
            return;
        }
    };

    if !settings.auto_update {
        return;
    }

    let now = unix_now();
    if should_skip_cache(now) {
        return;
    }

    match crate::update::check_latest_release() {
        Ok(check) => {
            write_cache(now);
            if check.update_available {
                let _ = writeln!(
                    io::stderr(),
                    "Update available: {} -> {}. Run `sscli update`.",
                    check.current_version,
                    check.latest_version
                );
            }
        }
        Err(err) => {
            tracing::debug!("Skipping update notice; GitHub check failed: {err}");
        }
    }
}

fn cache_path() -> Option<PathBuf> {
    let base = crate::app_settings::config_dir()?;
    Some(crate::app_settings::app_dir(&base).join("update-cache.json"))
}

fn should_skip_cache(now: u64) -> bool {
    let Some(path) = cache_path() else {
        return false;
    };

    let Ok(content) = fs::read_to_string(&path) else {
        return false;
    };
    let Ok(cache) = serde_json::from_str::<UpdateCache>(&content) else {
        return false;
    };

    now.saturating_sub(cache.last_checked_unix) < CACHE_TTL.as_secs()
}

fn write_cache(now: u64) {
    let Some(path) = cache_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        if let Err(err) = fs::create_dir_all(dir) {
            tracing::debug!("Failed creating settings dir {}: {err}", dir.display());
            return;
        }
    }

    let cache = UpdateCache {
        last_checked_unix: now,
    };
    let Ok(body) = serde_json::to_string(&cache) else {
        return;
    };
    let _ = fs::write(&path, body);
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
