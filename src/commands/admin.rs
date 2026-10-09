use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use serde_json::json;
use tiberius::Query;

use crate::cli::{
    AdminArgs, AdminBackupCommand, AdminBackupCreateArgs, AdminBackupFilelistArgs,
    AdminBackupRestorePlanArgs, AdminBackupRestoreTestArgs, AdminBackupVerifyArgs,
    AdminCheckDbArgs, AdminCommand, AdminSqlArgs, CliArgs,
};
use crate::commands::{common, compare_counts, sql_fragments, sql_utils};
use crate::config::OutputFormat;
use crate::db::{
    client, executor,
    types::{ResultSet, Value},
};
use crate::error::{AppError, ErrorKind};
use crate::output::{TableOptions, json as json_out, table};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RestoreFile {
    logical_name: String,
    file_type: String,
}

pub fn run(args: &CliArgs, cmd: &AdminArgs) -> Result<()> {
    match &cmd.command {
        AdminCommand::Sql(sql) => run_admin_sql(args, sql),
        AdminCommand::Backup(backup) => run_backup(args, backup),
        AdminCommand::CheckDb(checkdb) => run_checkdb(args, checkdb),
        AdminCommand::Help => Err(anyhow!("Missing admin subcommand")),
    }
}

fn run_admin_sql(args: &CliArgs, cmd: &AdminSqlArgs) -> Result<()> {
    let sql = cmd
        .sql
        .as_deref()
        .ok_or_else(|| anyhow!("Provide SQL text for admin sql"))?;
    let verify = cmd.verify.as_ref().map(|verify_sql| VerifyRequest {
        sql: verify_sql.clone(),
        expected_rows: cmd.expect_rows,
    });
    emit_or_execute_verified(args, sql, cmd.dry_run, verify.as_ref())
}

fn run_backup(args: &CliArgs, cmd: &AdminBackupCommand) -> Result<()> {
    match cmd {
        AdminBackupCommand::Create(create) => {
            emit_or_execute(args, &backup_create_sql(create), create.dry_run)
        }
        AdminBackupCommand::Verify(verify) => {
            emit_or_execute(args, &backup_verify_sql(verify), verify.dry_run)
        }
        AdminBackupCommand::Filelist(filelist) => {
            emit_or_execute(args, &backup_filelist_sql(filelist), filelist.dry_run)
        }
        AdminBackupCommand::RestorePlan(plan) => run_restore_plan(args, plan),
        AdminBackupCommand::RestoreTest(test) => run_restore_test(args, test),
        AdminBackupCommand::Help => Err(anyhow!("Missing admin backup subcommand")),
    }
}

fn run_checkdb(args: &CliArgs, cmd: &AdminCheckDbArgs) -> Result<()> {
    emit_or_execute(args, &checkdb_sql(cmd), cmd.dry_run)
}

/// Optional read-back executed after maintenance SQL so callers can observe its effect.
#[derive(Debug, Clone)]
struct VerifyRequest {
    sql: String,
    expected_rows: Option<usize>,
}

/// Per-batch execution record. `GO` splits an admin script into batches.
#[derive(Debug, Clone)]
struct BatchOutcome {
    index: usize,
    elapsed_ms: u128,
    result_sets: usize,
    rows_returned: usize,
    error: Option<String>,
}

impl BatchOutcome {
    fn succeeded(&self) -> bool {
        self.error.is_none()
    }
}

/// Result of the `--verify` read-back, including whether it met `--expect-rows`.
#[derive(Debug)]
struct VerifyOutcome {
    sql: String,
    rows: usize,
    expected_rows: Option<usize>,
    result_sets: Vec<ResultSet>,
}

impl VerifyOutcome {
    /// A verification without `--expect-rows` is informational and always passes.
    fn passed(&self) -> bool {
        self.expected_rows
            .is_none_or(|expected| expected == self.rows)
    }

    fn status_text(&self) -> String {
        match self.expected_rows {
            Some(expected) if expected == self.rows => {
                format!("passed (rows: {}, expected: {})", self.rows, expected)
            }
            Some(expected) => format!("failed (rows: {}, expected: {})", self.rows, expected),
            None => format!("observed (rows: {})", self.rows),
        }
    }
}

