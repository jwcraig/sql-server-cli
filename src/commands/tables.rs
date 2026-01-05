use anyhow::Result;
use serde_json::json;
use tiberius::Query;
use tracing::warn;

use crate::cli::{CliArgs, DescribeArgs, TablesArgs};
use crate::commands::{common, describe, paging};
use crate::config::OutputFormat;
use crate::db::client;
use crate::db::executor;
use crate::db::types::Value;
use crate::output::{TableOptions, json as json_out, table};

const LIMIT_DEFAULT: u64 = 200;
const LIMIT_MAX: u64 = 500;
const DESCRIBE_LIMIT_DEFAULT: u64 = 5;

pub fn run(args: &CliArgs, cmd: &TablesArgs) -> Result<()> {
    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);

    let include_views = cmd.include_views;
    let summary = cmd.summary;
    let with_counts = if summary { true } else { cmd.with_counts };

    // Use different default limit when --describe is set
    let default_limit = if cmd.describe {
        DESCRIBE_LIMIT_DEFAULT
    } else {
        LIMIT_DEFAULT
    };
    let (limit, limit_all) = parse_limit(cmd.limit.as_deref(), default_limit);
    let offset = common::parse_offset(cmd.offset);
    let fetch_all = summary || limit_all;

    let explicit_schema = cmd.schema.as_deref();
    let default_schemas = if explicit_schema.is_none() {
        resolved.connection.default_schemas.clone()
    } else {
        Vec::new()
    };

    let like = cmd.like.clone();

    let (rows, total) = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;

        let mut param_index = 0usize;
        let include_ph = next_param(&mut param_index);

        let (schema_filter_sql, _schema_placeholders) = if let Some(_schema) = explicit_schema {
            let schema_ph = next_param(&mut param_index);
            (
                format!(
                    "AND ({} IS NULL OR TABLE_SCHEMA = {})",
                    schema_ph, schema_ph
                ),
                vec![schema_ph],
            )
        } else if !default_schemas.is_empty() {
            let mut placeholders = Vec::new();
            for _ in &default_schemas {
                placeholders.push(next_param(&mut param_index));
            }
            (
                format!("AND TABLE_SCHEMA IN ({})", placeholders.join(", ")),
                placeholders,
            )
        } else {
            (String::new(), Vec::new())
        };

        let like_ph = next_param(&mut param_index);
        let offset_ph = if fetch_all {
            String::new()
        } else {
            next_param(&mut param_index)
        };
        let limit_ph = if fetch_all {
            String::new()
        } else {
            next_param(&mut param_index)
        };

        let schema_clause = if schema_filter_sql.is_empty() {
            String::new()
        } else {
            format!("{}\n", schema_filter_sql)
        };
        let rownum_clause = if fetch_all {
            String::new()
        } else {
            format!(
                "WHERE b.rownum BETWEEN ({} + 1) AND ({} + {})",
                offset_ph, offset_ph, limit_ph
            )
        };

        let list_sql = format!(
            "\
WITH base AS (
    SELECT
        TABLE_SCHEMA AS schemaName,
        TABLE_NAME AS name,
        TABLE_TYPE AS type,
        ROW_NUMBER() OVER (ORDER BY TABLE_SCHEMA, TABLE_NAME) AS rownum
    FROM INFORMATION_SCHEMA.TABLES
    WHERE ({} = 1 OR TABLE_TYPE = 'BASE TABLE')
      {}\
      AND ({} IS NULL OR TABLE_NAME LIKE {})
)
SELECT b.schemaName AS [schema],
       b.name AS [name],
       b.type AS [type],
       {} AS [rowCount]
