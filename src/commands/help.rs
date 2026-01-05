use std::io::{self, Write};

use crate::cli::build_cli;

pub fn run(show_all: bool, command: Option<&str>) -> anyhow::Result<()> {
    let mut cmd = build_cli(show_all);

    if let Some(name) = command {
        if let Some(sub) = cmd.find_subcommand_mut(name) {
            sub.print_long_help()?;
            io::stdout().flush()?;
            return Ok(());
        }
    }

    cmd.print_long_help()?;
    io::stdout().flush()?;
    Ok(())
}