/// Everything observed while running an admin statement, used to prove what happened.
///
/// A failed batch is recorded rather than discarded so callers can see how far a
/// multi-batch script got. `ALTER DATABASE ... SET SINGLE_USER` succeeding while
/// the following `DROP DATABASE` fails leaves real state behind, and the summary
/// has to make that visible.
#[derive(Debug)]
struct AdminExecution {
    batches: Vec<BatchOutcome>,
    result_sets: Vec<ResultSet>,
    elapsed_ms: u128,
    verify: Option<VerifyOutcome>,
}

impl AdminExecution {
    fn result_set_count(&self) -> usize {
        self.result_sets.len()
    }

    fn rows_returned(&self) -> usize {
        self.result_sets.iter().map(|set| set.rows.len()).sum()
    }

    fn succeeded_batches(&self) -> usize {
        self.batches
            .iter()
            .filter(|batch| batch.succeeded())
            .count()
    }

    /// The batch that stopped the run, if any. Execution halts on the first error.
    fn failed_batch(&self) -> Option<&BatchOutcome> {
        self.batches.iter().find(|batch| !batch.succeeded())
    }

    fn passed(&self) -> bool {
        self.failed_batch().is_none() && self.verify.as_ref().is_none_or(VerifyOutcome::passed)
    }

    /// Message for the error returned to the caller once the summary is printed.
    ///
    /// A batch is the smallest unit sscli can observe, but a batch holds many
    /// semicolon-separated statements, and the ones before the failure have
    /// already committed. The wording must never let a reader conclude that a
    /// failed batch left nothing behind.
    fn failure_message(&self) -> Option<String> {
        if let Some(batch) = self.failed_batch() {
            let message = batch
                .error
                .as_deref()
                .unwrap_or("unknown batch failure")
                .to_string();
            let earlier_batches = match batch.index {
                1 => String::new(),
                2 => "Batch 1 completed. ".to_string(),
                index => format!("Batches 1-{} completed. ", index - 1),
            };
            return Some(format!(
                "Batch {} of {} failed: {}. {}Statements before the failure inside batch {} may \
                 have taken effect; confirm the current state with a read-back (--verify).",
                batch.index,
                self.batches.len(),
                message,
                earlier_batches,
                batch.index
            ));
        }
        let verify = self.verify.as_ref().filter(|verify| !verify.passed())?;
        Some(format!(
            "Verification failed: {} returned {} rows, expected {}",
            verify.sql.trim(),
            verify.rows,
            verify.expected_rows.unwrap_or_default()
        ))
    }
}

fn emit_or_execute(args: &CliArgs, sql: &str, dry_run: bool) -> Result<()> {
    emit_or_execute_verified(args, sql, dry_run, None)
}

fn emit_or_execute_verified(
    args: &CliArgs,
    sql: &str,
    dry_run: bool,
    verify: Option<&VerifyRequest>,
) -> Result<()> {
    if dry_run {
        if !args.quiet {
            println!("{}", sql);
            if let Some(verify) = verify {
                println!("{}", verify.sql);
            }
        }
        return Ok(());
    }

    let resolved = common::load_config(args)?;
    common::print_target_banner(args, &resolved);

    let execution = execute_admin_sql(&resolved, sql, verify)?;
    emit_execution(args, &resolved, &execution)?;

    match execution.failure_message() {
        Some(message) => Err(AppError::new(ErrorKind::Query, message).into()),
        None => Ok(()),
    }
}

