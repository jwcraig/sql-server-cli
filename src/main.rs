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
    init_logging(args.verbose);
    commands::dispatch(&args)
}

fn init_logging(verbose: u8) {
    let filter = match verbose {
        0 => "warn,tiberius=error",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(io::stderr)
        .try_init();
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
