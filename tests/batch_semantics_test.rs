//! DB-backed checks that `sql` and `admin sql` run `GO` scripts like `sqlcmd -b`:
//! one session, session state carried across batches, stop at the first error.
//! Opt-in: set `SSCLI_INTEGRATION_TESTS=1` plus connection settings. The
//! rollback test creates and rolls back a table in the target database.

mod common;

use assert_cmd::cargo::cargo_bin_cmd;

/// A transaction, `#temp` table, `SET` option and `USE` from earlier batches are
/// all still in effect in the last batch, which then commits.
const SPANNING_SCRIPT: &str = "\
USE tempdb;
SET XACT_ABORT ON;
BEGIN TRANSACTION;
GO
CREATE TABLE #carried (id int);
INSERT #carried VALUES (1);
GO
SELECT @@TRANCOUNT AS trancount,
       (SELECT COUNT(*) FROM #carried) AS temp_rows,
       CASE WHEN (@@OPTIONS & 16384) = 16384 THEN 1 ELSE 0 END AS xact_abort,
       DB_NAME() AS db;
COMMIT TRANSACTION;
GO
";

fn spanning_row(stdout: &[u8]) -> serde_json::Value {
    let value: serde_json::Value = serde_json::from_slice(stdout).expect("json");
    value["resultSets"][0]["rows"][0].clone()
}

#[test]
fn sql_carries_transaction_and_session_state_across_batches() {
    if !common::integration_enabled() {
        return;
    }

    let output = cargo_bin_cmd!("sscli")
        .args(["sql", "--json", "--stdin"])
        .write_stdin(SPANNING_SCRIPT)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(
        spanning_row(&output),
        serde_json::json!([1, 1, 1, "tempdb"])
    );
}

#[test]
fn admin_sql_carries_transaction_and_session_state_across_batches() {
    if !common::integration_enabled() {
        return;
    }

    let output = cargo_bin_cmd!("sscli")
        .args(["admin", "sql", "--json", SPANNING_SCRIPT])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(
        spanning_row(&output),
        serde_json::json!([1, 1, 1, "tempdb"])
    );
}

#[test]
fn sql_stops_at_first_failing_batch_and_rolls_back_open_transaction() {
    if !common::integration_enabled() {
        return;
    }
    let table = format!("dbo.sscli_rollback_probe_{}", std::process::id());
    let script = format!(
        "BEGIN TRANSACTION;\nGO\nCREATE TABLE {table} (id int);\nGO\n\
         SELECT 1/0 AS boom;\nGO\nTHROW 50001, 'batch after the failure ran', 1;\nGO\n\
         COMMIT TRANSACTION;\nGO\n"
    );

    let failed = cargo_bin_cmd!("sscli")
        .args(["sql", "--stdin"])
        .write_stdin(script)
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&failed);
    assert!(stderr.contains("Divide by zero"), "{stderr}");
    assert!(!stderr.contains("batch after the failure ran"), "{stderr}");

    // The script never committed, so closing its session rolled the table back.
    let check = common::run_json([
        "sql",
        "--json",
        &format!("SELECT OBJECT_ID(N'{table}') AS probe"),
    ]);
    assert_eq!(
        check["resultSets"][0]["rows"][0][0],
        serde_json::Value::Null
    );
}