/// Run every batch of `sql` on one connection, then the optional read-back query.
///
/// Execution stops at the first failing batch, but the batches that already ran
/// are still returned so the caller can report partial progress.
fn execute_admin_sql(
    resolved: &crate::config::ResolvedConfig,
    sql: &str,
    verify: Option<&VerifyRequest>,
) -> Result<AdminExecution> {
    let batches = sql_utils::split_batches(sql);
    if batches.is_empty() {
        return Err(anyhow!("No SQL batches found"));
    }

    let started = Instant::now();
    let (outcomes, result_sets, verify_outcome) =
        tokio::runtime::Runtime::new()?.block_on(async {
            let mut client = client::connect(&resolved.connection).await?;
            let mut outcomes = Vec::with_capacity(batches.len());
            let mut result_sets: Vec<ResultSet> = Vec::new();
            let mut failed = false;

            for (idx, batch) in batches.iter().enumerate() {
                let batch_started = Instant::now();
                let outcome = match executor::run_batch(batch, &[], &mut client).await {
                    Ok(sets) => {
                        let outcome = BatchOutcome {
                            index: idx + 1,
                            elapsed_ms: batch_started.elapsed().as_millis(),
                            result_sets: sets.len(),
                            rows_returned: sets.iter().map(|set| set.rows.len()).sum(),
                            error: None,
                        };
                        result_sets.extend(sets);
                        outcome
                    }
                    Err(err) => {
                        failed = true;
                        BatchOutcome {
                            index: idx + 1,
                            elapsed_ms: batch_started.elapsed().as_millis(),
                            result_sets: 0,
                            rows_returned: 0,
                            error: Some(err.to_string()),
                        }
                    }
                };
                outcomes.push(outcome);
                if failed {
                    break;
                }
            }

            // A read-back after a failed batch would describe a half-applied
            // script, so it is skipped and reported as such.
            let verify_outcome = match verify {
                Some(request) if !failed => {
                    let sets = executor::run_batch(&request.sql, &[], &mut client)
                        .await
                        .context("verification query failed")?;
                    Some(VerifyOutcome {
                        sql: request.sql.clone(),
                        rows: sets.iter().map(|set| set.rows.len()).sum(),
                        expected_rows: request.expected_rows,
                        result_sets: sets,
                    })
                }
                _ => None,
            };

            Ok::<_, anyhow::Error>((outcomes, result_sets, verify_outcome))
        })?;

    Ok(AdminExecution {
        batches: outcomes,
        result_sets,
        elapsed_ms: started.elapsed().as_millis(),
        verify: verify_outcome,
    })
}

/// Emit result sets plus an always-present execution summary.
///
/// The summary is what makes a silent DDL success observable: without it a
/// successful `DROP DATABASE` and a command that never ran look identical.
fn emit_execution(
    args: &CliArgs,
    resolved: &crate::config::ResolvedConfig,
    execution: &AdminExecution,
) -> Result<()> {
    if args.quiet {
        return Ok(());
    }

    let format = common::output_format(args, resolved);
    if matches!(format, OutputFormat::Json) {
        println!(
            "{}",
            json_out::emit_json_value(
                &execution_to_json(execution),
                common::json_pretty(resolved)
            )?
        );
        return Ok(());
    }

    for (idx, result_set) in execution.result_sets.iter().enumerate() {
        if execution.result_sets.len() > 1 {
            println!("Result set {}", idx + 1);
        }
        let rendered = table::render_result_set_table(result_set, format, &TableOptions::default());
        println!("{}\n", rendered.output);
    }

    if let Some(verify) = &execution.verify {
        for result_set in &verify.result_sets {
            println!("Verification read-back");
            let rendered =
                table::render_result_set_table(result_set, format, &TableOptions::default());
            println!("{}\n", rendered.output);
        }
    }

    if execution.batches.len() > 1 {
        println!("{}\n", render_batch_table(execution, format).output);
    }

    let mut rows = vec![
        (
            "Status".to_string(),
            if execution.passed() { "ok" } else { "failed" }.to_string(),
        ),
        (
            "Batches".to_string(),
            format!(
                "{} of {} succeeded",
                execution.succeeded_batches(),
                execution.batches.len()
            ),
        ),
        (
            "ResultSets".to_string(),
            execution.result_set_count().to_string(),
        ),
        (
            "RowsReturned".to_string(),
            execution.rows_returned().to_string(),
        ),
        ("ElapsedMs".to_string(), execution.elapsed_ms.to_string()),
    ];
    // The failing batch's server message is reported on stderr, so the summary
    // only needs to name the batch. It must also say that the batch is not
    // all-or-nothing, or "0 of 1 succeeded" reads as "nothing happened".
    match (&execution.verify, execution.failed_batch()) {
        (Some(verify), _) => rows.push(("Verify".to_string(), verify.status_text())),
        (None, Some(batch)) => rows.push((
            "FailedBatch".to_string(),
            format!(
                "{} (statements before the failure may have taken effect)",
                batch.index
            ),
        )),
        (None, None) => {}
    }

    let rendered =
        table::render_key_value_table("Execution", &rows, format, &TableOptions::truncated());
    println!("{}", rendered.output);
    Ok(())
}

