use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use crate::cli::{CliArgs, IntegrationCommand, IntegrationInstallArgs, IntegrationsArgs};

const DEFAULT_SKILL_NAME: &str = "sscli";
const DEFAULT_GEMINI_NAME: &str = "sscli";

const SKILL_TEMPLATE: &str = include_str!("../../assets/SKILL.md.template");
const GEMINI_MD: &str = include_str!("../../assets/GEMINI.md");

pub fn run(args: &CliArgs, cmd: &IntegrationsArgs) -> Result<()> {
    match &cmd.command {
        IntegrationCommand::Help => {
            if !args.quiet {
                print_help();
            }
            Ok(())
        }
        IntegrationCommand::Skills(opts) => install_skills(opts, args.quiet),
        IntegrationCommand::Gemini(opts) => install_gemini(opts, args.quiet),
    }
}

fn print_help() {
    println!("sscli integrations");
    println!("Usage:");
    println!("  sscli integrations skills add [--global] [--name <skillName>]");
    println!("  sscli integrations gemini add [--global] [--name <extensionName>]");
}

fn install_skills(opts: &IntegrationInstallArgs, quiet: bool) -> Result<()> {
    let name = opts.name.as_deref().unwrap_or(DEFAULT_SKILL_NAME);
    if name.trim().is_empty() {
        return Err(anyhow!("--name must be non-empty"));
    }

    let cwd = std::env::current_dir()?;
    let (codex_root, claude_root) = if opts.global {
        let codex_home = std::env::var("CODEX_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
            .ok_or_else(|| anyhow!("Unable to resolve CODEX_HOME"))?;
        let claude_home = std::env::var("CLAUDE_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".claude")))
            .ok_or_else(|| anyhow!("Unable to resolve CLAUDE_HOME"))?;
        (codex_home.join("skills"), claude_home.join("skills"))
    } else {
        (
            cwd.join(".codex").join("skills"),
            cwd.join(".claude").join("skills"),
        )
    };

    let codex_dest = codex_root.join(name);
    let claude_dest = claude_root.join(name);

    write_skill(&codex_dest, name)?;
    write_skill(&claude_dest, name)?;

    if !quiet {
        println!("Installed skill '{}'", name);
        println!("- Codex:  {}", codex_dest.display());
        println!("- Claude: {}", claude_dest.display());
    }

    Ok(())
}

fn install_gemini(opts: &IntegrationInstallArgs, quiet: bool) -> Result<()> {
    let name = opts.name.as_deref().unwrap_or(DEFAULT_GEMINI_NAME);
    if name.trim().is_empty() {
        return Err(anyhow!("--name must be non-empty"));
    }

    let cwd = std::env::current_dir()?;
    let base = if opts.global {
        dirs::home_dir()
            .ok_or_else(|| anyhow!("Unable to resolve home directory"))?
            .join(".gemini")
    } else {
        cwd.join(".gemini")
    };

    let dest = base.join("extensions").join(name);
    fs::create_dir_all(&dest)?;

    fs::write(dest.join("GEMINI.md"), GEMINI_MD)?;
    fs::write(dest.join("gemini-extension.json"), render_gemini_json(name))?;

    if !quiet {
        println!(
            "Installed Gemini extension '{}' at {}",
            name,
            dest.display()
        );
    }
    Ok(())
}

fn write_skill(dest: &Path, name: &str) -> Result<()> {
    fs::create_dir_all(dest)?;
    fs::write(dest.join("SKILL.md"), render_skill_template(name))?;
    Ok(())
}

fn render_skill_template(name: &str) -> String {
    SKILL_TEMPLATE.replace("{name}", name)
}

fn render_gemini_json(name: &str) -> String {
    let version = env!("CARGO_PKG_VERSION");
    format!(
        "{{\n  \"name\": \"{name}\",\n  \"version\": \"{version}\",\n  \"contextFileName\": \"GEMINI.md\"\n}}\n"
    )
}
