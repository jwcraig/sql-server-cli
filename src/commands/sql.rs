use std::fs;
use std::time::Instant;

use anyhow::{anyhow, Result};
use serde_json::json;
use tiberius::Query;

use crate::cli::{CliArgs, SqlArgs};
use crate::commands::{common, sql_utils};
use crate::config::OutputFormat;
use crate::db::client;
use crate::db::executor;
use crate::db::types::ResultSet;
use crate::error::{AppError, ErrorKind};
use crate::output::{csv, json as json_out, table, TableOptions};
use crate::safety;

const MAX_ROWS_DEFAULT: u64 = 200;
const MAX_ROWS_MAX: u64 = 2000;

#[derive(Debug, Clone)]
struct BatchResult {
    index: usize,
    success: bool,
    elapsed_ms: u128,
    rows: usize,
    error: Option<String>,
}

pub fn run(args: &CliArgs, cmd: &SqlArgs) -> Result<()> {
    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);
    let allow_write = common::allow_write(args, &resolved);

    let sql_text = match (&cmd.sql, &cmd.file) {
        (Some(_), Some(_)) => return Err(anyhow!("Provide SQL text or --file, not both")),
        (None, None) => return Err(anyhow!("Provide SQL text or --file")),
        (Some(text), None) => text.clone(),
        (None, Some(path)) => fs::read_to_string(path)?,
    };

    let params = sql_utils::parse_params(&cmd.params)
        .map_err(|err| AppError::new(ErrorKind::Query, err.to_string()))?;

    let mut batches = if cmd.file.is_some() {
        sql_utils::split_batches(&sql_text)
    } else {
        vec![sql_text]
    };
    batches.retain(|batch| !batch.trim().is_empty());

    if batches.is_empty() {
        return Err(anyhow!("No SQL batches found"));
    }

    let batches = batches
        .iter()
        .map(|batch| sql_utils::replace_named_params(batch, &params, 1))
        .collect::<Vec<_>>();

    if !allow_write {
        for batch in &batches {
            safety::validate_read_only(batch)
                .map_err(|err| AppError::new(ErrorKind::Query, err.to_string()))?;
        }
    }

    if cmd.dry_run {
        if args.quiet {
            return Ok(());
        }
        emit_dry_run(&format, &resolved, &batches)?;
        return Ok(());
    }

    let max_rows = cmd
        .max_rows
        .unwrap_or(MAX_ROWS_DEFAULT)
        .clamp(1, MAX_ROWS_MAX) as usize;

    let (result_sets, batch_results, errors) = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;
        let mut all_sets: Vec<ResultSet> = Vec::new();
        let mut batch_results = Vec::new();
        let mut errors = Vec::new();

        for (idx, batch) in batches.iter().enumerate() {
            let started = Instant::now();
            let mut query = Query::new(batch.clone());
            for param in &params {
                query.bind(param.value.as_str());
            }

            match executor::run_query(query, &mut client).await {
                Ok(sets) => {
                    let rows = sets.iter().map(|rs| rs.rows.len()).sum();
                    all_sets.extend(sets);
                    batch_results.push(BatchResult {
                        index: idx + 1,
                        success: true,
                        elapsed_ms: started.elapsed().as_millis(),
                        rows,
                        error: None,
                    });
                }
                Err(err) => {
                    let message = err.to_string();
                    batch_results.push(BatchResult {
                        index: idx + 1,
                        success: false,
                        elapsed_ms: started.elapsed().as_millis(),
                        rows: 0,
                        error: Some(message.clone()),
                    });
                    errors.push(message);
                    if !cmd.continue_on_error {
                        return Err(err);
                    }
                }
            }
        }

        Ok::<_, anyhow::Error>((all_sets, batch_results, errors))
    })?;

    if !errors.is_empty() {
        for err in &errors {
            eprintln!("Batch error: {}", err);
        }
    }

    let csv_paths = if let Some(path) = cmd.csv.as_ref() {
        Some(csv::write_result_sets(
            path,
            &result_sets,
            resolved.settings.output.csv.multi_result_naming,
        )?)
    } else {
        None
    };

    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "success": errors.is_empty(),
            "batches": batch_results.iter().map(batch_to_json).collect::<Vec<_>>(),
            "resultSets": result_sets.iter().map(json_out::result_set_to_json).collect::<Vec<_>>(),
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

    let display_sets = truncate_result_sets(&result_sets, max_rows);
    for (idx, result_set) in display_sets.iter().enumerate() {
        if display_sets.len() > 1 {
            println!("Result set {}", idx + 1);
        }
        let rendered = table::render_result_set_table(result_set, format, &TableOptions::default());
        println!("{}", rendered);
        if idx + 1 < display_sets.len() {
            println!();
        }
    }

    if let Some(paths) = csv_paths {
        println!("\nCSV written:");
        for path in paths {
            println!("- {}", path.display());
        }
    }

    Ok(())
}

fn emit_dry_run(
    format: &OutputFormat,
    resolved: &crate::config::ResolvedConfig,
    batches: &[String],
) -> Result<()> {
    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "success": true,
            "dryRun": true,
            "batchCount": batches.len(),
            "batches": batches.iter().enumerate().map(|(idx, sql)| json!({"index": idx + 1, "sql": sql})).collect::<Vec<_>>(),
        });
        let body = json_out::emit_json_value(&payload, resolved.settings.output.json.pretty)?;
        println!("{}", body);
        return Ok(());
    }

    println!("Dry run: {} batch(es)", batches.len());
    for (idx, batch) in batches.iter().enumerate() {
        println!("\nBatch {}:\n{}", idx + 1, batch);
    }
    Ok(())
}

fn truncate_result_sets(result_sets: &[ResultSet], max_rows: usize) -> Vec<ResultSet> {
    result_sets
        .iter()
        .map(|rs| {
            if rs.rows.len() <= max_rows {
                rs.clone()
            } else {
                let mut truncated = rs.clone();
                truncated.rows.truncate(max_rows);
                truncated
            }
        })
        .collect()
}

fn batch_to_json(batch: &BatchResult) -> serde_json::Value {
    json!({
        "index": batch.index,
        "success": batch.success,
        "elapsedMs": batch.elapsed_ms,
        "rows": batch.rows,
        "error": batch.error,
    })
}