/// Per-batch table shown for multi-batch scripts so partial progress is visible.
fn render_batch_table(
    execution: &AdminExecution,
    format: OutputFormat,
) -> crate::output::RenderResult {
    let result_set = ResultSet {
        columns: ["Batch", "Status", "ResultSets", "RowsReturned", "ElapsedMs"]
            .into_iter()
            .map(|name| crate::db::types::Column {
                name: name.to_string(),
                data_type: None,
            })
            .collect(),
        rows: execution
            .batches
            .iter()
            .map(|batch| {
                vec![
                    Value::Int(batch.index as i64),
                    Value::Text(if batch.succeeded() { "ok" } else { "failed" }.to_string()),
                    Value::Int(batch.result_sets as i64),
                    Value::Int(batch.rows_returned as i64),
                    Value::Int(batch.elapsed_ms as i64),
                ]
            })
            .collect(),
    };
    table::render_result_set_table(&result_set, format, &TableOptions::truncated())
}

fn execution_to_json(execution: &AdminExecution) -> serde_json::Value {
    json!({
        "status": if execution.passed() { "ok" } else { "failed" },
        "error": execution.failure_message(),
        // A failed batch may still have applied the statements that preceded the
        // failing one inside that batch. Callers must not treat it as a no-op.
        "partialEffectPossible": execution.failed_batch().is_some(),
        "elapsedMs": execution.elapsed_ms,
        "batchCount": execution.batches.len(),
        "batchesSucceeded": execution.succeeded_batches(),
        "batches": execution
            .batches
            .iter()
            .map(|batch| json!({
                "index": batch.index,
                "success": batch.succeeded(),
                "elapsedMs": batch.elapsed_ms,
                "resultSets": batch.result_sets,
                "rowsReturned": batch.rows_returned,
                "error": batch.error,
            }))
            .collect::<Vec<_>>(),
        "resultSetCount": execution.result_set_count(),
        "rowsReturned": execution.rows_returned(),
        "resultSets": execution
            .result_sets
            .iter()
            .map(json_out::result_set_to_json)
            .collect::<Vec<_>>(),
        "verify": execution.verify.as_ref().map(|verify| json!({
            "sql": verify.sql,
            "rows": verify.rows,
            "expectedRows": verify.expected_rows,
            "passed": verify.passed(),
            "resultSets": verify
                .result_sets
                .iter()
                .map(json_out::result_set_to_json)
                .collect::<Vec<_>>(),
        })),
    })
}

fn backup_create_sql(cmd: &AdminBackupCreateArgs) -> String {
    let mut options = Vec::new();
    if cmd.copy_only {
        options.push("COPY_ONLY".to_string());
    }
    if cmd.compression {
        options.push("COMPRESSION".to_string());
    }
    if cmd.checksum {
        options.push("CHECKSUM".to_string());
    }
    if let Some(stats) = cmd.stats {
        options.push(format!("STATS = {}", stats));
    }

    let mut sql = format!(
        "BACKUP DATABASE {} TO DISK = {}",
        sql_fragments::quote_ident(&cmd.database),
        sql_fragments::quote_string(&sql_fragments::path_for_sql(&cmd.to))
    );
    if !options.is_empty() {
        sql.push_str(" WITH ");
        sql.push_str(&options.join(", "));
    }
    sql.push(';');
    sql
}

fn backup_verify_sql(cmd: &AdminBackupVerifyArgs) -> String {
    let mut sql = format!(
        "RESTORE VERIFYONLY FROM DISK = {}",
        sql_fragments::quote_string(&sql_fragments::path_for_sql(&cmd.from))
    );
    if cmd.checksum {
        sql.push_str(" WITH CHECKSUM");
    }
    sql.push(';');
    sql
}

fn backup_filelist_sql(cmd: &AdminBackupFilelistArgs) -> String {
    format!(
        "RESTORE FILELISTONLY FROM DISK = {};",
        sql_fragments::quote_string(&sql_fragments::path_for_sql(&cmd.from))
    )
}

