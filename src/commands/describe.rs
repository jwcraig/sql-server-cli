use anyhow::{anyhow, Result};
use serde_json::json;
use std::collections::BTreeMap;
use tiberius::Query;

use crate::cli::{CliArgs, DescribeArgs};
use crate::commands::common;
use crate::config::OutputFormat;
use crate::db::client;
use crate::db::executor;
use crate::db::types::{Column, ResultSet, Value};
use crate::output::{json as json_out, table, TableOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
enum ObjectType {
    Table,
    View,
    Trigger,
    Procedure,
    Function,
}

impl ObjectType {
    fn from_sql_type(s: &str) -> Option<Self> {
        match s.trim() {
            "U" => Some(ObjectType::Table),
            "V" => Some(ObjectType::View),
            "TR" => Some(ObjectType::Trigger),
            "P" => Some(ObjectType::Procedure),
            "FN" | "IF" | "TF" | "AF" => Some(ObjectType::Function),
            _ => None,
        }
    }

    fn from_cli_type(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "table" => Some(ObjectType::Table),
            "view" => Some(ObjectType::View),
            "trigger" => Some(ObjectType::Trigger),
            "proc" | "procedure" => Some(ObjectType::Procedure),
            "function" | "fn" => Some(ObjectType::Function),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            ObjectType::Table => "table",
            ObjectType::View => "view",
            ObjectType::Trigger => "trigger",
            ObjectType::Procedure => "procedure",
            ObjectType::Function => "function",
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            ObjectType::Table => "Table",
            ObjectType::View => "View",
            ObjectType::Trigger => "Trigger",
            ObjectType::Procedure => "Procedure",
            ObjectType::Function => "Function",
        }
    }

    fn sql_type_filter(&self) -> &'static str {
        match self {
            ObjectType::Table => "'U'",
            ObjectType::View => "'V'",
            ObjectType::Trigger => "'TR'",
            ObjectType::Procedure => "'P'",
            ObjectType::Function => "'FN', 'IF', 'TF', 'AF'",
        }
    }
}

#[derive(Debug, Clone)]
struct IndexInfo {
    name: String,
    index_type: String,
    is_unique: bool,
    is_primary: bool,
    key_columns: Vec<String>,
    included_columns: Vec<String>,
}

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

#[derive(Debug, Clone)]
struct ConstraintInfo {
    name: String,
    constraint_type: String,
    columns: Vec<String>,
}

/// Async function for describing a table with an existing client connection
pub async fn describe_table_async(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    table_name: &str,
    schema: Option<&str>,
    cmd: &DescribeArgs,
    format: crate::config::OutputFormat,
    json_pretty: bool,
) -> Result<String> {
    describe_table(client, table_name, schema, cmd, format, json_pretty).await
}

pub fn run(args: &CliArgs, cmd: &DescribeArgs) -> Result<()> {
    let object_name = cmd
        .object
        .as_deref()
        .ok_or_else(|| anyhow!("Missing object name. Usage: sscli describe <object>"))?;

    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);
    let json_pretty = common::json_pretty(&resolved);
    let schema = cmd.schema.clone();

    // If user specified a type, use it; otherwise auto-detect
    let forced_type = cmd
        .object_type
        .as_ref()
        .and_then(|t| ObjectType::from_cli_type(t));

    let result = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;
        describe_object(
            &mut client,
            object_name,
            schema.as_deref(),
            forced_type,
            cmd,
            format,
            json_pretty,
        )
        .await
    })?;

    if !args.quiet {
        print!("{}", result);
    }

    Ok(())
}

async fn describe_object(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    object_name: &str,
    schema: Option<&str>,
    forced_type: Option<ObjectType>,
    cmd: &DescribeArgs,
    format: OutputFormat,
    json_pretty: bool,
) -> Result<String> {
    // Detect all matching objects
    let matches = detect_all_matches(client, object_name, schema, forced_type.as_ref()).await?;

    if matches!(format, OutputFormat::Json) {
        // JSON mode: wrap multiple matches in a "matches" array
        describe_all_json(client, object_name, &matches, cmd, json_pretty).await
    } else {
        // Text mode: describe each match with headers
        describe_all_text(client, object_name, &matches, cmd, format).await
    }
}

async fn describe_all_json(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    object_name: &str,
    matches: &[ObjectMatch],
    cmd: &DescribeArgs,
    json_pretty: bool,
) -> Result<String> {
    let mut results: Vec<serde_json::Value> = Vec::new();

    for m in matches {
        let json_str = match m.object_type {
            ObjectType::Table => {
                describe_table(
                    client,
                    object_name,
                    Some(&m.schema),
                    cmd,
                    OutputFormat::Json,
                    json_pretty,
                )
                .await?
            }
            ObjectType::View => {
                describe_view(
                    client,
                    object_name,
                    Some(&m.schema),
                    cmd,
                    OutputFormat::Json,
                    json_pretty,
                )
                .await?
            }
            ObjectType::Trigger => {
                describe_trigger(
                    client,
                    object_name,
                    Some(&m.schema),
                    cmd,
                    OutputFormat::Json,
                    json_pretty,
                )
                .await?
            }
            ObjectType::Procedure => {
                describe_procedure(
                    client,
                    object_name,
                    Some(&m.schema),
                    cmd,
                    OutputFormat::Json,
                    json_pretty,
                )
                .await?
            }
            ObjectType::Function => {
                describe_function(
                    client,
                    object_name,
                    Some(&m.schema),
                    cmd,
                    OutputFormat::Json,
                    json_pretty,
                )
                .await?
            }
        };
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
            results.push(v);
        }
    }

    // Single match: return flat object, multiple matches: wrap in array with guidance
    if results.len() == 1 {
        json_out::emit_json_value(&results[0], json_pretty)
    } else {
        let payload = json!({
            "matches": results,
            "guidance": format!(
                "Multiple objects match '{}'. Filter with --schema <name> or --type <type>.",
                object_name
            )
        });
        json_out::emit_json_value(&payload, json_pretty)
    }
}

