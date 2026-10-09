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

/// Rewrite `@name` references to the `@P{n}` placeholders that bound
/// parameters use.
///
/// # Returns
///
/// The rewritten SQL and whether it referenced any parameter, so a caller can
/// skip `sp_executesql` for SQL that binds nothing.
pub fn replace_named_params(sql: &str, params: &[SqlParam], start_index: usize) -> (String, bool) {
    if params.is_empty() {
        return (sql.to_string(), false);
    }

    let mut map = HashMap::new();
    for (idx, param) in params.iter().enumerate() {
        let placeholder = format!("@P{}", start_index + idx);
        map.insert(param.name.to_lowercase(), placeholder);
    }

    let mut out = String::with_capacity(sql.len());
    let mut matched = false;
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
                matched = true;
            } else {
                out.push('@');
                out.push_str(&ident);
            }
        } else {
            out.push(ch);
        }
    }
    (out, matched)
}

/// Split a script into batches on `GO` lines, as `sqlcmd` does.
///
/// Each batch is the script text between two `GO` lines, byte for byte,
/// line endings and surrounding whitespace included. The server stores that
/// text as a module definition, so trimming it would make a `CREATE TRIGGER`
/// differ from the one `sqlcmd` creates. Whitespace-only batches are dropped.
///
/// # Arguments
///
/// * `script` - full script text; a leading UTF-8 byte order mark is ignored
///
/// # Returns
///
/// Batches in execution order, with `GO n` repeating its batch `n` times.
pub fn split_batches(script: &str) -> Vec<String> {
    let script = script.strip_prefix('\u{feff}').unwrap_or(script);
    let mut batches = Vec::new();
    let mut state = ScanState::default();
    let mut batch_start = 0;
    let mut line_start = 0;

    // `split_inclusive` keeps each line's `\n` (and any `\r` before it), so
    // byte offsets map straight back into `script`.
    for line in script.split_inclusive('\n') {
        let line_end = line_start + line.len();
        let content = line.trim_end_matches(['\n', '\r']);
        if let Some(repeat) = go_repeat_count(content, &mut state) {
            push_batch(&mut batches, &script[batch_start..line_start], repeat);
            batch_start = line_end;
        }
        line_start = line_end;
    }
    push_batch(&mut batches, &script[batch_start..], 1);

    batches
}

fn push_batch(batches: &mut Vec<String>, batch: &str, repeat: usize) {
    if batch.trim().is_empty() {
        return;
    }
    batches.extend(std::iter::repeat_n(batch, repeat).map(str::to_string));
}

#[derive(Debug, Clone, Copy, Default)]
struct ScanState {
    in_single_quote: bool,
    in_double_quote: bool,
    in_bracket_identifier: bool,
    block_comment_depth: usize,
}