fn run_restore_plan(args: &CliArgs, cmd: &AdminBackupRestorePlanArgs) -> Result<()> {
    let filelist = restore_filelist_sql(&cmd.from);
    if cmd.dry_run {
        if matches!(args.output.format, Some(crate::cli::OutputFormatArg::Json)) || args.output.json
        {
            let payload = json!({
                "backupPath": sql_fragments::path_for_sql(&cmd.from),
                "database": cmd.database,
                "dataDir": cmd.data_dir.as_ref().map(|path| sql_fragments::path_for_sql(path)),
                "filelistSql": filelist,
                "note": "Run without --dry-run to inspect logical files and build WITH MOVE restore SQL",
            });
            if !args.quiet {
                println!("{}", json_out::emit_json_value(&payload, true)?);
            }
            return Ok(());
        }
        if !args.quiet {
            println!("{}", filelist);
        }
        return Ok(());
    }

    let resolved = common::load_config(args)?;
    let files = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;
        let result_sets = executor::run_query(Query::new(filelist.clone()), &mut client).await?;
        restore_files_from_result_sets(&result_sets)
    })?;
    let restore_sql = cmd
        .data_dir
        .as_ref()
        .map(|data_dir| build_restore_database_sql(&cmd.from, &cmd.database, data_dir, &files))
        .transpose()?;

    if args.quiet {
        return Ok(());
    }

    let format = common::output_format(args, &resolved);
    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "backupPath": sql_fragments::path_for_sql(&cmd.from),
            "database": cmd.database,
            "dataDir": cmd.data_dir.as_ref().map(|path| sql_fragments::path_for_sql(path)),
            "filelistSql": filelist,
            "files": files,
            "restoreSql": restore_sql,
        });
        println!(
            "{}",
            json_out::emit_json_value(&payload, common::json_pretty(&resolved))?
        );
        return Ok(());
    }

    println!("{}", filelist);
    if let Some(sql) = restore_sql {
        println!();
        println!("{}", sql);
    }
    Ok(())
}

fn run_restore_test(args: &CliArgs, cmd: &AdminBackupRestoreTestArgs) -> Result<()> {
    let data_dir = cmd
        .data_dir
        .as_ref()
        .ok_or_else(|| anyhow!("--data-dir is required for restore-test"))?;
    let compare_tables = cmd.compare_counts.as_deref().unwrap_or(&[]);
    let filelist_sql = restore_filelist_sql(&cmd.from);
    let checkdb_sql = checkdb_sql(&AdminCheckDbArgs {
        database: cmd.database.clone(),
        physical_only: true,
        docker_volume_safe: true,
        dry_run: false,
    });
    let drop_sql = drop_database_sql(&cmd.database);

    if cmd.dry_run {
        if !args.quiet {
            println!("{}", filelist_sql);
            println!(
                "-- Run without --dry-run to generate RESTORE DATABASE WITH MOVE from FILELISTONLY."
            );
            println!("-- Verification database: {}", cmd.database);
            println!(
                "-- Data directory as resolved by SQL Server: {}",
                sql_fragments::path_for_sql(data_dir)
            );
            println!("-- Then run: {}", checkdb_sql);
            if !compare_tables.is_empty() {
                println!("-- Then compare counts for: {}", compare_tables.join(", "));
            }
            if cmd.drop_after {
                println!("-- Then drop verification database with:");
                println!("{}", drop_sql);
            }
        }
        return Ok(());
    }

    let resolved = common::load_config(args)?;
    let source_db = resolved.connection.database.clone();
    let restore_result = tokio::runtime::Runtime::new()?.block_on(async {
        let mut db = client::connect(&resolved.connection).await?;
        let filelist_sets = executor::run_query(Query::new(filelist_sql.clone()), &mut db).await?;
        let files = restore_files_from_result_sets(&filelist_sets)?;
        let restore_sql = build_restore_database_sql(&cmd.from, &cmd.database, data_dir, &files)?;

        executor::run_query(Query::new(restore_sql.clone()), &mut db)
            .await
            .context("restore database failed")?;
        executor::run_query(Query::new(checkdb_sql.clone()), &mut db)
            .await
            .context("DBCC CHECKDB failed")?;

        let count_result_set = if compare_tables.is_empty() {
            None
        } else {
            let count_sql =
                compare_counts::build_same_server_query(&source_db, &cmd.database, compare_tables)?;
            let mut result_sets = executor::run_query(Query::new(count_sql), &mut db)
                .await
                .context("compare-counts failed")?;
            Some(result_sets.pop().unwrap_or_default())
        };

        if cmd.drop_after {
            executor::run_query(Query::new(drop_sql.clone()), &mut db)
                .await
                .context("drop verification database failed")?;
        }

        Ok::<_, anyhow::Error>(RestoreTestResult {
            files,
            restore_sql,
            checkdb_sql,
            count_result_set,
            dropped: cmd.drop_after,
        })
    })?;

    emit_restore_test_result(args, &resolved, &cmd.database, &restore_result)
}

