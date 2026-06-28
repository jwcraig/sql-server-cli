use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use serde_json::json;
use tiberius::Query;

use crate::cli::{
    AdminArgs, AdminBackupCommand, AdminBackupCreateArgs, AdminBackupFilelistArgs,
    AdminBackupRestorePlanArgs, AdminBackupRestoreTestArgs, AdminBackupVerifyArgs,
    AdminCheckDbArgs, AdminCommand, AdminSqlArgs, CliArgs,
};
use crate::commands::{common, compare_counts, sql_fragments};
use crate::config::OutputFormat;
use crate::db::{
    client, executor,
    types::{ResultSet, Value},
};
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
    emit_or_execute(args, sql, cmd.dry_run)
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

fn emit_or_execute(args: &CliArgs, sql: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        if !args.quiet {
            println!("{}", sql);
        }
        return Ok(());
    }

    let resolved = common::load_config(args)?;
    if !args.quiet && !args.quiet_target {
        eprintln!(
            "Target: {}:{}/{}",
            resolved.connection.server, resolved.connection.port, resolved.connection.database
        );
    }

    let result_sets = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;
        executor::run_query(Query::new(sql.to_string()), &mut client).await
    })?;

    emit_result_sets(args, &resolved, &result_sets)
}

fn emit_result_sets(
    args: &CliArgs,
    resolved: &crate::config::ResolvedConfig,
    result_sets: &[crate::db::types::ResultSet],
) -> Result<()> {
    if args.quiet {
        return Ok(());
    }

    let format = common::output_format(args, resolved);
    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "resultSets": result_sets.iter().map(json_out::result_set_to_json).collect::<Vec<_>>(),
        });
        println!(
            "{}",
            json_out::emit_json_value(&payload, common::json_pretty(resolved))?
        );
        return Ok(());
    }

    for result_set in result_sets {
        let rendered = table::render_result_set_table(result_set, format, &TableOptions::default());
        println!("{}", rendered.output);
    }
    Ok(())
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
            sql_fragments::quote_string(&sql_fragments::path_for_sql(&path))
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

fn restore_file_path(
    database: &str,
    data_dir: &Path,
    file: &RestoreFile,
    data_seen: &mut usize,
    log_seen: &mut usize,
) -> PathBuf {
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
    data_dir.join(filename)
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

    use super::{RestoreFile, build_restore_database_sql};

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
