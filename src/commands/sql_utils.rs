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
    let mut state = ScanState::default();

    for line in script.lines() {
        if let Some(repeat) = go_repeat_count(line, &mut state) {
            if !current.is_empty() {
                let batch = current.join("\n").trim().to_string();
                for _ in 0..repeat {
                    batches.push(batch.clone());
                }
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

#[derive(Debug, Clone, Copy, Default)]
struct ScanState {
    in_single_quote: bool,
    in_double_quote: bool,
    in_bracket_identifier: bool,
    block_comment_depth: usize,
}

fn go_repeat_count(line: &str, state: &mut ScanState) -> Option<usize> {
    let visible = visible_sql_text(line, state);
    let trimmed = visible.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut parts = trimmed.split_whitespace();
    let keyword = parts.next()?;
    if !keyword.eq_ignore_ascii_case("GO") {
        return None;
    }

    match parts.next() {
        None => Some(1),
        Some(count) => {
            if parts.next().is_some() {
                return None;
            }
            count.parse::<usize>().ok().filter(|value| *value > 0)
        }
    }
}

fn visible_sql_text(line: &str, state: &mut ScanState) -> String {
    let mut visible = String::new();
    let mut chars = line
        .strip_prefix('\u{feff}')
        .unwrap_or(line)
        .chars()
        .peekable();

    while let Some(ch) = chars.next() {
        if state.block_comment_depth > 0 {
            if ch == '/' && chars.peek() == Some(&'*') {
                chars.next();
                state.block_comment_depth += 1;
                continue;
            }

            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                state.block_comment_depth -= 1;
            }
            continue;
        }

        if state.in_single_quote {
            visible.push(ch);
            if ch == '\'' {
                if chars.peek() == Some(&'\'') {
                    visible.push(chars.next().expect("peeked escaped quote"));
                } else {
                    state.in_single_quote = false;
                }
            }
            continue;
        }

        if state.in_double_quote {
            visible.push(ch);
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    visible.push(chars.next().expect("peeked escaped quote"));
                } else {
                    state.in_double_quote = false;
                }
            }
            continue;
        }

        if state.in_bracket_identifier {
            visible.push(ch);
            if ch == ']' {
                if chars.peek() == Some(&']') {
                    visible.push(chars.next().expect("peeked escaped bracket"));
                } else {
                    state.in_bracket_identifier = false;
                }
            }
            continue;
        }

        if ch == '-' && chars.peek() == Some(&'-') {
            break;
        }

        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            state.block_comment_depth = 1;
            continue;
        }

        if ch == '\'' {
            state.in_single_quote = true;
            visible.push(ch);
            continue;
        }

        if ch == '"' {
            state.in_double_quote = true;
            visible.push(ch);
            continue;
        }

        if ch == '[' {
            state.in_bracket_identifier = true;
            visible.push(ch);
            continue;
        }

        visible.push(ch);
    }

    visible
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

    #[test]
    fn splits_batches_on_go_with_repeat_count() {
        let script = "SELECT 1\nGO 2\nSELECT 3";
        let batches = split_batches(script);
        assert_eq!(batches, vec!["SELECT 1", "SELECT 1", "SELECT 3"]);
    }

    #[test]
    fn ignores_go_inside_single_quoted_string() {
        let script = "SELECT 'GO'\nGO\nSELECT 2";
        let batches = split_batches(script);
        assert_eq!(batches, vec!["SELECT 'GO'", "SELECT 2"]);
    }

    #[test]
    fn ignores_go_inside_comments() {
        let script = "SELECT 1\n-- GO\n/* GO */\nGO\nSELECT 2";
        let batches = split_batches(script);
        assert_eq!(batches, vec!["SELECT 1\n-- GO\n/* GO */", "SELECT 2"]);
    }

    #[test]
    fn supports_go_followed_by_comment() {
        let script = "SELECT 1\nGO -- split here\nSELECT 2";
        let batches = split_batches(script);
        assert_eq!(batches, vec!["SELECT 1", "SELECT 2"]);
    }

    #[test]
    fn ignores_go_inside_multiline_block_comment() {
        let script = "/*\nGO\n*/\nSELECT 1\nGO\nSELECT 2";
        let batches = split_batches(script);
        assert_eq!(batches, vec!["/*\nGO\n*/\nSELECT 1", "SELECT 2"]);
    }

    #[test]
    fn ignores_go_inside_nested_block_comments() {
        let script = "/* outer\n/* inner */\nGO\n*/\nSELECT 1\nGO\nSELECT 2";
        let batches = split_batches(script);
        assert_eq!(
            batches,
            vec!["/* outer\n/* inner */\nGO\n*/\nSELECT 1", "SELECT 2"]
        );
    }
}
