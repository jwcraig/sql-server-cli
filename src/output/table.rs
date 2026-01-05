use comfy_table::{presets, ContentArrangement, Table};

use crate::config::OutputFormat;
use crate::db::types::{ResultSet, Value};

const NULL_DISPLAY: &str = "—";
const ELLIPSIS: &str = "…";
const DEFAULT_MAX_CELL_WIDTH: usize = 140;
const DEFAULT_MAX_OUTPUT_CHARS: usize = 25_000;

#[derive(Debug, Clone)]
pub struct Pagination {
    pub total: Option<u64>,
    pub offset: u64,
    pub limit: u64,
}

#[derive(Debug, Clone)]
pub struct TableOptions {
    pub max_cell_width: usize,
    pub max_output_chars: usize,
    pub pagination: Option<Pagination>,
}

impl Default for TableOptions {
    fn default() -> Self {
        Self {
            max_cell_width: DEFAULT_MAX_CELL_WIDTH,
            max_output_chars: DEFAULT_MAX_OUTPUT_CHARS,
            pagination: None,
        }
    }
}

pub fn render_result_set_table(
    result_set: &ResultSet,
    format: OutputFormat,
    options: &TableOptions,
) -> String {
    let mut table = Table::new();
    match format {
        OutputFormat::Markdown => {
            table.load_preset(presets::ASCII_MARKDOWN);
        }
        _ => {
            table.load_preset(presets::UTF8_FULL);
        }
    }
    table.set_content_arrangement(ContentArrangement::Dynamic);

    let headers = result_set
        .columns
        .iter()
        .map(|col| col.name.clone())
        .collect::<Vec<_>>();
    table.set_header(headers);

    for row in &result_set.rows {
        let cells = row
            .iter()
            .map(|value| format_cell(value, options.max_cell_width))
            .collect::<Vec<_>>();
        table.add_row(cells);
    }

    let mut output = table.to_string();
    if let Some(pagination) = &options.pagination {
        let footer = pagination_footer(pagination);
        output.push('\n');
        output.push_str(&footer);
    }

    truncate_output(output, options.max_output_chars)
}

pub fn render_key_value_table(
    title: &str,
    rows: &[(String, String)],
    format: OutputFormat,
    options: &TableOptions,
) -> String {
    let mut table = Table::new();
    match format {
        OutputFormat::Markdown => {
            table.load_preset(presets::ASCII_MARKDOWN);
        }
        _ => {
            table.load_preset(presets::UTF8_FULL);
        }
    }
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![title.to_string(), "Value".to_string()]);

    for (key, value) in rows {
        let key = truncate_string(key, options.max_cell_width);
        let value = truncate_string(value, options.max_cell_width);
        table.add_row(vec![key, value]);
    }

    truncate_output(table.to_string(), options.max_output_chars)
}

fn format_cell(value: &Value, max_cell_width: usize) -> String {
    let raw = match value {
        Value::Null => NULL_DISPLAY.to_string(),
        _ => value.as_display(),
    };
    truncate_string(&raw, max_cell_width)
}

fn truncate_string(input: &str, max_len: usize) -> String {
    let len = input.chars().count();
    if len <= max_len {
        return input.to_string();
    }
    if max_len <= 1 {
        return ELLIPSIS.to_string();
    }
    let truncated: String = input.chars().take(max_len - 1).collect();
    format!("{}{}", truncated, ELLIPSIS)
}

fn pagination_footer(pagination: &Pagination) -> String {
    let start = pagination.offset + 1;
    let end = pagination.offset + pagination.limit;
    match pagination.total {
        Some(total) => format!(
            "Rows {}-{} of {} (next: --offset {})",
            start, end, total, end
        ),
        None => format!("Rows {}-{} (next: --offset {})", start, end, end),
    }
}

fn truncate_output(output: String, max_len: usize) -> String {
    if output.len() <= max_len {
        return output;
    }
    let mut truncated = output.chars().take(max_len).collect::<String>();
    truncated.push_str("\n[output truncated]");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::types::{Column, ResultSet};

    #[test]
    fn truncates_cells() {
        let value = Value::Text("abcdefghijklmnopqrstuvwxyz".to_string());
        let out = format_cell(&value, 8);
        assert_eq!(out, "abcdefg…");
    }

    #[test]
    fn renders_null_display() {
        let rs = ResultSet {
            columns: vec![Column {
                name: "value".to_string(),
                data_type: None,
            }],
            rows: vec![vec![Value::Null]],
        };
        let out = render_result_set_table(&rs, OutputFormat::Pretty, &TableOptions::default());
        assert!(out.contains("—"));
    }

    #[test]
    fn adds_pagination_footer() {
        let rs = ResultSet {
            columns: vec![Column {
                name: "value".to_string(),
                data_type: None,
            }],
            rows: vec![vec![Value::Int(1)]],
        };
        let mut options = TableOptions::default();
        options.pagination = Some(Pagination {
            total: Some(10),
            offset: 0,
            limit: 1,
        });
        let out = render_result_set_table(&rs, OutputFormat::Pretty, &options);
        assert!(out.contains("Rows 1-1 of 10"));
    }

    #[test]
    fn truncates_output_when_too_long() {
        let rs = ResultSet {
            columns: vec![Column {
                name: "value".to_string(),
                data_type: None,
            }],
            rows: vec![vec![Value::Text("x".repeat(200))]],
        };
        let mut options = TableOptions::default();
        options.max_output_chars = 50;
        let out = render_result_set_table(&rs, OutputFormat::Pretty, &options);
        assert!(out.contains("[output truncated]"));
    }
}