async fn describe_all_text(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    object_name: &str,
    matches: &[ObjectMatch],
    cmd: &DescribeArgs,
    format: OutputFormat,
) -> Result<String> {
    let mut output = String::new();
    let multiple = matches.len() > 1;

    for (i, m) in matches.iter().enumerate() {
        // Always show header with schema.name (type)
        let type_label = m.object_type.display_name();
        if i > 0 {
            output.push_str("\n---\n\n");
        }
        output.push_str(&format!(
            "## {}.{} ({})\n\n",
            m.schema, object_name, type_label
        ));

        let section = match m.object_type {
            ObjectType::Table => {
                describe_table(client, object_name, Some(&m.schema), cmd, format, false).await?
            }
            ObjectType::View => {
                describe_view(client, object_name, Some(&m.schema), cmd, format, false).await?
            }
            ObjectType::Trigger => {
                describe_trigger(client, object_name, Some(&m.schema), cmd, format, false).await?
            }
            ObjectType::Procedure => {
                describe_procedure(client, object_name, Some(&m.schema), cmd, format, false).await?
            }
            ObjectType::Function => {
                describe_function(client, object_name, Some(&m.schema), cmd, format, false).await?
            }
        };
        output.push_str(&section);
    }

    // Add disambiguation guidance if multiple matches
    if multiple {
        output.push_str("\n---\n");
        output.push_str(&format!(
            "Multiple objects match '{}'. Filter with: --schema <name>, --type <type>\n",
            object_name
        ));
    }

    Ok(output)
}

/// Represents a matched object with its type and schema
#[derive(Debug, Clone)]
struct ObjectMatch {
    object_type: ObjectType,
    schema: String,
}

/// Detect all matching objects for the given name
async fn detect_all_matches(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    object_name: &str,
    schema: Option<&str>,
    forced_type: Option<&ObjectType>,
) -> Result<Vec<ObjectMatch>> {
    // If user forced a type, only return that type
    if let Some(forced) = forced_type {
        let sql = format!(
            r#"
SELECT s.name AS schema_name
FROM sys.objects o
INNER JOIN sys.schemas s ON o.schema_id = s.schema_id
WHERE o.name = @P1
  AND (@P2 IS NULL OR s.name = @P2)
  AND o.type IN ({})
ORDER BY s.name
"#,
            forced.sql_type_filter()
        );
        let mut query = Query::new(sql);
        query.bind(object_name);
        query.bind(schema);
        let result_sets = executor::run_query(query, client).await?;
        let result_set = result_sets.into_iter().next().unwrap_or_default();

        let matches: Vec<ObjectMatch> = result_set
            .rows
            .iter()
            .filter_map(|row| {
                row.first().and_then(|v| match v {
                    Value::Text(s) => Some(ObjectMatch {
                        object_type: forced.clone(),
                        schema: s.clone(),
                    }),
                    _ => None,
                })
            })
            .collect();

        if matches.is_empty() {
            return Err(anyhow!("{} '{}' not found", forced.as_str(), object_name));
        }
        return Ok(matches);
    }

    // Auto-detect: search all object types, return ALL matches
    let sql = r#"
SELECT o.type, s.name AS schema_name
FROM sys.objects o
INNER JOIN sys.schemas s ON o.schema_id = s.schema_id
WHERE o.name = @P1
  AND (@P2 IS NULL OR s.name = @P2)
  AND o.type IN ('U', 'V', 'TR', 'P', 'FN', 'IF', 'TF', 'AF')
ORDER BY
    CASE o.type
        WHEN 'U' THEN 1  -- Tables first
        WHEN 'V' THEN 2  -- Then views
        WHEN 'P' THEN 3  -- Then procs
        WHEN 'TR' THEN 4 -- Then triggers
        ELSE 5           -- Then functions
    END,
    s.name
"#;
    let mut query = Query::new(sql);
    query.bind(object_name);
    query.bind(schema);
    let result_sets = executor::run_query(query, client).await?;
    let result_set = result_sets.into_iter().next().unwrap_or_default();

    let matches: Vec<ObjectMatch> = result_set
        .rows
        .iter()
        .filter_map(|row| {
            let type_str = match row.first() {
                Some(Value::Text(s)) => s.as_str(),
                _ => return None,
            };
            let schema_name = match row.get(1) {
                Some(Value::Text(s)) => s.clone(),
                _ => return None,
            };
            ObjectType::from_sql_type(type_str).map(|obj_type| ObjectMatch {
                object_type: obj_type,
                schema: schema_name,
            })
        })
        .collect();

    if matches.is_empty() {
        return Err(anyhow!("Object '{}' not found", object_name));
    }

    Ok(matches)
}

