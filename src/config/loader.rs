use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use super::env::{parse_bool, Env};
use super::schema::{
    ConfigFile, CsvMultiResultNaming, JsonContractVersion, OutputFormat, OutputSettings, Profile,
    Settings,
};

#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
    pub config_path: Option<PathBuf>,
    pub profile: Option<String>,
    pub server: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub timeout_ms: Option<u64>,
    pub encrypt: Option<bool>,
    pub trust_cert: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct LoadOptions {
    pub cli: CliOverrides,
    pub cwd: PathBuf,
    pub home_dir: Option<PathBuf>,
    pub xdg_config_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub config_path: Option<PathBuf>,
    pub profile_name: String,
    pub connection: ConnectionSettings,
    pub settings: SettingsResolved,
}

#[derive(Debug, Clone)]
pub struct ConnectionSettings {
    pub server: String,
    pub port: u16,
    pub database: String,
    pub user: Option<String>,
    pub password: Option<String>,
    pub encrypt: bool,
    pub trust_cert: bool,
    pub timeout_ms: u64,
    pub default_schemas: Vec<String>,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            server: "localhost".to_string(),
            port: 1433,
            database: "master".to_string(),
            user: None,
            password: None,
            encrypt: true,
            trust_cert: true,
            timeout_ms: 30_000,
            default_schemas: vec!["dbo".to_string()],
        }
    }
}

#[derive(Debug, Clone)]
pub struct SettingsResolved {
    pub allow_write_default: bool,
    pub output: OutputSettingsResolved,
}

#[derive(Debug, Clone)]
pub struct OutputSettingsResolved {
    pub default_format: OutputFormat,
    pub json: JsonSettingsResolved,
    pub csv: CsvSettingsResolved,
}

#[derive(Debug, Clone)]
pub struct JsonSettingsResolved {
    pub contract_version: JsonContractVersion,
    pub pretty: bool,
}

#[derive(Debug, Clone)]
pub struct CsvSettingsResolved {
    pub multi_result_naming: CsvMultiResultNaming,
}

impl Default for SettingsResolved {
    fn default() -> Self {
        Self {
            allow_write_default: false,
            output: OutputSettingsResolved {
                default_format: OutputFormat::Pretty,
                json: JsonSettingsResolved {
                    contract_version: JsonContractVersion::V1,
                    pretty: true,
                },
                csv: CsvSettingsResolved {
                    multi_result_naming: CsvMultiResultNaming::SuffixNumber,
                },
            },
        }
    }
}

pub fn load_config(options: &LoadOptions, env: &Env) -> Result<ResolvedConfig> {
    let config_path = resolve_config_path(options, env)?;
    let config_file = match &config_path {
        Some(path) => load_config_file(path)?,
        None => ConfigFile::default(),
    };

    let profile_name = resolve_profile_name(options, env, config_file.default_profile.as_deref());

    let mut connection = ConnectionSettings::default();
    let mut settings = SettingsResolved::default();

    if let Some(settings_cfg) = &config_file.settings {
        apply_settings(&mut settings, settings_cfg);
    }

    if let Some(profile) = config_file.profiles.get(&profile_name) {
        apply_profile(&mut connection, &mut settings, profile, env);
    }

    apply_env_overrides(&mut connection, &mut settings, env);
    apply_cli_overrides(&mut connection, &mut settings, &options.cli);

    Ok(ResolvedConfig {
        config_path,
        profile_name,
        connection,
        settings,
    })
}

fn resolve_profile_name(options: &LoadOptions, env: &Env, default_profile: Option<&str>) -> String {
    if let Some(profile) = options.cli.profile.as_deref() {
        return profile.to_string();
    }
    if let Some(profile) = env.get_any(&["SQL_SERVER_PROFILE", "SQLSERVER_PROFILE"]) {
        return profile;
    }
    if let Some(profile) = default_profile {
        return profile.to_string();
    }
    "default".to_string()
}