FROM base b
{}
{}
ORDER BY b.schemaName, b.name;\
",
            include_ph,
            schema_clause.clone(),
            like_ph,
            like_ph,
            if with_counts {
                "counts.row_count"
            } else {
                "NULL"
            },
            if with_counts {
                "OUTER APPLY (\
     SELECT SUM(ps.row_count) AS row_count
     FROM sys.dm_db_partition_stats ps
     WHERE ps.object_id = OBJECT_ID(QUOTENAME(b.schemaName) + '.' + QUOTENAME(b.name))
       AND ps.index_id IN (0,1)
 ) counts"
            } else {
                ""
            },
            rownum_clause,
        );

        let mut list_query = Query::new(list_sql);
        bind_base_params(
            &mut list_query,
            include_views,
            explicit_schema.map(|s| s.to_string()),
            &default_schemas,
            like.clone(),
        );
        if !fetch_all {
            list_query.bind(offset as i64);
            list_query.bind(limit as i64);
        }

        let list_sets = executor::run_query(list_query, &mut client).await?;
        let list_set = list_sets.into_iter().next().unwrap_or_default();

        let total = if fetch_all {
            list_set.rows.len() as u64
        } else {
            let count_sql = format!(
                "\
SELECT COUNT(*) AS total
FROM INFORMATION_SCHEMA.TABLES
WHERE ({} = 1 OR TABLE_TYPE = 'BASE TABLE')
  {}\
  AND ({} IS NULL OR TABLE_NAME LIKE {});\
",
                include_ph, schema_clause, like_ph, like_ph,
            );
            let mut count_query = Query::new(count_sql);
            bind_base_params(
                &mut count_query,
                include_views,
                explicit_schema.map(|s| s.to_string()),
                &default_schemas,
                like.clone(),
            );
            let count_sets = executor::run_query(count_query, &mut client).await?;
            count_sets
                .first()
                .and_then(|rs| rs.rows.first())
                .and_then(|row| row.first())
                .and_then(|value| match value {
                    crate::db::types::Value::Int(v) => (*v).try_into().ok(),
                    crate::db::types::Value::Float(v) => Some(*v as u64),
                    crate::db::types::Value::Text(s) => s.parse::<u64>().ok(),
                    _ => None,
                })
                .unwrap_or(0)
        };

        Ok::<_, anyhow::Error>((list_set, total))
    })?;

    let count = rows.rows.len() as u64;
    let paging = if fetch_all {
        paging::build_paging(total, count, 0, count.max(1))
    } else {
        paging::build_paging(total, count, offset, limit)
    };

    // Handle --describe mode: describe each table instead of listing
    if cmd.describe {
        return run_describe_mode(
            args, &rows, total, offset, limit, format, &resolved, &cmd.like,
        );
    }

    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "total": paging.total,
            "count": paging.count,
            "offset": paging.offset,
            "limit": paging.limit,
            "hasMore": paging.has_more,
            "nextOffset": paging.next_offset,
            "tables": json_out::result_set_rows_to_objects(&rows),
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
    if !fetch_all && paging.total > 0 {
        let page_limit = if count == 0 { limit } else { count };
        options.pagination = Some(table::Pagination {
            total: Some(paging.total),
            offset: paging.offset,
            limit: page_limit,
        });
    }
    let rendered = table::render_result_set_table(&rows, format, &options);
    println!("{}", rendered);

    Ok(())
}

