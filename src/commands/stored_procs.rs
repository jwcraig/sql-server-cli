use anyhow::Result;
use serde_json::json;
use tiberius::Query;

use crate::cli::{CliArgs, StoredProcsArgs};
use crate::commands::{common, paging};
use crate::config::OutputFormat;
use crate::db::client;
use crate::db::executor;
use crate::db::types::{Column, ResultSet, Value};
use crate::error::{AppError, ErrorKind};
use crate::output::{json as json_out, table, TableOptions};
use crate::safety;

const LIMIT_DEFAULT: u64 = 10;
const LIMIT_MAX: u64 = 100;

pub fn run(args: &CliArgs, cmd: &StoredProcsArgs) -> Result<()> {
    if let Some(proc_name) = cmd.exec.as_deref() {
        return exec_proc(args, proc_name, cmd.args.as_deref());
    }

    list_procs(args, cmd)
}

fn list_procs(args: &CliArgs, cmd: &StoredProcsArgs) -> Result<()> {
    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);

    let limit = common::parse_limit(cmd.limit, LIMIT_DEFAULT, LIMIT_MAX);
    let offset = common::parse_offset(cmd.offset);

    let include_system = cmd.include_system;
    let schema = cmd.schema.clone();
    let name = cmd.name.clone();

    let (rows, total) = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;
        let list_sql = r#"
WITH filtered AS (
    SELECT
        s.name AS schemaName,
        p.name AS procName,
        p.is_ms_shipped AS isSystem,
        p.modify_date AS modifiedAt,
        ROW_NUMBER() OVER (ORDER BY s.name, p.name) AS rownum
    FROM sys.procedures p
    INNER JOIN sys.schemas s ON p.schema_id = s.schema_id
    WHERE (@P1 IS NULL OR s.name = @P1)
      AND (@P2 IS NULL OR p.name LIKE @P2)
      AND (@P3 = 1 OR p.is_ms_shipped = 0)
)
SELECT schemaName AS [schema],
       procName AS name,
       isSystem AS isSystem,
       modifiedAt AS modifiedAt
FROM filtered
WHERE rownum BETWEEN (@P4 + 1) AND (@P4 + @P5)
ORDER BY schemaName, procName;
"#;

        let mut list_query = Query::new(list_sql);
        list_query.bind(schema.as_deref());
        list_query.bind(name.as_deref());
        list_query.bind(if include_system { 1i32 } else { 0i32 });
        list_query.bind(offset as i64);
        list_query.bind(limit as i64);
        let list_sets = executor::run_query(list_query, &mut client).await?;
        let list_set = list_sets.into_iter().next().unwrap_or_default();

        let count_sql = r#"
SELECT COUNT(*) AS total
FROM sys.procedures p
INNER JOIN sys.schemas s ON p.schema_id = s.schema_id
WHERE (@P1 IS NULL OR s.name = @P1)
  AND (@P2 IS NULL OR p.name LIKE @P2)
  AND (@P3 = 1 OR p.is_ms_shipped = 0);
"#;
        let mut count_query = Query::new(count_sql);
        count_query.bind(schema.as_deref());
        count_query.bind(name.as_deref());
        count_query.bind(if include_system { 1i32 } else { 0i32 });
        let count_sets = executor::run_query(count_query, &mut client).await?;
        let total = count_sets
            .first()
            .and_then(|rs| rs.rows.first())
            .and_then(|row| row.first())
            .and_then(|value| match value {
                Value::Int(v) => (*v).try_into().ok(),
                Value::Float(v) => Some(*v as u64),
                Value::Text(s) => s.parse::<u64>().ok(),
                _ => None,
            })
            .unwrap_or(0);

        Ok::<_, anyhow::Error>((list_set, total))
    })?;

    let count = rows.rows.len() as u64;
    let paging = paging::build_paging(total, count, offset, limit);

    let allowed = safety::allowed_procedures();
    let mut enriched_rows = Vec::new();
    for row in rows.rows {
        let name = value_to_string(row.get(1));
        let is_allowed = allowed
            .iter()
            .any(|proc_name| proc_name.eq_ignore_ascii_case(&name));
        enriched_rows.push(vec![
            row.get(0).cloned().unwrap_or(Value::Null),
            Value::Text(name),
            row.get(2).cloned().unwrap_or(Value::Null),
            Value::Text(if is_allowed { "yes" } else { "no" }.to_string()),
            row.get(3).cloned().unwrap_or(Value::Null),
        ]);
    }

    let result_set = ResultSet {
        columns: vec![
            Column {
                name: "schema".to_string(),
                data_type: None,
            },
            Column {
                name: "name".to_string(),
                data_type: None,
            },
            Column {
                name: "isSystem".to_string(),
                data_type: None,
            },
            Column {
                name: "isAllowed".to_string(),
                data_type: None,
            },
            Column {
                name: "modifiedAt".to_string(),
                data_type: None,
            },
        ],
        rows: enriched_rows,
    };

    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "total": paging.total,
            "count": paging.count,
            "offset": paging.offset,
            "limit": paging.limit,
            "hasMore": paging.has_more,
            "nextOffset": paging.next_offset,
            "procedures": json_out::result_set_rows_to_objects(&result_set),
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

    let mut options = TableOptions::default();
    if paging.total > 0 {
        let page_limit = if count == 0 { limit } else { count };
        options.pagination = Some(table::Pagination {
            total: Some(paging.total),
            offset: paging.offset,
            limit: page_limit,
        });
    }
    let rendered = table::render_result_set_table(&result_set, format, &options);
    println!("{}", rendered);

    Ok(())
}

fn exec_proc(args: &CliArgs, proc_name: &str, raw_args: Option<&str>) -> Result<()> {
    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);

    let statement = if let Some(extra) = raw_args {
        format!("EXEC {} {}", proc_name, extra)
    } else {
        format!("EXEC {}", proc_name)
    };
    if !common::allow_write(args, &resolved) {
        safety::validate_read_only(&statement)
            .map_err(|err| AppError::new(ErrorKind::Query, err.to_string()))?;
    }

    let result_sets = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;
        let query = Query::new(statement);
        executor::run_query(query, &mut client).await
    })?;

    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "procedure": proc_name,
            "resultSets": result_sets.iter().map(json_out::result_set_to_json).collect::<Vec<_>>(),
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

    if result_sets.is_empty() {
        println!("Procedure executed (no result set).");
        return Ok(());
    }

    for (idx, result_set) in result_sets.iter().enumerate() {
        if result_sets.len() > 1 {
            println!("Result set {}", idx + 1);
        }
        let rendered = table::render_result_set_table(result_set, format, &TableOptions::default());
        println!("{}", rendered);
        if idx + 1 < result_sets.len() {
            println!();
        }
    }

    Ok(())
}

fn value_to_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::Text(v)) => v.clone(),
        Some(Value::Int(v)) => v.to_string(),
        Some(Value::Bool(v)) => v.to_string(),
        Some(Value::Float(v)) => v.to_string(),
        _ => "".to_string(),
    }
}