fn checkdb_sql(cmd: &AdminCheckDbArgs) -> String {
    let mut options = Vec::new();
    if cmd.physical_only {
        options.push("PHYSICAL_ONLY");
    }
    if cmd.docker_volume_safe {
        options.push("TABLOCK");
    }
    options.push("NO_INFOMSGS");
    format!(
        "DBCC CHECKDB ({}) WITH {};",
        sql_fragments::quote_string(&cmd.database),
        options.join(", ")
    )
}

fn restore_filelist_sql(path: &Path) -> String {
    format!(
        "RESTORE FILELISTONLY FROM DISK = {};",
        sql_fragments::quote_string(&sql_fragments::path_for_sql(path))
    )
}

#[derive(Debug)]
struct RestoreTestResult {
    files: Vec<RestoreFile>,
    restore_sql: String,
    checkdb_sql: String,
    count_result_set: Option<ResultSet>,
    dropped: bool,
}

fn emit_restore_test_result(
    args: &CliArgs,
    resolved: &crate::config::ResolvedConfig,
    database: &str,
    result: &RestoreTestResult,
) -> Result<()> {
    if args.quiet {
        return Ok(());
    }

    let format = common::output_format(args, resolved);
    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "database": database,
            "files": result.files,
            "restoreSql": result.restore_sql,
            "checkdbSql": result.checkdb_sql,
            "counts": result.count_result_set.as_ref().map(json_out::result_set_rows_to_objects),
            "dropped": result.dropped,
        });
        println!(
            "{}",
            json_out::emit_json_value(&payload, common::json_pretty(resolved))?
        );
        return Ok(());
    }

    println!("Restored verification database: {}", database);
    println!("DBCC CHECKDB completed: {}", database);
    if let Some(result_set) = &result.count_result_set {
        let rendered = table::render_result_set_table(result_set, format, &TableOptions::default());
        println!("{}", rendered.output);
    }
    if result.dropped {
        println!("Dropped verification database: {}", database);
    }
    Ok(())
}

fn restore_files_from_result_sets(result_sets: &[ResultSet]) -> Result<Vec<RestoreFile>> {
    let result_set = result_sets
        .iter()
        .find(|set| !set.rows.is_empty())
        .ok_or_else(|| anyhow!("RESTORE FILELISTONLY returned no rows"))?;
    let logical_idx = column_index(result_set, "LogicalName")?;
    let type_idx = column_index(result_set, "Type")?;

    let mut files = Vec::with_capacity(result_set.rows.len());
    for row in &result_set.rows {
        let logical_name = value_as_text(row.get(logical_idx))
            .ok_or_else(|| anyhow!("RESTORE FILELISTONLY row has no LogicalName"))?;
        let file_type = value_as_text(row.get(type_idx))
            .ok_or_else(|| anyhow!("RESTORE FILELISTONLY row has no Type"))?;
        files.push(RestoreFile {
            logical_name,
            file_type,
        });
    }
    Ok(files)
}

fn column_index(result_set: &ResultSet, name: &str) -> Result<usize> {
    result_set
        .columns
        .iter()
        .position(|column| column.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow!("RESTORE FILELISTONLY did not include {} column", name))
}

