use anyhow::{Result, anyhow};
use serde_json::json;
use std::collections::BTreeMap;
use tiberius::Query;

use crate::cli::{CliArgs, IndexesArgs};
use crate::commands::common;
use crate::config::OutputFormat;
use crate::db::client;
use crate::db::executor;
use crate::db::types::{Column, ResultSet, Value};
use crate::output::{TableOptions, json as json_out, table};

#[derive(Debug, Clone)]
struct IndexInfo {
    schema: String,
    name: String,
    index_type: String,
    is_unique: bool,
    is_primary: bool,
    key_columns: Vec<String>,
    included_columns: Vec<String>,
    user_seeks: Option<i64>,
    user_updates: Option<i64>,
}

pub fn run(args: &CliArgs, cmd: &IndexesArgs) -> Result<()> {
    let table_raw = cmd
        .table
        .as_deref()
        .ok_or_else(|| anyhow!("Missing required --table"))?;
    let (table_name, schema_from_name) = common::normalize_object_input(table_raw);

    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);
    let schema = cmd.schema.clone().or(schema_from_name);

    let table_name_param = table_name.clone();
    let indexes = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;
        let sql = r#"
SELECT
    s.name AS schema_name,
    i.name AS index_name,
    i.type_desc AS index_type,
    i.is_unique,
    i.is_primary_key,
    ic.is_included_column,
    ic.key_ordinal,
    c.name AS column_name,
    usage_stats.user_seeks,
    usage_stats.user_updates
FROM sys.indexes i
INNER JOIN sys.objects o ON i.object_id = o.object_id
INNER JOIN sys.schemas s ON o.schema_id = s.schema_id
INNER JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id
INNER JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id
LEFT JOIN sys.dm_db_index_usage_stats usage_stats
    ON usage_stats.database_id = DB_ID()
   AND usage_stats.object_id = i.object_id
   AND usage_stats.index_id = i.index_id
WHERE o.type = 'U'
  AND o.name = @P1
  AND (@P2 IS NULL OR s.name = @P2)
  AND i.name IS NOT NULL
  AND i.is_hypothetical = 0
ORDER BY i.name, ic.key_ordinal, ic.index_column_id;
"#;

        let mut query = Query::new(sql);
        query.bind(table_name_param.as_str());
        query.bind(schema.as_deref());
        let result_sets = executor::run_query(query, &mut client).await?;
        let result_set = result_sets.into_iter().next().unwrap_or_default();

        let mut grouped: BTreeMap<String, IndexInfo> = BTreeMap::new();
        for row in result_set.rows {
            let index_name = value_to_string(row.get(1));
            let entry = grouped
                .entry(index_name.clone())
                .or_insert_with(|| IndexInfo {
                    schema: value_to_string(row.first()),
                    name: index_name.clone(),
                    index_type: value_to_string(row.get(2)),
                    is_unique: value_to_bool(row.get(3)),
                    is_primary: value_to_bool(row.get(4)),
                    key_columns: Vec::new(),
                    included_columns: Vec::new(),
                    user_seeks: value_to_i64(row.get(8)),
                    user_updates: value_to_i64(row.get(9)),
                });
            let column_name = value_to_string(row.get(7));
            let is_included = value_to_bool(row.get(5));
            if is_included {
                if !entry.included_columns.contains(&column_name) {
                    entry.included_columns.push(column_name);
                }
            } else if !entry.key_columns.contains(&column_name) {
                entry.key_columns.push(column_name);
            }
        }

        Ok::<_, anyhow::Error>(grouped.into_values().collect::<Vec<_>>())
    })?;

    if indexes.is_empty() {
        return Err(anyhow!("No indexes found for table '{}'.", table_name));
    }

    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "table": { "schema": indexes[0].schema, "name": table_name },
            "indexes": indexes.iter().map(index_to_json).collect::<Vec<_>>(),
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

    let result_set = indexes_to_result_set(&indexes, cmd.show_usage);
    let rendered = table::render_result_set_table(&result_set, format, &TableOptions::default());
    println!("{}", rendered);

    Ok(())
}

fn indexes_to_result_set(indexes: &[IndexInfo], show_usage: bool) -> ResultSet {
    let columns = vec![
        Column {
            name: "name".to_string(),
            data_type: None,
        },
        Column {
            name: "type".to_string(),
            data_type: None,
        },
        Column {
            name: "unique".to_string(),
            data_type: None,
        },
        Column {
            name: "primary".to_string(),
            data_type: None,
        },
        Column {
            name: "keyColumns".to_string(),
            data_type: None,
        },
        Column {
            name: "includedColumns".to_string(),
            data_type: None,
        },
        Column {
            name: "seeks".to_string(),
            data_type: None,
        },
        Column {
            name: "updates".to_string(),
            data_type: None,
        },
    ];

    let rows = indexes
        .iter()
        .map(|idx| {
            vec![
                Value::Text(idx.name.clone()),
                Value::Text(idx.index_type.clone()),
                Value::Text(if idx.is_unique { "yes" } else { "no" }.to_string()),
                Value::Text(if idx.is_primary { "yes" } else { "no" }.to_string()),
                Value::Text(if idx.key_columns.is_empty() {
                    "-".to_string()
                } else {
                    idx.key_columns.join(", ")
                }),
                Value::Text(if idx.included_columns.is_empty() {
                    "-".to_string()
                } else {
                    idx.included_columns.join(", ")
                }),
                if show_usage {
                    idx.user_seeks.map(Value::Int).unwrap_or(Value::Null)
                } else {
                    Value::Null
                },
                if show_usage {
                    idx.user_updates.map(Value::Int).unwrap_or(Value::Null)
                } else {
                    Value::Null
                },
            ]
        })
        .collect();

    ResultSet { columns, rows }
}

fn index_to_json(index: &IndexInfo) -> serde_json::Value {
    json!({
        "schema": index.schema,
        "name": index.name,
        "type": index.index_type,
        "unique": index.is_unique,
        "primary": index.is_primary,
        "keyColumns": index.key_columns,
        "includedColumns": index.included_columns,
        "userSeeks": index.user_seeks,
        "userUpdates": index.user_updates,
    })
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

fn value_to_bool(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(v)) => *v,
        Some(Value::Int(v)) => *v != 0,
        Some(Value::Text(v)) => v == "1" || v.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

fn value_to_i64(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Int(v)) => Some(*v),
        Some(Value::Float(v)) => Some(*v as i64),
        Some(Value::Text(v)) => v.parse::<i64>().ok(),
        _ => None,
    }
}