fn resolve_config_path(options: &LoadOptions, env: &Env) -> Result<Option<PathBuf>> {
    if let Some(path) = &options.cli.config_path {
        if !path.exists() {
            return Err(anyhow!("Config file not found: {}", path.display()));
        }
        return Ok(Some(path.clone()));
    }

    if let Some(path) = env.get_any(&["SQL_SERVER_CONFIG", "SQLSERVER_CONFIG"]) {
        let path = PathBuf::from(path);
        if !path.exists() {
            return Err(anyhow!("Config file not found: {}", path.display()));
        }
        return Ok(Some(path));
    }

    if let Some(path) = find_local_config(&options.cwd, options.home_dir.as_deref()) {
        return Ok(Some(path));
    }

    if let Some(path) = find_global_config(options.xdg_config_dir.as_deref()) {
        return Ok(Some(path));
    }

    Ok(None)
}

fn find_local_config(start: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let candidates = [
        ".sql-server/config.yaml",
        ".sql-server/config.yml",
        ".sql-server/config.json",
        ".sqlserver/config.yaml",
        ".sqlserver/config.yml",
        ".sqlserver/config.json",
    ];

    for dir in start.ancestors() {
        for candidate in &candidates {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }

        if let Some(home_dir) = home {
            if dir == home_dir {
                break;
            }
        }
    }

    None
}

fn find_global_config(xdg_config: Option<&Path>) -> Option<PathBuf> {
    let base = xdg_config?;
    let candidates = [
        "sql-server/config.yaml",
        "sql-server/config.yml",
        "sql-server/config.json",
    ];

    for candidate in &candidates {
        let path = base.join(candidate);
        if path.is_file() {
            return Some(path);
        }
    }

    None
}

fn load_config_file(path: &Path) -> Result<ConfigFile> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("yaml") | Some("yml") => {
            serde_yaml::from_str(&content).context("Failed to parse YAML config")
        }
        Some("json") => serde_json::from_str(&content).context("Failed to parse JSON config"),
        _ => Err(anyhow!("Unsupported config file extension")),
    }
}

fn apply_profile(
    connection: &mut ConnectionSettings,
    settings: &mut SettingsResolved,
    profile: &Profile,
    env: &Env,
) {
    if let Some(server) = &profile.server {
        connection.server = server.clone();
    }
    if let Some(port) = profile.port {
        connection.port = port;
    }
    if let Some(database) = &profile.database {
        connection.database = database.clone();
    }
    if let Some(user) = &profile.user {
        connection.user = Some(user.clone());
    }
    if let Some(password) = &profile.password {
        connection.password = Some(password.clone());
    } else if let Some(env_key) = &profile.password_env {
        if let Some(value) = env.get(env_key) {
            connection.password = Some(value);
        }
    }
    if let Some(encrypt) = profile.encrypt {
        connection.encrypt = encrypt;
    }
    if let Some(trust_cert) = profile.trust_cert {
        connection.trust_cert = trust_cert;
    }
    if let Some(timeout) = profile.timeout {
        connection.timeout_ms = timeout;
    }
    if let Some(default_schemas) = &profile.default_schemas {
        connection.default_schemas = default_schemas.clone();
    }

    if let Some(settings_profile) = &profile.settings {
        apply_settings(settings, settings_profile);
    }
}

fn apply_settings(settings: &mut SettingsResolved, overrides: &Settings) {
    if let Some(allow_write_default) = overrides.allow_write_default {
        settings.allow_write_default = allow_write_default;
    }
    if let Some(output) = &overrides.output {
        apply_output_settings(&mut settings.output, output);
    }
}

fn apply_output_settings(settings: &mut OutputSettingsResolved, overrides: &OutputSettings) {
    if let Some(default_format) = overrides.default_format {
        settings.default_format = default_format;
    }
    if let Some(json) = &overrides.json {
        if let Some(contract_version) = json.contract_version {
            settings.json.contract_version = contract_version;
        }
        if let Some(pretty) = json.pretty {
            settings.json.pretty = pretty;
        }
    }
    if let Some(csv) = &overrides.csv {
        if let Some(multi_result_naming) = csv.multi_result_naming {
            settings.csv.multi_result_naming = multi_result_naming;
        }
    }
}

