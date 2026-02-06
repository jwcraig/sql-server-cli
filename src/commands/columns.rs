use anyhow::{Result, anyhow};
use serde_json::json;
use tiberius::Query;

use crate::cli::{CliArgs, ColumnsArgs};
use crate::commands::{common, paging};
use crate::config::OutputFormat;
use crate::db::client;
use crate::db::executor;
use crate::db::types::{ResultSet, Value};
use crate::output::{TableOptions, json as json_out, table};

const LIMIT_DEFAULT: u64 = 50;
const LIMIT_MAX: u64 = 500;

pub fn run(args: &CliArgs, cmd: &ColumnsArgs) -> Result<()> {
    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);

    let limit = common::parse_limit(cmd.limit, LIMIT_DEFAULT, LIMIT_MAX);
    let offset = common::parse_offset(cmd.offset);
    let like = cmd.like.clone();
    let (object_name, schema_from_name) = match cmd.object.as_deref().or(cmd.table.as_deref()) {
        Some(t) => {
            let (name, schema_opt) = common::normalize_object_input(t);
            (Some(name), schema_opt)
        }
        None => (None, None),
    };
    let schema = cmd.schema.clone().or(schema_from_name);
    let table_filter = object_name.clone();

    // Auto-include views when the user supplies an explicit object name so
    // that `sscli columns <table-or-view>` works without extra flags.
    let include_views = if cmd.object.is_some() {
        true
    } else {
        cmd.include_views
    };

    let (rows, total) = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;

        // If the user passed a specific object and it's a stored procedure or
        // table-valued function, describe its first result set via metadata
        // discovery (no execution). This enables `sscli columns <proc>`.
        let object_meta =
            detect_object_meta(&mut client, table_filter.as_deref(), schema.as_deref()).await?;
        if matches!(
            object_meta.as_ref().map(|m| m.kind),
            Some(ObjectKind::Routine)
        ) {
            let meta = object_meta.as_ref().expect("checked above");
            let (list_set, total) = fetch_routine_columns(&mut client, meta, offset, limit).await?;

            return Ok::<_, anyhow::Error>((list_set, total));
        }

        let list_sql = r#"
WITH filtered AS (
    SELECT
        c.TABLE_SCHEMA AS schemaName,
        c.TABLE_NAME AS tableName,
        c.COLUMN_NAME AS columnName,
        c.DATA_TYPE AS dataType,
        c.IS_NULLABLE AS isNullable,
        ROW_NUMBER() OVER (ORDER BY c.TABLE_SCHEMA, c.TABLE_NAME, c.ORDINAL_POSITION) AS rownum
    FROM INFORMATION_SCHEMA.COLUMNS c
    INNER JOIN INFORMATION_SCHEMA.TABLES t
        ON c.TABLE_SCHEMA = t.TABLE_SCHEMA AND c.TABLE_NAME = t.TABLE_NAME
    WHERE (@P1 = 1 OR t.TABLE_TYPE = 'BASE TABLE')
      AND (@P2 IS NULL OR c.COLUMN_NAME LIKE @P2)
      AND (@P3 IS NULL OR c.TABLE_NAME LIKE @P3)
      AND (@P4 IS NULL OR c.TABLE_SCHEMA = @P4)
)
SELECT schemaName AS [schema],
       tableName AS tableName,
       columnName AS columnName,
       dataType AS dataType,
       isNullable AS isNullable
FROM filtered
WHERE rownum BETWEEN (@P5 + 1) AND (@P5 + @P6)
ORDER BY schemaName, tableName, columnName;
"#;

        let mut list_query = Query::new(list_sql);
        list_query.bind(if include_views { 1i32 } else { 0i32 });
        list_query.bind(like.as_deref());
        list_query.bind(table_filter.as_deref());
        list_query.bind(schema.as_deref());
        list_query.bind(offset as i64);
        list_query.bind(limit as i64);

        let list_sets = executor::run_query(list_query, &mut client).await?;
        let list_set = list_sets.into_iter().next().unwrap_or_default();

        let count_sql = r#"
SELECT COUNT(*) AS total
FROM INFORMATION_SCHEMA.COLUMNS c
INNER JOIN INFORMATION_SCHEMA.TABLES t
    ON c.TABLE_SCHEMA = t.TABLE_SCHEMA AND c.TABLE_NAME = t.TABLE_NAME
WHERE (@P1 = 1 OR t.TABLE_TYPE = 'BASE TABLE')
  AND (@P2 IS NULL OR c.COLUMN_NAME LIKE @P2)
  AND (@P3 IS NULL OR c.TABLE_NAME LIKE @P3)
  AND (@P4 IS NULL OR c.TABLE_SCHEMA = @P4);
