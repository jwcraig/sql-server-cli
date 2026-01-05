pub mod csv;
pub mod json;
pub mod table;

use std::io::IsTerminal;

use crate::cli::OutputFlags;
use crate::config::{OutputFormat, SettingsResolved};

pub use table::TableOptions;

pub fn select_format(flags: &OutputFlags, settings: &SettingsResolved) -> OutputFormat {
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
