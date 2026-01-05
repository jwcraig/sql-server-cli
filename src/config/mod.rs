mod env;
mod loader;
mod schema;

pub use env::{Env, parse_bool};
pub use loader::{
    CliOverrides, ConnectionSettings, LoadOptions, OutputSettingsResolved, ResolvedConfig,
    SettingsResolved, load_config,
};
pub use schema::{
    ConfigFile, CsvMultiResultNaming, JsonContractVersion, JsonSettings, OutputFormat,
    OutputSettings, Profile, Settings,
};

pub fn load_from_system(cli: &CliOverrides) -> anyhow::Result<ResolvedConfig> {
    let cwd = std::env::current_dir()?;
    let home_dir = dirs::home_dir();
    let xdg_config_dir = dirs::config_dir();
    let env = Env::from_system(cli.env_file.as_deref());
    let options = LoadOptions {
        cli: cli.clone(),
        cwd,
        home_dir,
        xdg_config_dir,
    };
    load_config(&options, &env)
}
