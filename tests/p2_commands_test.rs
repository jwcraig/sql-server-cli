mod common;

#[test]
fn sessions_json_smoke() {
    if !common::integration_enabled() {
        return;
    }

    let value = common::run_json(["sessions", "--json", "--limit", "1"]);
    assert!(value.get("sessions").is_some());
}

#[test]
fn query_stats_json_smoke() {
    if !common::integration_enabled() {
        return;
    }

    let value = common::run_json(["query-stats", "--json", "--limit", "1"]);
    assert!(value.get("queries").is_some());
}

#[test]
fn backups_json_smoke() {
    if !common::integration_enabled() {
        return;
    }

    let value = common::run_json(["backups", "--json", "--limit", "1"]);
    assert!(value.get("backups").is_some());
}
