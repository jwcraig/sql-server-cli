use anyhow::Result;

use crate::cli::CliArgs;
use crate::config::OutputFormat;
use crate::config::{self, CliOverrides, ResolvedConfig};
use crate::error::{AppError, ErrorKind};
use crate::output;

pub fn overrides_from_args(args: &CliArgs) -> CliOverrides {
    CliOverrides {
        config_path: args.config_path.clone(),
        env_file: args.env_file.clone(),
        profile: args.profile.clone(),
        server: args.server.clone(),
        port: args.port,
        database: args.database.clone(),
        user: args.user.clone(),
        password: args.password.clone(),
        password_env: args.password_env.clone(),
        timeout_ms: args.timeout_ms,
        encrypt: args.encrypt,
        trust_cert: args.trust_cert,
    }
}

/// Resolve the connection and settings for a command.
///
/// Warns on stderr when nothing named a server, because the built-in
/// `localhost:1433/master` default is rarely the intended target on a host
/// without a config file.
///
/// # Errors
///
/// Returns a `Config` error when the config file is unreadable or a named
/// profile does not exist.
pub fn load_config(args: &CliArgs) -> Result<ResolvedConfig> {
    let overrides = overrides_from_args(args);
    let resolved = config::load_from_system(&overrides)
        .map_err(|err| AppError::new(ErrorKind::Config, err.to_string()))?;
    if resolved.uses_builtin_target && !args.quiet && !args.quiet_target {
        eprintln!(
            "Warning: no config file, --server or SQL_SERVER/SSCLI_URL found; using the \
             built-in default localhost:1433/master. Pass --profile or --server, or create \
             .sql-server/config.yaml."
        );
    }
    Ok(resolved)
}

/// Print the `Target:` banner that SQL-executing commands show on stderr.
///
/// The banner names the profile and config file whenever one was used, so a
/// config picked up from the working directory is visible before SQL runs.
pub fn print_target_banner(args: &CliArgs, resolved: &ResolvedConfig) {
    if args.quiet || args.quiet_target {
        return;
    }
    let connection = &resolved.connection;
    match &resolved.config_path {
        Some(path) => eprintln!(
            "Target: {}:{}/{} (profile {}, {})",
            connection.server,
            connection.port,
            connection.database,
            resolved.profile_name,
            path.display()
        ),
        None => eprintln!(
            "Target: {}:{}/{}",
            connection.server, connection.port, connection.database
        ),
    }
}

pub fn output_format(args: &CliArgs, resolved: &ResolvedConfig) -> OutputFormat {
    output::select_format(&args.output, &resolved.settings)
}

pub fn json_pretty(resolved: &ResolvedConfig) -> bool {
    resolved.settings.output.json.pretty
}

pub fn parse_limit(value: Option<u64>, default: u64, max: u64) -> u64 {
    match value {
        Some(v) if v < 1 => default,
        Some(v) if v > max => max,
        Some(v) => v,
        None => default,
    }
}

pub fn parse_offset(value: Option<u64>) -> u64 {
    value.unwrap_or(0)
}

/// Normalize object identifiers supplied via CLI.
/// Accepts forms like `[schema].[name]`, `schema.name`, or just `name`.
/// Returns (object_name, schema_opt).
pub fn normalize_object_input(input: &str) -> (String, Option<String>) {
    let cleaned = input.replace(['[', ']'], "");
    if let Some((schema, object)) = cleaned.split_once('.') {
        (object.to_string(), Some(schema.to_string()))
    } else {
        (cleaned.to_string(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_object_input;

    #[test]
    fn strips_brackets_and_extracts_schema() {
        let (name, schema) = normalize_object_input("[web].[table]");
        assert_eq!(name, "table");
        assert_eq!(schema.as_deref(), Some("web"));
    }

    #[test]
    fn handles_dotted_without_brackets() {
        let (name, schema) = normalize_object_input("web.table");
        assert_eq!(name, "table");
        assert_eq!(schema.as_deref(), Some("web"));
    }

    #[test]
    fn returns_name_only_when_no_schema() {
        let (name, schema) = normalize_object_input("table");
        assert_eq!(name, "table");
        assert!(schema.is_none());
    }
}
