use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl Value {
    pub fn as_display(&self) -> String {
        match self {
            Value::Null => "".to_string(),
            Value::Bool(value) => value.to_string(),
            Value::Int(value) => format_number(*value),
            Value::Float(value) => value.to_string(),
            Value::Text(value) => value.clone(),
        }
    }

    pub fn as_csv(&self) -> String {
        match self {
            Value::Null => "".to_string(),
            Value::Bool(value) => value.to_string(),
            Value::Int(value) => value.to_string(),
            Value::Float(value) => value.to_string(),
            Value::Text(value) => value.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResultSet {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
}

fn format_number(value: i64) -> String {
    let digits = value.abs().to_string().chars().rev().collect::<Vec<_>>();
    let mut out = String::new();
    for (idx, ch) in digits.iter().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(*ch);
    }
    let mut out: String = out.chars().rev().collect();
    if value < 0 {
        out.insert(0, '-');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers_with_commas() {
        assert_eq!(format_number(1234567), "1,234,567");
        assert_eq!(format_number(-9876543), "-9,876,543");
    }
}
