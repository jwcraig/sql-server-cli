use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const SETTINGS_BASENAME: &str = "settings";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct AppSettings {
    #[serde(default, rename = "autoUpdate")]
    pub(crate) auto_update: bool,
}

pub(crate) fn config_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("SSCLI_CONFIG_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    dirs::config_dir()
}

pub(crate) fn app_dir(base: &Path) -> PathBuf {
    base.join("sscli")
}

pub(crate) fn load_settings() -> Result<AppSettings> {
    load_settings_from_dir(config_dir().as_deref())
}

fn load_settings_from_dir(base: Option<&Path>) -> Result<AppSettings> {
    let Some(base) = base else {
        return Ok(AppSettings::default());
    };

    let dir = app_dir(base);
    let candidates = [
        dir.join(format!("{}.json", SETTINGS_BASENAME)),
        dir.join(format!("{}.yaml", SETTINGS_BASENAME)),
        dir.join(format!("{}.yml", SETTINGS_BASENAME)),
    ];

    let Some(path) = candidates.iter().find(|path| path.is_file()) else {
        return Ok(AppSettings::default());
    };

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read settings file: {}", path.display()))?;

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("yaml") | Some("yml") => {
            serde_yaml::from_str(&content).context("Invalid YAML settings")
        }
        Some("json") => serde_json::from_str(&content).context("Invalid JSON settings"),
        _ => Ok(AppSettings::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_auto_update_false() {
        assert!(!AppSettings::default().auto_update);
    }
}