fn go_repeat_count(line: &str, state: &mut ScanState) -> Option<usize> {
    // A line that starts inside a string or bracketed identifier continues it,
    // so a `GO` there is data, not a separator.
    let continues_literal =
        state.in_single_quote || state.in_double_quote || state.in_bracket_identifier;
    let visible = visible_sql_text(line, state);
    if continues_literal {
        return None;
    }
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
    let mut chars = line.chars().peekable();

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
        let (replaced, matched) = replace_named_params(sql, &params, 1);
        assert!(replaced.contains("@P1"));
        assert!(replaced.contains("@P2"));
        assert!(matched);
    }

    #[test]
    fn reports_param_named_like_its_placeholder_as_matched() {
        let params = vec![SqlParam {
            name: "P1".to_string(),
            value: "5".to_string(),
        }];
        assert_eq!(
            replace_named_params("SELECT @P1", &params, 1),
            ("SELECT @P1".to_string(), true)
        );
        assert_eq!(
            replace_named_params("SELECT 1", &params, 1),
            ("SELECT 1".to_string(), false)
        );
    }

    #[test]
    fn splits_batches_on_go() {
        let script = "SELECT 1\nGO\nSELECT 2\nGO\nSELECT 3";
        let batches = split_batches(script);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0], "SELECT 1\n");
    }

    #[test]
    fn splits_batches_on_go_with_repeat_count() {
        let script = "SELECT 1\nGO 2\nSELECT 3";
        let batches = split_batches(script);
        assert_eq!(batches, vec!["SELECT 1\n", "SELECT 1\n", "SELECT 3"]);
    }

    #[test]
    fn ignores_go_inside_single_quoted_string() {
        let script = "SELECT 'GO'\nGO\nSELECT 2";
        let batches = split_batches(script);
        assert_eq!(batches, vec!["SELECT 'GO'\n", "SELECT 2"]);
    }

    #[test]
    fn ignores_go_inside_comments() {
        let script = "SELECT 1\n-- GO\n/* GO */\nGO\nSELECT 2";
        let batches = split_batches(script);
        assert_eq!(batches, vec!["SELECT 1\n-- GO\n/* GO */\n", "SELECT 2"]);
    }

    #[test]
    fn supports_go_followed_by_comment() {
        let script = "SELECT 1\nGO -- split here\nSELECT 2";
        let batches = split_batches(script);
        assert_eq!(batches, vec!["SELECT 1\n", "SELECT 2"]);
    }

    #[test]
    fn ignores_go_inside_multiline_block_comment() {
        let script = "/*\nGO\n*/\nSELECT 1\nGO\nSELECT 2";
        let batches = split_batches(script);
        assert_eq!(batches, vec!["/*\nGO\n*/\nSELECT 1\n", "SELECT 2"]);
    }

    #[test]
    fn splits_on_lowercase_indented_go_with_trailing_space() {
        let script = "SELECT 1\n  go  \nSELECT 2";
        assert_eq!(split_batches(script), vec!["SELECT 1\n", "SELECT 2"]);
    }

    #[test]
    fn splits_crlf_scripts() {
        let script = "SELECT 1\r\nGO\r\nSELECT 2\r\n";
        assert_eq!(split_batches(script), vec!["SELECT 1\r\n", "SELECT 2\r\n"]);
    }

    #[test]
    fn ignores_go_line_inside_multiline_string() {
        let script = "SELECT 'first\nGO\nlast'\nGO\nSELECT 2";
        assert_eq!(
            split_batches(script),
            vec!["SELECT 'first\nGO\nlast'\n", "SELECT 2"]
        );
    }

    #[test]
    fn ignores_go_inside_bracket_identifier_and_longer_keywords() {
        let script = "SELECT 1 AS [\nGO\n]\nGOTO done\ndone:\nGO\nSELECT 2";
        assert_eq!(
            split_batches(script),
            vec!["SELECT 1 AS [\nGO\n]\nGOTO done\ndone:\n", "SELECT 2"]
        );
    }

    #[test]
    fn keeps_go_with_invalid_count_as_sql_text() {
        let script = "SELECT 1\nGO x\nSELECT 2";
        assert_eq!(split_batches(script), vec!["SELECT 1\nGO x\nSELECT 2"]);
    }

    #[test]
    fn handles_final_go_and_consecutive_separators() {
        let script = "SELECT 1\nGO\nGO\nSELECT 2\nGO";
        assert_eq!(split_batches(script), vec!["SELECT 1\n", "SELECT 2\n"]);
    }

    #[test]
    fn keeps_batch_text_verbatim_and_drops_byte_order_mark() {
        let script = "\u{feff}-- header\nCREATE PROC p\nAS\n\tSELECT 1;  \n\nGO\n\n  SELECT 2\n";
        assert_eq!(
            split_batches(script),
            vec![
                "-- header\nCREATE PROC p\nAS\n\tSELECT 1;  \n\n",
                "\n  SELECT 2\n"
            ]
        );
    }

    #[test]
    fn ignores_go_inside_nested_block_comments() {
        let script = "/* outer\n/* inner */\nGO\n*/\nSELECT 1\nGO\nSELECT 2";
        let batches = split_batches(script);
        assert_eq!(
            batches,
            vec!["/* outer\n/* inner */\nGO\n*/\nSELECT 1\n", "SELECT 2"]
        );
    }
}
