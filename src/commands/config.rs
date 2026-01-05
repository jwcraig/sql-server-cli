use std::io::{self, Write};

use anyhow::Result;

use crate::cli::CliArgs;
use crate::commands::common;
use crate::config;
use crate::output::{self, json, table, TableOptions};

pub fn run(args: &CliArgs) -> Result<()> {
    let resolved = common::load_config(args)?;
    let format = output::select_format(&args.output, &resolved.settings);

    if args.quiet {
        return Ok(());
    }

    match format {
        config::OutputFormat::Json => {
            let payload = json::config_to_json(&resolved);
            let body = json::emit_json_value(&payload, resolved.settings.output.json.pretty)?;
            println!("{}", body);
        }
        _ => {
            let mut rows = vec![
                (
                    "configPath".to_string(),
                    resolved
                        .config_path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "(none)".to_string()),
                ),
                ("profileName".to_string(), resolved.profile_name.clone()),
                ("server".to_string(), resolved.connection.server.clone()),
                ("port".to_string(), resolved.connection.port.to_string()),
                ("database".to_string(), resolved.connection.database.clone()),
            ];
            if let Some(user) = &resolved.connection.user {
                rows.push(("user".to_string(), user.clone()));
            }
            rows.extend([
                (
                    "encrypt".to_string(),
                    resolved.connection.encrypt.to_string(),
                ),
                (
                    "trustCert".to_string(),
                    resolved.connection.trust_cert.to_string(),
                ),
                (
                    "timeoutMs".to_string(),
                    resolved.connection.timeout_ms.to_string(),
                ),
                (
                    "defaultSchemas".to_string(),
                    resolved.connection.default_schemas.join(","),
                ),
            ]);

            let rendered =
                table::render_key_value_table("Config", &rows, format, &TableOptions::default());
            writeln!(io::stdout(), "{}", rendered)?;
        }
    }

    Ok(())
}
