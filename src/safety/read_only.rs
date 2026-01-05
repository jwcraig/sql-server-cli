use anyhow::{anyhow, Result};
use std::collections::HashSet;

const BLOCKED_KEYWORDS: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "MERGE", "ALTER", "DROP", "CREATE", "TRUNCATE", "GRANT",
    "REVOKE", "BACKUP", "RESTORE",
];

const ALLOWED_PROCS: &[&str] = &[
    "sp_help",
    "sp_helptext",
    "sp_columns",
    "sp_tables",
    "sp_stored_procedures",
    "sp_statistics",
    "sp_pkeys",
    "sp_fkeys",
    "sp_server_info",
    "sp_databases",
    "sp_datatype_info",
    "sp_special_columns",
];

pub fn allowed_procedures() -> Vec<&'static str> {
    ALLOWED_PROCS.to_vec()
}

pub fn validate_read_only(sql: &str) -> Result<()> {
    let cleaned = strip_leading_comments(sql);
    let lead = first_token(cleaned).ok_or_else(|| anyhow!("Empty SQL input"))?;
    let lead_upper = lead.to_uppercase();

    if lead_upper == "EXEC" || lead_upper == "EXECUTE" {
        let target = extract_exec_target(cleaned)
            .ok_or_else(|| anyhow!("EXEC/EXECUTE requires a stored procedure name"))?;
        let normalized = normalize_proc_name(&target)
            .ok_or_else(|| anyhow!("EXEC target could not be parsed"))?;
        let allowed: HashSet<&str> = ALLOWED_PROCS.iter().copied().collect();
        if !allowed.contains(normalized.as_str()) {
            return Err(anyhow!(
                "Stored procedure '{}' is not in the allowlist",
                normalized
            ));
        }
    } else if lead_upper != "SELECT" && lead_upper != "WITH" {
        return Err(anyhow!(
            "Only read-only queries (SELECT/CTE/EXEC allowlist) are permitted"
        ));
    }

    if let Some(keyword) = find_blocked_keyword(sql) {
        return Err(anyhow!("Blocked keyword detected: {}", keyword));
    }

    Ok(())
}

fn strip_leading_comments(input: &str) -> &str {
    let mut remaining = input;
    loop {
        let trimmed = remaining.trim_start();
        if trimmed.starts_with("--") {
            if let Some(pos) = trimmed.find('\n') {
                remaining = &trimmed[pos + 1..];
                continue;
            }
            return "";
        }
        if trimmed.starts_with("/*") {
            if let Some(pos) = trimmed.find("*/") {
                remaining = &trimmed[pos + 2..];
                continue;
            }
            return "";
        }
        return trimmed;
    }
}

fn first_token(input: &str) -> Option<String> {
    let mut token = String::new();
    for ch in input.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            token.push(ch);
        } else if !token.is_empty() {
            break;
        }
        // Skip whitespace and other characters until we find the first token
    }
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn extract_exec_target(input: &str) -> Option<String> {
    let cleaned = strip_leading_comments(input);
    let mut chars = cleaned.chars().peekable();
    let mut seen_token = false;

    for ch in chars.by_ref() {
        if ch.is_alphanumeric() {
            seen_token = true;
            while let Some(next) = chars.peek() {
                if next.is_alphanumeric() || *next == '_' {
                    chars.next();
                } else {
                    break;
                }
            }
            break;
        }
    }

    if !seen_token {
        return None;
    }

    let mut target = String::new();
    for ch in chars.by_ref() {
        if ch.is_whitespace() {
            continue;
        }
        if ch == ';' {
            return None;
        }
        target.push(ch);
        break;
    }

    if target.is_empty() {
        return None;
    }

    for ch in chars {
        if ch.is_whitespace() || ch == '(' || ch == ';' {
            break;
        }
        target.push(ch);
    }

    Some(target)
}

fn normalize_proc_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parts: Vec<&str> = trimmed.split('.').collect();
    let last = parts.last()?;
    let name = last.trim_matches(|c| c == '[' || c == ']');
    if name.is_empty() {
        return None;
    }
    Some(name.to_lowercase())
}

fn find_blocked_keyword(input: &str) -> Option<String> {
    let mut token = String::new();
    for ch in input.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            token.push(ch);
        } else if !token.is_empty() {
            if is_blocked(&token) {
                return Some(token.to_uppercase());
            }
            token.clear();
        }
    }
    if !token.is_empty() && is_blocked(&token) {
        return Some(token.to_uppercase());
    }
    None
}

fn is_blocked(token: &str) -> bool {
    let upper = token.to_uppercase();
    BLOCKED_KEYWORDS.iter().any(|kw| *kw == upper)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_select() {
        assert!(validate_read_only("SELECT * FROM users").is_ok());
    }

    #[test]
    fn allows_with() {
        assert!(validate_read_only("WITH cte AS (SELECT 1) SELECT * FROM cte").is_ok());
    }

    #[test]
    fn allows_exec_allowed_proc() {
        assert!(validate_read_only("EXEC sp_help").is_ok());
        assert!(validate_read_only("EXEC dbo.sp_helptext").is_ok());
        assert!(validate_read_only("EXEC [dbo].[sp_helptext]").is_ok());
    }

    #[test]
    fn blocks_exec_unknown_proc() {
        let err = validate_read_only("EXEC sp_configure").unwrap_err();
        assert!(err.to_string().contains("allowlist"));
    }

    #[test]
    fn blocks_write_keyword() {
        let err = validate_read_only("SELECT 1; DROP TABLE users").unwrap_err();
        assert!(err.to_string().contains("Blocked keyword"));
    }

    #[test]
    fn blocks_non_select_prefix() {
        let err = validate_read_only("UPDATE users SET name='x'").unwrap_err();
        assert!(err.to_string().contains("read-only"));
    }
}
