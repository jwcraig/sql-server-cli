mod args;

pub use args::{
    AdminArgs, AdminBackupCommand, AdminBackupCreateArgs, AdminBackupFilelistArgs,
    AdminBackupRestorePlanArgs, AdminBackupRestoreTestArgs, AdminBackupVerifyArgs,
    AdminCheckDbArgs, AdminCommand, AdminSqlArgs, BackupsArgs, CliArgs, ColumnsArgs, CommandKind,
    CompareArgs, CompareCountsArgs, CompletionsArgs, ConfigArgs, DatabasesArgs, DescribeArgs,
    ForeignKeysArgs, HealthArgs, HealthCheckArgs, HealthCommand, IndexesArgs, InitArgs,
    IntegrationCommand, IntegrationInstallArgs, IntegrationsArgs, OutputFlags, OutputFormatArg,
    QueryStatsArgs, SessionsArgs, SqlArgs, StatusArgs, StoredProcsArgs, TableDataArgs, TablesArgs,
    UpdateArgs, build_cli,
};

pub fn parse() -> CliArgs {
    args::parse_args()
}