fn value_as_text(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::Text(text)) if !text.trim().is_empty() => Some(text.trim().to_string()),
        Some(Value::Int(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn build_restore_database_sql(
    backup_path: &Path,
    database: &str,
    data_dir: &Path,
    files: &[RestoreFile],
) -> Result<String> {
    if files.is_empty() {
        return Err(anyhow!(
            "Cannot build RESTORE DATABASE SQL without logical files"
        ));
    }

    let mut data_seen = 0usize;
    let mut log_seen = 0usize;
    let mut moves = Vec::with_capacity(files.len());
    for file in files {
        let path = restore_file_path(database, data_dir, file, &mut data_seen, &mut log_seen);
        moves.push(format!(
            "MOVE {} TO {}",
            sql_fragments::quote_string(&file.logical_name),
            sql_fragments::quote_string(&path)
        ));
    }

    let mut options = moves;
    options.push("RECOVERY".to_string());
    options.push("STATS = 5".to_string());

    Ok(format!(
        "\
IF DB_ID({database_name}) IS NOT NULL
    THROW 51000, N'Target database already exists. Choose a new verification database name or drop it intentionally first.', 1;
RESTORE DATABASE {database_ident}
FROM DISK = {backup_path}
WITH {options};",
        database_name = sql_fragments::quote_string(database),
        database_ident = sql_fragments::quote_ident(database),
        backup_path = sql_fragments::quote_string(&sql_fragments::path_for_sql(backup_path)),
        options = options.join(",\n     ")
    ))
}

/// Join a SQL Server directory and file name without importing the host's path
/// separator.
///
/// The result is interpreted by SQL Server, which routinely runs on a different
/// OS than sscli. `PathBuf::join` uses the separator of the machine sscli runs
/// on, so building the POSIX path `/var/opt/mssql/data` on Windows would yield
/// `/var/opt/mssql/data\WDM.mdf` and produce an unusable RESTORE statement.
fn join_sql_server_path(data_dir: &Path, filename: &str) -> String {
    let dir = sql_fragments::path_for_sql(data_dir);
    let trimmed = dir.trim_end_matches(['/', '\\']);
    // Only a path that uses backslashes and no forward slashes is a Windows path.
    let separator = if trimmed.contains('\\') && !trimmed.contains('/') {
        '\\'
    } else {
        '/'
    };
    format!("{}{}{}", trimmed, separator, filename)
}

fn restore_file_path(
    database: &str,
    data_dir: &Path,
    file: &RestoreFile,
    data_seen: &mut usize,
    log_seen: &mut usize,
) -> String {
    let safe_database = sanitize_file_component(database);
    let is_log = file.file_type.eq_ignore_ascii_case("L");
    let filename = if is_log {
        let suffix = if *log_seen == 0 {
            "_log".to_string()
        } else {
            format!("_log{}", *log_seen + 1)
        };
        *log_seen += 1;
        format!("{}{}.ldf", safe_database, suffix)
    } else {
        let suffix = if *data_seen == 0 {
            String::new()
        } else {
            format!("_data{}", *data_seen + 1)
        };
        *data_seen += 1;
        format!("{}{}.mdf", safe_database, suffix)
    };
    join_sql_server_path(data_dir, &filename)
}

fn sanitize_file_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "database".to_string()
    } else {
        sanitized
    }
}

