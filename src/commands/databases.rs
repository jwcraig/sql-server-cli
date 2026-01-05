use anyhow::Result;
use serde_json::json;
use tiberius::Query;

use crate::cli::{CliArgs, DatabasesArgs};
use crate::commands::{common, paging};
use crate::config::OutputFormat;
use crate::db::client;
use crate::db::executor;
use crate::db::types::Value;
use crate::output::{json as json_out, table, TableOptions};

const LIMIT_DEFAULT: u64 = 10;
const LIMIT_MAX: u64 = 100;

pub fn run(args: &CliArgs, cmd: &DatabasesArgs) -> Result<()> {
    let resolved = common::load_config(args)?;
    let format = common::output_format(args, &resolved);
    let limit = common::parse_limit(cmd.limit, LIMIT_DEFAULT, LIMIT_MAX);
    let offset = common::parse_offset(cmd.offset);

    let include_system = cmd.include_system;
    let name = cmd.name.clone();
    let owner = cmd.owner.clone();

    let (rows, total) = tokio::runtime::Runtime::new()?.block_on(async {
        let mut client = client::connect(&resolved.connection).await?;

        let list_sql = r#"
WITH filtered AS (
    SELECT
        name AS name,
        state_desc AS state,
        SUSER_SNAME(owner_sid) AS owner,
        recovery_model_desc AS recoveryModel,
        create_date AS createdAt,
        CASE WHEN database_id <= 4 THEN 1 ELSE 0 END AS isSystem,
        ROW_NUMBER() OVER (ORDER BY name) AS rownum
    FROM sys.databases
    WHERE (@P1 = 1 OR database_id > 4)
      AND (@P2 IS NULL OR name LIKE @P2)
      AND (@P3 IS NULL OR SUSER_SNAME(owner_sid) = @P3)
)
SELECT name, state, owner, recoveryModel, createdAt, isSystem
FROM filtered
WHERE rownum BETWEEN (@P4 + 1) AND (@P4 + @P5)
ORDER BY name;
"#;

        let mut list_query = Query::new(list_sql);
        list_query.bind(if include_system { 1i32 } else { 0i32 });
        list_query.bind(name.as_deref());
        list_query.bind(owner.as_deref());
        list_query.bind(offset as i64);
        list_query.bind(limit as i64);

        let list_sets = executor::run_query(list_query, &mut client).await?;
        let list_set = list_sets.into_iter().next().unwrap_or_default();

        let count_sql = r#"
SELECT COUNT(*) AS total
FROM sys.databases
WHERE (@P1 = 1 OR database_id > 4)
  AND (@P2 IS NULL OR name LIKE @P2)
  AND (@P3 IS NULL OR SUSER_SNAME(owner_sid) = @P3);
"#;
        let mut count_query = Query::new(count_sql);
        count_query.bind(if include_system { 1i32 } else { 0i32 });
        count_query.bind(name.as_deref());
        count_query.bind(owner.as_deref());
        let count_sets = executor::run_query(count_query, &mut client).await?;
        let total = count_sets
            .first()
            .and_then(|rs| rs.rows.first())
            .and_then(|row| row.first())
            .and_then(value_as_u64)
            .unwrap_or(0);

        Ok::<_, anyhow::Error>((list_set, total))
    })?;

    let count = rows.rows.len() as u64;
    let paging = paging::build_paging(total, count, offset, limit);

    if matches!(format, OutputFormat::Json) {
        let payload = json!({
            "total": paging.total,
            "count": paging.count,
            "offset": paging.offset,
            "limit": paging.limit,
            "hasMore": paging.has_more,
            "nextOffset": paging.next_offset,
            "databases": json_out::result_set_rows_to_objects(&rows),
        });
        let body = json_out::emit_json_value(&payload, common::json_pretty(&resolved))?;
        if !args.quiet {
            println!("{}", body);
        }
        return Ok(());
    }

    if args.quiet {
        return Ok(());
    }

    let mut options = TableOptions::default();
    if paging.total > 0 {
        let page_limit = if count == 0 { limit } else { count };
        options.pagination = Some(table::Pagination {
            total: Some(paging.total),
            offset: paging.offset,
            limit: page_limit,
        });
    }
    let rendered = table::render_result_set_table(&rows, format, &options);
    println!("{}", rendered);

    Ok(())
}

fn value_as_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Int(v) => (*v).try_into().ok(),
        Value::Float(v) => Some(*v as u64),
        Value::Text(s) => s.parse::<u64>().ok(),
        _ => None,
    }
}
