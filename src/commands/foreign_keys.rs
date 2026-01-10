use anyhow::{Result, anyhow};
use serde_json::json;
use std::collections::BTreeMap;
use tiberius::Query;

use crate::cli::{CliArgs, ForeignKeysArgs};
use crate::commands::common;
use crate::config::OutputFormat;
use crate::db::client;
use crate::db::executor;
use crate::db::types::{Column, ResultSet, Value};
use crate::output::{TableOptions, json as json_out, table};

#[derive(Debug, Clone)]
struct ForeignKeyInfo {
    name: String,
    direction: String,
    from_schema: String,
    from_table: String,
    to_schema: String,
    to_table: String,
    columns: Vec<String>,
    referenced_columns: Vec<String>,
    update_rule: String,
    delete_rule: String,
}

pub fn run(args: &CliArgs, cmd: &ForeignKeysArgs) -> Result<()> {
    let table_raw = cmd
        .table
        .as_deref()
        .ok_or_else(|| anyhow!("Missing required --table"))?;
    let (table_name, schema_from_name) = common::normalize_object_input(table_raw);
    let direction = cmd
        .direction
        .clone()
        .unwrap_or_else(|| "outbound".to_string());
    let direction = direction.to_lowercase();
    if !["outbound", "inbound", "both"].contains(&direction.as_str()) {
        return Err(anyhow!("--direction must be outbound, inbound, or both"));
    }

    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);
    let schema = cmd.schema.clone().or(schema_from_name);

    let table_name_param = table_name.clone();
    let fks = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;
        let sql = r#"
SELECT
    fk.name AS fk_name,
    schParent.name AS parent_schema,
    parent.name AS parent_table,
    cparent.name AS parent_column,
    schRef.name AS referenced_schema,
    referenced.name AS referenced_table,
    cref.name AS referenced_column,
    fk.update_referential_action_desc AS update_rule,
    fk.delete_referential_action_desc AS delete_rule,
    fkc.constraint_column_id
FROM sys.foreign_keys fk
INNER JOIN sys.tables parent ON fk.parent_object_id = parent.object_id
INNER JOIN sys.schemas schParent ON parent.schema_id = schParent.schema_id
INNER JOIN sys.tables referenced ON fk.referenced_object_id = referenced.object_id
INNER JOIN sys.schemas schRef ON referenced.schema_id = schRef.schema_id
INNER JOIN sys.foreign_key_columns fkc ON fk.object_id = fkc.constraint_object_id
INNER JOIN sys.columns cparent ON fkc.parent_object_id = cparent.object_id AND fkc.parent_column_id = cparent.column_id
INNER JOIN sys.columns cref ON fkc.referenced_object_id = cref.object_id AND fkc.referenced_column_id = cref.column_id
WHERE (
    @P3 = 1 AND parent.name = @P1 AND (@P2 IS NULL OR schParent.name = @P2)
) OR (
    @P4 = 1 AND referenced.name = @P1 AND (@P2 IS NULL OR schRef.name = @P2)
)
ORDER BY fk.name, fkc.constraint_column_id;
"#;

        let mut query = Query::new(sql);
        query.bind(table_name_param.as_str());
        query.bind(schema.as_deref());
        query.bind(if direction == "outbound" || direction == "both" { 1i32 } else { 0i32 });
        query.bind(if direction == "inbound" || direction == "both" { 1i32 } else { 0i32 });
        let result_sets = executor::run_query(query, &mut client).await?;
        let result_set = result_sets.into_iter().next().unwrap_or_default();

        let mut grouped: BTreeMap<String, ForeignKeyInfo> = BTreeMap::new();
        for row in result_set.rows {
            let fk_name = value_to_string(row.first());
            let parent_schema = value_to_string(row.get(1));
            let parent_table = value_to_string(row.get(2));
            let parent_column = value_to_string(row.get(3));
            let ref_schema = value_to_string(row.get(4));
            let ref_table = value_to_string(row.get(5));
            let ref_column = value_to_string(row.get(6));
            let update_rule = value_to_string(row.get(7));
            let delete_rule = value_to_string(row.get(8));

            let is_outbound = parent_table.eq_ignore_ascii_case(table_name_param.as_str());
            let entry = grouped.entry(fk_name.clone()).or_insert_with(|| ForeignKeyInfo {
                name: fk_name.clone(),
                direction: if is_outbound { "outbound".to_string() } else { "inbound".to_string() },
                from_schema: if is_outbound { parent_schema.clone() } else { ref_schema.clone() },
                from_table: if is_outbound { parent_table.clone() } else { ref_table.clone() },
                to_schema: if is_outbound { ref_schema.clone() } else { parent_schema.clone() },
                to_table: if is_outbound { ref_table.clone() } else { parent_table.clone() },
                columns: Vec::new(),
                referenced_columns: Vec::new(),
                update_rule: update_rule.clone(),
                delete_rule: delete_rule.clone(),
            });

            if entry.direction == "outbound" {
                entry.columns.push(parent_column);
                entry.referenced_columns.push(ref_column);
            } else {
                entry.columns.push(ref_column);
                entry.referenced_columns.push(parent_column);
            }
        }

        Ok::<_, anyhow::Error>(grouped.into_values().collect::<Vec<_>>())
    })?;

    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "table": { "schema": schema.clone().unwrap_or_else(|| "dbo".to_string()), "name": table_name },
            "direction": direction,
            "foreignKeys": fks.iter().map(fk_to_json).collect::<Vec<_>>(),
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

    let result_set = fks_to_result_set(&fks);
    let rendered = table::render_result_set_table(&result_set, format, &TableOptions::default());
    println!("{}", rendered);

    Ok(())
}

fn fks_to_result_set(fks: &[ForeignKeyInfo]) -> ResultSet {
    let columns = vec![
        Column {
            name: "name".to_string(),
            data_type: None,
        },
        Column {
            name: "direction".to_string(),
            data_type: None,
        },
        Column {
            name: "fromTable".to_string(),
            data_type: None,
        },
        Column {
            name: "columns".to_string(),
            data_type: None,
        },
        Column {
            name: "toTable".to_string(),
            data_type: None,
        },
        Column {
            name: "referencedColumns".to_string(),
            data_type: None,
        },
        Column {
            name: "updateRule".to_string(),
            data_type: None,
        },
        Column {
            name: "deleteRule".to_string(),
            data_type: None,
        },
    ];

    let rows = fks
        .iter()
        .map(|fk| {
            vec![
                Value::Text(fk.name.clone()),
                Value::Text(fk.direction.clone()),
                Value::Text(format!("{}.{}", fk.from_schema, fk.from_table)),
                Value::Text(fk.columns.join(", ")),
                Value::Text(format!("{}.{}", fk.to_schema, fk.to_table)),
                Value::Text(fk.referenced_columns.join(", ")),
                Value::Text(fk.update_rule.clone()),
                Value::Text(fk.delete_rule.clone()),
            ]
        })
        .collect();

    ResultSet { columns, rows }
}

fn fk_to_json(fk: &ForeignKeyInfo) -> serde_json::Value {
    json!({
        "name": fk.name,
        "direction": fk.direction,
        "from": { "schema": fk.from_schema, "table": fk.from_table },
        "to": { "schema": fk.to_schema, "table": fk.to_table },
        "columns": fk.columns,
        "referencedColumns": fk.referenced_columns,
        "updateRule": fk.update_rule,
        "deleteRule": fk.delete_rule,
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
