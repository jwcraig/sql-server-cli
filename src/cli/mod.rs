mod args;

pub use args::{
    build_cli, BackupsArgs, CliArgs, ColumnsArgs, CommandKind, CompletionsArgs, ConfigArgs,
    DatabasesArgs, DescribeArgs, ForeignKeysArgs, IndexesArgs, InitArgs, IntegrationCommand,
    IntegrationInstallArgs, IntegrationsArgs, OutputFlags, QueryStatsArgs, SessionsArgs, SqlArgs,
    StatusArgs, StoredProcsArgs, TableDataArgs, TablesArgs,
};

pub fn parse() -> CliArgs {
    args::parse_args()
}
