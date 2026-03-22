use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

#[derive(Debug, Clone, Default)]
pub struct Env {
    vars: HashMap<String, String>,
}

impl Env {
    /// Load environment variables from the process, optionally loading an explicit env file first.
    pub fn from_system(env_file: Option<&Path>) -> Result<Self> {
        if let Some(path) = env_file {
            dotenvy::from_path_override(path)
                .with_context(|| format!("Failed to load env file: {}", path.display()))?;
        }

        let vars = std::env::vars().collect();
        Ok(Self { vars })
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
