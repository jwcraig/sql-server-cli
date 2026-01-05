use anyhow::{anyhow, Result};
use serde_json::json;
use tiberius::Query;

use crate::cli::{CliArgs, TableDataArgs};
use crate::commands::{common, paging, sql_utils};
use crate::config::OutputFormat;
use crate::db::client;
use crate::db::executor;
use crate::error::{AppError, ErrorKind};
use crate::output::{csv, json as json_out, table, TableOptions};

const LIMIT_DEFAULT: u64 = 25;
const LIMIT_MAX: u64 = 500;

pub fn run(args: &CliArgs, cmd: &TableDataArgs) -> Result<()> {
    let table_name = cmd
        .table
        .as_deref()
        .ok_or_else(|| anyhow!("Missing required --table"))?;

    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);

    let (schema, table_name) = resolve_schema_table(cmd.schema.as_deref(), table_name, &resolved);

    let limit = common::parse_limit(cmd.limit, LIMIT_DEFAULT, LIMIT_MAX);
    let offset = common::parse_offset(cmd.offset);

    let columns_raw = cmd.columns.clone();
    let where_clause = cmd.where_clause.clone();
    let order_by = cmd
        .order_by
        .clone()
        .unwrap_or_else(|| "(SELECT 0)".to_string());

    let params = sql_utils::parse_params(&cmd.params)
        .map_err(|err| AppError::new(ErrorKind::Query, err.to_string()))?;

    let (result_set, total, output_columns, csv_paths) = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;

        let column_tokens = parse_columns(columns_raw.as_deref());
        let (select_list, output_columns) = if column_tokens.len() == 1 && column_tokens[0] == "*" {
            let names = fetch_column_names(&mut client, &schema, &table_name).await?;
            let list = names
                .iter()
                .map(|name| quote_identifier(name))
                .collect::<Vec<_>>()
                .join(", ");
            (list, names)
        } else {
            let list = column_tokens.join(", ");
            (list, column_tokens)
        };

        let replaced_where = where_clause
            .as_deref()
            .map(|clause| sql_utils::replace_named_params(clause, &params, 1));
        let where_sql = replaced_where
            .as_ref()
            .map(|clause| format!("WHERE {}", clause))
            .unwrap_or_default();

        let param_count = params.len();
        let offset_placeholder = format!("@P{}", param_count + 1);
        let limit_placeholder = format!("@P{}", param_count + 2);

        let qualified_table = format!(
            "{}.{}",
            quote_identifier(&schema),
            quote_identifier(&table_name)
        );
        let sql = format!(
            "SELECT {select_list} FROM {qualified_table} {where_sql} ORDER BY {order_by} OFFSET {offset_placeholder} ROWS FETCH NEXT {limit_placeholder} ROWS ONLY;",
        );

        let mut query = Query::new(sql);
        for param in &params {
            query.bind(param.value.as_str());
        }
        query.bind(offset as i64);
        query.bind(limit as i64);
        let result_sets = executor::run_query(query, &mut client).await?;
        let result_set = result_sets.into_iter().next().unwrap_or_default();

        let count_sql = format!("SELECT COUNT(*) AS total FROM {qualified_table} {where_sql};");
        let mut count_query = Query::new(count_sql);
        for param in &params {
            count_query.bind(param.value.as_str());
        }
        let count_sets = executor::run_query(count_query, &mut client).await?;
        let total = count_sets
            .first()
            .and_then(|rs| rs.rows.first())
            .and_then(|row| row.first())
            .and_then(|value| match value {
                crate::db::types::Value::Int(v) => (*v).try_into().ok(),
                crate::db::types::Value::Float(v) => Some(*v as u64),
                crate::db::types::Value::Text(s) => s.parse::<u64>().ok(),
                _ => None,
            })
            .unwrap_or(result_set.rows.len() as u64);

        let csv_paths = if let Some(path) = cmd.csv.as_ref() {
            Some(csv::write_result_sets(path, &[result_set.clone()], resolved.settings.output.csv.multi_result_naming)?)
        } else {
            None
        };

        Ok::<_, anyhow::Error>((result_set, total, output_columns, csv_paths))
    })?;

    let count = result_set.rows.len() as u64;
    let paging = paging::build_paging(total, count, offset, limit);

    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "table": { "schema": schema, "name": table_name },
            "columns": output_columns,
            "rows": result_set.rows,
            "total": paging.total,
            "offset": paging.offset,
            "limit": paging.limit,
            "hasMore": paging.has_more,
            "nextOffset": paging.next_offset,
            "csvPaths": csv_paths.as_ref().map(|paths| paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()),
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

    if let Some(paths) = csv_paths {
        println!("\nCSV written:");
        for path in paths {
            println!("- {}", path.display());
        }
    }

    Ok(())
}

fn resolve_schema_table(
    schema: Option<&str>,
    table: &str,
    resolved: &crate::config::ResolvedConfig,
) -> (String, String) {
    if schema.is_none() {
        if let Some((left, right)) = table.split_once('.') {
            return (left.to_string(), right.to_string());
        }
    }

    let schema = schema
        .map(|s| s.to_string())
        .or_else(|| resolved.connection.default_schemas.first().cloned())
        .unwrap_or_else(|| "dbo".to_string());

    (schema, table.to_string())
}

fn parse_columns(raw: Option<&str>) -> Vec<String> {
    if let Some(raw) = raw {
        let list = raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if list.is_empty() {
            vec!["*".to_string()]
        } else {
            list
        }
    } else {
        vec!["*".to_string()]
    }
}

async fn fetch_column_names(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    schema: &str,
    table: &str,
) -> Result<Vec<String>> {
    let sql = r#"
SELECT COLUMN_NAME
FROM INFORMATION_SCHEMA.COLUMNS
WHERE TABLE_NAME = @P1
  AND (@P2 IS NULL OR TABLE_SCHEMA = @P2)
ORDER BY ORDINAL_POSITION;
"#;
    let mut query = Query::new(sql);
    query.bind(table);
    query.bind(Some(schema));
    let result_sets = executor::run_query(query, client).await?;
    let result_set = result_sets.into_iter().next().unwrap_or_default();

    if result_set.rows.is_empty() {
        return Err(anyhow!("Table '{}' not found", table));
    }

    Ok(result_set
        .rows
        .iter()
        .filter_map(|row| row.first())
        .map(|value| match value {
            crate::db::types::Value::Text(s) => s.clone(),
            _ => value.as_display(),
        })
        .collect())
}

fn quote_identifier(input: &str) -> String {
    if is_simple_identifier(input) {
        format!("[{}]", input.replace(']', "]]"))
    } else {
        input.to_string()
    }
}

fn is_simple_identifier(input: &str) -> bool {
    input.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
}
