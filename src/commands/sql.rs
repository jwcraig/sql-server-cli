use std::fs;
use std::io::Read;
use std::time::Instant;

use anyhow::{Result, anyhow};
use serde_json::json;

use crate::cli::{CliArgs, SqlArgs};
use crate::commands::{common, sql_utils};
use crate::config::OutputFormat;
use crate::db::client;
use crate::db::executor;
use crate::db::types::ResultSet;
use crate::error::{AppError, ErrorKind};
use crate::output::{TableOptions, csv, json as json_out, raw, table};

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
    let sql_text = match (&cmd.sql, &cmd.file, cmd.stdin) {
        (Some(_), Some(_), _) => {
            return Err(anyhow!(
                "Provide SQL text, --file, or --stdin, not multiple inputs"
            ));
        }
        (Some(_), None, true) => {
            return Err(anyhow!(
                "Provide SQL text, --file, or --stdin, not multiple inputs"
            ));
        }
        (None, Some(_), true) => {
            return Err(anyhow!(
                "Provide SQL text, --file, or --stdin, not multiple inputs"
            ));
        }
        (None, None, false) => return Err(anyhow!("Provide SQL text, --file, or --stdin")),
        (Some(text), None, false) => text.clone(),
        (None, Some(path), false) => fs::read_to_string(path)?,
        (None, None, true) => {
            let mut sql = String::new();
            std::io::stdin().read_to_string(&mut sql)?;
            sql
        }
    };

    let params = sql_utils::parse_params(&cmd.params)
        .map_err(|err| AppError::new(ErrorKind::Query, err.to_string()))?;

    let mut batches = sql_utils::split_batches(&sql_text);
    batches.retain(|batch| !batch.trim().is_empty());

    if batches.is_empty() {
        return Err(anyhow!("No SQL batches found"));
    }

    // Only a batch that references a parameter needs `sp_executesql`; every
    // other batch runs as a plain batch so session state carries over.
    let (batches, uses_params): (Vec<String>, Vec<bool>) = batches
        .iter()
        .map(|batch| sql_utils::replace_named_params(batch, &params, 1))
        .unzip();
    let param_values: Vec<&str> = params.iter().map(|param| param.value.as_str()).collect();

    common::print_target_banner(args, &resolved);

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
            let bound: &[&str] = if uses_params[idx] { &param_values } else { &[] };

            match executor::run_batch(batch, bound, &mut client).await {
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
                    if !cmd.continue_on_error {
                        // Like `sqlcmd -b`: stop here. Closing the session rolls
                        // back a transaction the script left open.
                        if batches.len() == 1 {
                            return Err(err);
                        }
                        return Err(AppError::new(
                            ErrorKind::Query,
                            format!("Batch {} of {} failed: {}", idx + 1, batches.len(), message),
                        )
                        .into());
                    }
                    errors.push(message);
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

    if let Some(raw_format) = raw_format(&format) {
        let options = raw::RawOptions {
            format: raw_format,
            headers: !cmd.no_headers,
            null_value: cmd.null_value.clone().unwrap_or_default(),
            result_set: cmd.result_set,
            all_result_sets: cmd.all_result_sets,
        };
        let body = raw::render_result_sets(&result_sets, &options)?;
        if let Some(path) = cmd.output.as_ref() {
            if path.as_os_str() == "-" {
                if !args.quiet {
                    print!("{}", body);
                }
            } else {
                fs::write(path, body)?;
            }
        } else if !args.quiet {
            print!("{}", body);
        }
        return Ok(());
    }

    if args.quiet {
        return Ok(());
    }

    let table_options = if cmd.no_truncate {
        TableOptions::unlimited()
    } else {
        TableOptions::truncated()
    };

    let display_sets = truncate_result_sets(&result_sets, max_rows);
    if display_sets.is_empty() {
        // Statements such as DDL return no result sets. Without this summary the
        // command would print nothing at all and a caller could not tell a
        // successful run from one that never executed.
        println!(
            "{}",
            render_execution_summary(&batch_results, errors.is_empty(), format).output
        );
    }
    for (idx, result_set) in display_sets.iter().enumerate() {
        if display_sets.len() > 1 {
            println!("Result set {}", idx + 1);
        }
        let result = table::render_result_set_table(result_set, format, &table_options);
        println!("{}", result.output);
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

fn raw_format(format: &OutputFormat) -> Option<raw::RawFormat> {
    match format {
        OutputFormat::Tsv => Some(raw::RawFormat::Tsv),
        OutputFormat::Csv => Some(raw::RawFormat::Csv),
        OutputFormat::Jsonl => Some(raw::RawFormat::Jsonl),
        _ => None,
    }
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

/// Render the run summary shown when a script produces no result sets.
fn render_execution_summary(
    batches: &[BatchResult],
    success: bool,
    format: OutputFormat,
) -> crate::output::RenderResult {
    let succeeded = batches.iter().filter(|batch| batch.success).count();
    let elapsed_ms: u128 = batches.iter().map(|batch| batch.elapsed_ms).sum();
    let rows = vec![
        (
            "Status".to_string(),
            if success { "ok" } else { "failed" }.to_string(),
        ),
        (
            "Batches".to_string(),
            format!("{} of {} succeeded", succeeded, batches.len()),
        ),
        ("ResultSets".to_string(), "0".to_string()),
        ("RowsReturned".to_string(), "0".to_string()),
        ("ElapsedMs".to_string(), elapsed_ms.to_string()),
    ];
    table::render_key_value_table("Execution", &rows, format, &TableOptions::default())
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
