mod backups;
mod columns;
mod common;
mod completions;
mod config;
mod databases;
mod describe;
mod foreign_keys;
mod help;
mod indexes;
mod init;
mod integrations;
mod paging;
mod query_stats;
mod sessions;
mod sql;
mod sql_utils;
mod status;
mod stored_procs;
mod table_data;
mod tables;

use anyhow::Result;

use crate::cli::{CliArgs, CommandKind};

pub fn dispatch(args: &CliArgs) -> Result<()> {
    match &args.command {
        CommandKind::Help { all, command } => help::run(*all, command.as_deref()),
        CommandKind::Status(cmd) => status::run(args, cmd),
        CommandKind::Databases(cmd) => databases::run(args, cmd),
        CommandKind::Tables(cmd) => tables::run(args, cmd),
        CommandKind::Describe(cmd) => describe::run(args, cmd),
        CommandKind::Sql(cmd) => sql::run(args, cmd),
        CommandKind::TableData(cmd) => table_data::run(args, cmd),
        CommandKind::Columns(cmd) => columns::run(args, cmd),
        CommandKind::Indexes(cmd) => indexes::run(args, cmd),
        CommandKind::ForeignKeys(cmd) => foreign_keys::run(args, cmd),
        CommandKind::StoredProcs(cmd) => stored_procs::run(args, cmd),
        CommandKind::Sessions(cmd) => sessions::run(args, cmd),
        CommandKind::QueryStats(cmd) => query_stats::run(args, cmd),
        CommandKind::Backups(cmd) => backups::run(args, cmd),
        CommandKind::Init(cmd) => init::run(args, cmd),
        CommandKind::Config(_) => config::run(args),
        CommandKind::Completions(cmd) => completions::run(args, cmd),
        CommandKind::Integrations(cmd) => integrations::run(args, cmd),
    }
}