fn drop_database_sql(database: &str) -> String {
    format!(
        "\
IF DB_ID({database_name}) IS NOT NULL
BEGIN
    ALTER DATABASE {database_ident} SET SINGLE_USER WITH ROLLBACK IMMEDIATE;
    DROP DATABASE {database_ident};
END;",
        database_name = sql_fragments::quote_string(database),
        database_ident = sql_fragments::quote_ident(database)
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        AdminExecution, BatchOutcome, RestoreFile, VerifyOutcome, build_restore_database_sql,
        execution_to_json, join_sql_server_path,
    };

    fn batch(index: usize, error: Option<&str>) -> BatchOutcome {
        BatchOutcome {
            index,
            elapsed_ms: 4,
            result_sets: 0,
            rows_returned: 0,
            error: error.map(str::to_string),
        }
    }

    fn execution(verify: Option<VerifyOutcome>) -> AdminExecution {
        AdminExecution {
            batches: vec![batch(1, None)],
            result_sets: Vec::new(),
            elapsed_ms: 7,
            verify,
        }
    }

    fn verify_outcome(rows: usize, expected_rows: Option<usize>) -> VerifyOutcome {
        VerifyOutcome {
            sql: "SELECT name FROM sys.databases WHERE name = 'WDM_VERIFY'".to_string(),
            rows,
            expected_rows,
            result_sets: Vec::new(),
        }
    }

    #[test]
    fn verification_without_expectation_is_informational() {
        let outcome = verify_outcome(3, None);
        assert!(outcome.passed());
        assert_eq!(outcome.status_text(), "observed (rows: 3)");
    }

    #[test]
    fn verification_passes_when_row_count_matches_expectation() {
        let outcome = verify_outcome(0, Some(0));
        assert!(outcome.passed());
        assert_eq!(outcome.status_text(), "passed (rows: 0, expected: 0)");
    }

    #[test]
    fn verification_fails_when_row_count_differs_from_expectation() {
        let outcome = verify_outcome(1, Some(0));
        assert!(!outcome.passed());
        assert_eq!(outcome.status_text(), "failed (rows: 1, expected: 0)");
    }

    #[test]
    fn failed_batch_reports_how_far_a_multi_batch_script_got() {
        let execution = AdminExecution {
            batches: vec![batch(1, None), batch(2, Some("Cannot drop the database"))],
            result_sets: Vec::new(),
            elapsed_ms: 9,
            verify: None,
        };

        assert!(!execution.passed());
        assert_eq!(execution.succeeded_batches(), 1);
        assert_eq!(
            execution.failure_message().as_deref(),
            Some(
                "Batch 2 of 2 failed: Cannot drop the database. Batch 1 completed. Statements \
                 before the failure inside batch 2 may have taken effect; confirm the current \
                 state with a read-back (--verify)."
            )
        );

        let payload = execution_to_json(&execution);
        assert_eq!(payload["status"], "failed");
        assert_eq!(payload["batchesSucceeded"], 1);
        assert_eq!(payload["partialEffectPossible"], true);
        assert_eq!(payload["batches"][0]["success"], true);
        assert_eq!(payload["batches"][1]["success"], false);
        assert_eq!(payload["batches"][1]["error"], "Cannot drop the database");
    }

    /// A single failing batch is the semicolon-separated case: earlier statements
    /// in that batch have committed, so the message must not imply a no-op.
    #[test]
    fn single_batch_failure_warns_that_earlier_statements_may_have_applied() {
        let execution = AdminExecution {
            batches: vec![batch(1, Some("Invalid object name"))],
            result_sets: Vec::new(),
            elapsed_ms: 2,
            verify: None,
        };

        let message = execution
            .failure_message()
            .expect("a failed batch always produces a message");
        assert_eq!(
            message,
            "Batch 1 of 1 failed: Invalid object name. Statements before the failure inside batch \
             1 may have taken effect; confirm the current state with a read-back (--verify)."
        );
        assert!(!message.contains("No earlier batch ran"));
        assert_eq!(execution_to_json(&execution)["partialEffectPossible"], true);
    }

    #[test]
    fn execution_json_reports_ok_status_for_statements_without_result_sets() {
        let payload = execution_to_json(&execution(None));

        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["batchCount"], 1);
        assert_eq!(payload["resultSetCount"], 0);
        assert_eq!(payload["rowsReturned"], 0);
        assert_eq!(payload["elapsedMs"], 7);
        assert!(payload["verify"].is_null());
    }

    #[test]
    fn execution_json_reports_failed_status_when_verification_misses_expectation() {
        let payload = execution_to_json(&execution(Some(verify_outcome(1, Some(0)))));

        assert_eq!(payload["status"], "failed");
        assert_eq!(payload["verify"]["rows"], 1);
        assert_eq!(payload["verify"]["expectedRows"], 0);
        assert_eq!(payload["verify"]["passed"], false);
    }

    /// SQL Server interprets these paths, so they must not pick up the host's
    /// separator. This failed on Windows runners before `join_sql_server_path`.
    #[test]
    fn sql_server_paths_keep_their_own_separator_regardless_of_host() {
        assert_eq!(
            join_sql_server_path(Path::new("/var/opt/mssql/data"), "WDM.mdf"),
            "/var/opt/mssql/data/WDM.mdf"
        );
        assert_eq!(
            join_sql_server_path(Path::new("/var/opt/mssql/data/"), "WDM.mdf"),
            "/var/opt/mssql/data/WDM.mdf"
        );
        assert_eq!(
            join_sql_server_path(Path::new(r"C:\SQLData"), "WDM.mdf"),
            r"C:\SQLData\WDM.mdf"
        );
    }

    #[test]
    fn restore_database_sql_moves_logical_files_into_data_dir() {
        let files = vec![
            RestoreFile {
                logical_name: "WDM".to_string(),
                file_type: "D".to_string(),
            },
            RestoreFile {
                logical_name: "WDM_log".to_string(),
                file_type: "L".to_string(),
            },
        ];

        let sql = build_restore_database_sql(
            Path::new("/var/opt/mssql/backup/WDM.bak"),
            "WDM_VERIFY",
            Path::new("/var/opt/mssql/data"),
            &files,
        )
        .expect("restore SQL should be generated");

        assert!(sql.contains("RESTORE DATABASE [WDM_VERIFY]"));
        assert!(sql.contains("MOVE N'WDM' TO N'/var/opt/mssql/data/WDM_VERIFY.mdf'"));
        assert!(sql.contains("MOVE N'WDM_log' TO N'/var/opt/mssql/data/WDM_VERIFY_log.ldf'"));
        assert!(sql.contains("RECOVERY"));
    }
}
