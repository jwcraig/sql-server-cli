use serde::Serialize;
use serde_json::json;

use crate::config::ResolvedConfig;
use crate::db::types::ResultSet;

pub fn emit_json<T: Serialize>(value: &T, pretty: bool) -> anyhow::Result<String> {
    if pretty {
        Ok(serde_json::to_string_pretty(value)?)
    } else {
        Ok(serde_json::to_string(value)?)
    }
}

pub fn emit_json_value(value: &serde_json::Value, pretty: bool) -> anyhow::Result<String> {
    if pretty {
        Ok(serde_json::to_string_pretty(value)?)
    } else {
        Ok(serde_json::to_string(value)?)
    }
}

pub fn error_json(message: &str, kind: &str) -> serde_json::Value {
    json!({
        "error": {
            "message": message,
            "kind": kind,
        }
    })
}

pub fn result_set_to_json(result_set: &ResultSet) -> serde_json::Value {
    json!({
        "columns": result_set.columns,
        "rows": result_set.rows,
    })
}

pub fn result_set_rows_to_objects(result_set: &ResultSet) -> Vec<serde_json::Value> {
    result_set
        .rows
        .iter()
        .map(|row| {
            let mut map = serde_json::Map::new();
            for (col, value) in result_set.columns.iter().zip(row.iter()) {
                let value = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
                map.insert(col.name.clone(), value);
            }
            serde_json::Value::Object(map)
        })
        .collect()
}

pub fn config_to_json(resolved: &ResolvedConfig) -> serde_json::Value {
    json!({
        "configPath": resolved.config_path.as_ref().map(|p| p.display().to_string()),
        "profileName": resolved.profile_name,
        "connection": {
            "server": resolved.connection.server,
            "port": resolved.connection.port,
            "database": resolved.connection.database,
            "user": resolved.connection.user,
            "password": resolved.connection.password,
            "encrypt": resolved.connection.encrypt,
            "trustCert": resolved.connection.trust_cert,
            "timeoutMs": resolved.connection.timeout_ms,
            "defaultSchemas": resolved.connection.default_schemas,
        },
        "settings": {
            "allowWriteDefault": resolved.settings.allow_write_default,
            "output": {
                "defaultFormat": resolved.settings.output.default_format.as_str(),
                "json": {
                    "contractVersion": resolved.settings.output.json.contract_version.as_str(),
                    "pretty": resolved.settings.output.json.pretty,
                },
                "csv": {
                    "multiResultNaming": resolved.settings.output.csv.multi_result_naming.as_str(),
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConnectionSettings, SettingsResolved};
    use crate::db::types::{Column, ResultSet, Value};

    #[test]
    fn emits_error_json() {
        let value = error_json("boom", "Internal");
        assert_eq!(value["error"]["message"], "boom");
        assert_eq!(value["error"]["kind"], "Internal");
    }

    #[test]
    fn config_json_includes_defaults() {
        let resolved = ResolvedConfig {
            config_path: None,
            profile_name: "default".to_string(),
            connection: ConnectionSettings::default(),
            settings: SettingsResolved::default(),
        };
        let value = config_to_json(&resolved);
        assert_eq!(value["profileName"], "default");
        assert_eq!(value["settings"]["output"]["defaultFormat"], "pretty");
    }

    #[test]
    fn result_set_rows_to_objects_builds_maps() {
        let result_set = ResultSet {
            columns: vec![Column {
                name: "name".to_string(),
                data_type: None,
            }],
            rows: vec![vec![Value::Text("db".to_string())]],
        };
        let objects = result_set_rows_to_objects(&result_set);
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0]["name"], "db");
    }
}
