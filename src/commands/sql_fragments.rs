use anyhow::{Result, anyhow};

pub fn quote_string(value: &str) -> String {
    format!("N'{}'", value.replace('\'', "''"))
}

pub fn quote_ident(value: &str) -> String {
    format!("[{}]", value.replace(']', "]]"))
}

pub fn quote_db_object(database: &str, object: &str) -> Result<String> {
    let (schema, table) = split_table_name(object)?;
    Ok(format!(
        "{}.{}.{}",
        quote_ident(database),
        quote_ident(&schema),
        quote_ident(&table)
    ))
}

pub fn split_table_name(object: &str) -> Result<(String, String)> {
    let cleaned = object.replace(['[', ']'], "");
    let mut parts = cleaned.split('.');
    let first = parts.next().unwrap_or("").trim();
    let second = parts.next().map(str::trim);
    if parts.next().is_some() {
        return Err(anyhow!(
            "Expected table name as schema.table or table: {}",
            object
        ));
    }

    match second {
        Some(table) if !first.is_empty() && !table.is_empty() => {
            Ok((first.to_string(), table.to_string()))
        }
        None if !first.is_empty() => Ok(("dbo".to_string(), first.to_string())),
        _ => Err(anyhow!("Invalid table name: {}", object)),
    }
}

pub fn path_for_sql(path: &std::path::Path) -> String {
    path.to_string_lossy().to_string()
}