async fn describe_table(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    table_name: &str,
    schema: Option<&str>,
    cmd: &DescribeArgs,
    format: OutputFormat,
    json_pretty: bool,
) -> Result<String> {
    let include_indexes = !cmd.no_indexes;
    let include_triggers = !cmd.no_triggers;
    let include_ddl = !cmd.no_ddl;
    let include_fks = cmd.include_all || cmd.include_fks;
    let include_constraints = cmd.include_all || cmd.include_constraints;

    let columns_rs = fetch_columns(client, table_name, schema).await?;
    let indexes = if include_indexes {
        fetch_indexes(client, table_name, schema).await?
    } else {
        Vec::new()
    };
    let fks = if include_fks {
        fetch_foreign_keys(client, table_name, schema).await?
    } else {
        Vec::new()
    };
    let constraints = if include_constraints {
        fetch_constraints(client, table_name, schema).await?
    } else {
        Vec::new()
    };
    let triggers_rs = if include_triggers {
        let t = fetch_triggers(client, table_name, schema).await?;
        if t.rows.is_empty() {
            None
        } else {
            Some(t)
        }
    } else {
        None
    };
    let ddl = if include_ddl {
        fetch_table_ddl(client, table_name, schema).await?
    } else {
        None
    };

    format_table_output(
        table_name,
        schema.unwrap_or("dbo"),
        &columns_rs,
        &indexes,
        &fks,
        &constraints,
        triggers_rs.as_ref(),
        ddl.as_deref(),
        format,
        json_pretty,
        include_indexes,
        include_fks,
        include_constraints,
    )
}

async fn describe_view(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    view_name: &str,
    schema: Option<&str>,
    cmd: &DescribeArgs,
    format: OutputFormat,
    json_pretty: bool,
) -> Result<String> {
    let include_ddl = !cmd.no_ddl;

    let columns_rs = fetch_columns(client, view_name, schema).await?;
    let ddl = if include_ddl {
        fetch_object_definition(client, view_name, schema).await?
    } else {
        None
    };

    format_view_output(
        view_name,
        schema.unwrap_or("dbo"),
        &columns_rs,
        ddl.as_deref(),
        format,
        json_pretty,
    )
}

async fn describe_trigger(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    trigger_name: &str,
    schema: Option<&str>,
    cmd: &DescribeArgs,
    format: OutputFormat,
    json_pretty: bool,
) -> Result<String> {
    let include_ddl = !cmd.no_ddl;

    // Get trigger metadata
    let sql = r#"
SELECT
    tr.name AS trigger_name,
    s.name AS schema_name,
    OBJECT_NAME(tr.parent_id) AS parent_table,
    tr.is_disabled,
    tr.is_instead_of_trigger,
    STUFF((
        SELECT ', ' + te.type_desc
        FROM sys.trigger_events te
        WHERE te.object_id = tr.object_id
        FOR XML PATH('')
    ), 1, 2, '') AS events
FROM sys.triggers tr
LEFT JOIN sys.objects o ON tr.object_id = o.object_id
LEFT JOIN sys.schemas s ON o.schema_id = s.schema_id
WHERE tr.name = @P1
  AND (@P2 IS NULL OR s.name = @P2)
"#;
    let mut query = Query::new(sql);
    query.bind(trigger_name);
    query.bind(schema);
    let result_sets = executor::run_query(query, client).await?;
    let result_set = result_sets.into_iter().next().unwrap_or_default();

    if result_set.rows.is_empty() {
        return Err(anyhow!("Trigger '{}' not found", trigger_name));
    }

    let row = result_set.rows.first().unwrap();
    let parent_table = value_to_string(row.get(2));
    let is_disabled = value_to_bool(row.get(3));
    let is_instead_of = value_to_bool(row.get(4));
    let events = value_to_string(row.get(5));

    let ddl = if include_ddl {
        fetch_object_definition(client, trigger_name, schema).await?
    } else {
        None
    };

    let mut output = String::new();

    if matches!(format, OutputFormat::Json) {
        let mut payload = json!({
            "object": {
                "name": trigger_name,
                "schema": schema.unwrap_or("dbo"),
                "type": "trigger"
            },
            "parentTable": parent_table,
            "isDisabled": is_disabled,
            "isInsteadOf": is_instead_of,
            "events": events,
        });
        if let Some(ddl_text) = ddl {
            payload["ddl"] = json!(ddl_text);
        }
        output = json_out::emit_json_value(&payload, json_pretty)?;
    } else {
        if let Some(ddl_text) = ddl {
            output.push_str("Definition\n```sql\n");
            output.push_str(&ddl_text);
            output.push_str("\n```\n\n");
        }

        output.push_str(&format!("Parent Table: {}\n", parent_table));
        output.push_str(&format!("Events: {}\n", events));
        output.push_str(&format!(
            "Disabled: {}\n",
            if is_disabled { "yes" } else { "no" }
        ));
        output.push_str(&format!(
            "Instead Of: {}\n",
            if is_instead_of { "yes" } else { "no" }
        ));
    }

    Ok(output)
}

