use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

#[test]
fn admin_checkdb_dry_run_uses_tablock_for_docker_volume_safe_mode() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args([
        "admin",
        "checkdb",
        "--database",
        "WDM",
        "--physical-only",
        "--docker-volume-safe",
        "--dry-run",
    ]);

    cmd.assert().success().stdout(predicate::str::contains(
        "DBCC CHECKDB (N'WDM') WITH PHYSICAL_ONLY, TABLOCK, NO_INFOMSGS;",
    ));
}

#[test]
fn admin_backup_verify_dry_run_emits_verifyonly_sql() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args([
        "admin",
        "backup",
        "verify",
        "--from",
        "/var/opt/mssql/backup/WDM.bak",
        "--checksum",
        "--dry-run",
    ]);

    cmd.assert().success().stdout(predicate::str::contains(
        "RESTORE VERIFYONLY FROM DISK = N'/var/opt/mssql/backup/WDM.bak' WITH CHECKSUM;",
    ));
}

#[test]
fn admin_sql_dry_run_emits_maintenance_and_verification_sql() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args([
        "admin",
        "sql",
        "DROP DATABASE [WDM_VERIFY];",
        "--verify",
        "SELECT name FROM sys.databases WHERE name = 'WDM_VERIFY'",
        "--expect-rows",
        "0",
        "--dry-run",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("DROP DATABASE [WDM_VERIFY];"))
        .stdout(predicate::str::contains(
            "SELECT name FROM sys.databases WHERE name = 'WDM_VERIFY'",
        ));
}

#[test]
fn admin_sql_expect_rows_requires_a_verification_query() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args([
        "admin",
        "sql",
        "DROP DATABASE [WDM_VERIFY];",
        "--expect-rows",
        "0",
        "--dry-run",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--verify"));
}

#[test]
fn compare_counts_dry_run_emits_count_query_without_connecting() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args([
        "compare-counts",
        "--source-db",
        "WDM_SOURCE",
        "--target-db",
        "WDM_TARGET",
        "--tables",
        "dbo.T_MP,dbo.data_set",
        "--dry-run",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("[WDM_SOURCE].[dbo].[T_MP]"))
        .stdout(predicate::str::contains("[WDM_TARGET].[dbo].[data_set]"));
}

#[test]
fn health_identities_dry_run_emits_identity_catalog_query() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["health", "identities", "--schema", "dbo", "--dry-run"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("sys.identity_columns"))
        .stdout(predicate::str::contains("schemaName"));
}
