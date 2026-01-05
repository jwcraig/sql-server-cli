use anyhow::Result;
use serde_json::json;
use tiberius::Query;

use crate::cli::{BackupsArgs, CliArgs};
use crate::commands::common;
use crate::config::OutputFormat;
use crate::db::client;
use crate::db::executor;
use crate::output::{TableOptions, json as json_out, table};

const LIMIT_DEFAULT: u64 = 20;
const LIMIT_MAX: u64 = 200;

pub fn run(args: &CliArgs, cmd: &BackupsArgs) -> Result<()> {
    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);

    let limit = common::parse_limit(cmd.limit, LIMIT_DEFAULT, LIMIT_MAX);
    let since_days = cmd.since.unwrap_or(7);
    let backup_type = cmd.backup_type.clone().unwrap_or_else(|| "all".to_string());
    let backup_type = backup_type.to_lowercase();

    let (type_d, type_i, type_l) = match backup_type.as_str() {
        "full" => (Some("D"), None, None),
        "diff" => (None, Some("I"), None),
        "log" => (None, None, Some("L")),
        _ => (Some("D"), Some("I"), Some("L")),
    };

    let database = cmd.database.clone();

    let result_set = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;
        let sql = r#"
SELECT TOP (@P1)
    bs.database_name AS databaseName,
    bs.backup_start_date AS backupStart,
    bs.backup_finish_date AS backupFinish,
    CASE bs.type
        WHEN 'D' THEN 'FULL'
        WHEN 'I' THEN 'DIFF'
        WHEN 'L' THEN 'LOG'
        ELSE bs.type
    END AS backupType,
    bs.backup_size AS backupSize,
    bmf.physical_device_name AS device
FROM msdb.dbo.backupset bs
LEFT JOIN msdb.dbo.backupmediafamily bmf ON bs.media_set_id = bmf.media_set_id
WHERE (@P2 IS NULL OR bs.database_name = @P2)
  AND bs.backup_start_date >= DATEADD(day, -@P3, SYSUTCDATETIME())
  AND ((@P4 IS NOT NULL AND bs.type = 'D')
    OR (@P5 IS NOT NULL AND bs.type = 'I')
    OR (@P6 IS NOT NULL AND bs.type = 'L'))
ORDER BY bs.backup_start_date DESC;
"#;
        let mut query = Query::new(sql);
        query.bind(limit as i64);
        query.bind(database.as_deref());
        query.bind(since_days as i64);
        query.bind(type_d);
        query.bind(type_i);
        query.bind(type_l);
        let result_sets = executor::run_query(query, &mut client).await?;
        Ok::<_, anyhow::Error>(result_sets.into_iter().next().unwrap_or_default())
    })?;

    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "database": database,
            "sinceDays": since_days,
            "type": backup_type,
            "backups": json_out::result_set_rows_to_objects(&result_set),
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

    let rendered = table::render_result_set_table(&result_set, format, &TableOptions::default());
    println!("{}", rendered);

    Ok(())
}