async fn describe_procedure(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    proc_name: &str,
    schema: Option<&str>,
    cmd: &DescribeArgs,
    format: OutputFormat,
    json_pretty: bool,
) -> Result<String> {
    let include_ddl = !cmd.no_ddl;

    // Get procedure parameters
    let sql = r#"
SELECT
    p.name AS param_name,
    TYPE_NAME(p.user_type_id) AS data_type,
    p.max_length,
    p.precision,
    p.scale,
    p.is_output,
    p.has_default_value,
    p.default_value
FROM sys.parameters p
INNER JOIN sys.objects o ON p.object_id = o.object_id
INNER JOIN sys.schemas s ON o.schema_id = s.schema_id
WHERE o.name = @P1
  AND (@P2 IS NULL OR s.name = @P2)
  AND o.type = 'P'
ORDER BY p.parameter_id
"#;
    let mut query = Query::new(sql);
    query.bind(proc_name);
    query.bind(schema);
    let result_sets = executor::run_query(query, client).await?;
    let params_rs = result_sets.into_iter().next().unwrap_or_default();

    let ddl = if include_ddl {
        fetch_object_definition(client, proc_name, schema).await?
    } else {
        None
    };

    let mut output = String::new();

    if matches!(format, OutputFormat::Json) {
        let params: Vec<_> = params_rs.rows.iter().map(|row| {
            json!({
                "name": value_to_string(row.first()),
                "dataType": value_to_string(row.get(1)),
                "maxLength": row.get(2).and_then(|v| match v { Value::Int(i) => Some(*i), _ => None }),
                "isOutput": value_to_bool(row.get(5)),
            })
        }).collect();

        let mut payload = json!({
            "object": {
                "name": proc_name,
                "schema": schema.unwrap_or("dbo"),
                "type": "procedure"
            },
            "parameters": params,
        });
        if let Some(ddl_text) = ddl {
            payload["ddl"] = json!(ddl_text);
        }
        output = json_out::emit_json_value(&payload, json_pretty)?;
    } else {
        if let Some(ddl_text) = ddl {
            output.push_str("Definition\n```sql\n");
            output.push_str(&ddl_text);
            output.push_str("\n```\n\n");
        }

        if !params_rs.rows.is_empty() {
            output.push_str("Parameters\n");
            let params_display = format_params_result_set(&params_rs);
            output.push_str(&table::render_result_set_table(
                &params_display,
                format,
                &TableOptions::default(),
            ));
        } else {
            output.push_str("(no parameters)\n");
        }
    }

    Ok(output)
}

async fn describe_function(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    fn_name: &str,
    schema: Option<&str>,
    cmd: &DescribeArgs,
    format: OutputFormat,
    json_pretty: bool,
) -> Result<String> {
    let include_ddl = !cmd.no_ddl;

    // Get function type and return type
    let sql = r#"
SELECT
    o.type_desc,
    ISNULL(TYPE_NAME(r.user_type_id), 'TABLE') AS return_type
FROM sys.objects o
INNER JOIN sys.schemas s ON o.schema_id = s.schema_id
LEFT JOIN sys.parameters r ON o.object_id = r.object_id AND r.parameter_id = 0
WHERE o.name = @P1
  AND (@P2 IS NULL OR s.name = @P2)
  AND o.type IN ('FN', 'IF', 'TF', 'AF')
"#;
    let mut query = Query::new(sql);
    query.bind(fn_name);
    query.bind(schema);
    let result_sets = executor::run_query(query, client).await?;
    let meta_rs = result_sets.into_iter().next().unwrap_or_default();

    let (fn_type, return_type) = if let Some(row) = meta_rs.rows.first() {
        (value_to_string(row.first()), value_to_string(row.get(1)))
    } else {
        return Err(anyhow!("Function '{}' not found", fn_name));
    };

    // Get function parameters (excluding return param at position 0)
    let sql = r#"
SELECT
    p.name AS param_name,
    TYPE_NAME(p.user_type_id) AS data_type,
    p.max_length,
    p.is_output
FROM sys.parameters p
INNER JOIN sys.objects o ON p.object_id = o.object_id
INNER JOIN sys.schemas s ON o.schema_id = s.schema_id
WHERE o.name = @P1
  AND (@P2 IS NULL OR s.name = @P2)
  AND o.type IN ('FN', 'IF', 'TF', 'AF')
  AND p.parameter_id > 0
ORDER BY p.parameter_id
"#;
    let mut query = Query::new(sql);
    query.bind(fn_name);
    query.bind(schema);
    let result_sets = executor::run_query(query, client).await?;
    let params_rs = result_sets.into_iter().next().unwrap_or_default();

    let ddl = if include_ddl {
        fetch_object_definition(client, fn_name, schema).await?
    } else {
        None
    };

    let mut output = String::new();

    if matches!(format, OutputFormat::Json) {
        let params: Vec<_> = params_rs
            .rows
            .iter()
            .map(|row| {
                json!({
                    "name": value_to_string(row.first()),
                    "dataType": value_to_string(row.get(1)),
                })
            })
            .collect();

        let mut payload = json!({
            "object": {
                "name": fn_name,
                "schema": schema.unwrap_or("dbo"),
                "type": "function"
            },
            "functionType": fn_type,
            "returnType": return_type,
            "parameters": params,
        });
        if let Some(ddl_text) = ddl {
            payload["ddl"] = json!(ddl_text);
        }
        output = json_out::emit_json_value(&payload, json_pretty)?;
    } else {
        if let Some(ddl_text) = ddl {
            output.push_str("Definition\n```sql\n");
            output.push_str(&ddl_text);
            output.push_str("\n```\n\n");
        }

        output.push_str(&format!("Type: {}\n", fn_type));
        output.push_str(&format!("Returns: {}\n\n", return_type));

        if !params_rs.rows.is_empty() {
            output.push_str("Parameters\n");
            let params_display = format_fn_params_result_set(&params_rs);
            output.push_str(&table::render_result_set_table(
                &params_display,
                format,
                &TableOptions::default(),
            ));
        } else {
            output.push_str("(no parameters)\n");
        }
    }

    Ok(output)
}

