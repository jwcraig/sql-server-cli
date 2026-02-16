use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use tiberius::Query;
use tokio::net::TcpStream;
use tokio_util::compat::Compat;

use crate::config::ResolvedConfig;
use crate::db::executor;
use crate::db::types::Value;

const CACHE_FILE_NAME: &str = "object-index.json";
const CACHE_TTL_SECS: u64 = 300;

/// Object families supported by schema resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupScope {
    /// Resolve only base tables.
    TablesOnly,
    /// Resolve both base tables and views.
    TablesAndViews,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ObjectIndexEntry {
    schema: String,
    name: String,
    object_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ObjectIndexCache {
    profile: String,
    server: String,
    database: String,
    generated_at_unix: u64,
    entries: Vec<ObjectIndexEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObjectMatch {
    schema: String,
    name: String,
}

/// Resolve an object name to a specific schema and object pair.
///
/// # Arguments
///
/// * `client` - Connected SQL Server client.
/// * `resolved` - Resolved configuration for the active command invocation.
/// * `object_name` - Object name provided by the user (without schema).
/// * `schema_hint` - Optional explicit schema (from `--schema` or `schema.object` input).
/// * `scope` - Object kinds to include during lookup.
/// * `display_kind` - Human-readable object kind label used in errors/prompts.
/// * `allow_prompt` - Whether interactive disambiguation is allowed.
///
/// # Returns
///
/// A tuple of `(schema, object_name)`.
///
/// # Errors
///
/// Returns an error when no object matches, when multiple matches exist and no
/// choice can be made, or when catalog/cache operations fail.
pub async fn resolve_schema_for_object(
    client: &mut tiberius::Client<Compat<TcpStream>>,
    resolved: &ResolvedConfig,
    object_name: &str,
    schema_hint: Option<&str>,
    scope: LookupScope,
    display_kind: &str,
    allow_prompt: bool,
) -> Result<(String, String)> {
    if let Some(schema) = schema_hint {
        return Ok((schema.to_string(), object_name.to_string()));
    }

    let matches = find_object_matches(client, resolved, object_name, scope).await?;
    if matches.is_empty() {
        return Err(anyhow!(
            "{} '{}' not found",
            title_case(display_kind),
            object_name
        ));
    }

    if matches.len() == 1 {
        let first = &matches[0];
        return Ok((first.schema.clone(), first.name.clone()));
    }

    if allow_prompt && io::stdin().is_terminal() && io::stderr().is_terminal() {
        let selected = prompt_for_match(display_kind, object_name, &matches)?;
        return Ok((selected.schema, selected.name));
    }

    Err(anyhow!(
        "{}",
        ambiguous_object_error(display_kind, object_name, &matches)
    ))
}

async fn find_object_matches(
    client: &mut tiberius::Client<Compat<TcpStream>>,
    resolved: &ResolvedConfig,
    object_name: &str,
    scope: LookupScope,
) -> Result<Vec<ObjectMatch>> {
    let cache = read_cache(resolved)
        .filter(|cache| cache_matches_connection(cache, resolved) && cache_is_fresh(cache));

    let index = if let Some(cache) = cache {
        cache
    } else {
        refresh_cache(client, resolved).await?
    };

    let mut matches = filter_matches(&index.entries, object_name, scope);
    if matches.is_empty() {
        // Cache misses can occur on stale metadata; refresh once from the source of truth.
        let refreshed = refresh_cache(client, resolved).await?;
        matches = filter_matches(&refreshed.entries, object_name, scope);
    }

    Ok(matches)
}

fn filter_matches(
    entries: &[ObjectIndexEntry],
    object_name: &str,
    scope: LookupScope,
) -> Vec<ObjectMatch> {
    let mut matches: Vec<ObjectMatch> = entries
        .iter()
        .filter(|entry| entry.name.eq_ignore_ascii_case(object_name))
        .filter(|entry| match scope {
            LookupScope::TablesOnly => entry.object_type.eq_ignore_ascii_case("BASE TABLE"),
            LookupScope::TablesAndViews => {
                entry.object_type.eq_ignore_ascii_case("BASE TABLE")
                    || entry.object_type.eq_ignore_ascii_case("VIEW")
            }
        })
        .map(|entry| ObjectMatch {
            schema: entry.schema.clone(),
            name: entry.name.clone(),
        })
        .collect();

    matches.sort_by(|a, b| {
        a.schema
            .to_ascii_lowercase()
            .cmp(&b.schema.to_ascii_lowercase())
            .then(
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase()),
            )
    });
    matches
}

async fn refresh_cache(
    client: &mut tiberius::Client<Compat<TcpStream>>,
    resolved: &ResolvedConfig,
) -> Result<ObjectIndexCache> {
    let sql = r#"
SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE
FROM INFORMATION_SCHEMA.TABLES
WHERE TABLE_TYPE IN ('BASE TABLE', 'VIEW')
ORDER BY TABLE_SCHEMA, TABLE_NAME;
"#;

    let result_sets = executor::run_query(Query::new(sql), client).await?;
    let result_set = result_sets.into_iter().next().unwrap_or_default();

    let entries: Vec<ObjectIndexEntry> = result_set
        .rows
        .iter()
        .filter_map(|row| {
            let schema = match row.first() {
                Some(Value::Text(v)) => v.clone(),
                _ => return None,
            };
            let name = match row.get(1) {
                Some(Value::Text(v)) => v.clone(),
                _ => return None,
            };
            let object_type = match row.get(2) {
                Some(Value::Text(v)) => v.clone(),
                _ => return None,
            };

            Some(ObjectIndexEntry {
                schema,
                name,
                object_type,
            })
        })
        .collect();

    let cache = ObjectIndexCache {
        profile: resolved.profile_name.clone(),
        server: resolved.connection.server.clone(),
        database: resolved.connection.database.clone(),
        generated_at_unix: now_unix(),
        entries,
    };

    if let Some(path) = cache_path(resolved) {
        let _ = write_cache(&path, &cache);
    }

    Ok(cache)
}

fn read_cache(resolved: &ResolvedConfig) -> Option<ObjectIndexCache> {
    let path = cache_path(resolved)?;
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str::<ObjectIndexCache>(&content).ok()
}

fn write_cache(path: &Path, cache: &ObjectIndexCache) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create cache directory: {}", parent.display()))?;
    }
    let body =
        serde_json::to_string_pretty(cache).context("Failed to serialize object index cache")?;
    fs::write(path, body).with_context(|| format!("Failed to write cache file: {}", path.display()))
}

