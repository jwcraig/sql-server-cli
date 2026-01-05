use anyhow::Result;
use serde_json::json;

use crate::cli::{CliArgs, UpdateArgs};
use crate::config::OutputFormat;
use crate::output::{TableOptions, json as json_out, table};

pub fn run(args: &CliArgs, _cmd: &UpdateArgs) -> Result<()> {
    let check = crate::update::check_latest_release()?;

    if args.output.json {
        let payload = json!({
            "currentVersion": check.current_version,
            "latestVersion": check.latest_version,
            "updateAvailable": check.update_available,
            "repo": check.repo,
            "releaseUrl": check.release_url,
            "instructions": update_instructions(),
        });

        let body = json_out::emit_json_value(&payload, true)?;
        if !args.quiet {
            println!("{}", body);
        }
        return Ok(());
    }

    if args.quiet {
        return Ok(());
    }

    let format = output_format(args);
    let mut rows = vec![
        ("CurrentVersion".to_string(), check.current_version),
        ("LatestVersion".to_string(), check.latest_version),
        (
            "UpdateAvailable".to_string(),
            if check.update_available {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
        ("Repo".to_string(), check.repo),
    ];
    if let Some(url) = check.release_url {
        rows.push(("ReleaseUrl".to_string(), url));
    }

    let rendered = table::render_key_value_table("Update", &rows, format, &TableOptions::default());
    println!("{}", rendered);

    if check.update_available {
        println!();
        println!("Update:");
        for line in update_instructions() {
            println!("  {}", line);
        }
    }

    Ok(())
}

fn update_instructions() -> Vec<&'static str> {
    vec![
        "brew upgrade sscli",
        "scoop update sscli",
        "cargo install sscli --force",
        "cargo binstall sscli --force",
    ]
}

fn output_format(args: &CliArgs) -> OutputFormat {
    if args.output.markdown {
        return OutputFormat::Markdown;
    }
    OutputFormat::Pretty
}
