pub mod csv;
pub mod json;
pub mod raw;
pub mod table;

use std::io::IsTerminal;

use crate::cli::OutputFlags;
use crate::config::{OutputFormat, SettingsResolved};

pub use table::{RenderResult, TableOptions, TruncationInfo};

pub fn select_format(flags: &OutputFlags, settings: &SettingsResolved) -> OutputFormat {
    if let Some(format) = flags.format {
        return match format {
            crate::cli::OutputFormatArg::Pretty => OutputFormat::Pretty,
            crate::cli::OutputFormatArg::Markdown => OutputFormat::Markdown,
            crate::cli::OutputFormatArg::Json => OutputFormat::Json,
            crate::cli::OutputFormatArg::Tsv => OutputFormat::Tsv,
            crate::cli::OutputFormatArg::Csv => OutputFormat::Csv,
            crate::cli::OutputFormatArg::Jsonl => OutputFormat::Jsonl,
        };
    }
    if flags.json {
        return OutputFormat::Json;
    }
    if flags.markdown {
        return OutputFormat::Markdown;
    }
    if flags.pretty {
        return OutputFormat::Pretty;
    }

    let is_tty = std::io::stdout().is_terminal();
    if is_tty {
        settings.output.default_format
    } else {
        OutputFormat::Markdown
    }
}
