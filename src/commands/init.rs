use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde_json::json;

use crate::cli::{CliArgs, InitArgs};
use crate::commands::common;
use crate::config::OutputFormat;
use crate::output::json as json_out;

pub fn run(args: &CliArgs, cmd: &InitArgs) -> Result<()> {
    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);

    let profile_name = cmd.profile.as_deref().unwrap_or("default");
    let target = resolve_target_path(cmd.path.as_ref())?;

    if target.exists() && !cmd.force {
        return Err(anyhow!("Config already exists: {}", target.display()));
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    let template = render_config_template(profile_name);
    fs::write(&target, template)?;

    if args.quiet {
        return Ok(());
    }

    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "path": target.display().to_string(),
            "created": true,
            "overwritten": cmd.force,
        });
        let body = json_out::emit_json_value(&payload, common::json_pretty(&resolved))?;
        println!("{}", body);
    } else {
        println!("Wrote config to {}", target.display());
    }

    Ok(())
}

fn resolve_target_path(path: Option<&PathBuf>) -> Result<PathBuf> {
    if let Some(path) = path {
        if path
            .extension()
            .and_then(|s| s.to_str())
            .map_or(false, |ext| matches!(ext, "yaml" | "yml" | "json"))
        {
            return Ok(path.clone());
        }
        if path.is_dir() {
            return Ok(path.join(".sql-server").join("config.yaml"));
        }
        return Ok(path.join(".sql-server").join("config.yaml"));
    }

    Ok(Path::new(".sql-server").join("config.yaml"))
}

fn render_config_template(profile: &str) -> String {
    format!(
        r#"# sscli configuration
# Defaults favor read-only access.

defaultProfile: {profile}
settings:
  allowWriteDefault: false
  output:
    # defaultFormat controls output when no explicit flag is used.
    # Values: pretty | markdown | json
    defaultFormat: pretty
    json:
      # contractVersion allows JSON shape upgrades while keeping defaults stable.
      # Values: v1
      contractVersion: v1
      # pretty controls indentation when emitting JSON.
      pretty: true
    csv:
      # multiResultNaming controls CSV file naming for multiple result sets.
      # Values: suffix-number | placeholder
      multiResultNaming: suffix-number

profiles:
  {profile}:
    server: localhost
    port: 1433
    database: master
    user: sa
    passwordEnv: SQL_PASSWORD
    password: null
    encrypt: true
    trustCert: true
    timeout: 30000
    defaultSchemas: [dbo]
    settings:
      allowWriteDefault: false
"#
    )
}