fn apply_env_overrides(
    connection: &mut ConnectionSettings,
    _settings: &mut SettingsResolved,
    env: &Env,
) {
    if let Some(url) = env.get_any(&["DATABASE_URL", "DB_URL", "SQLSERVER_URL"]) {
        if let Ok(parsed) = parse_connection_url(&url) {
            if let Some(server) = parsed.server {
                connection.server = server;
            }
            if let Some(port) = parsed.port {
                connection.port = port;
            }
            if let Some(database) = parsed.database {
                connection.database = database;
            }
            if let Some(user) = parsed.user {
                connection.user = Some(user);
            }
            if let Some(password) = parsed.password {
                connection.password = Some(password);
            }
        }
    }

    if let Some(server) = env.get_any(&["SQL_SERVER", "SQLSERVER_HOST", "DB_HOST"]) {
        connection.server = server;
    }
    if let Some(port) = env.get_any(&["SQL_PORT", "SQLSERVER_PORT", "DB_PORT"]) {
        if let Ok(port) = port.parse::<u16>() {
            connection.port = port;
        }
    }
    if let Some(database) = env.get_any(&["SQL_DATABASE", "SQLSERVER_DB", "DATABASE", "DB_NAME"]) {
        connection.database = database;
    }
    if let Some(user) = env.get_any(&["SQL_USER", "SQLSERVER_USER", "DB_USER"]) {
        connection.user = Some(user);
    }
    if let Some(password) = env.get_any(&["SQL_PASSWORD", "SQLSERVER_PASSWORD", "DB_PASSWORD"]) {
        connection.password = Some(password);
    }
    if let Some(encrypt) = env.get("SQL_ENCRYPT").and_then(|v| parse_bool(&v)) {
        connection.encrypt = encrypt;
    }
    if let Some(trust_cert) = env
        .get("SQL_TRUST_SERVER_CERTIFICATE")
        .and_then(|v| parse_bool(&v))
    {
        connection.trust_cert = trust_cert;
    }
    if let Some(timeout) = env.get_any(&["SQL_CONNECT_TIMEOUT", "DB_CONNECT_TIMEOUT"]) {
        if let Ok(timeout) = timeout.parse::<u64>() {
            connection.timeout_ms = timeout;
        }
    }
}

fn apply_cli_overrides(
    connection: &mut ConnectionSettings,
    _settings: &mut SettingsResolved,
    cli: &CliOverrides,
) {
    if let Some(server) = &cli.server {
        connection.server = server.clone();
    }
    if let Some(port) = cli.port {
        connection.port = port;
    }
    if let Some(database) = &cli.database {
        connection.database = database.clone();
    }
    if let Some(user) = &cli.user {
        connection.user = Some(user.clone());
    }
    if let Some(password) = &cli.password {
        connection.password = Some(password.clone());
    }
    if let Some(timeout_ms) = cli.timeout_ms {
        connection.timeout_ms = timeout_ms;
    }
    if let Some(encrypt) = cli.encrypt {
        connection.encrypt = encrypt;
    }
    if let Some(trust_cert) = cli.trust_cert {
        connection.trust_cert = trust_cert;
    }
}

#[derive(Debug, Default)]
struct ParsedUrl {
    server: Option<String>,
    port: Option<u16>,
    database: Option<String>,
    user: Option<String>,
    password: Option<String>,
}