fn cache_matches_connection(cache: &ObjectIndexCache, resolved: &ResolvedConfig) -> bool {
    cache
        .server
        .eq_ignore_ascii_case(resolved.connection.server.as_str())
        && cache
            .database
            .eq_ignore_ascii_case(resolved.connection.database.as_str())
}

fn cache_is_fresh(cache: &ObjectIndexCache) -> bool {
    now_unix().saturating_sub(cache.generated_at_unix) <= CACHE_TTL_SECS
}

fn cache_path(resolved: &ResolvedConfig) -> Option<PathBuf> {
    let base = cache_base_dir(resolved)?;
    let profile = sanitize_profile_name(&resolved.profile_name);
    Some(base.join("profiles").join(profile).join(CACHE_FILE_NAME))
}

fn cache_base_dir(resolved: &ResolvedConfig) -> Option<PathBuf> {
    if let Some(config_path) = resolved.config_path.as_ref() {
        let parent = config_path.parent()?;
        if is_local_sql_server_dir(parent) {
            return Some(parent.to_path_buf());
        }
    }

    let cwd = std::env::current_dir().ok()?;
    Some(cwd.join(".sql-server"))
}

fn is_local_sql_server_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(|name| {
            name.eq_ignore_ascii_case(".sql-server") || name.eq_ignore_ascii_case(".sqlserver")
        })
        .unwrap_or(false)
}

