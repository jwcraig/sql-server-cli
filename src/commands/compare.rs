use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use chrono::Local;
use regex::Regex;
use serde::Serialize;
use similar::TextDiff;
use tiberius::Query;
use tokio::runtime::Runtime;

use crate::cli::{CliArgs, CompareArgs};
use crate::commands::common;
use crate::config::{CliOverrides, ConnectionSettings, OutputFormat, ResolvedConfig, parse_bool};
use crate::db::types::{Column, ResultSet, Value};
use crate::db::{client, executor};
use crate::output::json as json_out;

const DEFAULT_SCHEMAS: &[&str] = &["dbo", "web", "rbac", "notification"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    name: String,
    modules: Vec<ModuleRow>,
    indexes: Vec<IndexRow>,
    constraints: Vec<ConstraintRow>,
    tables: Vec<TableRow>,
    table_columns: Vec<TableColumnRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModuleRow {
    schema_name: String,
    name: String,
    r#type: String,
    definition: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IndexRow {
    schema_name: String,
    table_name: String,
    r#type: String,
    is_unique: bool,
    is_primary_key: bool,
    is_unique_constraint: bool,
    key_columns: String,
    include_columns: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConstraintRow {
    schema_name: String,
    table_name: String,
    name: String,
    r#type: String,
    definition: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TableRow {
    schema_name: String,
    table_name: String,
    columns: String,
    indexes: String,
    checks: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TableColumnRow {
    schema_name: String,
    table_name: String,
    column_id: i64,
    column_name: String,
    data_type: String,
    max_length: i64,
    precision: i64,
    scale: i64,
    is_nullable: bool,
    is_identity: bool,
    default_definition: String,
    computed_definition: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct DiffSet {
    changed: Vec<String>,
    missing_in_right: Vec<String>,
    missing_in_left: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompareSummary {
    modules: DiffSet,
    indexes: DiffSet,
    constraints: DiffSet,
    tables: DiffSet,
}

/// Execute the `compare` command: fetch snapshots, diff, and emit summary or apply script.
pub fn run(args: &CliArgs, cmd: &CompareArgs) -> Result<()> {
    let base_overrides = common::overrides_from_args(args);
    let source_profile = cmd.source.clone().or_else(|| args.profile.clone());
    let target_profile = cmd.target.clone();

    let source_cfg = apply_connection_override(
        resolve_profile(&base_overrides, source_profile.as_deref())?,
        &cmd.source_connection,
    )?;
    let target_cfg = apply_connection_override(
        resolve_profile(&base_overrides, Some(&target_profile))?,
        &cmd.target_connection,
    )?;

    let schemas = resolve_schemas(cmd, &source_cfg, &target_cfg);
    let rt = Runtime::new()?;

    let output_format = common::output_format(args, &source_cfg);
    let json_pretty = common::json_pretty(&source_cfg);

    let (source_snap, target_snap) = rt.block_on(async {
        tokio::try_join!(
            fetch_snapshot(&source_cfg.profile_name, &source_cfg.connection, &schemas),
            fetch_snapshot(&target_cfg.profile_name, &target_cfg.connection, &schemas),
        )
    })?;

    if let Some(object) = &cmd.object {
        handle_object_diff(args, cmd, &source_snap, &target_snap, object)?;
        return Ok(());
    }

    let summary = summarize(
        &source_snap,
        &target_snap,
        cmd.ignore_whitespace,
        cmd.strip_comments,
    );

    if cmd.apply_script {
        let script = render_apply_script(&summary, &source_snap, &target_snap, cmd.include_drops);
        write_apply_script(cmd.apply_path.as_deref(), &script)?;
        return Ok(());
    }

    if cmd.summary {
        output_summary(
            args,
            cmd,
            &summary,
            &source_snap.name,
            &target_snap.name,
            output_format,
            json_pretty,
        )?;
        let drifted = has_drift(&summary);
        if drifted {
            std::process::exit(3);
        }
        return Ok(());
    }

    // Full snapshot output
    if matches!(output_format, OutputFormat::Json) || args.output.json {
        let payload = serde_json::json!({
            "source": source_snap,
            "target": target_snap,
        });
        let body = json_out::emit_json_value(&payload, json_pretty)?;
        if !args.quiet {
            println!("{body}");
        }
    } else {
        // Pretty snapshot output is verbose; direct users to JSON
        println!("Use --json or --summary for readable output.");
    }

    Ok(())
}

fn resolve_profile(base: &CliOverrides, profile: Option<&str>) -> Result<ResolvedConfig> {
    let mut overrides = base.clone();
    overrides.profile = profile.map(str::to_string);
    crate::config::load_from_system(&overrides)
}

fn apply_connection_override(
    resolved: ResolvedConfig,
    connection_override: &Option<String>,
) -> Result<ResolvedConfig> {
    if let Some(raw) = connection_override {
        let parsed = parse_connection_string(raw)?;
        return Ok(ResolvedConfig {
            connection: parsed,
            ..resolved
        });
    }
    Ok(resolved)
}

fn resolve_schemas(
    cmd: &CompareArgs,
    source: &ResolvedConfig,
    target: &ResolvedConfig,
) -> Vec<String> {
    if let Some(list) = &cmd.schemas {
        let mut schemas: Vec<String> = list.iter().map(|s| s.trim().to_string()).collect();
        schemas.retain(|s| !s.is_empty());
        if !schemas.is_empty() {
            return schemas;
        }
    }

    if !source.connection.default_schemas.is_empty() {
        return source.connection.default_schemas.clone();
    }
    if !target.connection.default_schemas.is_empty() {
        return target.connection.default_schemas.clone();
    }

    DEFAULT_SCHEMAS.iter().map(|s| s.to_string()).collect()
}

async fn fetch_snapshot(
    name: &str,
    settings: &ConnectionSettings,
    schemas: &[String],
) -> Result<Snapshot> {
    let mut client = client::connect(settings).await?;
    let sql = build_sql(schemas);

    let modules_rs = executor::run_query(Query::new(sql.modules), &mut client).await?;
    let indexes_rs = executor::run_query(Query::new(sql.indexes), &mut client).await?;
    let constraints_rs = executor::run_query(Query::new(sql.constraints), &mut client).await?;
    let tables_rs = executor::run_query(Query::new(sql.tables), &mut client).await?;
    let cols_rs = executor::run_query(Query::new(sql.table_columns), &mut client).await?;

    let modules = map_modules(modules_rs.first());
    let indexes = map_indexes(indexes_rs.first());
    let constraints = map_constraints(constraints_rs.first());
    let tables = map_tables(tables_rs.first());
    let table_columns = map_table_columns(cols_rs.first());

    Ok(Snapshot {
        name: name.to_string(),
        modules,
        indexes,
        constraints,
        tables,
        table_columns,
    })
}

struct SnapshotSql {
    modules: String,
    indexes: String,
    constraints: String,
    tables: String,
    table_columns: String,
}

fn build_sql(schemas: &[String]) -> SnapshotSql {
    let schema_list = schemas
        .iter()
        .map(|s| format!("'{}'", s.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");

    let modules = format!(
        "
        SELECT s.name AS schema_name, o.name, o.type, ISNULL(sm.definition, N'') AS definition
        FROM sys.objects o
        JOIN sys.schemas s ON s.schema_id = o.schema_id
        LEFT JOIN sys.sql_modules sm ON sm.object_id = o.object_id
        WHERE s.name IN ({schema_list})
          AND o.type IN ('P','V','FN','IF','TF','TR')
        ORDER BY s.name, o.name, o.type;
    "
    );

    let tables = format!(
        "
        WITH cols AS (
          SELECT
            s.name AS schema_name,
            t.name AS table_name,
            c.column_id,
            c.name AS column_name,
            TYPE_NAME(c.user_type_id) AS data_type,
            c.max_length,
            c.precision,
            c.scale,
            c.is_nullable,
            c.is_identity,
            OBJECT_DEFINITION(dc.object_id) AS default_definition,
            cc.definition AS computed_definition
          FROM sys.tables t
          JOIN sys.schemas s ON s.schema_id = t.schema_id
          JOIN sys.columns c ON c.object_id = t.object_id
          LEFT JOIN sys.default_constraints dc ON dc.object_id = c.default_object_id
          LEFT JOIN sys.computed_columns cc ON cc.object_id = c.object_id AND cc.column_id = c.column_id
          WHERE s.name IN ({schema_list})
        ),
        colagg AS (
          SELECT schema_name, table_name,
                 STRING_AGG(
                   CONCAT(
                     column_id, ':', column_name, ':', data_type, ':', max_length, ':', precision, ':', scale, ':',
                     is_nullable, ':', is_identity, ':', ISNULL(default_definition,''), ':', ISNULL(computed_definition,'')
                   ), '||'
                 ) WITHIN GROUP (ORDER BY column_id) AS columns
          FROM cols
          GROUP BY schema_name, table_name
        ),
        idx AS (
          SELECT s.name AS schema_name, t.name AS table_name,
                 STRING_AGG(i.name, ',') WITHIN GROUP (ORDER BY i.name) AS idxs
          FROM sys.indexes i
          JOIN sys.tables t ON t.object_id = i.object_id
          JOIN sys.schemas s ON s.schema_id = t.schema_id
          WHERE s.name IN ({schema_list}) AND i.is_primary_key = 0 AND i.is_unique_constraint = 0 AND i.name IS NOT NULL
          GROUP BY s.name, t.name
        ),
        chk AS (
          SELECT s.name AS schema_name, t.name AS table_name,
                 STRING_AGG(c.definition, '||') WITHIN GROUP (ORDER BY c.name) AS checks
          FROM sys.check_constraints c
          JOIN sys.tables t ON t.object_id = c.parent_object_id
          JOIN sys.schemas s ON s.schema_id = t.schema_id
          WHERE s.name IN ({schema_list})
          GROUP BY s.name, t.name
        )
        SELECT
          c.schema_name,
          c.table_name,
          c.columns,
          ISNULL(i.idxs,'') AS indexes,
          ISNULL(ch.checks,'') AS checks
        FROM colagg c
        LEFT JOIN idx i ON i.schema_name = c.schema_name AND i.table_name = c.table_name
        LEFT JOIN chk ch ON ch.schema_name = c.schema_name AND ch.table_name = c.table_name;
    "
    );

    let table_columns = format!(
        "
        SELECT
          s.name AS schema_name,
          t.name AS table_name,
          c.column_id,
          c.name AS column_name,
          TYPE_NAME(c.user_type_id) AS data_type,
          c.max_length,
          c.precision,
          c.scale,
          c.is_nullable,
          c.is_identity,
          OBJECT_DEFINITION(dc.object_id) AS default_definition,
          cc.definition AS computed_definition
        FROM sys.tables t
        JOIN sys.schemas s ON s.schema_id = t.schema_id
        JOIN sys.columns c ON c.object_id = t.object_id
        LEFT JOIN sys.default_constraints dc ON dc.object_id = c.default_object_id
        LEFT JOIN sys.computed_columns cc ON cc.object_id = c.object_id AND cc.column_id = c.column_id
        WHERE s.name IN ({schema_list});
    "
    );

    let indexes = format!(
        "
        SELECT s.name AS schema_name,
               t.name AS table_name,
               i.name AS [index],
               i.type_desc,
               i.is_unique,
               i.is_primary_key,
               i.is_unique_constraint,
               key_cols.keys AS key_columns,
               include_cols.includes AS include_columns
        FROM sys.indexes i
          JOIN sys.tables t ON t.object_id = i.object_id
          JOIN sys.schemas s ON s.schema_id = t.schema_id
          CROSS APPLY (
            SELECT STRING_AGG(CONCAT(c.name, ' ', CASE WHEN ic.is_descending_key = 1 THEN 'DESC' ELSE 'ASC' END), ',')
                   WITHIN GROUP (ORDER BY ic.key_ordinal) AS keys
            FROM sys.index_columns ic
              JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id
            WHERE ic.object_id = i.object_id
              AND ic.index_id = i.index_id
              AND ic.is_included_column = 0
          ) key_cols
          CROSS APPLY (
            SELECT STRING_AGG(c.name, ',') AS includes
            FROM sys.index_columns ic
              JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id
            WHERE ic.object_id = i.object_id
              AND ic.index_id = i.index_id
              AND ic.is_included_column = 1
          ) include_cols
        WHERE s.name IN ({schema_list})
          AND i.is_hypothetical = 0
          AND i.name IS NOT NULL
        ORDER BY s.name, t.name, i.name;
    "
    );

    let constraints = format!(
        "
        SELECT s.name AS schema_name,
               o.name AS table_name,
               fk.name AS name,
               'FK' AS type,
               OBJECT_DEFINITION(fk.object_id) AS definition
        FROM sys.foreign_keys fk
          JOIN sys.objects o ON o.object_id = fk.parent_object_id
          JOIN sys.schemas s ON s.schema_id = o.schema_id
        WHERE s.name IN ({schema_list})
        UNION ALL
        SELECT s.name AS schema_name,
               t.name AS table_name,
               kc.name,
               kc.type_desc,
               OBJECT_DEFINITION(kc.object_id)
        FROM sys.key_constraints kc
          JOIN sys.tables t ON t.object_id = kc.parent_object_id
          JOIN sys.schemas s ON s.schema_id = t.schema_id
        WHERE s.name IN ({schema_list})
        UNION ALL
        SELECT s.name AS schema_name,
               t.name AS table_name,
               c.name,
               'CHECK',
               OBJECT_DEFINITION(c.object_id)
        FROM sys.check_constraints c
          JOIN sys.tables t ON t.object_id = c.parent_object_id
          JOIN sys.schemas s ON s.schema_id = t.schema_id
        WHERE s.name IN ({schema_list})
        UNION ALL
        SELECT s.name AS schema_name,
               t.name AS table_name,
               d.name,
               'DEFAULT',
               OBJECT_DEFINITION(d.object_id)
        FROM sys.default_constraints d
          JOIN sys.tables t ON t.object_id = d.parent_object_id
          JOIN sys.schemas s ON s.schema_id = t.schema_id
        WHERE s.name IN ({schema_list})
        ORDER BY schema_name, table_name, name;
    "
    );

    SnapshotSql {
        modules,
        indexes,
        constraints,
        tables,
        table_columns,
    }
}

fn map_modules(rs: Option<&ResultSet>) -> Vec<ModuleRow> {
    let rs = match rs {
        Some(rs) => rs,
        None => return Vec::new(),
    };
    let idx_schema = col_idx(&rs.columns, "schema_name");
    let idx_name = col_idx(&rs.columns, "name");
    let idx_type = col_idx(&rs.columns, "type");
    let idx_def = col_idx(&rs.columns, "definition");

    rs.rows
        .iter()
        .map(|row| ModuleRow {
            schema_name: get_text(row, idx_schema),
            name: get_text(row, idx_name),
            r#type: get_text(row, idx_type),
            definition: get_text(row, idx_def),
        })
        .collect()
}

fn map_indexes(rs: Option<&ResultSet>) -> Vec<IndexRow> {
    let rs = match rs {
        Some(rs) => rs,
        None => return Vec::new(),
    };
    let idx_schema = col_idx(&rs.columns, "schema_name");
    let idx_table = col_idx(&rs.columns, "table_name");
    let idx_type_desc = col_idx(&rs.columns, "type_desc");
    let idx_unique = col_idx(&rs.columns, "is_unique");
    let idx_pk = col_idx(&rs.columns, "is_primary_key");
    let idx_unique_const = col_idx(&rs.columns, "is_unique_constraint");
    let idx_keys = col_idx(&rs.columns, "key_columns");
    let idx_inc = col_idx(&rs.columns, "include_columns");

    rs.rows
        .iter()
        .map(|row| IndexRow {
            schema_name: get_text(row, idx_schema),
            table_name: get_text(row, idx_table),
            r#type: get_text(row, idx_type_desc),
            is_unique: get_bool(row, idx_unique),
            is_primary_key: get_bool(row, idx_pk),
            is_unique_constraint: get_bool(row, idx_unique_const),
            key_columns: get_text(row, idx_keys),
            include_columns: get_text(row, idx_inc),
        })
        .collect()
}

fn map_constraints(rs: Option<&ResultSet>) -> Vec<ConstraintRow> {
    let rs = match rs {
        Some(rs) => rs,
        None => return Vec::new(),
    };
    let idx_schema = col_idx(&rs.columns, "schema_name");
    let idx_table = col_idx(&rs.columns, "table_name");
    let idx_name = col_idx(&rs.columns, "name");
    let idx_type = col_idx(&rs.columns, "type");
    let idx_def = col_idx(&rs.columns, "definition");

    rs.rows
        .iter()
        .map(|row| ConstraintRow {
            schema_name: get_text(row, idx_schema),
            table_name: get_text(row, idx_table),
            name: get_text(row, idx_name),
            r#type: get_text(row, idx_type),
            definition: get_text(row, idx_def),
        })
        .collect()
}

fn map_tables(rs: Option<&ResultSet>) -> Vec<TableRow> {
    let rs = match rs {
        Some(rs) => rs,
        None => return Vec::new(),
    };
    let idx_schema = col_idx(&rs.columns, "schema_name");
    let idx_table = col_idx(&rs.columns, "table_name");
    let idx_cols = col_idx(&rs.columns, "columns");
    let idx_indexes = col_idx(&rs.columns, "indexes");
    let idx_checks = col_idx(&rs.columns, "checks");

    rs.rows
        .iter()
        .map(|row| TableRow {
            schema_name: get_text(row, idx_schema),
            table_name: get_text(row, idx_table),
            columns: get_text(row, idx_cols),
            indexes: get_text(row, idx_indexes),
            checks: get_text(row, idx_checks),
        })
        .collect()
}

fn map_table_columns(rs: Option<&ResultSet>) -> Vec<TableColumnRow> {
    let rs = match rs {
        Some(rs) => rs,
        None => return Vec::new(),
    };
    let idx_schema = col_idx(&rs.columns, "schema_name");
    let idx_table = col_idx(&rs.columns, "table_name");
    let idx_id = col_idx(&rs.columns, "column_id");
    let idx_name = col_idx(&rs.columns, "column_name");
    let idx_type = col_idx(&rs.columns, "data_type");
    let idx_len = col_idx(&rs.columns, "max_length");
    let idx_precision = col_idx(&rs.columns, "precision");
    let idx_scale = col_idx(&rs.columns, "scale");
    let idx_nullable = col_idx(&rs.columns, "is_nullable");
    let idx_identity = col_idx(&rs.columns, "is_identity");
    let idx_default = col_idx(&rs.columns, "default_definition");
    let idx_computed = col_idx(&rs.columns, "computed_definition");

    rs.rows
        .iter()
        .map(|row| TableColumnRow {
            schema_name: get_text(row, idx_schema),
            table_name: get_text(row, idx_table),
            column_id: get_int(row, idx_id),
            column_name: get_text(row, idx_name),
            data_type: get_text(row, idx_type),
            max_length: get_int(row, idx_len),
            precision: get_int(row, idx_precision),
            scale: get_int(row, idx_scale),
            is_nullable: get_bool(row, idx_nullable),
            is_identity: get_bool(row, idx_identity),
            default_definition: get_text(row, idx_default),
            computed_definition: get_text(row, idx_computed),
        })
        .collect()
}

fn col_idx(cols: &[Column], name: &str) -> Option<usize> {
    cols.iter().position(|c| c.name.eq_ignore_ascii_case(name))
}

fn get_text(row: &[Value], idx: Option<usize>) -> String {
    idx.and_then(|i| row.get(i))
        .map(|v| match v {
            Value::Text(t) => t.clone(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "".to_string(),
        })
        .unwrap_or_default()
}

fn get_int(row: &[Value], idx: Option<usize>) -> i64 {
    idx.and_then(|i| row.get(i))
        .map(|v| match v {
            Value::Int(i) => *i,
            Value::Float(f) => *f as i64,
            Value::Bool(b) => {
                if *b {
                    1
                } else {
                    0
                }
            }
            Value::Text(t) => t.parse::<i64>().unwrap_or(0),
            Value::Null => 0,
        })
        .unwrap_or(0)
}

fn get_bool(row: &[Value], idx: Option<usize>) -> bool {
    idx.and_then(|i| row.get(i))
        .map(|v| match v {
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Text(t) => matches!(t.as_str(), "1" | "true" | "True" | "TRUE"),
            Value::Null => false,
        })
        .unwrap_or(false)
}

fn normalize_definition(definition: &str, ignore_whitespace: bool, strip_comments: bool) -> String {
    let mut d = definition.replace("\r\n", "\n");
    if strip_comments {
        d = strip_sql_comments(&d);
    }
    d = d.trim().to_string();
    if ignore_whitespace {
        d = whitespace_re().replace_all(&d, " ").to_string();
    }
    d
}

fn strip_sql_comments(definition: &str) -> String {
    let without_block = block_comment_re().replace_all(definition, "");
    line_comment_re()
        .replace_all(&without_block, "")
        .to_string()
}

fn whitespace_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+").expect("valid regex"))
}

fn block_comment_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)/\*.*?\*/").expect("valid regex"))
}

fn line_comment_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)--.*$").expect("valid regex"))
}

fn build_module_map(
    rows: &[ModuleRow],
    ignore_whitespace: bool,
    strip_comments: bool,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for row in rows {
        let key = format!("{}.{}.{}", row.schema_name, row.r#type, row.name);
        let value = normalize_definition(&row.definition, ignore_whitespace, strip_comments);
        map.insert(key, value);
    }
    map
}

fn build_index_map(rows: &[IndexRow]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for row in rows {
        let signature = serde_json::json!({
            "type": row.r#type,
            "unique": row.is_unique,
            "primaryKey": row.is_primary_key,
            "uniqueConstraint": row.is_unique_constraint,
            "keyColumns": row.key_columns,
            "includeColumns": row.include_columns,
        });
        let key = format!("{}.{}::{}", row.schema_name, row.table_name, signature);
        map.insert(key, signature.to_string());
    }
    map
}

fn build_constraint_map(
    rows: &[ConstraintRow],
    ignore_whitespace: bool,
    strip_comments: bool,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for row in rows {
        let def = normalize_definition(&row.definition, ignore_whitespace, strip_comments);
        let key = format!(
            "{}.{}.{}::{}",
            row.schema_name, row.table_name, row.r#type, def
        );
        map.insert(key.clone(), key);
    }
    map
}

fn build_table_map(rows: &[TableRow]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for row in rows {
        let signature = serde_json::json!({
            "columns": row.columns,
            "indexes": row.indexes,
            "checks": row.checks,
        });
        let key = format!("{}.{}", row.schema_name, row.table_name);
        map.insert(key, signature.to_string());
    }
    map
}

fn diff_maps(left: &HashMap<String, String>, right: &HashMap<String, String>) -> DiffSet {
    let mut changed = Vec::new();
    let mut missing_in_right = Vec::new();
    let mut missing_in_left = Vec::new();

    for (k, v) in left {
        if let Some(rv) = right.get(k) {
            if rv != v {
                changed.push(k.clone());
            }
        } else {
            missing_in_right.push(k.clone());
        }
    }
    for k in right.keys() {
        if !left.contains_key(k) {
            missing_in_left.push(k.clone());
        }
    }

    changed.sort();
    missing_in_left.sort();
    missing_in_right.sort();

    DiffSet {
        changed,
        missing_in_left,
        missing_in_right,
    }
}

fn summarize(
    left: &Snapshot,
    right: &Snapshot,
    ignore_whitespace: bool,
    strip_comments: bool,
) -> CompareSummary {
    let mod_left = build_module_map(&left.modules, ignore_whitespace, strip_comments);
    let mod_right = build_module_map(&right.modules, ignore_whitespace, strip_comments);
    let idx_left = build_index_map(&left.indexes);
    let idx_right = build_index_map(&right.indexes);
    let con_left = build_constraint_map(&left.constraints, ignore_whitespace, strip_comments);
    let con_right = build_constraint_map(&right.constraints, ignore_whitespace, strip_comments);
    let tbl_left = build_table_map(&left.tables);
    let tbl_right = build_table_map(&right.tables);

    CompareSummary {
        modules: diff_maps(&mod_left, &mod_right),
        indexes: diff_maps(&idx_left, &idx_right),
        constraints: diff_maps(&con_left, &con_right),
        tables: diff_maps(&tbl_left, &tbl_right),
    }
}

fn pretty_summary(left_name: &str, right_name: &str, summary: &CompareSummary) -> String {
    let mut lines = Vec::new();
    let mut render = |title: &str, diff: &DiffSet| {
        lines.push(format!("=== {title} ==="));
        lines.push(format!("changed: {}", diff.changed.len()));
        if !diff.changed.is_empty() {
            lines.push(format!("  {}", diff.changed.join(", ")));
        }
        lines.push(format!(
            "missing in {right_name}: {}",
            diff.missing_in_right.len()
        ));
        if !diff.missing_in_right.is_empty() {
            lines.push(format!("  {}", diff.missing_in_right.join(", ")));
        }
        lines.push(format!(
            "missing in {left_name}: {}",
            diff.missing_in_left.len()
        ));
        if !diff.missing_in_left.is_empty() {
            lines.push(format!("  {}", diff.missing_in_left.join(", ")));
        }
        lines.push(String::new());
    };

    render("Modules", &summary.modules);
    render("Indexes", &summary.indexes);
    render("Constraints", &summary.constraints);
    render("Tables", &summary.tables);
    lines.join("\n")
}

fn has_drift(summary: &CompareSummary) -> bool {
    !summary.modules.changed.is_empty()
        || !summary.modules.missing_in_left.is_empty()
        || !summary.modules.missing_in_right.is_empty()
        || !summary.indexes.changed.is_empty()
        || !summary.indexes.missing_in_left.is_empty()
        || !summary.indexes.missing_in_right.is_empty()
        || !summary.constraints.changed.is_empty()
        || !summary.constraints.missing_in_left.is_empty()
        || !summary.constraints.missing_in_right.is_empty()
        || !summary.tables.changed.is_empty()
        || !summary.tables.missing_in_left.is_empty()
        || !summary.tables.missing_in_right.is_empty()
}

fn output_summary(
    args: &CliArgs,
    cmd: &CompareArgs,
    summary: &CompareSummary,
    source_name: &str,
    target_name: &str,
    format: OutputFormat,
    json_pretty: bool,
) -> Result<()> {
    if matches!(format, OutputFormat::Json) || args.output.json {
        let body = json_out::emit_json_value(&serde_json::to_value(summary)?, json_pretty)?;
        if !args.quiet {
            println!("{body}");
        }
        return Ok(());
    }

    if cmd.compact {
        let pretty = cmd.pretty || args.output.pretty;
        let rendered = match format {
            OutputFormat::Markdown => markdown_summary(source_name, target_name, summary),
            _ if pretty => pretty_summary(source_name, target_name, summary),
            _ => pretty_summary(source_name, target_name, summary),
        };
        if !args.quiet {
            println!("{rendered}");
        }
        return Ok(());
    }

    let rows = drift_rows(summary);
    let counts = render_counts_table(summary, format);
    if !args.quiet {
        println!("{counts}");
        if !rows.is_empty() {
            println!();
            let rendered = render_drift_table(rows, format);
            println!("{rendered}");
        }
    }
    Ok(())
}

fn markdown_summary(left: &str, right: &str, summary: &CompareSummary) -> String {
    fn section(title: &str, diff: &DiffSet, left: &str, right: &str) -> String {
        let mut lines = Vec::new();
        lines.push(format!("### {title}"));
        lines.push(format!("- changed: {}", diff.changed.len()));
        if !diff.changed.is_empty() {
            lines.push(format!("  - `{}`", diff.changed.join("`, `")));
        }
        lines.push(format!(
            "- missing in {right}: {}",
            diff.missing_in_right.len()
        ));
        if !diff.missing_in_right.is_empty() {
            lines.push(format!("  - `{}`", diff.missing_in_right.join("`, `")));
        }
        lines.push(format!(
            "- missing in {left}: {}",
            diff.missing_in_left.len()
        ));
        if !diff.missing_in_left.is_empty() {
            lines.push(format!("  - `{}`", diff.missing_in_left.join("`, `")));
        }
        lines.join("\n")
    }

    let mut out = Vec::new();
    out.push(format!("## Drift Summary: {left} vs {right}"));
    out.push(section("Modules", &summary.modules, left, right));
    out.push(section("Indexes", &summary.indexes, left, right));
    out.push(section("Constraints", &summary.constraints, left, right));
    out.push(section("Tables", &summary.tables, left, right));
    out.join("\n\n")
}

#[derive(Clone)]
struct DriftRow {
    object: String,
    kind: String,
    status: String,
}

fn drift_rows(summary: &CompareSummary) -> Vec<DriftRow> {
    let mut rows = Vec::new();

    let mut push_module = |key: &str, status: &str| {
        if let Some((schema, type_code, name)) = parse_module_key(key) {
            rows.push(DriftRow {
                object: format!("{schema}.{name}"),
                kind: type_keyword(type_code).to_string(),
                status: status.to_string(),
            });
        }
    };

    for key in &summary.modules.changed {
        push_module(key, "Changed");
    }
    for key in &summary.modules.missing_in_right {
        push_module(key, "Only in source");
    }
    for key in &summary.modules.missing_in_left {
        push_module(key, "Only in target");
    }

    let mut push_table_like = |key: &str, kind: &str, status: &str| {
        rows.push(DriftRow {
            object: key.to_string(),
            kind: kind.to_string(),
            status: status.to_string(),
        });
    };

    for key in &summary.tables.changed {
        push_table_like(key, "Table", "Changed");
    }
    for key in &summary.tables.missing_in_right {
        push_table_like(key, "Table", "Only in source");
    }
    for key in &summary.tables.missing_in_left {
        push_table_like(key, "Table", "Only in target");
    }

    let mut push_index = |key: &str, status: &str| {
        if let Some(obj) = parse_index_key(key) {
            push_table_like(&obj, "Index", status);
        }
    };
    for key in &summary.indexes.changed {
        push_index(key, "Changed");
    }
    for key in &summary.indexes.missing_in_right {
        push_index(key, "Only in source");
    }
    for key in &summary.indexes.missing_in_left {
        push_index(key, "Only in target");
    }

    let mut push_constraint = |key: &str, status: &str| {
        if let Some((obj, kind)) = parse_constraint_key(key) {
            rows.push(DriftRow {
                object: obj,
                kind,
                status: status.to_string(),
            });
        }
    };
    for key in &summary.constraints.changed {
        push_constraint(key, "Changed");
    }
    for key in &summary.constraints.missing_in_right {
        push_constraint(key, "Only in source");
    }
    for key in &summary.constraints.missing_in_left {
        push_constraint(key, "Only in target");
    }

    rows
}

fn parse_module_key(key: &str) -> Option<(&str, &str, String)> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() < 3 {
        return None;
    }
    let schema = parts[0];
    let type_code = parts[1];
    let name = parts[2..].join(".");
    Some((schema, type_code, name))
}

fn parse_index_key(key: &str) -> Option<String> {
    key.split_once("::").map(|(obj, _)| obj.to_string())
}

fn parse_constraint_key(key: &str) -> Option<(String, String)> {
    if let Some((left, _def)) = key.split_once("::") {
        let parts: Vec<&str> = left.split('.').collect();
        if parts.len() >= 3 {
            let schema = parts[0];
            let table = parts[1];
            let kind = parts[2].to_string();
            return Some((format!("{schema}.{table}"), kind));
        }
    }
    None
}

fn render_drift_table(rows: Vec<DriftRow>, format: OutputFormat) -> String {
    let rs = ResultSet {
        columns: vec![
            Column {
                name: "Object".to_string(),
                data_type: None,
            },
            Column {
                name: "Kind".to_string(),
                data_type: None,
            },
            Column {
                name: "Status".to_string(),
                data_type: None,
            },
        ],
        rows: rows
            .into_iter()
            .map(|r| {
                vec![
                    Value::Text(r.object),
                    Value::Text(r.kind),
                    Value::Text(r.status),
                ]
            })
            .collect(),
    };
    let opts = crate::output::table::TableOptions::default();
    crate::output::table::render_result_set_table(&rs, format, &opts)
}

fn render_counts_table(summary: &CompareSummary, format: OutputFormat) -> String {
    let rs = ResultSet {
        columns: vec![
            Column {
                name: "Type".to_string(),
                data_type: None,
            },
            Column {
                name: "Changed".to_string(),
                data_type: None,
            },
            Column {
                name: "Only in source".to_string(),
                data_type: None,
            },
            Column {
                name: "Only in target".to_string(),
                data_type: None,
            },
        ],
        rows: vec![
            row_counts("Modules", &summary.modules),
            row_counts("Tables", &summary.tables),
            row_counts("Indexes", &summary.indexes),
            row_counts("Constraints", &summary.constraints),
        ],
    };
    let opts = crate::output::table::TableOptions::default();
    crate::output::table::render_result_set_table(&rs, format, &opts)
}

fn row_counts(kind: &str, diff: &DiffSet) -> Vec<Value> {
    vec![
        Value::Text(kind.to_string()),
        Value::Int(diff.changed.len() as i64),
        Value::Int(diff.missing_in_right.len() as i64),
        Value::Int(diff.missing_in_left.len() as i64),
    ]
}

fn match_object_predicate(object: &str) -> Box<dyn Fn(&ModuleRow) -> bool + '_> {
    let parts: Vec<&str> = object.split('.').collect();
    if parts.len() == 2 {
        let schema = parts[0].to_lowercase();
        let name = parts[1].to_lowercase();
        return Box::new(move |row: &ModuleRow| {
            row.schema_name.eq_ignore_ascii_case(&schema) && row.name.eq_ignore_ascii_case(&name)
        });
    }
    let name = object.to_lowercase();
    Box::new(move |row: &ModuleRow| row.name.eq_ignore_ascii_case(&name))
}

fn pick_first_module(snapshot: &Snapshot, object: &str) -> Option<ModuleRow> {
    let pred = match_object_predicate(object);
    snapshot.modules.iter().find(|m| pred(m)).cloned()
}

fn handle_object_diff(
    args: &CliArgs,
    cmd: &CompareArgs,
    left: &Snapshot,
    right: &Snapshot,
    object: &str,
) -> Result<()> {
    let left_obj = pick_first_module(left, object);
    let right_obj = pick_first_module(right, object);

    if left_obj.is_none() && right_obj.is_none() {
        println!("Object '{object}' not found in either side.");
        std::process::exit(4);
    }

    let norm_left = left_obj
        .as_ref()
        .map(|m| normalize_definition(&m.definition, cmd.ignore_whitespace, cmd.strip_comments))
        .unwrap_or_default();
    let norm_right = right_obj
        .as_ref()
        .map(|m| normalize_definition(&m.definition, cmd.ignore_whitespace, cmd.strip_comments))
        .unwrap_or_default();

    let raw_left = left_obj
        .as_ref()
        .map(|m| m.definition.replace("\r\n", "\n"))
        .unwrap_or_default();
    let raw_right = right_obj
        .as_ref()
        .map(|m| m.definition.replace("\r\n", "\n"))
        .unwrap_or_default();

    if left_obj.is_some() && right_obj.is_some() && norm_left == norm_right {
        if !args.quiet {
            println!("No substantive drift for {object} (whitespace/comments ignored).");
        }
        return Ok(());
    }

    if let (Some(ref l), Some(ref r)) = (left_obj.as_ref(), right_obj.as_ref()) {
        let header_left = format!("{}:{}.{}.{}", left.name, l.schema_name, l.name, l.r#type);
        let header_right = format!("{}:{}.{}.{}", right.name, r.schema_name, r.name, r.r#type);
        let diff = TextDiff::from_lines(&raw_left, &raw_right)
            .unified_diff()
            .context_radius(5)
            .header(&header_left, &header_right)
            .to_string();
        println!("{diff}");
        std::process::exit(3);
    } else {
        println!(
            "Left: {}",
            left_obj
                .as_ref()
                .map(|m| format!("{}.{}", m.schema_name, m.name))
                .unwrap_or_else(|| "missing".to_string())
        );
        println!("{}", raw_left);
        println!("---");
        println!(
            "Right: {}",
            right_obj
                .as_ref()
                .map(|m| format!("{}.{}", m.schema_name, m.name))
                .unwrap_or_else(|| "missing".to_string())
        );
        println!("{}", raw_right);
        std::process::exit(3);
    }
}

fn type_keyword(code: &str) -> &'static str {
    match code {
        "P" => "PROCEDURE",
        "V" => "VIEW",
        "FN" | "IF" | "TF" => "FUNCTION",
        "TR" => "TRIGGER",
        _ => "OBJECT",
    }
}

fn create_or_alter(definition: &str, type_key: &str) -> String {
    let cleaned = definition.trim();
    let regex = Regex::new(&format!(r"(?i)\bCREATE\s+(OR\s+ALTER\s+)?{}\b", type_key))
        .expect("valid regex");
    if regex.is_match(cleaned) {
        return regex
            .replace(cleaned, format!("CREATE OR ALTER {type_key}"))
            .to_string();
    }
    Regex::new("(?i)\\bCREATE\\b")
        .expect("valid regex")
        .replace(cleaned, format!("CREATE OR ALTER {type_key}"))
        .to_string()
}

fn columns_by_table(rows: &[TableColumnRow]) -> HashMap<String, Vec<TableColumnRow>> {
    let mut map: HashMap<String, Vec<TableColumnRow>> = HashMap::new();
    for row in rows {
        let key = format!("{}.{}", row.schema_name, row.table_name);
        map.entry(key).or_default().push(row.clone());
    }
    for cols in map.values_mut() {
        cols.sort_by_key(|c| c.column_id);
    }
    map
}

fn format_type(col: &TableColumnRow) -> String {
    let dt = col.data_type.to_lowercase();
    let len = col.max_length;
    let prec = col.precision;
    let scale = col.scale;
    let length_types = [
        "varchar",
        "char",
        "nvarchar",
        "nchar",
        "varbinary",
        "binary",
    ];
    if length_types.contains(&dt.as_str()) {
        let mut l = len;
        if matches!(dt.as_str(), "nvarchar" | "nchar") {
            l = if l > 0 { l / 2 } else { l };
        }
        let size = if l == -1 {
            "max".to_string()
        } else {
            l.to_string()
        };
        return format!("{dt}({size})");
    }
    let precision_types = ["decimal", "numeric"];
    if precision_types.contains(&dt.as_str()) {
        return format!("{dt}({prec},{scale})");
    }
    if matches!(dt.as_str(), "datetime2" | "time" | "datetimeoffset") {
        return format!("{dt}({scale})");
    }
    dt
}

fn column_definition(col: &TableColumnRow) -> String {
    if !col.computed_definition.is_empty() {
        return format!("[{}] AS {}", col.column_name, col.computed_definition);
    }
    let mut parts = vec![format!("[{}]", col.column_name), format_type(col)];
    if col.is_identity {
        parts.push("IDENTITY".to_string());
    }
    parts.push(if col.is_nullable { "NULL" } else { "NOT NULL" }.to_string());
    if !col.default_definition.is_empty() {
        parts.push(format!("DEFAULT {}", col.default_definition));
    }
    parts.join(" ")
}

fn render_add_columns(
    table_key: &str,
    source_cols: &[TableColumnRow],
    target_cols: &[TableColumnRow],
) -> Vec<String> {
    let mut target_by_name = HashMap::new();
    for col in target_cols {
        target_by_name.insert(col.column_name.to_lowercase(), col);
    }
    let to_add: Vec<&TableColumnRow> = source_cols
        .iter()
        .filter(|c| !target_by_name.contains_key(&c.column_name.to_lowercase()))
        .collect();
    if to_add.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let (schema, table) = table_key.split_once('.').unwrap_or(("", table_key));
    lines.push(format!(
        "-- Adding {} column(s) to {schema}.{table}",
        to_add.len()
    ));
    lines.push(format!("ALTER TABLE [{schema}].[{table}]"));
    lines.push(format!(
        "  ADD {}",
        to_add
            .iter()
            .map(|c| column_definition(c))
            .collect::<Vec<_>>()
            .join(",\n      ")
    ));
    lines.push("GO".to_string());
    lines.push(String::new());
    lines
}

fn render_apply_script(
    summary: &CompareSummary,
    source: &Snapshot,
    target: &Snapshot,
    include_drops: bool,
) -> String {
    let mut source_map = HashMap::new();
    for row in &source.modules {
        let key = format!("{}.{}.{}", row.schema_name, row.r#type, row.name);
        source_map.insert(key, row);
    }

    let mut target_table_sig = HashMap::new();
    let mut source_table_sig = HashMap::new();
    for t in &target.tables {
        let key = format!("{}.{}", t.schema_name, t.table_name);
        target_table_sig.insert(
            key,
            serde_json::json!({
                "columns": t.columns,
                "indexes": t.indexes,
                "checks": t.checks
            })
            .to_string(),
        );
    }
    for t in &source.tables {
        let key = format!("{}.{}", t.schema_name, t.table_name);
        source_table_sig.insert(
            key,
            serde_json::json!({
                "columns": t.columns,
                "indexes": t.indexes,
                "checks": t.checks
            })
            .to_string(),
        );
    }

    let src_cols = columns_by_table(&source.table_columns);
    let tgt_cols = columns_by_table(&target.table_columns);

    let mut module_lines = Vec::new();
    let mut drop_lines = Vec::new();
    let mut table_lines = Vec::new();

    let emit_module = |row: &ModuleRow, reason: &str, out: &mut Vec<String>| {
        let type_key = type_keyword(&row.r#type);
        out.push(format!(
            "-- {reason}: {}.{} ({type_key})",
            row.schema_name, row.name
        ));
        out.push(create_or_alter(&row.definition, type_key));
        out.push("GO".to_string());
        out.push(String::new());
    };

    for key in &summary.modules.changed {
        if let Some(row) = source_map.get(key) {
            emit_module(row, "ALTER", &mut module_lines);
        }
    }
    for key in &summary.modules.missing_in_left {
        if let Some(row) = source_map.get(key) {
            emit_module(row, "CREATE", &mut module_lines);
        }
    }

    if include_drops && !summary.modules.missing_in_right.is_empty() {
        drop_lines.push("-- Dropping objects that exist only in target".to_string());
        for key in &summary.modules.missing_in_right {
            let parts: Vec<&str> = key.split('.').collect();
            if parts.len() >= 3 {
                let schema = parts[0];
                let type_code = parts[1];
                let name = parts[2..].join(".");
                let type_key = type_keyword(type_code);
                if matches!(type_key, "PROCEDURE" | "FUNCTION" | "VIEW") {
                    drop_lines.push(format!("DROP {type_key} IF EXISTS [{schema}].[{name}];"));
                } else {
                    drop_lines.push(format!("-- TODO: drop {type_key} {schema}.{name} manually"));
                }
            }
        }
        drop_lines.push("GO".to_string());
    }

    if !summary.tables.changed.is_empty()
        || !summary.tables.missing_in_left.is_empty()
        || !summary.tables.missing_in_right.is_empty()
    {
        table_lines.push(
            "-- Table drift detected; non-destructive additions are applied automatically; other changes remain commented."
                .to_string(),
        );
        for key in &summary.tables.changed {
            let right_sig = source_table_sig.get(key).cloned();
            let left_sig = target_table_sig.get(key).cloned();
            table_lines.push(render_table_alter(key, left_sig, right_sig));
            table_lines.extend(render_add_columns(
                key,
                src_cols.get(key).cloned().unwrap_or_default().as_slice(),
                tgt_cols.get(key).cloned().unwrap_or_default().as_slice(),
            ));
        }
        for key in &summary.tables.missing_in_left {
            table_lines.push(format!(
                "-- Table {key} exists only in source. Consider creating it locally."
            ));
            if let Some(cols) = src_cols.get(key) {
                let create_cols = cols
                    .iter()
                    .map(column_definition)
                    .collect::<Vec<_>>()
                    .join(",\n  ");
                let (schema, table) = key.split_once('.').unwrap_or(("", key.as_str()));
                table_lines.push(format!(
                    "CREATE TABLE [{schema}].[{table}] (\n  {create_cols}\n);\nGO\n"
                ));
            }
        }
        for key in &summary.tables.missing_in_right {
            table_lines.push(format!(
                "-- Table {key} exists only in target. Decide whether to drop or keep."
            ));
        }
    }

    let mut lines = Vec::new();
    if !table_lines.is_empty() {
        lines.extend(table_lines);
    }
    if !drop_lines.is_empty() {
        lines.extend(drop_lines);
    }
    if !module_lines.is_empty() {
        lines.extend(module_lines);
    }
    if lines.is_empty() {
        lines.push("-- No drift detected; nothing to apply".to_string());
    }
    lines.join("\n")
}

fn render_table_alter(key: &str, left_sig: Option<String>, right_sig: Option<String>) -> String {
    let changes = diff_table_details(left_sig.as_deref(), right_sig.as_deref());
    let mut stmts = Vec::new();
    let (schema, table) = key.split_once('.').unwrap_or(("", key));
    stmts.push(format!(
        "-- TODO: Table drift detected for {schema}.{table}"
    ));
    if changes.columns_changed {
        stmts.push(format!(
            "--   Columns differ (type/nullability/default/identity/computed). Review and craft ALTER TABLE for {schema}.{table}."
        ));
    }
    if changes.indexes_changed {
        stmts.push(
            "--   Non-PK/unique indexes differ. Consider recreating indexes to match source."
                .to_string(),
        );
    }
    if changes.checks_changed {
        stmts.push("--   CHECK constraints differ. Align definitions as needed.".to_string());
    }
    stmts.push(String::new());
    stmts.join("\n")
}

#[derive(Default)]
struct TableDiffFlags {
    columns_changed: bool,
    indexes_changed: bool,
    checks_changed: bool,
}

fn parse_table_signature(sig: Option<&str>) -> Option<TableRowSignature> {
    sig.and_then(|s| serde_json::from_str::<TableRowSignature>(s).ok())
}

#[derive(serde::Deserialize)]
struct TableRowSignature {
    columns: String,
    indexes: String,
    checks: String,
}

fn diff_table_details(left_sig: Option<&str>, right_sig: Option<&str>) -> TableDiffFlags {
    let left = parse_table_signature(left_sig).unwrap_or(TableRowSignature {
        columns: String::new(),
        indexes: String::new(),
        checks: String::new(),
    });
    let right = parse_table_signature(right_sig).unwrap_or(TableRowSignature {
        columns: String::new(),
        indexes: String::new(),
        checks: String::new(),
    });
    TableDiffFlags {
        columns_changed: left.columns != right.columns,
        indexes_changed: left.indexes != right.indexes,
        checks_changed: left.checks != right.checks,
    }
}

fn parse_connection_string(raw: &str) -> Result<ConnectionSettings> {
    if raw.contains("://") {
        return parse_url_style(raw);
    }
    parse_ado_style(raw)
}

fn parse_url_style(raw: &str) -> Result<ConnectionSettings> {
    let mut conn = ConnectionSettings::default();
    let mut remaining = raw.trim();
    if let Some(idx) = remaining.find("://") {
        remaining = &remaining[idx + 3..];
    }

    let mut auth_part = None;
    let mut host_part = remaining;
    if let Some(idx) = remaining.find('@') {
        auth_part = Some(&remaining[..idx]);
        host_part = &remaining[idx + 1..];
    }

    if let Some(auth) = auth_part {
        let mut parts = auth.splitn(2, ':');
        let user = parts.next().unwrap_or("");
        if !user.is_empty() {
            conn.user = Some(user.to_string());
        }
        if let Some(pass) = parts.next() {
            if !pass.is_empty() {
                conn.password = Some(pass.to_string());
            }
        }
    }

    let mut host_port = host_part;
    if let Some(idx) = host_part.find('/') {
        host_port = &host_part[..idx];
        let db = &host_part[idx + 1..];
        if !db.is_empty() {
            conn.database = db.to_string();
        }
    }

    if !host_port.is_empty() {
        let mut parts = host_port.splitn(2, ':');
        let host = parts.next().unwrap_or("");
        if !host.is_empty() {
            conn.server = host.to_string();
        }
        if let Some(port) = parts.next() {
            if let Ok(port) = port.parse::<u16>() {
                conn.port = port;
            }
        }
    }

    Ok(conn)
}

fn parse_ado_style(raw: &str) -> Result<ConnectionSettings> {
    let mut conn = ConnectionSettings::default();
    for part in raw.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut kv = trimmed.splitn(2, '=');
        let key = kv.next().unwrap_or("").trim().to_lowercase();
        let value = kv.next().unwrap_or("").trim();
        match key.as_str() {
            "server" | "data source" | "addr" | "address" | "network address" => {
                if let Some((host, port_str)) = value.split_once(',') {
                    conn.server = host.to_string();
                    if let Ok(port) = port_str.parse::<u16>() {
                        conn.port = port;
                    }
                } else {
                    conn.server = value.to_string();
                }
            }
            "database" | "initial catalog" => conn.database = value.to_string(),
            "user id" | "uid" | "user" => conn.user = Some(value.to_string()),
            "password" | "pwd" => conn.password = Some(value.to_string()),
            "encrypt" => {
                if let Some(b) = parse_bool(value) {
                    conn.encrypt = b;
                }
            }
            "trustservercertificate" | "trust server certificate" => {
                if let Some(b) = parse_bool(value) {
                    conn.trust_cert = b;
                }
            }
            "connection timeout" | "connect timeout" => {
                if let Ok(secs) = value.parse::<u64>() {
                    conn.timeout_ms = secs * 1000;
                }
            }
            "trusted_connection" | "integrated security" => {
                // If using integrated security, omit SQL auth fields.
                if let Some(true) = parse_bool(value) {
                    conn.user = None;
                    conn.password = None;
                }
            }
            _ => {}
        }
    }
    Ok(conn)
}

fn write_apply_script(path: Option<&str>, script: &str) -> Result<()> {
    if let Some("-") = path {
        println!("{script}");
        return Ok(());
    }

    let target_path = if let Some(p) = path {
        PathBuf::from(p)
    } else {
        let ts = Local::now().format("%Y%m%d-%H%M%S");
        PathBuf::from(format!("db-apply-diff-{ts}.sql"))
    };
    if let Some(parent) = target_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }
    }
    fs::write(&target_path, script)
        .with_context(|| format!("Failed to write {}", target_path.display()))?;
    println!("Wrote apply script to {}", target_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_definition_with_comments_and_whitespace() {
        let sql = " \n/* header */\nCREATE PROC Foo AS\n-- inline\nSELECT 1 \n";
        let normalized = normalize_definition(sql, true, true);
        assert_eq!(normalized, "CREATE PROC Foo AS SELECT 1");
    }

    #[test]
    fn diff_maps_detects_missing_sides() {
        let mut left = HashMap::new();
        left.insert("a".to_string(), "1".to_string());
        left.insert("b".to_string(), "2".to_string());
        let mut right = HashMap::new();
        right.insert("a".to_string(), "1".to_string());
        right.insert("c".to_string(), "3".to_string());

        let diff = diff_maps(&left, &right);
        assert!(diff.changed.is_empty());
        assert_eq!(diff.missing_in_right, vec!["b".to_string()]);
        assert_eq!(diff.missing_in_left, vec!["c".to_string()]);
    }

    #[test]
    fn render_add_columns_emits_alter_table() {
        let src = vec![TableColumnRow {
            schema_name: "dbo".into(),
            table_name: "Users".into(),
            column_id: 1,
            column_name: "Id".into(),
            data_type: "int".into(),
            max_length: 4,
            precision: 10,
            scale: 0,
            is_nullable: false,
            is_identity: true,
            default_definition: "".into(),
            computed_definition: "".into(),
        }];
        let tgt = Vec::new();
        let lines = render_add_columns("dbo.Users", &src, &tgt).join("\n");
        assert!(lines.contains("ALTER TABLE [dbo].[Users]"));
        assert!(lines.contains("[Id] int"));
    }
}