fn parse_connection_url(input: &str) -> Result<ParsedUrl> {
    let mut remaining = input.trim();
    if let Some(idx) = remaining.find("://") {
        remaining = &remaining[idx + 3..];
    }

    let mut auth_part = None;
    let mut host_part = remaining;
    if let Some(idx) = remaining.find('@') {
        auth_part = Some(&remaining[..idx]);
        host_part = &remaining[idx + 1..];
    }

    let mut host_port = host_part;
    let mut path_part = None;
    if let Some(idx) = host_part.find('/') {
        host_port = &host_part[..idx];
        path_part = Some(&host_part[idx + 1..]);
    }

    let mut parsed = ParsedUrl::default();

    if let Some(auth) = auth_part {
        let mut parts = auth.splitn(2, ':');
        let user = parts.next().unwrap_or("");
        if !user.is_empty() {
            parsed.user = Some(user.to_string());
        }
        if let Some(pass) = parts.next() {
            if !pass.is_empty() {
                parsed.password = Some(pass.to_string());
            }
        }
    }

    if !host_port.is_empty() {
        let mut parts = host_port.splitn(2, ':');
        let host = parts.next().unwrap_or("");
        if !host.is_empty() {
            parsed.server = Some(host.to_string());
        }
        if let Some(port) = parts.next() {
            if let Ok(port) = port.parse::<u16>() {
                parsed.port = Some(port);
            }
        }
    }

    if let Some(path) = path_part {
        let db = path.split('?').next().unwrap_or("");
        if !db.is_empty() {
            parsed.database = Some(db.to_string());
        }
    }

    if parsed.server.is_none() && parsed.database.is_none() && parsed.user.is_none() {
        return Err(anyhow!("Invalid connection URL"));
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let mut dir = env::temp_dir();
        dir.push(format!("sscli-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn parses_connection_url() {
        let parsed =
            parse_connection_url("sqlserver://user:pass@localhost:1433/db").expect("parse");
        assert_eq!(parsed.server.as_deref(), Some("localhost"));
        assert_eq!(parsed.port, Some(1433));
        assert_eq!(parsed.database.as_deref(), Some("db"));
        assert_eq!(parsed.user.as_deref(), Some("user"));
        assert_eq!(parsed.password.as_deref(), Some("pass"));
    }

    #[test]
    fn loads_config_from_cli_path() {
        let dir = temp_dir("config");
        let config_path = dir.join("config.yaml");
        fs::write(
            &config_path,
            "defaultProfile: test\nprofiles:\n  test:\n    server: example\n",
        )
        .expect("write config");

        let options = LoadOptions {
            cli: CliOverrides {
                config_path: Some(config_path.clone()),
                ..CliOverrides::default()
            },
            cwd: dir.clone(),
            home_dir: None,
            xdg_config_dir: None,
        };
        let env = Env::from_pairs(&[]);
        let resolved = load_config(&options, &env).expect("load config");
        assert_eq!(resolved.connection.server, "example");
    }

    #[test]
    fn env_overrides_config_profile() {
        let dir = temp_dir("env-override");
        let config_path = dir.join("config.yml");
        fs::write(
            &config_path,
            "defaultProfile: test\nprofiles:\n  test:\n    server: config-host\n",
        )
        .expect("write config");

        let options = LoadOptions {
            cli: CliOverrides {
                config_path: Some(config_path),
                ..CliOverrides::default()
            },
            cwd: dir,
            home_dir: None,
            xdg_config_dir: None,
        };
        let env = Env::from_pairs(&[("SQL_SERVER", "env-host")]);
        let resolved = load_config(&options, &env).expect("load config");
        assert_eq!(resolved.connection.server, "env-host");
    }

    #[test]
    fn profile_password_env_is_used() {
        let dir = temp_dir("password-env");
        let config_path = dir.join("config.yml");
        fs::write(
            &config_path,
            "defaultProfile: test\nprofiles:\n  test:\n    passwordEnv: TEST_DB_PASS\n",
        )
        .expect("write config");

        let options = LoadOptions {
            cli: CliOverrides {
                config_path: Some(config_path),
                ..CliOverrides::default()
            },
            cwd: dir,
            home_dir: None,
            xdg_config_dir: None,
        };
        let env = Env::from_pairs(&[("TEST_DB_PASS", "secret")]);
        let resolved = load_config(&options, &env).expect("load config");
        assert_eq!(resolved.connection.password.as_deref(), Some("secret"));
    }

    #[test]
    fn default_profile_used_when_missing() {
        let options = LoadOptions {
            cli: CliOverrides::default(),
            cwd: env::current_dir().expect("cwd"),
            home_dir: None,
            xdg_config_dir: None,
        };
        let env = Env::from_pairs(&[]);
        let resolved = load_config(&options, &env).expect("load config");
        assert_eq!(resolved.profile_name, "default");
    }
}
