use anyhow::Result;

use crate::cli::CliArgs;
use crate::config::OutputFormat;
use crate::config::{self, CliOverrides, ResolvedConfig};
use crate::error::{AppError, ErrorKind};
use crate::output;

pub fn overrides_from_args(args: &CliArgs) -> CliOverrides {
    CliOverrides {
        config_path: args.config_path.clone(),
        profile: args.profile.clone(),
        server: args.server.clone(),
        port: args.port,
        database: args.database.clone(),
        user: args.user.clone(),
        password: args.password.clone(),
        timeout_ms: args.timeout_ms,
        encrypt: args.encrypt,
        trust_cert: args.trust_cert,
    }
}

pub fn load_config(args: &CliArgs) -> Result<ResolvedConfig> {
    let overrides = overrides_from_args(args);
    config::load_from_system(&overrides)
        .map_err(|err| AppError::new(ErrorKind::Config, err.to_string()).into())
}

pub fn output_format(args: &CliArgs, resolved: &ResolvedConfig) -> OutputFormat {
    output::select_format(&args.output, &resolved.settings)
}

pub fn json_pretty(resolved: &ResolvedConfig) -> bool {
    resolved.settings.output.json.pretty
}

pub fn allow_write(args: &CliArgs, resolved: &ResolvedConfig) -> bool {
    args.allow_write || resolved.settings.allow_write_default
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