/// Handle --describe mode: iterate through tables and describe each one
#[allow(clippy::too_many_arguments)]
fn run_describe_mode(
    args: &CliArgs,
    rows: &crate::db::types::ResultSet,
    total: u64,
    offset: u64,
    limit: u64,
    format: OutputFormat,
    resolved: &crate::config::ResolvedConfig,
    like_filter: &Option<String>,
) -> Result<()> {
    let count = rows.rows.len() as u64;

    if args.quiet {
        return Ok(());
    }

    // Extract table info from rows (schema, name, type)
    let tables: Vec<(String, String, String)> = rows
        .rows
        .iter()
        .enumerate()
        .filter_map(|(idx, row)| {
            let schema = match row.first() {
                Some(Value::Text(s)) => s.clone(),
                _ => {
                    warn!("Skipping row {}: missing or invalid schema value", idx);
                    return None;
                }
            };
            let name = match row.get(1) {
                Some(Value::Text(s)) => s.clone(),
                _ => {
                    warn!("Skipping row {}: missing or invalid name value", idx);
                    return None;
                }
            };
            // Get object type (BASE TABLE or VIEW), default to Table
            let obj_type = match row.get(2) {
                Some(Value::Text(s)) => {
                    if s.to_uppercase().contains("VIEW") {
                        "View"
                    } else {
                        "Table"
                    }
                }
                _ => "Table",
            }
            .to_string();
            Some((schema, name, obj_type))
        })
        .collect();

    if tables.is_empty() {
        println!("No tables found.");
        return Ok(());
    }

    // Header showing what we're describing
    if total > count {
        println!(
            "Describing {} of {} matching tables (use --limit to see more)\n",
            count, total
        );
    } else {
        println!(
            "Describing {} table{}\n",
            count,
            if count == 1 { "" } else { "s" }
        );
    }

    // Create describe args (use defaults)
    let describe_args = DescribeArgs {
        object: None,
        schema: None,
        object_type: Some("table".to_string()),
        include_all: false,
        no_indexes: false,
        no_triggers: false,
        no_ddl: false,
        include_fks: false,
        include_constraints: false,
    };

    let json_pretty = common::json_pretty(resolved);

    // Use a single runtime and connection for all describes
    // Collect errors per-table instead of failing on first error
    let (json_results, errors) = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;
        let mut results: Vec<serde_json::Value> = Vec::new();
        let mut errors: Vec<(String, String, String)> = Vec::new(); // (schema, name, error)

        for (i, (schema, name, obj_type)) in tables.iter().enumerate() {
            if matches!(format, OutputFormat::Json) {
                // JSON mode: collect results
                match describe::describe_table_async(
                    &mut client,
                    name,
                    Some(schema.as_str()),
                    &describe_args,
                    OutputFormat::Json,
                    json_pretty,
                )
                .await
                {
                    Ok(result) => match serde_json::from_str::<serde_json::Value>(&result) {
                        Ok(v) => results.push(v),
                        Err(e) => {
                            warn!(
                                "Failed to parse describe output for {}.{}: {}",
                                schema, name, e
                            );
                            errors.push((
                                schema.clone(),
                                name.clone(),
                                format!("JSON parse error: {}", e),
                            ));
                        }
                    },
                    Err(e) => {
                        warn!("Failed to describe {}.{}: {}", schema, name, e);
                        errors.push((schema.clone(), name.clone(), e.to_string()));
                    }
                }
            } else {
                // Text mode: print with separators
                if i > 0 {
                    println!("\n---\n");
                }
                println!("## {}.{} ({})\n", schema, name, obj_type);
                match describe::describe_table_async(
                    &mut client,
                    name,
                    Some(schema.as_str()),
                    &describe_args,
                    format,
                    false,
                )
                .await
                {
                    Ok(result) => print!("{}", result),
                    Err(e) => {
                        warn!("Failed to describe {}.{}: {}", schema, name, e);
                        println!("Error: {}\n", e);
                        errors.push((schema.clone(), name.clone(), e.to_string()));
                    }
                }
            }
        }

        Ok::<_, anyhow::Error>((results, errors))
    })?;

    // Calculate pagination values
    let has_more = total > offset + count;
    let next_offset = if has_more { Some(offset + count) } else { None };

    // Output JSON if in JSON mode
    if matches!(format, OutputFormat::Json) {
        let mut payload = json!({
            "total": total,
            "count": count,
            "offset": offset,
            "limit": limit,
            "hasMore": has_more,
            "nextOffset": next_offset,
            "tables": json_results,
        });
        // Include errors if any occurred
        if !errors.is_empty() {
            payload["errors"] = json!(
                errors
                    .iter()
                    .map(|(schema, name, err)| {
                        json!({"schema": schema, "name": name, "error": err})
                    })
                    .collect::<Vec<_>>()
            );
        }
        let body = json_out::emit_json_value(&payload, json_pretty)?;
        println!("{}", body);
        return Ok(());
    }

    // Show error summary for text mode if any occurred
    if !errors.is_empty() {
        println!("\n---");
        println!("Errors ({}):", errors.len());
        for (schema, name, err) in &errors {
            println!("  - {}.{}: {}", schema, name, err);
        }
    }

    // Paging guidance for text mode
    if has_more {
        let next = next_offset.unwrap_or(offset + count);
        let remaining = total - next;
        println!("\n---");
        println!(
            "Showing {} of {} tables. {} more available.",
            count, total, remaining
        );

        // Build suggested next command
        let mut next_cmd = String::from("sscli tables --describe");
        if let Some(like) = like_filter {
            next_cmd.push_str(&format!(" --like \"{}\"", like));
        }
        next_cmd.push_str(&format!(" --offset {} --limit {}", next, limit));
        println!("Next: {}", next_cmd);
    }

    Ok(())
}

fn parse_limit(raw: Option<&str>, default: u64) -> (u64, bool) {
    if let Some(value) = raw {
        // "all" or "0" means fetch everything
        if value.eq_ignore_ascii_case("all") || value == "0" {
            return (LIMIT_MAX, true);
        }
        if let Ok(parsed) = value.parse::<u64>() {
            if parsed > LIMIT_MAX {
                return (LIMIT_MAX, false);
            }
            return (parsed, false);
        }
    }
    (default, false)
}

fn next_param(counter: &mut usize) -> String {
    *counter += 1;
    format!("@P{}", counter)
}

fn bind_base_params(
    query: &mut Query<'_>,
    include_views: bool,
    explicit_schema: Option<String>,
    default_schemas: &[String],
    like: Option<String>,
) {
    query.bind(if include_views { 1i32 } else { 0i32 });
    if let Some(schema) = explicit_schema {
        query.bind(schema);
    } else if !default_schemas.is_empty() {
        for schema in default_schemas {
            query.bind(schema.clone());
        }
    }
    query.bind(like);
}
