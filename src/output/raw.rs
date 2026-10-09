use anyhow::{Result, anyhow};
use serde_json::Value as JsonValue;

use crate::db::types::{ResultSet, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawFormat {
    Tsv,
    Csv,
    Jsonl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawOptions {
    pub format: RawFormat,
    pub headers: bool,
    pub null_value: String,
    pub result_set: Option<usize>,
    pub all_result_sets: bool,
}

pub fn render_result_sets(result_sets: &[ResultSet], options: &RawOptions) -> Result<String> {
    let sets = select_result_sets(result_sets, options)?;
    let mut output = String::new();

    for result_set in sets {
        let rendered = match options.format {
            RawFormat::Tsv => render_delimited(result_set, b'\t', options)?,
            RawFormat::Csv => render_delimited(result_set, b',', options)?,
            RawFormat::Jsonl => render_jsonl(result_set)?,
        };
        output.push_str(&rendered);
    }

    Ok(output)
}

fn select_result_sets<'a>(
    result_sets: &'a [ResultSet],
    options: &RawOptions,
) -> Result<Vec<&'a ResultSet>> {
    if let Some(index) = options.result_set {
        if index == 0 {
            return Err(anyhow!("--result-set is 1-based"));
        }
        let result_set = result_sets
            .get(index - 1)
            .ok_or_else(|| anyhow!("Result set {} not found", index))?;
        return Ok(vec![result_set]);
    }

    if options.all_result_sets {
        return Ok(result_sets.iter().collect());
    }

    if result_sets.len() == 1 {
        return Ok(vec![&result_sets[0]]);
    }

    Err(anyhow!(
        "Multiple result sets returned; use --result-set <n> or --all-result-sets"
    ))
}

fn render_delimited(result_set: &ResultSet, delimiter: u8, options: &RawOptions) -> Result<String> {
    let mut bytes = Vec::new();
    {
        let mut writer = csv::WriterBuilder::new()
            .delimiter(delimiter)
            .terminator(csv::Terminator::Any(b'\n'))
            .from_writer(&mut bytes);

        if options.headers {
            writer.write_record(result_set.columns.iter().map(|column| column.name.as_str()))?;
        }

        for row in &result_set.rows {
            let record = row
                .iter()
                .map(|value| raw_text_value(value, &options.null_value, delimiter))
                .collect::<Vec<_>>();
            writer.write_record(record)?;
        }
        writer.flush()?;
    }

    String::from_utf8(bytes).map_err(Into::into)
}

fn raw_text_value(value: &Value, null_value: &str, delimiter: u8) -> String {
    let raw = match value {
        Value::Null => return null_value.to_string(),
        _ => value.as_csv(),
    };

    if delimiter == b'\t' {
        raw.replace(['\t', '\r', '\n'], " ")
    } else {
        raw
    }
}

fn render_jsonl(result_set: &ResultSet) -> Result<String> {
    let mut output = String::new();
    for row in &result_set.rows {
        let mut map = serde_json::Map::new();
        for (column, value) in result_set.columns.iter().zip(row.iter()) {
            let json_value = match value {
                Value::Null => JsonValue::Null,
                _ => serde_json::to_value(value)?,
            };
            map.insert(column.name.clone(), json_value);
        }
        output.push_str(&serde_json::to_string(&JsonValue::Object(map))?);
        output.push('\n');
    }
    Ok(output)
}
