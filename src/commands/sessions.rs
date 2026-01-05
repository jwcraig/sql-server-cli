use anyhow::Result;
use serde_json::json;
use tiberius::Query;

use crate::cli::{CliArgs, SessionsArgs};
use crate::commands::common;
use crate::config::OutputFormat;
use crate::db::client;
use crate::db::executor;
use crate::output::{json as json_out, table, TableOptions};

const LIMIT_DEFAULT: u64 = 20;
const LIMIT_MAX: u64 = 200;

pub fn run(args: &CliArgs, cmd: &SessionsArgs) -> Result<()> {
    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);

    let limit = common::parse_limit(cmd.limit, LIMIT_DEFAULT, LIMIT_MAX);
    let database = cmd.database.clone();
    let login = cmd.login.clone();
    let host = cmd.host.clone();
    let status = cmd.status.clone();

    let result_set = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;
        let sql = r#"
SELECT TOP (@P5)
    s.session_id AS sessionId,
    s.login_name AS loginName,
    s.host_name AS hostName,
    s.program_name AS programName,
    s.status AS sessionStatus,
    DB_NAME(s.database_id) AS databaseName,
    r.command AS command,
    r.status AS requestStatus,
    r.cpu_time AS cpuTime,
    r.total_elapsed_time AS elapsedTime,
    r.wait_type AS waitType,
    r.blocking_session_id AS blockingSessionId
FROM sys.dm_exec_sessions s
LEFT JOIN sys.dm_exec_requests r ON s.session_id = r.session_id
WHERE s.is_user_process = 1
  AND (@P1 IS NULL OR DB_NAME(s.database_id) = @P1)
  AND (@P2 IS NULL OR s.login_name = @P2)
  AND (@P3 IS NULL OR s.host_name = @P3)
  AND (@P4 IS NULL OR s.status = @P4)
ORDER BY r.total_elapsed_time DESC, s.session_id;
"#;
        let mut query = Query::new(sql);
        query.bind(database.as_deref());
        query.bind(login.as_deref());
        query.bind(host.as_deref());
        query.bind(status.as_deref());
        query.bind(limit as i64);
        let result_sets = executor::run_query(query, &mut client).await?;
        Ok::<_, anyhow::Error>(result_sets.into_iter().next().unwrap_or_default())
    })?;

    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "filters": {
                "database": database,
                "login": login,
                "host": host,
                "status": status,
                "limit": limit,
            },
            "count": result_set.rows.len(),
            "sessions": json_out::result_set_rows_to_objects(&result_set),
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
