use std::fs;

use anyhow::Result;
use serde_json::json;
use tiberius::Query;

use crate::cli::{CliArgs, HealthArgs, HealthCheckArgs, HealthCommand};
use crate::commands::{common, sql_fragments};
use crate::config::OutputFormat;
use crate::db::{client, executor};
use crate::output::{TableOptions, json as json_out, table};

pub fn run(args: &CliArgs, cmd: &HealthArgs) -> Result<()> {
    let (kind, check_args, sql) = match &cmd.command {
        HealthCommand::Constraints(check_args) => {
            ("constraints", check_args, constraints_sql(check_args)?)
        }
        HealthCommand::Triggers(check_args) => ("triggers", check_args, triggers_sql(check_args)?),
        HealthCommand::Identities(check_args) => {
            ("identities", check_args, identities_sql(check_args)?)
        }
        HealthCommand::Help => return Err(anyhow::anyhow!("Missing health subcommand")),
    };

    if check_args.dry_run {
        if !args.quiet {
            println!("{}", sql);
        }
        return Ok(());
    }

    let resolved = common::load_config(args)?;
    let result_set = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;
        let result_sets = executor::run_query(Query::new(sql), &mut client).await?;
        Ok::<_, anyhow::Error>(result_sets.into_iter().next().unwrap_or_default())
    })?;

    if !args.quiet {
        let format = common::output_format(args, &resolved);
        if matches!(format, OutputFormat::Json) {
            let payload = json!({
                "kind": kind,
                "checks": json_out::result_set_rows_to_objects(&result_set),
            });
            println!(
                "{}",
                json_out::emit_json_value(&payload, common::json_pretty(&resolved))?
            );
        } else {
            let rendered =
                table::render_result_set_table(&result_set, format, &TableOptions::default());
            println!("{}", rendered.output);
        }
    }

    Ok(())
}

fn constraints_sql(args: &HealthCheckArgs) -> Result<String> {
    let filter = table_filter(args, "s", "t")?;
    Ok(format!(
        r#"
SELECT
    CONCAT(s.name, N'.', t.name) AS tableName,
    fk.name AS objectName,
    N'foreign_key' AS kind,
    CAST(CASE WHEN fk.is_disabled = 0 THEN 1 ELSE 0 END AS bit) AS enabled,
    CAST(CASE WHEN fk.is_not_trusted = 0 THEN 1 ELSE 0 END AS bit) AS trusted,
    fk.delete_referential_action_desc AS deleteAction,
    fk.update_referential_action_desc AS updateAction,
    CASE
        WHEN fk.is_disabled = 1 THEN N'disabled'
        WHEN fk.is_not_trusted = 1 THEN N'untrusted'
        ELSE N'ok'
    END AS status
FROM sys.foreign_keys fk
INNER JOIN sys.tables t ON fk.parent_object_id = t.object_id
INNER JOIN sys.schemas s ON t.schema_id = s.schema_id
{filter}
ORDER BY s.name, t.name, fk.name;"#
    ))
}

fn triggers_sql(args: &HealthCheckArgs) -> Result<String> {
    let filter = table_filter(args, "s", "t")?;
    Ok(format!(
        r#"
SELECT
    CONCAT(s.name, N'.', t.name) AS tableName,
    tr.name AS objectName,
    N'trigger' AS kind,
    CAST(CASE WHEN tr.is_disabled = 0 THEN 1 ELSE 0 END AS bit) AS enabled,
    tr.is_instead_of_trigger AS isInsteadOfTrigger,
    CASE WHEN tr.is_disabled = 1 THEN N'disabled' ELSE N'ok' END AS status
FROM sys.triggers tr
INNER JOIN sys.tables t ON tr.parent_id = t.object_id
INNER JOIN sys.schemas s ON t.schema_id = s.schema_id
{filter}
ORDER BY s.name, t.name, tr.name;"#
    ))
}

fn identities_sql(args: &HealthCheckArgs) -> Result<String> {
    let filter = table_filter(args, "s", "t")?;
    Ok(format!(
        r#"
SELECT
    s.name AS schemaName,
    t.name AS tableName,
    CONCAT(s.name, N'.', t.name) AS tableObject,
    c.name AS columnName,
    ic.seed_value AS seed,
    ic.increment_value AS increment,
    IDENT_CURRENT(QUOTENAME(s.name) + N'.' + QUOTENAME(t.name)) AS identityCurrent,
    CAST(NULL AS bigint) AS maxId,
    CAST(NULL AS bigint) AS delta,
    N'inspect' AS status
FROM sys.identity_columns ic
INNER JOIN sys.columns c ON ic.object_id = c.object_id AND ic.column_id = c.column_id
INNER JOIN sys.tables t ON ic.object_id = t.object_id
INNER JOIN sys.schemas s ON t.schema_id = s.schema_id
{filter}
ORDER BY s.name, t.name, c.column_id;"#
    ))
}

fn table_filter(args: &HealthCheckArgs, schema_alias: &str, table_alias: &str) -> Result<String> {
    let mut predicates = Vec::new();
    if let Some(schema) = &args.schema {
        predicates.push(format!(
            "{}.name = {}",
            schema_alias,
            sql_fragments::quote_string(schema)
        ));
    }

    let tables = load_tables(args)?;
    if !tables.is_empty() {
        let mut table_predicates = Vec::with_capacity(tables.len());
        for table in tables {
            let (schema, name) = sql_fragments::split_table_name(&table)?;
            table_predicates.push(format!(
                "({schema_alias}.name = {} AND {table_alias}.name = {})",
                sql_fragments::quote_string(&schema),
                sql_fragments::quote_string(&name)
            ));
        }
        predicates.push(format!("({})", table_predicates.join(" OR ")));
    }

    if predicates.is_empty() {
        return Ok(String::new());
    }
    Ok(format!("WHERE {}", predicates.join(" AND ")))
}

fn load_tables(args: &HealthCheckArgs) -> Result<Vec<String>> {
    let mut tables = Vec::new();
    if let Some(values) = &args.tables {
        tables.extend(
            values
                .iter()
                .filter(|value| !value.trim().is_empty())
                .cloned(),
        );
    }
    if let Some(path) = &args.tables_file {
        let content = fs::read_to_string(path)?;
        tables.extend(
            content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(str::to_string),
        );
    }
    Ok(tables)
}
