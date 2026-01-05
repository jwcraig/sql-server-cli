use std::io;

use anyhow::{anyhow, Result};
use clap_complete::{generate, Shell};

use crate::cli::{build_cli, CliArgs, CompletionsArgs};

pub fn run(_args: &CliArgs, cmd: &CompletionsArgs) -> Result<()> {
    let shell_name = cmd
        .shell
        .as_deref()
        .ok_or_else(|| anyhow!("--shell is required"))?;

    let shell = match shell_name {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "powershell" => Shell::PowerShell,
        "elvish" => Shell::Elvish,
        _ => return Err(anyhow!("Unsupported shell: {}", shell_name)),
    };

    let mut cmd = build_cli(true);
    generate(shell, &mut cmd, "sscli", &mut io::stdout());
    Ok(())
}
