use anyhow::Result;
use futures_util::TryStreamExt;

use crate::db::types::{Column, ResultSet, Value};
use crate::error::{AppError, ErrorKind};

pub async fn run_query(
    query: tiberius::Query<'_>,
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
) -> Result<Vec<ResultSet>> {
    let stream = query
        .query(client)
        .await
        .map_err(|err| AppError::new(ErrorKind::Query, err.to_string()))?;
    collect_result_sets(stream).await
}

/// Run one script batch (the text between two `GO` lines) the way `sqlcmd` does.
///
/// A batch without parameters goes to the server as a plain SQL batch, so session
/// state it leaves behind (an open transaction, `SET` options, `USE`, `#temp`
/// tables) carries over to the next batch on the same connection. `Query` would
/// wrap it in `sp_executesql`, whose scope discards that state and rejects a
/// transaction left open with error 266.
///
/// A batch with parameters still needs `sp_executesql`, so a transaction cannot
/// stay open across it.
///
/// # Arguments
///
/// * `batch` - SQL text of a single batch, without the `GO` separator
/// * `params` - values bound to `@P1..@Pn`; empty for a plain batch
/// * `client` - the connection every batch of the script shares
///
/// # Errors
///
/// Returns the first server error of the batch, after the server has finished
/// the batch, as `sqlcmd -b` would report it.
pub async fn run_batch<'a>(
    batch: &'a str,
    params: &[&'a str],
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
) -> Result<Vec<ResultSet>> {
    if !params.is_empty() {
        let mut query = tiberius::Query::new(batch);
        for param in params {
            query.bind(*param);
        }
        return run_query(query, client).await;
    }

    let stream = client
        .simple_query(batch)
        .await
        .map_err(|err| AppError::new(ErrorKind::Query, err.to_string()))?;
    collect_result_sets(stream).await
}

pub async fn collect_result_sets(stream: tiberius::QueryStream<'_>) -> Result<Vec<ResultSet>> {
    let mut stream = stream;
    let mut collector = ResultSetCollector::default();

    while let Some(item) = stream
        .try_next()
        .await
        .map_err(|err| AppError::new(ErrorKind::Query, err.to_string()))?
    {
        match item {
            tiberius::QueryItem::Metadata(metadata) => {
                collector.start_result_set(metadata.columns());
            }
            tiberius::QueryItem::Row(row) => {
                collector.push_row(&row);
            }
        }
    }

    Ok(collector.finish())
}

#[derive(Debug, Default)]
struct ResultSetCollector {
    result_sets: Vec<ResultSet>,
    current: Option<ResultSet>,
}

impl ResultSetCollector {
    fn start_result_set(&mut self, columns: &[tiberius::Column]) {
        self.finish_current();
        self.current = Some(ResultSet {
            columns: map_columns(columns),
            rows: Vec::new(),
        });
    }

    fn push_row(&mut self, row: &tiberius::Row) {
        if self.current.is_none() {
            self.current = Some(ResultSet {
                columns: map_columns(row.columns()),
                rows: Vec::new(),
            });
        }
        let values = row.cells().map(|(_, data)| map_column_data(data)).collect();
        self.push_values(values);
    }

    fn push_values(&mut self, values: Vec<Value>) {
        if self.current.is_none() {
            self.current = Some(ResultSet::default());
        }
        self.current
            .as_mut()
            .expect("current result set must exist before pushing row values")
            .rows
            .push(values);
    }

    fn finish(mut self) -> Vec<ResultSet> {
        self.finish_current();
        self.result_sets
    }

    fn finish_current(&mut self) {
        if let Some(result_set) = self.current.take() {
            self.result_sets.push(result_set);
        }
    }
}

fn map_columns(columns: &[tiberius::Column]) -> Vec<Column> {
    columns
        .iter()
        .map(|col| Column {
            name: col.name().to_string(),
            data_type: None,
        })
        .collect()
}

