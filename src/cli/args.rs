use std::path::PathBuf;

use clap::{Arg, ArgAction, ArgMatches, Command, ValueHint};

#[derive(Debug, Clone)]
pub struct OutputFlags {
    pub json: bool,
    pub markdown: bool,
    pub pretty: bool,
}

#[derive(Debug, Clone)]
pub struct CliArgs {
    pub config_path: Option<PathBuf>,
    pub env_file: Option<PathBuf>,
    pub profile: Option<String>,
    pub server: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub timeout_ms: Option<u64>,
    pub allow_write: bool,
    pub encrypt: Option<bool>,
    pub trust_cert: Option<bool>,
    pub output: OutputFlags,
    pub verbose: u8,
    pub quiet: bool,
    pub command: CommandKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    Help { all: bool, command: Option<String> },
    Status(StatusArgs),
    Databases(DatabasesArgs),
    Tables(TablesArgs),
    Describe(DescribeArgs),
    Sql(SqlArgs),
    TableData(TableDataArgs),
    Columns(ColumnsArgs),
    Indexes(IndexesArgs),
    ForeignKeys(ForeignKeysArgs),
    StoredProcs(StoredProcsArgs),
    Sessions(SessionsArgs),
    QueryStats(QueryStatsArgs),
    Backups(BackupsArgs),
    Init(InitArgs),
    Config(ConfigArgs),
    Completions(CompletionsArgs),
    Integrations(IntegrationsArgs),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatusArgs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabasesArgs {
    pub name: Option<String>,
    pub owner: Option<String>,
    pub include_system: bool,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TablesArgs {
    pub schema: Option<String>,
    pub like: Option<String>,
    pub include_views: bool,
    pub with_counts: bool,
    pub summary: bool,
    pub describe: bool,
    pub limit: Option<String>,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescribeArgs {
    pub object: Option<String>,
    pub schema: Option<String>,
    pub object_type: Option<String>,
    pub include_all: bool,
    pub no_indexes: bool,
    pub no_triggers: bool,
    pub no_ddl: bool,
    pub include_fks: bool,
    pub include_constraints: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlArgs {
    pub sql: Option<String>,
    pub file: Option<PathBuf>,
    pub params: Vec<String>,
    pub max_rows: Option<u64>,
    pub csv: Option<PathBuf>,
    pub dry_run: bool,
    pub continue_on_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDataArgs {
    pub table: Option<String>,
    pub schema: Option<String>,
    pub columns: Option<String>,
    pub where_clause: Option<String>,
    pub order_by: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub params: Vec<String>,
    pub csv: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnsArgs {
    pub like: Option<String>,
    pub table: Option<String>,
    pub schema: Option<String>,
    pub include_views: bool,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexesArgs {
    pub table: Option<String>,
    pub schema: Option<String>,
    pub show_usage: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKeysArgs {
    pub table: Option<String>,
    pub schema: Option<String>,
    pub direction: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredProcsArgs {
    pub schema: Option<String>,
    pub name: Option<String>,
    pub include_system: bool,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub exec: Option<String>,
    pub args: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionsArgs {
    pub database: Option<String>,
    pub login: Option<String>,
    pub host: Option<String>,
    pub status: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryStatsArgs {
    pub database: Option<String>,
    pub order: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupsArgs {
    pub database: Option<String>,
    pub since: Option<u64>,
    pub backup_type: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitArgs {
    pub path: Option<PathBuf>,
    pub force: bool,
    pub profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfigArgs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionsArgs {
    pub shell: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationsArgs {
    pub command: IntegrationCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrationCommand {
    Help,
    Skills(IntegrationInstallArgs),
    Gemini(IntegrationInstallArgs),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationInstallArgs {
    pub global: bool,
    pub name: Option<String>,
}

pub fn build_cli(show_all: bool) -> Command {
    let mut cmd = Command::new("sscli")
        .about("SQL Server CLI tool for database inspection")
        .version(env!("CARGO_PKG_VERSION"))
        .arg_required_else_help(true)
        .disable_help_subcommand(true)
        .subcommand_value_name("COMMAND");

    cmd = add_global_args(cmd);

    cmd = cmd.subcommand(command_help());

    cmd = cmd.subcommand(command_status(show_all));
    cmd = cmd.subcommand(command_databases(show_all));
    cmd = cmd.subcommand(command_tables(show_all));
    cmd = cmd.subcommand(command_describe(show_all));
    cmd = cmd.subcommand(command_sql(show_all));
    cmd = cmd.subcommand(command_table_data(show_all));
    cmd = cmd.subcommand(command_columns(show_all));
    cmd = cmd.subcommand(command_init(show_all));
    cmd = cmd.subcommand(command_config(show_all));

    cmd = cmd.subcommand(command_indexes(show_all));
    cmd = cmd.subcommand(command_foreign_keys(show_all));
    cmd = cmd.subcommand(command_stored_procs(show_all));
    cmd = cmd.subcommand(command_completions(show_all));
    cmd = cmd.subcommand(command_sessions(show_all));
    cmd = cmd.subcommand(command_query_stats(show_all));
    cmd = cmd.subcommand(command_backups(show_all));
    cmd = cmd.subcommand(command_integrations(show_all));

    cmd
}

pub fn parse_args() -> CliArgs {
    let matches = build_cli(false).get_matches();
    parse_matches(&matches)
}

fn add_global_args(cmd: Command) -> Command {
    cmd.arg(
        Arg::new("config")
            .long("config")
            .value_name("PATH")
            .value_hint(ValueHint::FilePath)
            .global(true)
            .help("Override config file location"),
    )
    .arg(
        Arg::new("env-file")
            .long("env-file")
            .value_name("PATH")
            .value_hint(ValueHint::FilePath)
            .global(true)
            .help("Load environment variables from file (default: .env)"),
    )
    .arg(
        Arg::new("profile")
            .long("profile")
            .value_name("NAME")
            .global(true)
            .help("Select connection profile"),
    )
    .arg(
        Arg::new("server")
            .long("server")
            .value_name("HOST")
            .global(true)
            .help("SQL Server hostname"),
    )
    .arg(
        Arg::new("port")
            .long("port")
            .value_name("PORT")
            .value_parser(clap::value_parser!(u16))
            .global(true)
            .help("SQL Server port (default: 1433)"),
    )
    .arg(
        Arg::new("database")
            .long("database")
            .value_name("NAME")
            .global(true)
            .help("Database name (default: master)"),
    )
    .arg(
        Arg::new("user")
            .long("user")
            .value_name("USER")
            .global(true)
            .help("SQL Server username"),
    )
    .arg(
        Arg::new("password")
            .long("password")
            .value_name("PASS")
            .global(true)
            .help("SQL Server password"),
    )
    .arg(
        Arg::new("timeout")
            .long("timeout")
            .value_name("MS")
            .value_parser(clap::value_parser!(u64))
            .global(true)
            .help("Connection timeout in milliseconds"),
    )
    .arg(
        Arg::new("allow-write")
            .long("allow-write")
            .action(ArgAction::SetTrue)
            .global(true)
            .help("Allow write operations (dangerous; applies to SQL-executing commands only)"),
    )
    .arg(
        Arg::new("encrypt")
            .long("encrypt")
            .value_parser(clap::value_parser!(bool))
            .global(true)
            .help("Enable connection encryption"),
    )
    .arg(
        Arg::new("trust-cert")
            .long("trust-cert")
            .value_parser(clap::value_parser!(bool))
            .global(true)
            .help("Trust server certificate"),
    )
    .arg(
        Arg::new("json")
            .long("json")
            .action(ArgAction::SetTrue)
            .global(true)
            .help("Output as JSON"),
    )
    .arg(
        Arg::new("markdown")
            .long("markdown")
            .action(ArgAction::SetTrue)
            .global(true)
            .help("Force markdown table output"),
    )
    .arg(
        Arg::new("pretty")
            .long("pretty")
            .long("pretty-print")
            .action(ArgAction::SetTrue)
            .global(true)
            .help("Force pretty-printed table output"),
    )
    .arg(
        Arg::new("verbose")
            .short('v')
            .long("verbose")
            .action(ArgAction::Count)
            .global(true)
            .help("Enable debug logging"),
    )
    .arg(
        Arg::new("quiet")
            .short('q')
            .long("quiet")
            .action(ArgAction::SetTrue)
            .global(true)
            .help("Suppress non-error output"),
    )
}

fn command_help() -> Command {
    Command::new("help")
        .about("Show help for commands")
        .arg(
            Arg::new("all")
                .long("all")
                .action(ArgAction::SetTrue)
                .help("Show all commands, including advanced ones"),
        )
        .arg(Arg::new("command").value_name("COMMAND"))
}

fn command_core(
    name: &'static str,
    about: &'static str,
    aliases: &'static [&'static str],
    _show_all: bool,
) -> Command {
    let mut cmd = Command::new(name).about(about);
    for alias in aliases {
        cmd = cmd.visible_alias(*alias);
    }
    cmd
}

fn command_advanced(
    name: &'static str,
    about: &'static str,
    aliases: &'static [&'static str],
    show_all: bool,
) -> Command {
    let mut cmd = Command::new(name).about(about);
    for alias in aliases {
        cmd = cmd.visible_alias(*alias);
    }
    if !show_all {
        cmd = cmd.hide(true);
    }
    cmd
}

fn command_status(show_all: bool) -> Command {
    command_core(
        "status",
        "Connectivity smoke test",
        &["db-status"],
        show_all,
    )
}

fn command_databases(show_all: bool) -> Command {
    command_core("databases", "List databases", &[], show_all)
        .arg(Arg::new("name").long("name").value_name("pattern"))
        .arg(Arg::new("owner").long("owner").value_name("login"))
        .arg(
            Arg::new("include-system")
                .long("include-system")
                .action(ArgAction::SetTrue)
                .help("Include system databases"),
        )
        .arg(
            Arg::new("limit")
                .long("limit")
                .value_name("n")
                .value_parser(clap::value_parser!(u64)),
        )
        .arg(
            Arg::new("offset")
                .long("offset")
                .value_name("n")
                .value_parser(clap::value_parser!(u64)),
        )
}

fn command_tables(show_all: bool) -> Command {
    command_core("tables", "Browse tables/views", &[], show_all)
        .arg(Arg::new("schema").long("schema").value_name("name"))
        .arg(Arg::new("like").long("like").value_name("pattern"))
        .arg(
            Arg::new("include-views")
                .long("include-views")
                .action(ArgAction::SetTrue)
                .help("Include views alongside tables"),
        )
        .arg(
            Arg::new("with-counts")
                .long("with-counts")
                .action(ArgAction::SetTrue)
                .help("Attach estimated row counts"),
        )
        .arg(
            Arg::new("summary")
                .long("summary")
                .action(ArgAction::SetTrue)
                .help("Show all tables in a single view"),
        )
        .arg(
            Arg::new("describe")
                .long("describe")
                .action(ArgAction::SetTrue)
                .help("Describe each table (DDL, columns, indexes). Default limit 5, use --limit for more."),
        )
        .arg(Arg::new("limit").long("limit").value_name("n|all|0"))
        .arg(
            Arg::new("offset")
                .long("offset")
                .value_name("n")
                .value_parser(clap::value_parser!(u64)),
        )
}

fn command_describe(show_all: bool) -> Command {
    command_core(
        "describe",
        "Describe any database object (table, view, trigger, proc, function)",
        &["desc"],
        show_all,
    )
    .arg(
        Arg::new("object")
            .index(1)
            .value_name("OBJECT")
            .help("Object name to describe"),
    )
    .arg(Arg::new("schema").long("schema").value_name("name"))
    .arg(
        Arg::new("type")
            .long("type")
            .value_name("TYPE")
            .value_parser(["table", "view", "trigger", "proc", "function"])
            .help("Force object type (auto-detected if omitted)"),
    )
    .arg(
        Arg::new("all")
            .long("all")
            .action(ArgAction::SetTrue)
            .help("Include foreign keys and constraints (tables only)"),
    )
    .arg(
        Arg::new("no-indexes")
            .long("no-indexes")
            .action(ArgAction::SetTrue)
            .help("Exclude indexes from output (tables only)"),
    )
    .arg(
        Arg::new("no-triggers")
            .long("no-triggers")
            .action(ArgAction::SetTrue)
            .help("Exclude triggers from output (tables only)"),
    )
    .arg(
        Arg::new("no-ddl")
            .long("no-ddl")
            .action(ArgAction::SetTrue)
            .help("Exclude DDL/definition from output"),
    )
    .arg(
        Arg::new("include-fks")
            .long("include-fks")
            .action(ArgAction::SetTrue)
            .help("Include foreign key relationships (tables only)"),
    )
    .arg(
        Arg::new("include-constraints")
            .long("include-constraints")
            .action(ArgAction::SetTrue)
            .help("Include check/unique constraints (tables only)"),
    )
}

fn command_sql(show_all: bool) -> Command {
    command_core(
        "sql",
        "Execute SQL (read-only default)",
        &["query"],
        show_all,
    )
    .arg(
        Arg::new("sql")
            .index(1)
            .value_name("SQL")
            .help("SQL statement to execute"),
    )
    .arg(
        Arg::new("file")
            .long("file")
            .value_name("path")
            .value_hint(ValueHint::FilePath)
            .conflicts_with("sql"),
    )
    .arg(
        Arg::new("param")
            .long("param")
            .value_name("name=value")
            .action(ArgAction::Append),
    )
    .arg(
        Arg::new("max-rows")
            .long("max-rows")
            .value_name("n")
            .value_parser(clap::value_parser!(u64)),
    )
    .arg(
        Arg::new("csv")
            .long("csv")
            .value_name("file")
            .value_hint(ValueHint::FilePath),
    )
    .arg(
        Arg::new("dry-run")
            .long("dry-run")
            .action(ArgAction::SetTrue),
    )
    .arg(
        Arg::new("continue-on-error")
            .long("continue-on-error")
            .action(ArgAction::SetTrue),
    )
}

fn command_table_data(show_all: bool) -> Command {
    command_core(
        "table-data",
        "Sample/browse data",
        &["data", "head"],
        show_all,
    )
    .arg(Arg::new("table").long("table").value_name("name"))
    .arg(Arg::new("schema").long("schema").value_name("name"))
    .arg(Arg::new("columns").long("columns").value_name("list"))
    .arg(Arg::new("where").long("where").value_name("expr"))
    .arg(Arg::new("order-by").long("order-by").value_name("expr"))
    .arg(
        Arg::new("limit")
            .long("limit")
            .value_name("n")
            .value_parser(clap::value_parser!(u64)),
    )
    .arg(
        Arg::new("offset")
            .long("offset")
            .value_name("n")
            .value_parser(clap::value_parser!(u64)),
    )
    .arg(
        Arg::new("param")
            .long("param")
            .value_name("name=value")
            .action(ArgAction::Append),
    )
    .arg(
        Arg::new("csv")
            .long("csv")
            .value_name("file")
            .value_hint(ValueHint::FilePath),
    )
}

fn command_columns(show_all: bool) -> Command {
    command_core(
        "columns",
        "Column discovery across tables",
        &["cols", "find-column"],
        show_all,
    )
    .arg(Arg::new("like").long("like").value_name("pattern"))
    .arg(Arg::new("table").long("table").value_name("pattern"))
    .arg(Arg::new("schema").long("schema").value_name("name"))
    .arg(
        Arg::new("include-views")
            .long("include-views")
            .action(ArgAction::SetTrue)
            .help("Include views in the search"),
    )
    .arg(
        Arg::new("limit")
            .long("limit")
            .value_name("n")
            .value_parser(clap::value_parser!(u64)),
    )
    .arg(
        Arg::new("offset")
            .long("offset")
            .value_name("n")
            .value_parser(clap::value_parser!(u64)),
    )
}

fn command_indexes(show_all: bool) -> Command {
    command_advanced("indexes", "Table index inspection", &[], show_all)
        .arg(Arg::new("table").long("table").value_name("name"))
        .arg(Arg::new("schema").long("schema").value_name("name"))
        .arg(
            Arg::new("show-usage")
                .long("show-usage")
                .action(ArgAction::SetTrue)
                .help("Include usage stats"),
        )
}

fn command_foreign_keys(show_all: bool) -> Command {
    command_advanced(
        "foreign-keys",
        "Table relationships",
        &["fks", "fk"],
        show_all,
    )
    .arg(Arg::new("table").long("table").value_name("name"))
    .arg(Arg::new("schema").long("schema").value_name("name"))
    .arg(Arg::new("direction").long("direction").value_name("mode"))
}

fn command_stored_procs(show_all: bool) -> Command {
    command_advanced(
        "stored-procs",
        "List/exec read-only procs",
        &["procs", "stored-procedures"],
        show_all,
    )
    .arg(Arg::new("schema").long("schema").value_name("name"))
    .arg(Arg::new("name").long("name").value_name("pattern"))
    .arg(
        Arg::new("include-system")
            .long("include-system")
            .action(ArgAction::SetTrue)
            .help("Include system procedures"),
    )
    .arg(
        Arg::new("limit")
            .long("limit")
            .value_name("n")
            .value_parser(clap::value_parser!(u64)),
    )
    .arg(
        Arg::new("offset")
            .long("offset")
            .value_name("n")
            .value_parser(clap::value_parser!(u64)),
    )
    .arg(Arg::new("exec").long("exec").value_name("proc"))
    .arg(Arg::new("args").long("args").value_name("text"))
}

fn command_sessions(show_all: bool) -> Command {
    command_advanced("sessions", "Active sessions", &["connections"], show_all)
        .arg(Arg::new("database").long("database").value_name("name"))
        .arg(Arg::new("login").long("login").value_name("name"))
        .arg(Arg::new("host").long("host").value_name("name"))
        .arg(Arg::new("status").long("status").value_name("state"))
        .arg(
            Arg::new("limit")
                .long("limit")
                .value_name("n")
                .value_parser(clap::value_parser!(u64)),
        )
}

fn command_query_stats(show_all: bool) -> Command {
    command_advanced("query-stats", "Top cached queries", &["stats"], show_all)
        .arg(Arg::new("database").long("database").value_name("name"))
        .arg(Arg::new("order").long("order").value_name("metric"))
        .arg(
            Arg::new("limit")
                .long("limit")
                .value_name("n")
                .value_parser(clap::value_parser!(u64)),
        )
}

fn command_backups(show_all: bool) -> Command {
    command_advanced(
        "backups",
        "Recent backups",
        &["backup-info", "backup"],
        show_all,
    )
    .arg(Arg::new("database").long("database").value_name("name"))
    .arg(
        Arg::new("since")
            .long("since")
            .value_name("days")
            .value_parser(clap::value_parser!(u64)),
    )
    .arg(Arg::new("type").long("type").value_name("kind"))
    .arg(
        Arg::new("limit")
            .long("limit")
            .value_name("n")
            .value_parser(clap::value_parser!(u64)),
    )
}

fn command_init(show_all: bool) -> Command {
    command_core("init", "Create config file", &[], show_all)
        .arg(
            Arg::new("path")
                .long("path")
                .value_name("path")
                .value_hint(ValueHint::FilePath),
        )
        .arg(Arg::new("force").long("force").action(ArgAction::SetTrue))
        .arg(Arg::new("profile").long("profile").value_name("name"))
}

fn command_config(show_all: bool) -> Command {
    command_core("config", "Display resolved config", &[], show_all)
}

fn command_completions(show_all: bool) -> Command {
    command_advanced("completions", "Generate shell completions", &[], show_all).arg(
        Arg::new("shell")
            .long("shell")
            .value_name("name")
            .value_parser(["bash", "zsh", "fish", "powershell", "elvish"]),
    )
}

fn command_integrations(show_all: bool) -> Command {
    let skills = Command::new("skills")
        .about("Install agent skills")
        .subcommand(
            Command::new("add")
                .about("Install bundled skill files")
                .arg(Arg::new("global").long("global").action(ArgAction::SetTrue))
                .arg(Arg::new("name").long("name").value_name("name")),
        );

    let gemini = Command::new("gemini")
        .about("Install Gemini extension")
        .subcommand(
            Command::new("add")
                .about("Install bundled Gemini extension")
                .arg(Arg::new("global").long("global").action(ArgAction::SetTrue))
                .arg(Arg::new("name").long("name").value_name("name")),
        );

    command_advanced(
        "integrations",
        "Optional editor/agent integrations",
        &["integrate"],
        show_all,
    )
    .subcommand(skills)
    .subcommand(gemini)
}

fn parse_matches(matches: &ArgMatches) -> CliArgs {
    let config_path = matches.get_one::<String>("config").map(PathBuf::from);
    let env_file = matches.get_one::<String>("env-file").map(PathBuf::from);
    let profile = matches.get_one::<String>("profile").cloned();
    let server = matches.get_one::<String>("server").cloned();
    let port = matches.get_one::<u16>("port").copied();
    let database = matches.get_one::<String>("database").cloned();
    let user = matches.get_one::<String>("user").cloned();
    let password = matches.get_one::<String>("password").cloned();
    let timeout_ms = matches.get_one::<u64>("timeout").copied();
    let allow_write = matches.get_flag("allow-write");
    let encrypt = matches.get_one::<bool>("encrypt").copied();
    let trust_cert = matches.get_one::<bool>("trust-cert").copied();
    let output = OutputFlags {
        json: matches.get_flag("json"),
        markdown: matches.get_flag("markdown"),
        pretty: matches.get_flag("pretty"),
    };
    let verbose = matches.get_count("verbose");
    let quiet = matches.get_flag("quiet");

    let command = match matches.subcommand() {
        Some(("help", sub_m)) => CommandKind::Help {
            all: sub_m.get_flag("all"),
            command: sub_m.get_one::<String>("command").cloned(),
        },
        Some(("status", _)) => CommandKind::Status(StatusArgs),
        Some(("databases", sub_m)) => CommandKind::Databases(DatabasesArgs {
            name: sub_m.get_one::<String>("name").cloned(),
            owner: sub_m.get_one::<String>("owner").cloned(),
            include_system: sub_m.get_flag("include-system"),
            limit: sub_m.get_one::<u64>("limit").copied(),
            offset: sub_m.get_one::<u64>("offset").copied(),
        }),
        Some(("tables", sub_m)) => CommandKind::Tables(TablesArgs {
            schema: sub_m.get_one::<String>("schema").cloned(),
            like: sub_m.get_one::<String>("like").cloned(),
            include_views: sub_m.get_flag("include-views"),
            with_counts: sub_m.get_flag("with-counts"),
            summary: sub_m.get_flag("summary"),
            describe: sub_m.get_flag("describe"),
            limit: sub_m.get_one::<String>("limit").cloned(),
            offset: sub_m.get_one::<u64>("offset").copied(),
        }),
        Some(("describe", sub_m)) => CommandKind::Describe(DescribeArgs {
            object: sub_m.get_one::<String>("object").cloned(),
            schema: sub_m.get_one::<String>("schema").cloned(),
            object_type: sub_m.get_one::<String>("type").cloned(),
            include_all: sub_m.get_flag("all"),
            no_indexes: sub_m.get_flag("no-indexes"),
            no_triggers: sub_m.get_flag("no-triggers"),
            no_ddl: sub_m.get_flag("no-ddl"),
            include_fks: sub_m.get_flag("include-fks"),
            include_constraints: sub_m.get_flag("include-constraints"),
        }),
        Some(("sql", sub_m)) => CommandKind::Sql(SqlArgs {
            sql: sub_m.get_one::<String>("sql").cloned(),
            file: sub_m.get_one::<String>("file").map(PathBuf::from),
            params: sub_m
                .get_many::<String>("param")
                .map(|values| values.cloned().collect())
                .unwrap_or_default(),
            max_rows: sub_m.get_one::<u64>("max-rows").copied(),
            csv: sub_m.get_one::<String>("csv").map(PathBuf::from),
            dry_run: sub_m.get_flag("dry-run"),
            continue_on_error: sub_m.get_flag("continue-on-error"),
        }),
        Some(("table-data", sub_m)) => CommandKind::TableData(TableDataArgs {
            table: sub_m.get_one::<String>("table").cloned(),
            schema: sub_m.get_one::<String>("schema").cloned(),
            columns: sub_m.get_one::<String>("columns").cloned(),
            where_clause: sub_m.get_one::<String>("where").cloned(),
            order_by: sub_m.get_one::<String>("order-by").cloned(),
            limit: sub_m.get_one::<u64>("limit").copied(),
            offset: sub_m.get_one::<u64>("offset").copied(),
            params: sub_m
                .get_many::<String>("param")
                .map(|values| values.cloned().collect())
                .unwrap_or_default(),
            csv: sub_m.get_one::<String>("csv").map(PathBuf::from),
        }),
        Some(("columns", sub_m)) => CommandKind::Columns(ColumnsArgs {
            like: sub_m.get_one::<String>("like").cloned(),
            table: sub_m.get_one::<String>("table").cloned(),
            schema: sub_m.get_one::<String>("schema").cloned(),
            include_views: sub_m.get_flag("include-views"),
            limit: sub_m.get_one::<u64>("limit").copied(),
            offset: sub_m.get_one::<u64>("offset").copied(),
        }),
        Some(("indexes", sub_m)) => CommandKind::Indexes(IndexesArgs {
            table: sub_m.get_one::<String>("table").cloned(),
            schema: sub_m.get_one::<String>("schema").cloned(),
            show_usage: sub_m.get_flag("show-usage"),
        }),
        Some(("foreign-keys", sub_m)) => CommandKind::ForeignKeys(ForeignKeysArgs {
            table: sub_m.get_one::<String>("table").cloned(),
            schema: sub_m.get_one::<String>("schema").cloned(),
            direction: sub_m.get_one::<String>("direction").cloned(),
        }),
        Some(("stored-procs", sub_m)) => CommandKind::StoredProcs(StoredProcsArgs {
            schema: sub_m.get_one::<String>("schema").cloned(),
            name: sub_m.get_one::<String>("name").cloned(),
            include_system: sub_m.get_flag("include-system"),
            limit: sub_m.get_one::<u64>("limit").copied(),
            offset: sub_m.get_one::<u64>("offset").copied(),
            exec: sub_m.get_one::<String>("exec").cloned(),
            args: sub_m.get_one::<String>("args").cloned(),
        }),
        Some(("sessions", sub_m)) => CommandKind::Sessions(SessionsArgs {
            database: sub_m.get_one::<String>("database").cloned(),
            login: sub_m.get_one::<String>("login").cloned(),
            host: sub_m.get_one::<String>("host").cloned(),
            status: sub_m.get_one::<String>("status").cloned(),
            limit: sub_m.get_one::<u64>("limit").copied(),
        }),
        Some(("query-stats", sub_m)) => CommandKind::QueryStats(QueryStatsArgs {
            database: sub_m.get_one::<String>("database").cloned(),
            order: sub_m.get_one::<String>("order").cloned(),
            limit: sub_m.get_one::<u64>("limit").copied(),
        }),
        Some(("backups", sub_m)) => CommandKind::Backups(BackupsArgs {
            database: sub_m.get_one::<String>("database").cloned(),
            since: sub_m.get_one::<u64>("since").copied(),
            backup_type: sub_m.get_one::<String>("type").cloned(),
            limit: sub_m.get_one::<u64>("limit").copied(),
        }),
        Some(("init", sub_m)) => CommandKind::Init(InitArgs {
            path: sub_m.get_one::<String>("path").map(PathBuf::from),
            force: sub_m.get_flag("force"),
            profile: sub_m.get_one::<String>("profile").cloned(),
        }),
        Some(("config", _)) => CommandKind::Config(ConfigArgs),
        Some(("completions", sub_m)) => CommandKind::Completions(CompletionsArgs {
            shell: sub_m.get_one::<String>("shell").cloned(),
        }),
        Some(("integrations", sub_m)) => CommandKind::Integrations(parse_integrations(sub_m)),
        _ => CommandKind::Help {
            all: false,
            command: None,
        },
    };

    CliArgs {
        config_path,
        env_file,
        profile,
        server,
        port,
        database,
        user,
        password,
        timeout_ms,
        allow_write,
        encrypt,
        trust_cert,
        output,
        verbose,
        quiet,
        command,
    }
}

fn parse_integrations(matches: &ArgMatches) -> IntegrationsArgs {
    let command = match matches.subcommand() {
        Some(("skills", sub_m)) => match sub_m.subcommand() {
            Some(("add", add_m)) => IntegrationCommand::Skills(IntegrationInstallArgs {
                global: add_m.get_flag("global"),
                name: add_m.get_one::<String>("name").cloned(),
            }),
            _ => IntegrationCommand::Help,
        },
        Some(("gemini", sub_m)) => match sub_m.subcommand() {
            Some(("add", add_m)) => IntegrationCommand::Gemini(IntegrationInstallArgs {
                global: add_m.get_flag("global"),
                name: add_m.get_one::<String>("name").cloned(),
            }),
            _ => IntegrationCommand::Help,
        },
        _ => IntegrationCommand::Help,
    };

    IntegrationsArgs { command }
}
