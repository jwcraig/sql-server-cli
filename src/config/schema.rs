use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    pub default_profile: Option<String>,
    pub settings: Option<Settings>,
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub allow_write_default: Option<bool>,
    pub output: Option<OutputSettings>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OutputSettings {
    pub default_format: Option<OutputFormat>,
    pub json: Option<JsonSettings>,
    pub csv: Option<CsvSettings>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JsonSettings {
    pub contract_version: Option<JsonContractVersion>,
    pub pretty: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CsvSettings {
    pub multi_result_naming: Option<CsvMultiResultNaming>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub server: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub user: Option<String>,
    pub password_env: Option<String>,
    pub password: Option<String>,
    pub encrypt: Option<bool>,
    pub trust_cert: Option<bool>,
    pub timeout: Option<u64>,
    pub default_schemas: Option<Vec<String>>,
    pub settings: Option<Settings>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Pretty,
    Markdown,
    Json,
}

impl OutputFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputFormat::Pretty => "pretty",
            OutputFormat::Markdown => "markdown",
            OutputFormat::Json => "json",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JsonContractVersion {
    #[serde(rename = "v1")]
    V1,
}

impl JsonContractVersion {
    pub fn as_str(&self) -> &'static str {
        match self {
            JsonContractVersion::V1 => "v1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CsvMultiResultNaming {
    #[serde(rename = "suffix-number")]
    SuffixNumber,
    #[serde(rename = "placeholder")]
    Placeholder,
}

impl CsvMultiResultNaming {
    pub fn as_str(&self) -> &'static str {
        match self {
            CsvMultiResultNaming::SuffixNumber => "suffix-number",
            CsvMultiResultNaming::Placeholder => "placeholder",
        }
    }
}
