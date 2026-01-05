use anyhow::Result;

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

pub async fn collect_result_sets(stream: tiberius::QueryStream<'_>) -> Result<Vec<ResultSet>> {
    let result_sets = stream
        .into_results()
        .await
        .map_err(|err| AppError::new(ErrorKind::Query, err.to_string()))?;
    let mut output = Vec::new();

    for rows in result_sets {
        let columns = rows
            .first()
            .map(|row| {
                row.columns()
                    .iter()
                    .map(|col| Column {
                        name: col.name().to_string(),
                        data_type: None,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut converted_rows = Vec::new();
        for row in rows {
            let values = row.cells().map(|(_, data)| map_column_data(data)).collect();
            converted_rows.push(values);
        }

        output.push(ResultSet {
            columns,
            rows: converted_rows,
        });
    }

    Ok(output)
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
                Value::Text(format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, hours, mins, secs))
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
                Value::Text(format!("{:04}-{:02}-{:02} {:02}:{:02}:00", y, m, d, hours, mins))
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
                    y, m, d, time_str, sign, abs_mins / 60, abs_mins % 60
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
        let frac_str = format!("{:09}", frac_nanos).trim_end_matches('0').to_string();
        format!("{:02}:{:02}:{:02}.{}", hours, mins, secs, frac_str)
    } else {
        format!("{:02}:{:02}:{:02}", hours, mins, secs)
    }
}
