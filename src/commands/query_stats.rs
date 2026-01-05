use anyhow::Result;
use serde_json::json;
use tiberius::Query;

use crate::cli::{CliArgs, QueryStatsArgs};
use crate::commands::common;
use crate::config::OutputFormat;
use crate::db::client;
use crate::db::executor;
use crate::output::{json as json_out, table, TableOptions};

const LIMIT_DEFAULT: u64 = 10;
const LIMIT_MAX: u64 = 100;

pub fn run(args: &CliArgs, cmd: &QueryStatsArgs) -> Result<()> {
    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);

    let limit = common::parse_limit(cmd.limit, LIMIT_DEFAULT, LIMIT_MAX);
    let database = cmd.database.clone();
    let order_key = cmd.order.clone().unwrap_or_else(|| "cpu".to_string());
    let order_key = order_key.to_lowercase();

    let order_column = match order_key.as_str() {
        "duration" => "qs.total_elapsed_time",
        "reads" => "qs.total_logical_reads",
        "executions" => "qs.execution_count",
        _ => "qs.total_worker_time",
    };

    let sql = format!(
        "\
SELECT TOP (@P2)
    DB_NAME(st.dbid) AS databaseName,
    qs.total_worker_time AS totalWorkerTime,
    qs.total_elapsed_time AS totalElapsedTime,
    qs.total_logical_reads AS totalLogicalReads,
    qs.total_logical_writes AS totalLogicalWrites,
    qs.execution_count AS executionCount,
    qs.creation_time AS creationTime,
    qs.last_execution_time AS lastExecutionTime,
    SUBSTRING(
        st.text,
        (qs.statement_start_offset/2) + 1,
        ((CASE qs.statement_end_offset WHEN -1 THEN DATALENGTH(st.text) ELSE qs.statement_end_offset END - qs.statement_start_offset)/2) + 1
    ) AS sqlText
FROM sys.dm_exec_query_stats qs
CROSS APPLY sys.dm_exec_sql_text(qs.sql_handle) st
WHERE (@P1 IS NULL OR DB_NAME(st.dbid) = @P1)
ORDER BY {} DESC;\
",
        order_column
    );

    let result_set = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;
        let mut query = Query::new(sql);
        query.bind(database.as_deref());
        query.bind(limit as i64);
        let result_sets = executor::run_query(query, &mut client).await?;
        Ok::<_, anyhow::Error>(result_sets.into_iter().next().unwrap_or_default())
    })?;

    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "order": order_key,
            "database": database,
            "queries": json_out::result_set_rows_to_objects(&result_set),
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
