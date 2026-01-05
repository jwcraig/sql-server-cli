use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct Env {
    vars: HashMap<String, String>,
}

impl Env {
    /// Load environment variables from the system, optionally loading a custom env file first.
    /// If `env_file` is None, loads `.env` from the current directory (if present).
    /// If `env_file` is Some(path), loads that file instead (silently ignores if missing).
    pub fn from_system(env_file: Option<&Path>) -> Self {
        // Load env file (custom path or default .env)
        match env_file {
            Some(path) => {
                let _ = dotenvy::from_path(path);
            }
            None => {
                let _ = dotenvy::dotenv();
            }
        }
        let vars = std::env::vars().collect();
        Self { vars }
    }

    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        let mut vars = HashMap::new();
        for (k, v) in pairs {
            vars.insert((*k).to_string(), (*v).to_string());
        }
        Self { vars }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.vars.get(key).cloned()
    }

    pub fn get_any(&self, keys: &[&str]) -> Option<String> {
        for key in keys {
            if let Some(value) = self.vars.get(*key) {
                return Some(value.clone());
            }
        }
        None
    }
}

pub fn parse_bool(input: &str) -> Option<bool> {
    match input.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}
