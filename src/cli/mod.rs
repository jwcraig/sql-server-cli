mod args;

pub use args::{
    BackupsArgs, CliArgs, ColumnsArgs, CommandKind, CompletionsArgs, ConfigArgs, DatabasesArgs,
    DescribeArgs, ForeignKeysArgs, IndexesArgs, InitArgs, IntegrationCommand,
    IntegrationInstallArgs, IntegrationsArgs, OutputFlags, QueryStatsArgs, SessionsArgs, SqlArgs,
    StatusArgs, StoredProcsArgs, TableDataArgs, TablesArgs, UpdateArgs, build_cli,
};

pub fn parse() -> CliArgs {
    args::parse_args()
}