fn sanitize_profile_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "default".to_string();
    }

    trimmed
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn prompt_for_match(
    display_kind: &str,
    object_name: &str,
    matches: &[ObjectMatch],
) -> Result<ObjectMatch> {
    eprintln!(
        "Multiple {}s named '{}' found:",
        display_kind.to_lowercase(),
        object_name
    );
    for (idx, matched) in matches.iter().enumerate() {
        eprintln!("  {}. {}.{}", idx + 1, matched.schema, matched.name);
    }
    eprint!(
        "Choose {} [1-{}] or 'q' to cancel: ",
        display_kind.to_lowercase(),
        matches.len()
    );
    io::stderr().flush().context("Failed to flush prompt")?;

    let mut selection = String::new();
    io::stdin()
        .read_line(&mut selection)
        .context("Failed to read selection")?;
    let selection = selection.trim();

    if selection.eq_ignore_ascii_case("q") || selection.is_empty() {
        return Err(anyhow!("{} selection canceled", title_case(display_kind)));
    }

    let idx = selection
        .parse::<usize>()
        .map_err(|_| anyhow!("Invalid selection '{}'", selection))?;
    if idx == 0 || idx > matches.len() {
        return Err(anyhow!(
            "Selection '{}' is out of range. Choose a value from 1 to {}.",
            selection,
            matches.len()
        ));
    }

    Ok(matches[idx - 1].clone())
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return "Object".to_string();
    };
    let mut out = String::new();
    out.push(first.to_ascii_uppercase());
    out.push_str(chars.as_str());
    out
}

fn ambiguous_object_error(
    display_kind: &str,
    object_name: &str,
    matches: &[ObjectMatch],
) -> String {
    let candidates = matches
        .iter()
        .map(|item| format!("{}.{}", item.schema, item.name))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Multiple {}s named '{}' found: {}. Re-run with --schema <name>.",
        display_kind.to_lowercase(),
        object_name,
        candidates
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::config::{ConnectionSettings, ResolvedConfig, SettingsResolved};

    use super::{LookupScope, ObjectMatch};
    use super::{
        ObjectIndexEntry, ambiguous_object_error, cache_path, filter_matches, sanitize_profile_name,
    };

    fn test_config(config_path: Option<PathBuf>, profile: &str) -> ResolvedConfig {
        ResolvedConfig {
            config_path,
            profile_name: profile.to_string(),
            connection: ConnectionSettings {
                server: "localhost".to_string(),
                port: 1433,
                database: "master".to_string(),
                user: None,
                password: None,
                encrypt: true,
                trust_cert: true,
                timeout_ms: 30_000,
                default_schemas: vec!["dbo".to_string()],
            },
            settings: SettingsResolved::default(),
        }
    }

    #[test]
    fn sanitize_profile_name_replaces_unsafe_characters() {
        let value = sanitize_profile_name("my profile/dev");
        assert_eq!(value, "my_profile_dev");
    }

    #[test]
    fn cache_path_uses_local_sql_server_profile_folder() {
        let config = test_config(
            Some(PathBuf::from("/tmp/project/.sql-server/config.yaml")),
            "local/dev",
        );
        let path = cache_path(&config).expect("cache path");
        assert_eq!(
            path,
            PathBuf::from("/tmp/project/.sql-server/profiles/local_dev/object-index.json")
        );
    }

    #[test]
    fn filter_matches_honors_scope_and_case() {
        let entries = vec![
            ObjectIndexEntry {
                schema: "dbo".to_string(),
                name: "Equipment".to_string(),
                object_type: "BASE TABLE".to_string(),
            },
            ObjectIndexEntry {
                schema: "reporting".to_string(),
                name: "equipment".to_string(),
                object_type: "VIEW".to_string(),
            },
        ];

        let tables_only = filter_matches(&entries, "equipment", LookupScope::TablesOnly);
        assert_eq!(
            tables_only,
            vec![ObjectMatch {
                schema: "dbo".to_string(),
                name: "Equipment".to_string(),
            }]
        );

        let all = filter_matches(&entries, "equipment", LookupScope::TablesAndViews);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn ambiguity_error_lists_candidates() {
        let matches = vec![
            ObjectMatch {
                schema: "dbo".to_string(),
                name: "equipment".to_string(),
            },
            ObjectMatch {
                schema: "plant_master".to_string(),
                name: "equipment".to_string(),
            },
        ];
        let message = ambiguous_object_error("table", "equipment", &matches);
        assert!(message.contains("dbo.equipment"));
        assert!(message.contains("plant_master.equipment"));
    }
}
