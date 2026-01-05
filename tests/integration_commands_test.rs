mod common;

fn fetch_indexed_table() -> (String, String) {
    if let Ok(table) = std::env::var("SSCLI_TEST_TABLE") {
        let schema = std::env::var("SSCLI_TEST_SCHEMA").unwrap_or_else(|_| "dbo".to_string());
        return (schema, table);
    }

    let query = "\
SELECT TOP (1)\n\
    s.name AS schemaName,\n\
    t.name AS tableName\n\
FROM sys.indexes i\n\
INNER JOIN sys.tables t ON i.object_id = t.object_id\n\
INNER JOIN sys.schemas s ON t.schema_id = s.schema_id\n\
WHERE i.name IS NOT NULL\n\
ORDER BY t.name;";

    let value = common::run_json(["sql", "--json", query]);
    let rows = value["resultSets"][0]["rows"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    if rows.is_empty() {
        panic!("No indexed tables found. Set SSCLI_TEST_SCHEMA/SSCLI_TEST_TABLE to override.");
    }

    let row = rows[0].as_array().expect("row array");
    let schema = row
        .get(0)
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let table = row
        .get(1)
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();

    if schema.is_empty() || table.is_empty() {
        panic!("Failed to resolve test table from SQL query.");
    }

    (schema, table)
}

#[test]
fn status_json_smoke() {
    if !common::integration_enabled() {
        return;
    }

    let value = common::run_json(["status", "--json"]);
    assert_eq!(value["status"], "ok");
}

#[test]
fn databases_json_smoke() {
    if !common::integration_enabled() {
        return;
    }

    let value = common::run_json(["databases", "--json", "--limit", "1", "--include-system"]);
    assert!(value.get("databases").is_some());
}

#[test]
fn tables_json_smoke() {
    if !common::integration_enabled() {
        return;
    }

    let (schema, table) = fetch_indexed_table();
    let value = common::run_json([
        "tables",
        "--json",
        "--schema",
        schema.as_str(),
        "--like",
        table.as_str(),
        "--include-views",
        "--limit",
        "1",
    ]);
    assert!(value.get("tables").is_some());
}

#[test]
fn columns_json_smoke() {
    if !common::integration_enabled() {
        return;
    }

    let (schema, table) = fetch_indexed_table();
    let value = common::run_json([
        "columns",
        "--json",
        "--schema",
        schema.as_str(),
        "--table",
        table.as_str(),
        "--include-views",
        "--limit",
        "1",
    ]);
    assert!(value.get("columns").is_some());
}

#[test]
fn describe_json_smoke() {
    if !common::integration_enabled() {
        return;
    }

    let (schema, table) = fetch_indexed_table();
    let value = common::run_json([
        "describe",
        "--json",
        "--schema",
        schema.as_str(),
        "--table",
        table.as_str(),
    ]);
    assert!(value.get("columns").is_some());
}

#[test]
fn table_data_json_smoke() {
    if !common::integration_enabled() {
        return;
    }

    let (schema, table) = fetch_indexed_table();
    let value = common::run_json([
        "table-data",
        "--json",
        "--schema",
        schema.as_str(),
        "--table",
        table.as_str(),
        "--limit",
        "1",
    ]);
    assert!(value.get("rows").is_some());
}

#[test]
fn sql_json_smoke() {
    if !common::integration_enabled() {
        return;
    }

    let value = common::run_json(["sql", "--json", "SELECT 1 AS value"]);
    assert!(value.get("resultSets").is_some());
}

#[test]
fn indexes_json_smoke() {
    if !common::integration_enabled() {
        return;
    }

    let (schema, table) = fetch_indexed_table();
    let value = common::run_json([
        "indexes",
        "--json",
        "--schema",
        schema.as_str(),
        "--table",
        table.as_str(),
    ]);
    assert!(value.get("indexes").is_some());
}

#[test]
fn foreign_keys_json_smoke() {
    if !common::integration_enabled() {
        return;
    }

    let (schema, table) = fetch_indexed_table();
    let value = common::run_json([
        "foreign-keys",
        "--json",
        "--schema",
        schema.as_str(),
        "--table",
        table.as_str(),
        "--direction",
        "both",
    ]);
    assert!(value.get("foreignKeys").is_some());
}

#[test]
fn stored_procs_json_smoke() {
    if !common::integration_enabled() {
        return;
    }

    let value = common::run_json(["stored-procs", "--json", "--limit", "1", "--include-system"]);
    assert!(value.get("procedures").is_some());
}