"#;
        let mut count_query = Query::new(count_sql);
        count_query.bind(if include_views { 1i32 } else { 0i32 });
        count_query.bind(like.as_deref());
        count_query.bind(table_filter.as_deref());
        count_query.bind(schema.as_deref());
        let count_sets = executor::run_query(count_query, &mut client).await?;
        let total = count_sets
            .first()
            .and_then(|rs| rs.rows.first())
            .and_then(|row| row.first())
            .and_then(value_as_u64)
            .unwrap_or(0);

        Ok::<_, anyhow::Error>((list_set, total))
    })?;

    let count = rows.rows.len() as u64;
    let paging = paging::build_paging(total, count, offset, limit);

    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "total": paging.total,
            "count": paging.count,
            "offset": paging.offset,
            "limit": paging.limit,
            "hasMore": paging.has_more,
            "nextOffset": paging.next_offset,
            "columns": json_out::result_set_rows_to_objects(&rows),
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
    let result = table::render_result_set_table(&rows, format, &options);
    println!("{}", result.output);

    Ok(())
}

fn value_as_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Int(v) => (*v).try_into().ok(),
        Value::Float(v) => Some(*v as u64),
        Value::Text(s) => s.parse::<u64>().ok(),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObjectKind {
    TableOrView,
    Routine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObjectMeta {
    kind: ObjectKind,
    schema: String,
    name: String,
    object_id: i32,
}

async fn detect_object_meta(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    object_name: Option<&str>,
    schema: Option<&str>,
) -> Result<Option<ObjectMeta>> {
    let Some(name) = object_name else {
        return Ok(None);
    };

    let sql = r#"
SELECT TOP (1) o.object_id, o.type, s.name AS schema_name, o.name
FROM sys.objects o
INNER JOIN sys.schemas s ON o.schema_id = s.schema_id
WHERE o.name = @P1
  AND (@P2 IS NULL OR s.name = @P2)
  AND o.type IN ('U', 'V', 'P', 'PC', 'IF', 'TF')
ORDER BY CASE o.type WHEN 'U' THEN 1 WHEN 'V' THEN 2 ELSE 3 END;
"#;

    let mut query = Query::new(sql);
    query.bind(name);
    query.bind(schema);

    let result_sets = executor::run_query(query, client).await?;
    let result_set = result_sets.into_iter().next().unwrap_or_default();

    let Some(row) = result_set.rows.first() else {
        return Ok(None);
    };

    let type_val = match row.get(1) {
        Some(Value::Text(t)) => t.as_str(),
        _ => return Ok(None),
    };
    let schema_val = match row.get(2) {
        Some(Value::Text(t)) => t.clone(),
        _ => return Ok(None),
    };
    let name_val = match row.get(3) {
        Some(Value::Text(t)) => t.clone(),
        _ => return Ok(None),
    };
    let object_id = match row.first() {
        Some(Value::Int(v)) => *v as i32,
        _ => return Ok(None),
    };

    let kind = match type_val.trim() {
        "U" | "V" => ObjectKind::TableOrView,
        "P" | "PC" | "IF" | "TF" => ObjectKind::Routine,
        _ => ObjectKind::TableOrView,
    };

    Ok(Some(ObjectMeta {
        kind,
        schema: schema_val,
        name: name_val,
        object_id,
    }))
}

async fn fetch_routine_columns(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    meta: &ObjectMeta,
    offset: u64,
    limit: u64,
) -> Result<(ResultSet, u64)> {
    // Primary path: use object_id for stable metadata.
    let sql = r#"
WITH described AS (
    SELECT
        @P2 AS schemaName,
        @P3 AS tableName,
        dfrs.name AS columnName,
        dfrs.system_type_name AS dataType,
        CASE WHEN dfrs.is_nullable = 1 THEN 'YES' ELSE 'NO' END AS isNullable,
        dfrs.column_ordinal AS ordinal,
        ROW_NUMBER() OVER (ORDER BY dfrs.column_ordinal) AS rownum
    FROM sys.dm_exec_describe_first_result_set_for_object(@P1, NULL) AS dfrs
    WHERE dfrs.error_state IS NULL
)
SELECT
    schemaName AS [schema],
    tableName AS tableName,
    columnName AS columnName,
    dataType AS dataType,
    isNullable AS isNullable,
    COUNT(*) OVER () AS totalCount
FROM described
WHERE rownum BETWEEN (@P4 + 1) AND (@P4 + @P5)
ORDER BY rownum;
"#;

    let mut query = Query::new(sql);
    query.bind(meta.object_id);
    query.bind(meta.schema.as_str());
    query.bind(meta.name.as_str());
    query.bind(offset as i64);
    query.bind(limit as i64);

    let sets = executor::run_query(query, client).await?;
    let mut result_set = sets.into_iter().next().unwrap_or_default();

    let (result_set, total) = if result_set.rows.is_empty() {
        // Fallback: sp_describe_first_result_set for edge cases (dynamic SQL, temp tables).
        fetch_routine_columns_via_sp(client, meta, offset, limit).await?
    } else {
        let total = result_set
            .rows
            .first()
            .and_then(|row| row.get(5))
            .and_then(value_as_u64)
            .unwrap_or(result_set.rows.len() as u64);

        // Drop helper column to align with table/view output shape
        for row in result_set.rows.iter_mut() {
            if row.len() > 5 {
                row.pop();
            }
        }

        (result_set, total)
    };

    if result_set.rows.is_empty() {
        let msg = routine_metadata_error(client, meta)
            .await?
            .unwrap_or_else(|| "Metadata could not be determined for this routine".to_string());
        return Err(anyhow!(msg));
    }

    Ok((result_set, total))
}

async fn fetch_routine_columns_via_sp(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    meta: &ObjectMeta,
    offset: u64,
    limit: u64,
) -> Result<(ResultSet, u64)> {
    let escaped_schema = meta.schema.replace(']', "]]");
    let escaped_name = meta.name.replace(']', "]]");
    let exec_stmt = format!("EXEC [{}].[{}]", escaped_schema, escaped_name);

    let sql = r#"
DECLARE @dfr TABLE (
    column_ordinal INT,
    name SYSNAME,
    system_type_name NVARCHAR(256),
    is_nullable BIT,
    error_state INT
);

INSERT INTO @dfr (column_ordinal, name, system_type_name, is_nullable, error_state)
EXEC sys.sp_describe_first_result_set @tsql = @P1, @params = NULL, @browse_information_mode = 1;

WITH described AS (
    SELECT
        @P2 AS schemaName,
        @P3 AS tableName,
        name AS columnName,
        system_type_name AS dataType,
        CASE WHEN is_nullable = 1 THEN 'YES' ELSE 'NO' END AS isNullable,
        column_ordinal AS ordinal,
        ROW_NUMBER() OVER (ORDER BY column_ordinal) AS rownum
    FROM @dfr
    WHERE error_state IS NULL
)
SELECT
    schemaName AS [schema],
    tableName AS tableName,
    columnName AS columnName,
    dataType AS dataType,
    isNullable AS isNullable,
    COUNT(*) OVER () AS totalCount
FROM described
WHERE rownum BETWEEN (@P4 + 1) AND (@P4 + @P5)
ORDER BY rownum;
"#;

    let mut query = Query::new(sql);
    query.bind(exec_stmt.as_str());
    query.bind(meta.schema.as_str());
    query.bind(meta.name.as_str());
    query.bind(offset as i64);
    query.bind(limit as i64);

    let sets = match executor::run_query(query, client).await {
        Ok(s) => s,
        Err(err) => {
            let mut message = format!(
                "Unable to describe first result set for {}.{}: {}",
                meta.schema, meta.name, err
            );
            let lower = message.to_lowercase();
            if lower.contains("temp table")
                || lower.contains("temporary_table")
                || lower.contains("11526")
            {
                message.push_str(&format!(
                    " Hint: routines that use temp tables often block metadata discovery. Try `sscli describe {}.{}` to inspect the definition.",
                    meta.schema, meta.name
                ));
            }
            return Err(anyhow!(message));
        }
    };
    let mut result_set = sets.into_iter().next().unwrap_or_default();

    let total = result_set
        .rows
        .first()
        .and_then(|row| row.get(5))
        .and_then(value_as_u64)
        .unwrap_or(result_set.rows.len() as u64);

    // Drop helper column to align with table/view output shape
    for row in result_set.rows.iter_mut() {
        if row.len() > 5 {
            row.pop();
        }
    }

    Ok((result_set, total))
}

async fn routine_metadata_error(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    meta: &ObjectMeta,
) -> Result<Option<String>> {
    let sql = r#"
SELECT TOP (1) error_message
FROM sys.dm_exec_describe_first_result_set_for_object(@P1, NULL)
WHERE error_state IS NOT NULL;
"#;
    let mut query = Query::new(sql);
    query.bind(meta.object_id);
    let sets = executor::run_query(query, client).await?;
    let message = sets
        .first()
        .and_then(|rs| rs.rows.first())
        .and_then(|row| row.first())
        .and_then(|v| match v {
            Value::Text(s) => Some(s.clone()),
            _ => None,
        });

    let message = message.map(|msg| {
        format!(
            "{} Hint: routines with temp tables often need a full describe. Try `sscli describe {}.{}`.",
            msg, meta.schema, meta.name
        )
    });

    Ok(message)
}
