#![allow(clippy::uninlined_format_args)]

use std::io::{self, IsTerminal, Write};

use owo_colors::OwoColorize;
use sscli::cli;
use sscli::commands;
use sscli::error;
use sscli::output::json;

fn main() {
    if let Err(err) = run() {
        let message = err.to_string();
        let args = cli::parse();
        let kind = error::classify_error(&err);
        if args.output.json {
            let payload = json::error_json(&message, kind.as_str());
            if let Ok(body) = json::emit_json_value(&payload, true) {
                let _ = writeln!(io::stderr(), "{}", body);
            }
        } else {
            print_error(&message);
        }
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let args = cli::parse();
    init_logging(&args);
    commands::dispatch(&args)
}

fn init_logging(args: &cli::CliArgs) {
    let mut filter = match args.verbose {
        // Tiberius logs every server token error at ERROR level before returning
        // it, which duplicates sscli's own error output above the structured
        // summary. Silence it by default; `-v` and above restore driver logs.
        0 => "warn,tiberius=off",
        1 => "info",
        2 => "debug",
        _ => "trace",
    }
    .to_string();
    if should_quiet_tls_warning(args) && !filter.contains("tiberius=") {
        filter.push_str(",tiberius=error");
    }
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(io::stderr)
        .try_init();
}

fn should_quiet_tls_warning(args: &cli::CliArgs) -> bool {
    args.quiet_tls_warning
        || matches!(args.trust_cert, Some(true))
        || matches!(
            args.output.format,
            Some(
                cli::OutputFormatArg::Tsv | cli::OutputFormatArg::Csv | cli::OutputFormatArg::Jsonl
            )
        )
}

fn print_error(message: &str) {
    if should_color_stderr() {
        let line = format!("Error: {}", message);
        let _ = writeln!(io::stderr(), "{}", line.red());
    } else {
        let _ = writeln!(io::stderr(), "Error: {}", message);
    }
}

fn should_color_stderr() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    io::stderr().is_terminal()
}
