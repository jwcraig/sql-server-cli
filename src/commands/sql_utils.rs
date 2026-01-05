use anyhow::{Result, anyhow};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SqlParam {
    pub name: String,
    pub value: String,
}

pub fn parse_params(raw: &[String]) -> Result<Vec<SqlParam>> {
    let mut params = Vec::new();
    for entry in raw {
        let mut parts = entry.splitn(2, '=');
        let name = parts.next().unwrap_or("").trim();
        let value = parts.next();
        if name.is_empty() {
            return Err(anyhow!("Invalid --param '{}'. Missing name.", entry));
        }
        let value = value.ok_or_else(|| anyhow!("Invalid --param '{}'. Use name=value.", entry))?;
        params.push(SqlParam {
            name: name.to_string(),
            value: value.to_string(),
        });
    }
    Ok(params)
}

pub fn replace_named_params(sql: &str, params: &[SqlParam], start_index: usize) -> String {
    if params.is_empty() {
        return sql.to_string();
    }

    let mut map = HashMap::new();
    for (idx, param) in params.iter().enumerate() {
        let placeholder = format!("@P{}", start_index + idx);
        map.insert(param.name.to_lowercase(), placeholder);
    }

    let mut out = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '@' {
            let mut ident = String::new();
            while let Some(next) = chars.peek() {
                if next.is_alphanumeric() || *next == '_' {
                    ident.push(*next);
                    chars.next();
                } else {
                    break;
                }
            }
            if ident.is_empty() {
                out.push('@');
            } else if let Some(replacement) = map.get(&ident.to_lowercase()) {
                out.push_str(replacement);
            } else {
                out.push('@');
                out.push_str(&ident);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn split_batches(script: &str) -> Vec<String> {
    let mut batches = Vec::new();
    let mut current = Vec::new();

    for line in script.lines() {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("GO") {
            if !current.is_empty() {
                batches.push(current.join("\n").trim().to_string());
                current.clear();
            }
        } else {
            current.push(line.to_string());
        }
    }

    if !current.is_empty() {
        batches.push(current.join("\n").trim().to_string());
    }

    batches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_params() {
        let params = parse_params(&["foo=bar".to_string(), "x=1".to_string()]).unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "foo");
        assert_eq!(params[1].value, "1");
    }

    #[test]
    fn replaces_named_params() {
        let params = vec![
            SqlParam {
                name: "foo".to_string(),
                value: "bar".to_string(),
            },
            SqlParam {
                name: "baz".to_string(),
                value: "qux".to_string(),
            },
        ];
        let sql = "SELECT * FROM t WHERE a=@foo AND b=@baz";
        let replaced = replace_named_params(sql, &params, 1);
        assert!(replaced.contains("@P1"));
        assert!(replaced.contains("@P2"));
    }

    #[test]
    fn splits_batches_on_go() {
        let script = "SELECT 1\nGO\nSELECT 2\nGO\nSELECT 3";
        let batches = split_batches(script);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0], "SELECT 1");
    }
}