fn map_column_data(data: &tiberius::ColumnData<'_>) -> Value {
    use tiberius::ColumnData::*;
    match data {
        U8(value) => value.map(|v| Value::Int(v as i64)).unwrap_or(Value::Null),
        I16(value) => value.map(|v| Value::Int(v as i64)).unwrap_or(Value::Null),
        I32(value) => value.map(|v| Value::Int(v as i64)).unwrap_or(Value::Null),
        I64(value) => value.map(Value::Int).unwrap_or(Value::Null),
        F32(value) => value.map(|v| Value::Float(v as f64)).unwrap_or(Value::Null),
        F64(value) => value.map(Value::Float).unwrap_or(Value::Null),
        Bit(value) => value.map(Value::Bool).unwrap_or(Value::Null),
        String(value) => value
            .as_ref()
            .map(|v| Value::Text(v.to_string()))
            .unwrap_or(Value::Null),
        Guid(value) => value
            .as_ref()
            .map(|v| Value::Text(v.to_string()))
            .unwrap_or(Value::Null),
        Binary(value) => value
            .as_ref()
            .map(|v| Value::Text(format!("{:?}", v)))
            .unwrap_or(Value::Null),
        Numeric(value) => value
            .as_ref()
            .map(|v| Value::Text(v.to_string()))
            .unwrap_or(Value::Null),
        Xml(value) => value
            .as_ref()
            .map(|v| Value::Text(v.to_string()))
            .unwrap_or(Value::Null),
        DateTime(value) => value
            .as_ref()
            .map(|v| {
                // tiberius DateTime: days since 1900-01-01, seconds_fragments in 1/300th seconds
                let (y, m, d) = days_to_ymd(v.days() as i64);
                let total_secs = v.seconds_fragments() / 300;
                let hours = total_secs / 3600;
                let mins = (total_secs % 3600) / 60;
                let secs = total_secs % 60;
                Value::Text(format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                    y, m, d, hours, mins, secs
                ))
            })
            .unwrap_or(Value::Null),
        SmallDateTime(value) => value
            .as_ref()
            .map(|v| {
                // SmallDateTime: days since 1900-01-01, seconds_fragments in minutes
                let (y, m, d) = days_to_ymd(v.days() as i64);
                let total_mins = v.seconds_fragments();
                let hours = total_mins / 60;
                let mins = total_mins % 60;
                Value::Text(format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}:00",
                    y, m, d, hours, mins
                ))
            })
            .unwrap_or(Value::Null),
        #[cfg(feature = "tds73")]
        Time(value) => value
            .map(|v| Value::Text(format_tds_time(v)))
            .unwrap_or(Value::Null),
        #[cfg(feature = "tds73")]
        Date(value) => value
            .map(|v| {
                let (y, m, d) = days_to_ymd_from_year1(v.days() as i64);
                Value::Text(format!("{:04}-{:02}-{:02}", y, m, d))
            })
            .unwrap_or(Value::Null),
        #[cfg(feature = "tds73")]
        DateTime2(value) => value
            .map(|v| {
                let (y, m, d) = days_to_ymd_from_year1(v.date().days() as i64);
                let time_str = format_tds_time(v.time());
                Value::Text(format!("{:04}-{:02}-{:02} {}", y, m, d, time_str))
            })
            .unwrap_or(Value::Null),
        #[cfg(feature = "tds73")]
        DateTimeOffset(value) => value
            .map(|v| {
                let (y, m, d) = days_to_ymd_from_year1(v.datetime2().date().days() as i64);
                let time_str = format_tds_time(v.datetime2().time());
                let offset_mins = v.offset();
                let sign = if offset_mins >= 0 { '+' } else { '-' };
                let abs_mins = offset_mins.abs();
                Value::Text(format!(
                    "{:04}-{:02}-{:02} {} {}{:02}:{:02}",
                    y,
                    m,
                    d,
                    time_str,
                    sign,
                    abs_mins / 60,
                    abs_mins % 60
                ))
            })
            .unwrap_or(Value::Null),
    }
}

/// Convert days since 1900-01-01 to (year, month, day)
fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    // Start from 1900-01-01
    let mut year = 1900i32;
    let mut remaining = days;

    // Fast-forward years
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    // Find month
    let leap = is_leap_year(year);
    let days_in_months: [i64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for &dim in &days_in_months {
        if remaining < dim {
            break;
        }
        remaining -= dim;
        month += 1;
    }

    let day = (remaining + 1) as u32;
    (year, month, day)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Convert days since year 1 (Jan 1, year 1) to (year, month, day)
/// Used for TDS 7.3+ Date/DateTime2/DateTimeOffset types
#[cfg(feature = "tds73")]
fn days_to_ymd_from_year1(days: i64) -> (i32, u32, u32) {
    let mut year = 1i32;
    let mut remaining = days;

    // Fast-forward years
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    // Find month
    let leap = is_leap_year(year);
    let days_in_months: [i64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for &dim in &days_in_months {
        if remaining < dim {
            break;
        }
        remaining -= dim;
        month += 1;
    }

    let day = (remaining + 1) as u32;
    (year, month, day)
}

/// Format TDS 7.3 Time type to string
#[cfg(feature = "tds73")]
fn format_tds_time(time: tiberius::time::Time) -> String {
    let increments = time.increments();
    let scale = time.scale();
    // Convert increments to nanoseconds
    let nanos = increments * 10u64.pow(9 - scale as u32);
    let total_secs = nanos / 1_000_000_000;
    let frac_nanos = nanos % 1_000_000_000;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if frac_nanos > 0 {
        // Trim trailing zeros from fractional part
        let frac_str = format!("{:09}", frac_nanos)
            .trim_end_matches('0')
            .to_string();
        format!("{:02}:{:02}:{:02}.{}", hours, mins, secs, frac_str)
    } else {
        format!("{:02}:{:02}:{:02}", hours, mins, secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiberius::{Column as TdsColumn, ColumnType};

    fn tds_column(name: &str) -> TdsColumn {
        TdsColumn::new(name.to_string(), ColumnType::Int4)
    }

    #[test]
    fn collector_preserves_headers_for_empty_result_set_before_rows() {
        let mut collector = ResultSetCollector::default();
        collector.start_result_set(&[tds_column("empty_int")]);
        collector.start_result_set(&[tds_column("full_int")]);
        collector.push_values(vec![Value::Int(7)]);

        let result_sets = collector.finish();

        assert_eq!(result_sets.len(), 2);
        assert_eq!(result_sets[0].columns[0].name, "empty_int");
        assert!(result_sets[0].rows.is_empty());
        assert_eq!(result_sets[1].columns[0].name, "full_int");
        assert_eq!(result_sets[1].rows, vec![vec![Value::Int(7)]]);
    }

    #[test]
    fn collector_keeps_consecutive_empty_result_sets() {
        let mut collector = ResultSetCollector::default();
        collector.start_result_set(&[tds_column("first_empty")]);
        collector.start_result_set(&[tds_column("second_empty")]);

        let result_sets = collector.finish();

        assert_eq!(result_sets.len(), 2);
        assert_eq!(result_sets[0].columns[0].name, "first_empty");
        assert!(result_sets[0].rows.is_empty());
        assert_eq!(result_sets[1].columns[0].name, "second_empty");
        assert!(result_sets[1].rows.is_empty());
    }
}
