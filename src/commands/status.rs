use std::time::Instant;

use anyhow::Result;
use serde_json::json;
use tiberius::Query;

use crate::cli::{CliArgs, StatusArgs};
use crate::commands::common;
use crate::config::OutputFormat;
use crate::db::client;
use crate::db::executor;
use crate::db::types::Value;
use crate::output::{json as json_out, table, TableOptions};

pub fn run(args: &CliArgs, _cmd: &StatusArgs) -> Result<()> {
    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);

    let started = Instant::now();
    let result_sets = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;
        let query = Query::new(
            "SELECT @@SERVERNAME AS serverName, @@VERSION AS serverVersion, DB_NAME() AS currentDatabase, CONVERT(varchar(33), SYSDATETIMEOFFSET(), 127) AS currentTime",
        );
        executor::run_query(query, &mut client).await
    })?;

    let latency_ms = started.elapsed().as_millis();
    let mut server_name = "unknown".to_string();
    let mut server_version = "unknown".to_string();
    let mut current_database = "unknown".to_string();
    let mut timestamp = "unknown".to_string();

    if let Some(rs) = result_sets.first() {
        if let Some(row) = rs.rows.first() {
            for (idx, col) in rs.columns.iter().enumerate() {
                let value = row.get(idx);
                match col.name.as_str() {
                    "serverName" => server_name = value_to_string(value),
                    "serverVersion" => server_version = value_to_string(value),
                    "currentDatabase" => current_database = value_to_string(value),
                    "currentTime" => timestamp = value_to_string(value),
                    _ => {}
                }
            }
        }
    }

    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "status": "ok",
            "latencyMs": latency_ms,
            "serverName": server_name,
            "serverVersion": server_version,
            "currentDatabase": current_database,
            "timestamp": timestamp,
            "warnings": Vec::<String>::new(),
        });
        let body = json_out::emit_json_value(&payload, common::json_pretty(&resolved))?;
        if !args.quiet {
            println!("{}", body);
        }
        return Ok(());
    }

    if args.quiet {
        return Ok(());
    }

    let rows = vec![
        ("Status".to_string(), "ok".to_string()),
        ("LatencyMs".to_string(), latency_ms.to_string()),
        ("Server".to_string(), server_name),
        ("Version".to_string(), server_version),
        ("CurrentDatabase".to_string(), current_database),
        ("Timestamp".to_string(), timestamp),
    ];

    let rendered = table::render_key_value_table("Status", &rows, format, &TableOptions::default());
    println!("{}", rendered);

    Ok(())
}

fn value_to_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::Null) | None => "unknown".to_string(),
        Some(v) => v.as_display(),
    }
}