// Helper functions

async fn fetch_columns(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    table_name: &str,
    schema: Option<&str>,
) -> Result<ResultSet> {
    let sql = r#"
SELECT
    COLUMN_NAME AS name,
    DATA_TYPE AS dataType,
    IS_NULLABLE AS isNullable,
    COLUMN_DEFAULT AS defaultValue,
    CHARACTER_MAXIMUM_LENGTH AS maxLength,
    NUMERIC_PRECISION AS numericPrecision,
    NUMERIC_SCALE AS numericScale
FROM INFORMATION_SCHEMA.COLUMNS
WHERE TABLE_NAME = @P1
  AND (@P2 IS NULL OR TABLE_SCHEMA = @P2)
ORDER BY ORDINAL_POSITION;
"#;
    let mut query = Query::new(sql);
    query.bind(table_name);
    query.bind(schema);
    let result_sets = executor::run_query(query, client).await?;
    Ok(result_sets.into_iter().next().unwrap_or_default())
}

async fn fetch_indexes(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    table_name: &str,
    schema: Option<&str>,
) -> Result<Vec<IndexInfo>> {
    let sql = r#"
SELECT
    i.name AS index_name,
    i.type_desc AS index_type,
    i.is_unique,
    i.is_primary_key,
    ic.is_included_column,
    ic.key_ordinal,
    c.name AS column_name
FROM sys.indexes i
INNER JOIN sys.objects o ON i.object_id = o.object_id
INNER JOIN sys.schemas s ON o.schema_id = s.schema_id
INNER JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id
INNER JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id
WHERE o.type = 'U'
  AND o.name = @P1
  AND (@P2 IS NULL OR s.name = @P2)
  AND i.name IS NOT NULL
  AND i.is_hypothetical = 0
ORDER BY i.name, ic.key_ordinal, ic.index_column_id;
"#;
    let mut query = Query::new(sql);
    query.bind(table_name);
    query.bind(schema);
    let result_sets = executor::run_query(query, client).await?;
    let result_set = result_sets.into_iter().next().unwrap_or_default();

    let mut grouped: BTreeMap<String, IndexInfo> = BTreeMap::new();
    for row in result_set.rows {
        let index_name = value_to_string(row.first());
        let entry = grouped
            .entry(index_name.clone())
            .or_insert_with(|| IndexInfo {
                name: index_name.clone(),
                index_type: value_to_string(row.get(1)),
                is_unique: value_to_bool(row.get(2)),
                is_primary: value_to_bool(row.get(3)),
                key_columns: Vec::new(),
                included_columns: Vec::new(),
            });
        let column_name = value_to_string(row.get(6));
        let is_included = value_to_bool(row.get(4));
        if is_included {
            if !entry.included_columns.contains(&column_name) {
                entry.included_columns.push(column_name);
            }
        } else if !entry.key_columns.contains(&column_name) {
            entry.key_columns.push(column_name);
        }
    }

    Ok(grouped.into_values().collect())
}

