use comfy_table::{ContentArrangement, Table, presets};

use crate::config::OutputFormat;
use crate::db::types::{ResultSet, Value};

const NULL_DISPLAY: &str = "—";
const ELLIPSIS: &str = "…";
const QUERY_MAX_CELL_WIDTH: usize = 140;
const QUERY_MAX_OUTPUT_CHARS: usize = 25_000;

#[derive(Debug, Clone)]
pub struct Pagination {
    pub total: Option<u64>,
    pub offset: u64,
    pub limit: u64,
}

/// Metadata about truncation applied during rendering.
#[derive(Debug, Clone, Default)]
pub struct TruncationInfo {
    /// Whether the output was truncated.
    pub truncated: bool,
    /// Original character count before truncation (if truncated).
    pub original_chars: Option<usize>,
    /// Character count after truncation (if truncated).
    pub truncated_chars: Option<usize>,
}

/// Rendered table output with truncation metadata.
#[derive(Debug, Clone)]
pub struct RenderResult {
    pub output: String,
    pub truncation: TruncationInfo,
}

#[derive(Debug, Clone)]
pub struct TableOptions {
    pub max_cell_width: usize,
    pub max_output_chars: usize,
    pub pagination: Option<Pagination>,
}

impl Default for TableOptions {
    fn default() -> Self {
        Self::unlimited()
    }
}

impl TableOptions {
    /// Create options with query-oriented truncation limits.
    pub fn truncated() -> Self {
        Self {
            max_cell_width: QUERY_MAX_CELL_WIDTH,
            max_output_chars: QUERY_MAX_OUTPUT_CHARS,
            pagination: None,
        }
    }

    /// Create options with no truncation limits.
    pub fn unlimited() -> Self {
        Self {
            max_cell_width: usize::MAX,
            max_output_chars: usize::MAX,
            pagination: None,
        }
    }
}

pub fn render_result_set_table(
    result_set: &ResultSet,
    format: OutputFormat,
    options: &TableOptions,
) -> RenderResult {
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
    // Remove default left/right padding to avoid visible gaps inside cells when using UTF-8 borders.
    for column in table.column_iter_mut() {
        column.set_padding((0, 0));
    }

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
) -> RenderResult {
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
    for column in table.column_iter_mut() {
        column.set_padding((0, 0));
    }
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

fn truncate_output(output: String, max_len: usize) -> RenderResult {
    let original_len = output.len();
    if original_len <= max_len {
        return RenderResult {
            output,
            truncation: TruncationInfo::default(),
        };
    }
    let truncated_len = max_len;
    let omitted = original_len - truncated_len;
    let mut truncated = output.chars().take(max_len).collect::<String>();
    truncated.push_str(&format!(
        "\n[output truncated: {} chars omitted; use --no-truncate for full output]",
        omitted
    ));
    RenderResult {
        output: truncated,
        truncation: TruncationInfo {
            truncated: true,
            original_chars: Some(original_len),
            truncated_chars: Some(truncated_len),
        },
    }
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
        let result = render_result_set_table(&rs, OutputFormat::Pretty, &TableOptions::default());
        assert!(result.output.contains("—"));
        assert!(!result.truncation.truncated);
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
        let options = TableOptions {
            pagination: Some(Pagination {
                total: Some(10),
                offset: 0,
                limit: 1,
            }),
            ..TableOptions::default()
        };
        let result = render_result_set_table(&rs, OutputFormat::Pretty, &options);
        assert!(result.output.contains("Rows 1-1 of 10"));
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
        let mut options = TableOptions::truncated();
        options.max_output_chars = 50;
        let result = render_result_set_table(&rs, OutputFormat::Pretty, &options);
        assert!(result.output.contains("[output truncated:"));
        assert!(result.truncation.truncated);
        assert!(result.truncation.original_chars.unwrap() > 50);
        assert_eq!(result.truncation.truncated_chars.unwrap(), 50);
    }

    #[test]
    fn default_options_do_not_truncate() {
        let rs = ResultSet {
            columns: vec![Column {
                name: "value".to_string(),
                data_type: None,
            }],
            rows: vec![vec![Value::Text("x".repeat(50_000))]],
        };
        let result = render_result_set_table(&rs, OutputFormat::Pretty, &TableOptions::default());
        assert!(!result.truncation.truncated);
        assert!(!result.output.contains("[output truncated]"));
    }

    #[test]
    fn truncated_options_apply_limits() {
        let rs = ResultSet {
            columns: vec![Column {
                name: "value".to_string(),
                data_type: None,
            }],
            rows: (0..400)
                .map(|_| vec![Value::Text("x".repeat(300))])
                .collect(),
        };
        let result = render_result_set_table(&rs, OutputFormat::Pretty, &TableOptions::truncated());
        assert!(result.truncation.truncated);
        assert!(result.output.contains("[output truncated:"));
    }

    #[test]
    fn unlimited_options_no_truncate() {
        let rs = ResultSet {
            columns: vec![Column {
                name: "value".to_string(),
                data_type: None,
            }],
            rows: vec![vec![Value::Text("x".repeat(50_000))]],
        };
        let result = render_result_set_table(&rs, OutputFormat::Pretty, &TableOptions::unlimited());
        assert!(!result.truncation.truncated);
        assert!(!result.output.contains("[output truncated]"));
    }
}
