use std::fs;

use anyhow::{Result, anyhow};
use serde_json::json;
use tiberius::Query;

use crate::cli::{CliArgs, CompareCountsArgs};
use crate::commands::{common, sql_fragments};
use crate::config::OutputFormat;
use crate::db::{client, executor};
use crate::output::{TableOptions, json as json_out, table};

pub fn run(args: &CliArgs, cmd: &CompareCountsArgs) -> Result<()> {
    let tables = load_tables(cmd)?;
    let source_db = cmd
        .source_db
        .as_deref()
        .ok_or_else(|| anyhow!("--source-db is required for same-server compare-counts"))?;
    let target_db = cmd
        .target_db
        .as_deref()
        .ok_or_else(|| anyhow!("--target-db is required for same-server compare-counts"))?;
    let sql = build_same_server_query(source_db, target_db, &tables)?;

    if cmd.dry_run {
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

    let mismatched = result_set.rows.iter().any(|row| {
        row.last()
            .is_some_and(|value| value.as_csv().eq_ignore_ascii_case("mismatch"))
    });

    if !args.quiet {
        let format = common::output_format(args, &resolved);
        if matches!(format, OutputFormat::Json) {
            let payload = json!({
                "sourceDb": source_db,
                "targetDb": target_db,
                "counts": json_out::result_set_rows_to_objects(&result_set),
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

    if mismatched {
        std::process::exit(3);
    }

    Ok(())
}

fn load_tables(cmd: &CompareCountsArgs) -> Result<Vec<String>> {
    let mut tables = Vec::new();
    if let Some(values) = &cmd.tables {
        tables.extend(
            values
                .iter()
                .filter(|value| !value.trim().is_empty())
                .cloned(),
        );
    }
    if let Some(path) = &cmd.tables_file {
        let content = fs::read_to_string(path)?;
        tables.extend(
            content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(str::to_string),
        );
    }
    if tables.is_empty() {
        return Err(anyhow!("Provide --tables or --tables-file"));
    }
    Ok(tables)
}

pub(crate) fn build_same_server_query(
    source_db: &str,
    target_db: &str,
    tables: &[String],
) -> Result<String> {
    let mut parts = Vec::with_capacity(tables.len());
    for table in tables {
        let source = sql_fragments::quote_db_object(source_db, table)?;
        let target = sql_fragments::quote_db_object(target_db, table)?;
        let table_label = sql_fragments::quote_string(table);
        parts.push(format!(
            "\
SELECT {table_label} AS tableName,
       sourceCount,
       targetCount,
       targetCount - sourceCount AS delta,
       CASE WHEN sourceCount = targetCount THEN N'ok' ELSE N'mismatch' END AS status
FROM (
    SELECT
        (SELECT COUNT_BIG(*) FROM {source}) AS sourceCount,
        (SELECT COUNT_BIG(*) FROM {target}) AS targetCount
) counts"
        ));
    }
    Ok(format!("{};", parts.join("\nUNION ALL\n")))
}