async fn fetch_foreign_keys(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    table_name: &str,
    schema: Option<&str>,
) -> Result<Vec<ForeignKeyInfo>> {
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
WHERE (parent.name = @P1 AND (@P2 IS NULL OR schParent.name = @P2))
   OR (referenced.name = @P1 AND (@P2 IS NULL OR schRef.name = @P2))
ORDER BY fk.name, fkc.constraint_column_id;
"#;
    let mut query = Query::new(sql);
    query.bind(table_name);
    query.bind(schema);
    let result_sets = executor::run_query(query, client).await?;
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

        let is_outbound = parent_table.eq_ignore_ascii_case(table_name);
        let entry = grouped
            .entry(fk_name.clone())
            .or_insert_with(|| ForeignKeyInfo {
                name: fk_name.clone(),
                direction: if is_outbound {
                    "outbound".to_string()
                } else {
                    "inbound".to_string()
                },
                from_schema: if is_outbound {
                    parent_schema.clone()
                } else {
                    ref_schema.clone()
                },
                from_table: if is_outbound {
                    parent_table.clone()
                } else {
                    ref_table.clone()
                },
                to_schema: if is_outbound {
                    ref_schema.clone()
                } else {
                    parent_schema.clone()
                },
                to_table: if is_outbound {
                    ref_table.clone()
                } else {
                    parent_table.clone()
                },
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

    Ok(grouped.into_values().collect())
}

async fn fetch_constraints(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    table_name: &str,
    schema: Option<&str>,
) -> Result<Vec<ConstraintInfo>> {
    let sql = r#"
SELECT
    tc.CONSTRAINT_NAME AS constraintName,
    tc.CONSTRAINT_TYPE AS constraintType,
    kcu.COLUMN_NAME AS columnName
FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc
LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu
  ON tc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME
 AND tc.TABLE_SCHEMA = kcu.TABLE_SCHEMA
WHERE tc.TABLE_NAME = @P1
  AND (@P2 IS NULL OR tc.TABLE_SCHEMA = @P2)
ORDER BY tc.CONSTRAINT_NAME, kcu.ORDINAL_POSITION;
"#;
    let mut query = Query::new(sql);
    query.bind(table_name);
    query.bind(schema);
    let result_sets = executor::run_query(query, client).await?;
    let result_set = result_sets.into_iter().next().unwrap_or_default();

    let mut grouped: BTreeMap<String, ConstraintInfo> = BTreeMap::new();
    for row in result_set.rows {
        let name = value_to_string(row.first());
        let constraint_type = value_to_string(row.get(1));
        let column_name = value_to_string(row.get(2));
        let entry = grouped
            .entry(name.clone())
            .or_insert_with(|| ConstraintInfo {
                name: name.clone(),
                constraint_type: constraint_type.clone(),
                columns: Vec::new(),
            });
        if !column_name.is_empty() {
            entry.columns.push(column_name);
        }
    }

    Ok(grouped.into_values().collect())
}

async fn fetch_triggers(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    table_name: &str,
    schema: Option<&str>,
) -> Result<ResultSet> {
    let sql = r#"
SELECT
    tr.name AS name,
    CASE WHEN tr.is_disabled = 1 THEN 'yes' ELSE 'no' END AS isDisabled,
    CONVERT(varchar, tr.create_date, 120) AS createdAt,
    CONVERT(varchar, tr.modify_date, 120) AS modifiedAt
FROM sys.triggers tr
INNER JOIN sys.tables t ON tr.parent_id = t.object_id
INNER JOIN sys.schemas s ON t.schema_id = s.schema_id
WHERE t.name = @P1
  AND (@P2 IS NULL OR s.name = @P2)
ORDER BY tr.name;
"#;
    let mut query = Query::new(sql);
    query.bind(table_name);
    query.bind(schema);
    let result_sets = executor::run_query(query, client).await?;
    Ok(result_sets.into_iter().next().unwrap_or_default())
}

async fn fetch_table_ddl(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    table_name: &str,
    schema: Option<&str>,
) -> Result<Option<String>> {
    let schema_name = schema.unwrap_or("dbo");

    // Note: seed_value and increment_value are sql_variant, so cast to bigint
    let sql = r#"
SELECT
    c.name AS column_name,
    t.name AS data_type,
    c.max_length,
    c.precision,
    c.scale,
    c.is_nullable,
    c.is_identity,
    CAST(ic.seed_value AS bigint) AS seed_value,
    CAST(ic.increment_value AS bigint) AS increment_value,
    dc.definition AS default_value,
    cc.definition AS computed_definition,
    c.is_computed
FROM sys.columns c
INNER JOIN sys.types t ON c.user_type_id = t.user_type_id
INNER JOIN sys.objects o ON c.object_id = o.object_id
INNER JOIN sys.schemas s ON o.schema_id = s.schema_id
LEFT JOIN sys.default_constraints dc ON c.default_object_id = dc.object_id
LEFT JOIN sys.computed_columns cc ON c.object_id = cc.object_id AND c.column_id = cc.column_id
LEFT JOIN sys.identity_columns ic ON c.object_id = ic.object_id AND c.column_id = ic.column_id
WHERE o.name = @P1
  AND (@P2 IS NULL OR s.name = @P2)
ORDER BY c.column_id
"#;
    let mut query = Query::new(sql);
    query.bind(table_name);
    query.bind(schema);
    let result_sets = executor::run_query(query, client).await?;
    let result_set = result_sets.into_iter().next().unwrap_or_default();

    if result_set.rows.is_empty() {
        return Ok(None);
    }

    let mut ddl = format!("CREATE TABLE [{}].[{}] (\n", schema_name, table_name);
    let mut column_defs: Vec<String> = Vec::new();

    for row in &result_set.rows {
        let col_name = value_to_string(row.first());
        let data_type = value_to_string(row.get(1));
        let max_length = row.get(2).and_then(|v| match v {
            Value::Int(i) => Some(*i),
            _ => None,
        });
        let precision = row.get(3).and_then(|v| match v {
            Value::Int(i) => Some(*i as u8),
            _ => None,
        });
        let scale = row.get(4).and_then(|v| match v {
            Value::Int(i) => Some(*i as u8),
            _ => None,
        });
        let is_nullable = value_to_bool(row.get(5));
        let is_identity = value_to_bool(row.get(6));
        let seed = row.get(7).and_then(|v| match v {
            Value::Int(i) => Some(*i),
            Value::Text(s) => s.parse().ok(),
            _ => None,
        });
        let increment = row.get(8).and_then(|v| match v {
            Value::Int(i) => Some(*i),
            Value::Text(s) => s.parse().ok(),
            _ => None,
        });
        let default_value = value_to_string(row.get(9));
        let computed_def = value_to_string(row.get(10));
        let is_computed = value_to_bool(row.get(11));

        let type_spec = format_type_spec(&data_type, max_length, precision, scale);

        let mut col_def = format!("    [{}] {}", col_name, type_spec);

        if is_computed && !computed_def.is_empty() {
            col_def = format!("    [{}] AS {}", col_name, computed_def);
        } else {
            if is_identity {
                let s = seed.unwrap_or(1);
                let i = increment.unwrap_or(1);
                col_def.push_str(&format!(" IDENTITY({}, {})", s, i));
            }
            if !is_nullable {
                col_def.push_str(" NOT NULL");
            } else {
                col_def.push_str(" NULL");
            }
            if !default_value.is_empty() {
                col_def.push_str(&format!(" DEFAULT {}", default_value));
            }
        }

        column_defs.push(col_def);
    }

    ddl.push_str(&column_defs.join(",\n"));
    ddl.push_str("\n);");

    Ok(Some(ddl))
}

async fn fetch_object_definition(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    object_name: &str,
    schema: Option<&str>,
) -> Result<Option<String>> {
    let schema_name = schema.unwrap_or("dbo");
    let full_name = format!("[{}].[{}]", schema_name, object_name);

    let sql = "SELECT OBJECT_DEFINITION(OBJECT_ID(@P1))";
    let mut query = Query::new(sql);
    query.bind(&full_name);
    let result_sets = executor::run_query(query, client).await?;
    let result_set = result_sets.into_iter().next().unwrap_or_default();

    Ok(result_set.rows.first().and_then(|row| {
        row.first().and_then(|v| match v {
            Value::Text(s) => Some(s.clone()),
            _ => None,
        })
    }))
}

fn format_type_spec(
    data_type: &str,
    max_length: Option<i64>,
    precision: Option<u8>,
    scale: Option<u8>,
) -> String {
    match data_type.to_lowercase().as_str() {
        "varchar" | "nvarchar" | "char" | "nchar" | "varbinary" | "binary" => {
            if let Some(len) = max_length {
                if len == -1 {
                    format!("{}(MAX)", data_type)
                } else {
                    let display_len = if data_type.starts_with('n') {
                        len / 2
                    } else {
                        len
                    };
                    format!("{}({})", data_type, display_len)
                }
            } else {
                data_type.to_string()
            }
        }
        "decimal" | "numeric" => {
            let p = precision.unwrap_or(18);
            let s = scale.unwrap_or(0);
            format!("{}({}, {})", data_type, p, s)
        }
        "float" => {
            if let Some(p) = precision {
                if p != 53 {
                    return format!("float({})", p);
                }
            }
            "float".to_string()
        }
        "datetime2" | "datetimeoffset" | "time" => {
            if let Some(s) = scale {
                if s != 7 {
                    return format!("{}({})", data_type, s);
                }
            }
            data_type.to_string()
        }
        _ => data_type.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn format_table_output(
    table_name: &str,
    schema: &str,
    columns_rs: &ResultSet,
    indexes: &[IndexInfo],
    fks: &[ForeignKeyInfo],
    constraints: &[ConstraintInfo],
    triggers_rs: Option<&ResultSet>,
    ddl: Option<&str>,
    format: OutputFormat,
    json_pretty: bool,
    include_indexes: bool,
    include_fks: bool,
    include_constraints: bool,
) -> Result<String> {
    let mut output = String::new();

    if matches!(format, OutputFormat::Json) {
        let mut payload = json!({
            "object": {
                "schema": schema,
                "name": table_name,
                "type": "table"
            },
            "columns": json_out::result_set_rows_to_objects(columns_rs),
        });

        if include_indexes && !indexes.is_empty() {
            payload["indexes"] =
                serde_json::Value::Array(indexes.iter().map(index_to_json).collect());
        }
        if include_fks && !fks.is_empty() {
            payload["foreignKeys"] = serde_json::Value::Array(fks.iter().map(fk_to_json).collect());
        }
        if include_constraints && !constraints.is_empty() {
            payload["constraints"] = serde_json::Value::Array(
                constraints.iter().map(|c| json!({"name": c.name, "type": c.constraint_type, "columns": c.columns})).collect()
            );
        }
        if let Some(triggers) = triggers_rs {
            payload["triggers"] =
                serde_json::Value::Array(json_out::result_set_rows_to_objects(triggers));
        }
        if let Some(ddl_text) = ddl {
            payload["ddl"] = json!(ddl_text);
        }

        output = json_out::emit_json_value(&payload, json_pretty)?;
    } else {
        if let Some(ddl_text) = ddl {
            output.push_str("DDL\n```sql\n");
            output.push_str(ddl_text);
            output.push_str("\n```\n\n");
        }

        output.push_str("Columns\n");
        output.push_str(&table::render_result_set_table(
            columns_rs,
            format,
            &TableOptions::default(),
        ));

        if include_indexes && !indexes.is_empty() {
            output.push_str("\nIndexes\n");
            let rs = indexes_to_result_set(indexes);
            output.push_str(&table::render_result_set_table(
                &rs,
                format,
                &TableOptions::default(),
            ));
        }

        if include_fks && !fks.is_empty() {
            output.push_str("\nForeign Keys\n");
            let rs = fks_to_result_set(fks);
            output.push_str(&table::render_result_set_table(
                &rs,
                format,
                &TableOptions::default(),
            ));
        }

        if include_constraints && !constraints.is_empty() {
            output.push_str("\nConstraints\n");
            let rs = constraints_to_result_set(constraints);
            output.push_str(&table::render_result_set_table(
                &rs,
                format,
                &TableOptions::default(),
            ));
        }

        if let Some(triggers) = triggers_rs {
            output.push_str("\nTriggers\n");
            output.push_str(&table::render_result_set_table(
                triggers,
                format,
                &TableOptions::default(),
            ));
        }
    }

    Ok(output)
}

fn format_view_output(
    view_name: &str,
    schema: &str,
    columns_rs: &ResultSet,
    ddl: Option<&str>,
    format: OutputFormat,
    json_pretty: bool,
) -> Result<String> {
    let mut output = String::new();

    if matches!(format, OutputFormat::Json) {
        let mut payload = json!({
            "object": {
                "schema": schema,
                "name": view_name,
                "type": "view"
            },
            "columns": json_out::result_set_rows_to_objects(columns_rs),
        });
        if let Some(ddl_text) = ddl {
            payload["ddl"] = json!(ddl_text);
        }
        output = json_out::emit_json_value(&payload, json_pretty)?;
    } else {
        if let Some(ddl_text) = ddl {
            output.push_str("Definition\n```sql\n");
            output.push_str(ddl_text);
            output.push_str("\n```\n\n");
        }

        output.push_str("Columns\n");
        output.push_str(&table::render_result_set_table(
            columns_rs,
            format,
            &TableOptions::default(),
        ));
    }

    Ok(output)
}

fn indexes_to_result_set(indexes: &[IndexInfo]) -> ResultSet {
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
    ];

    let rows = indexes
        .iter()
        .map(|idx| {
            vec![
                Value::Text(idx.name.clone()),
                Value::Text(idx.index_type.clone()),
                Value::Text(if idx.is_unique { "yes" } else { "no" }.to_string()),
                Value::Text(if idx.is_primary { "yes" } else { "no" }.to_string()),
                Value::Text(idx.key_columns.join(", ")),
                Value::Text(idx.included_columns.join(", ")),
            ]
        })
        .collect();

    ResultSet { columns, rows }
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

fn constraints_to_result_set(constraints: &[ConstraintInfo]) -> ResultSet {
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
            name: "columns".to_string(),
            data_type: None,
        },
    ];

    let rows = constraints
        .iter()
        .map(|c| {
            vec![
                Value::Text(c.name.clone()),
                Value::Text(c.constraint_type.clone()),
                Value::Text(c.columns.join(", ")),
            ]
        })
        .collect();

    ResultSet { columns, rows }
}

fn format_params_result_set(params_rs: &ResultSet) -> ResultSet {
    let columns = vec![
        Column {
            name: "name".to_string(),
            data_type: None,
        },
        Column {
            name: "dataType".to_string(),
            data_type: None,
        },
        Column {
            name: "maxLength".to_string(),
            data_type: None,
        },
        Column {
            name: "isOutput".to_string(),
            data_type: None,
        },
    ];

    let rows = params_rs
        .rows
        .iter()
        .map(|row| {
            vec![
                Value::Text(value_to_string(row.first())),
                Value::Text(value_to_string(row.get(1))),
                row.get(2).cloned().unwrap_or(Value::Null),
                Value::Text(
                    if value_to_bool(row.get(5)) {
                        "yes"
                    } else {
                        "no"
                    }
                    .to_string(),
                ),
            ]
        })
        .collect();

    ResultSet { columns, rows }
}

fn format_fn_params_result_set(params_rs: &ResultSet) -> ResultSet {
    let columns = vec![
        Column {
            name: "name".to_string(),
            data_type: None,
        },
        Column {
            name: "dataType".to_string(),
            data_type: None,
        },
    ];

    let rows = params_rs
        .rows
        .iter()
        .map(|row| {
            vec![
                Value::Text(value_to_string(row.first())),
                Value::Text(value_to_string(row.get(1))),
            ]
        })
        .collect();

    ResultSet { columns, rows }
}

fn index_to_json(index: &IndexInfo) -> serde_json::Value {
    json!({
        "name": index.name,
        "type": index.index_type,
        "unique": index.is_unique,
        "primary": index.is_primary,
        "keyColumns": index.key_columns,
        "includedColumns": index.included_columns,
    })
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

fn value_to_bool(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(v)) => *v,
        Some(Value::Int(v)) => *v != 0,
        Some(Value::Text(v)) => v == "1" || v.eq_ignore_ascii_case("true"),
        _ => false,
    }
}
